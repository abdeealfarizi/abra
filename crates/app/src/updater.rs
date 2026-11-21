use nacre_core::update::{self, Release};
use std::sync::Mutex;
use std::time::Duration;

#[derive(Clone, Debug, PartialEq)]
pub enum Phase {
    Idle,
    Checking,
    Latest,
    Available(Release),
    Downloading(String),
    Failed(String),
}

static STATE: Mutex<Phase> = Mutex::new(Phase::Idle);
static BUSY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

pub fn phase() -> Phase {
    STATE.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

fn set(p: Phase) {
    *STATE.lock().unwrap_or_else(|e| e.into_inner()) = p;
    crate::prefs::broadcast();
}

fn endpoint() -> (String, String) {
    match std::env::var("NACRE_UPDATE_URL") {
        Ok(u) if u.starts_with("http://127.0.0.1:") => {
            let origin: String = u.splitn(4, '/').take(3).collect::<Vec<_>>().join("/");
            (u, format!("{origin}/"))
        }
        _ => (update::API.to_string(), format!("https://github.com/{}/releases/download/", update::REPO)),
    }
}

fn seen_file() -> std::path::PathBuf {
    crate::paths::config_dir().join("update_seen.txt")
}

pub fn init() {
    std::thread::Builder::new()
        .name("abra-update".into())
        .spawn(|| {
            let first = if std::env::var_os("NACRE_UPDATE_URL").is_some() { 3 } else { 60 };
            std::thread::sleep(Duration::from_secs(first));
            loop {
                if crate::prefs::get().update_check {
                    check(false);
                }
                std::thread::sleep(Duration::from_secs(12 * 3600));
            }
        })
        .ok();
}

pub fn check_now() {
    if BUSY.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    std::thread::Builder::new().name("abra-update-now".into()).spawn(|| check(true)).ok();
}

fn check(manual: bool) {
    if BUSY.swap(true, std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    if manual {
        set(Phase::Checking);
    }
    let (url, allowed) = endpoint();
    let result = nacre_platform::net::http_get_accept(&url, Some("application/vnd.github+json"), Duration::from_secs(12));
    let next = match result {
        Some((200, body)) => match update::parse_release_from(&body, &allowed) {
            Some(r) if update::is_newer(&r.version, CURRENT) => Some(Phase::Available(r)),
            Some(_) => Some(Phase::Latest),
            None => manual.then(|| Phase::Failed(crate::prefs::tr("Data rilis tidak terbaca").to_string())),
        },
        Some((code, _)) => manual.then(|| Phase::Failed(format!("HTTP {code}"))),
        None => manual.then(|| Phase::Failed(crate::prefs::tr("Tidak tersambung ke internet").to_string())),
    };
    nacre_platform::log::line("update", &format!("cek selesai: {next:?}"));
    if let Some(Phase::Available(r)) = &next {
        let seen = std::fs::read_to_string(seen_file()).unwrap_or_default();
        if seen.trim() != r.version {
            let _ = std::fs::write(seen_file(), &r.version);
            let title = crate::prefs::tr("Pembaruan tersedia");
            crate::notch::push_pill(nacre_core::notch::pills::Kind::Update, "update", title, &format!("ABRA {}", r.version), Some(nacre_core::notch::pills::UPDATE_MS));
        }
    }
    let auto = test_mode() && std::env::var_os("NACRE_UPDATE_AUTOINSTALL").is_some() && matches!(next, Some(Phase::Available(_)));
    if let Some(p) = next {
        set(p);
    } else if phase() == Phase::Checking {
        set(Phase::Idle);
    }
    BUSY.store(false, std::sync::atomic::Ordering::Relaxed);
    if auto {
        install();
    }
}

fn test_mode() -> bool {
    std::env::var("NACRE_UPDATE_URL").is_ok_and(|u| u.starts_with("http://127.0.0.1:"))
}

pub fn installed() -> bool {
    if test_mode() {
        return true;
    }
    let Ok(exe) = std::env::current_exe() else { return false };
    let Some(dir) = exe.parent() else { return false };
    let Some(base) = std::env::var_os("LOCALAPPDATA") else { return false };
    let want = std::path::Path::new(&base).join("Programs").join("ABRA");
    dir.to_string_lossy().to_lowercase() == want.to_string_lossy().to_lowercase()
}

pub fn install() {
    let Phase::Available(r) = phase() else { return };
    if !installed() {
        nacre_platform::launch::open(&r.page);
        return;
    }
    if BUSY.swap(true, std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    set(Phase::Downloading(r.version.clone()));
    std::thread::Builder::new()
        .name("abra-update-install".into())
        .spawn(move || {
            let outcome = fetch_and_verify(&r);
            match outcome {
                Ok(path) => {
                    nacre_platform::log::line("update", &format!("installer {} terverifikasi, menjalankan", r.version));
                    if nacre_platform::launch::run_visible(&path.to_string_lossy(), "/SILENT /SUPPRESSMSGBOXES /NORESTART /RELAUNCH") {
                        std::thread::sleep(Duration::from_millis(800));
                        crate::mainwin::request_quit();
                    } else {
                        set(Phase::Failed(crate::prefs::tr("Installer gagal dijalankan").to_string()));
                    }
                }
                Err(e) => {
                    nacre_platform::log::line("update", &format!("pembaruan dibatalkan: {e}"));
                    set(Phase::Failed(e));
                }
            }
            BUSY.store(false, std::sync::atomic::Ordering::Relaxed);
        })
        .ok();
}

fn fetch_and_verify(r: &Release) -> Result<std::path::PathBuf, String> {
    let (Some(url), Some(name)) = (&r.setup_url, &r.setup_name) else { return Err(crate::prefs::tr("Installer tidak ada di rilis ini").to_string()) };
    let Some(want) = &r.sha256 else { return Err(crate::prefs::tr("Sidik jari installer tidak tersedia").to_string()) };
    if !update::safe_file_name(name) {
        return Err(crate::prefs::tr("Nama berkas installer tidak wajar").to_string());
    }
    let dir = std::env::temp_dir().join("ABRA-update");
    let path = dir.join(name);
    nacre_platform::net::download(url, &path, Duration::from_secs(180), 200 * 1024 * 1024)?;
    let got = nacre_platform::net::sha256_file(&path).unwrap_or_default();
    if &got != want {
        let _ = std::fs::remove_file(&path);
        return Err(crate::prefs::tr("Sidik jari installer tidak cocok").to_string());
    }
    Ok(path)
}

pub fn open_release_notes() {
    match phase() {
        Phase::Available(r) => nacre_platform::launch::open(&r.page),
        _ => nacre_platform::launch::open(update::RELEASES),
    }
}
