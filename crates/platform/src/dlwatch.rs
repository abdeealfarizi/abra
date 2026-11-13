//! Pemantau unduhan untuk notch, satu thread yang memeriksa tiap detik:
//! - Microsoft Store: AppInstallManager (persen asli). Namespace ini tidak ada di crate `windows`, jadi dipanggil lewat
//!   vtable COM mentah dengan GUID antarmuka resmi (IAppInstallManager/Item/Status).
//! - Steam: appmanifest_*.acf di tiap perpustakaan (BytesDownloaded / BytesToDownload).
//! - Peramban: berkas .crdownload / .part di folder Unduhan; ukuran total tidak diketahui (busur berputar + ukuran).
//!   Pengunduhnya dicari lewat Restart Manager (proses yang membuka berkas) untuk ikon.
//! Hanya metadata yang dibaca; isi berkas tidak pernah dibuka.

use nacre_core::notch::download::{steam_libraries, steam_progress, vdf_value, Download, Source};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub active: Vec<Download>,
    /// Unduhan yang baru saja selesai (sekali kirim).
    pub finished: Option<Download>,
}

pub struct Watch {
    pub snapshots: Receiver<Snapshot>,
}

const STORE_ICON: &str = "shell:AppsFolder\\Microsoft.WindowsStore_8wekyb3d8bbwe!App";
const PARTIAL: [&str; 4] = [".crdownload", ".part", ".partial", ".download"];
/// Berkas parsial yang tidak bertambah selama ini dianggap dijeda (Chrome menyimpan .crdownload saat dijeda).
const STALL: Duration = Duration::from_secs(10);

// ---------------------------------------------------------------------------------------------------------------
// Microsoft Store (COM mentah)
// ---------------------------------------------------------------------------------------------------------------

mod store {
    use std::ffi::c_void;
    use windows::core::{GUID, HRESULT, HSTRING};
    use windows::Win32::System::WinRT::RoActivateInstance;
    use windows_core::{IUnknown, Interface};

    const IID_MANAGER: GUID = GUID::from_u128(0x9353e170_8441_4b45_bd72_7c2fa925beee);

    pub struct Item {
        pub key: String,
        pub state: i32,
        pub percent: f64,
        pub bytes: u64,
    }

    /// Fungsi ke-`i` di vtable objek `p`.
    unsafe fn slot<F: Copy>(p: *mut c_void, i: usize) -> F {
        let vt = *(p as *const *const *const c_void);
        std::mem::transmute_copy(&*vt.add(i))
    }

    type GetPtr = unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT;
    type GetAt = unsafe extern "system" fn(*mut c_void, u32, *mut *mut c_void) -> HRESULT;
    type GetU32 = unsafe extern "system" fn(*mut c_void, *mut u32) -> HRESULT;
    type GetI32 = unsafe extern "system" fn(*mut c_void, *mut i32) -> HRESULT;
    type GetU64 = unsafe extern "system" fn(*mut c_void, *mut u64) -> HRESULT;
    type GetF64 = unsafe extern "system" fn(*mut c_void, *mut f64) -> HRESULT;
    type GetStr = unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT;

    /// Pointer COM yang dilepas saat keluar cakupan.
    fn own(p: *mut c_void) -> Option<IUnknown> {
        (!p.is_null()).then(|| unsafe { IUnknown::from_raw(p) })
    }

    pub struct Manager(IUnknown);

    impl Manager {
        pub fn new() -> Option<Manager> {
            let insp = unsafe { RoActivateInstance(&HSTRING::from("Windows.ApplicationModel.Store.Preview.InstallControl.AppInstallManager")) }.ok()?;
            let mut p: *mut c_void = std::ptr::null_mut();
            unsafe { insp.query(&IID_MANAGER, &mut p) }.ok().ok()?;
            own(p).map(Manager)
        }

        /// Item antrean pemasangan (IAppInstallManager.AppInstallItems → IVectorView<AppInstallItem>).
        pub fn items(&self) -> Vec<Item> {
            let mut out = Vec::new();
            unsafe {
                let m = self.0.as_raw();
                let mut v: *mut c_void = std::ptr::null_mut();
                if slot::<GetPtr>(m, 6)(m, &mut v).is_err() {
                    return out;
                }
                let Some(view) = own(v) else { return out };
                let vp = view.as_raw();
                let mut n = 0u32;
                if slot::<GetU32>(vp, 7)(vp, &mut n).is_err() {
                    return out;
                }
                for i in 0..n.min(32) {
                    let mut ip: *mut c_void = std::ptr::null_mut();
                    if slot::<GetAt>(vp, 6)(vp, i, &mut ip).is_err() {
                        continue;
                    }
                    let Some(item) = own(ip) else { continue };
                    let it = item.as_raw();
                    // IAppInstallItem: 6 ProductId, 7 PackageFamilyName, 10 GetCurrentStatus
                    let mut hs: *mut c_void = std::ptr::null_mut();
                    let key = if slot::<GetStr>(it, 6)(it, &mut hs).is_ok() && !hs.is_null() {
                        let h: HSTRING = std::mem::transmute(hs);
                        h.to_string()
                    } else {
                        format!("item{i}")
                    };
                    let mut sp: *mut c_void = std::ptr::null_mut();
                    if slot::<GetPtr>(it, 10)(it, &mut sp).is_err() {
                        continue;
                    }
                    let Some(status) = own(sp) else { continue };
                    let s = status.as_raw();
                    // IAppInstallStatus: 6 InstallState, 8 BytesDownloaded, 9 PercentComplete
                    let (mut state, mut bytes, mut pct) = (0i32, 0u64, 0f64);
                    let _ = slot::<GetI32>(s, 6)(s, &mut state);
                    let _ = slot::<GetU64>(s, 8)(s, &mut bytes);
                    let _ = slot::<GetF64>(s, 9)(s, &mut pct);
                    out.push(Item { key, state, percent: pct, bytes });
                }
            }
            out
        }
    }

    /// AppInstallState: Pending 0, Starting 1, AcquiringLicense 2, Downloading 3, RestoringData 4, Installing 5,
    /// Completed 6 (selainnya: dibatalkan, dijeda, galat).
    pub fn is_active(state: i32) -> bool {
        (0..=5).contains(&state)
    }
    pub const COMPLETED: i32 = 6;
}

// ---------------------------------------------------------------------------------------------------------------
// Steam
// ---------------------------------------------------------------------------------------------------------------

fn reg_str(path: &str, name: &str) -> Option<String> {
    use windows::core::HSTRING;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};
    unsafe {
        let (p, n) = (HSTRING::from(path), HSTRING::from(name));
        let mut size = 0u32;
        RegGetValueW(HKEY_CURRENT_USER, &p, &n, RRF_RT_REG_SZ, None, None, Some(&mut size)).ok().ok()?;
        let mut buf = vec![0u16; (size as usize).div_ceil(2).max(1)];
        RegGetValueW(HKEY_CURRENT_USER, &p, &n, RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut size)).ok().ok()?;
        let len = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..len]))
    }
}

struct Steam {
    exe: String,
    manifests: Vec<PathBuf>,
}

fn steam() -> Option<Steam> {
    let root = std::env::var("NACRE_TEST_STEAM").ok().or_else(|| reg_str("Software\\Valve\\Steam", "SteamPath"))?.replace('/', "\\");
    let exe = reg_str("Software\\Valve\\Steam", "SteamExe").map(|e| e.replace('/', "\\")).unwrap_or_else(|| format!("{root}\\steam.exe"));
    let mut libs = vec![root.clone()];
    if let Ok(vdf) = std::fs::read_to_string(Path::new(&root).join("steamapps").join("libraryfolders.vdf")) {
        for l in steam_libraries(&vdf) {
            if !libs.iter().any(|x| x.eq_ignore_ascii_case(&l)) {
                libs.push(l);
            }
        }
    }
    let mut manifests = Vec::new();
    for l in libs {
        if let Ok(rd) = std::fs::read_dir(Path::new(&l).join("steamapps")) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().to_lowercase();
                if n.starts_with("appmanifest_") && n.ends_with(".acf") {
                    manifests.push(e.path());
                }
            }
        }
    }
    Some(Steam { exe, manifests })
}

// ---------------------------------------------------------------------------------------------------------------
// Peramban
// ---------------------------------------------------------------------------------------------------------------

struct Part {
    size: u64,
    changed: Instant,
    writer: Option<String>,
    tries: u8,
}

fn partial_suffix(name: &str) -> Option<&'static str> {
    let l = name.to_lowercase();
    PARTIAL.iter().copied().find(|s| l.ends_with(s))
}

/// Exe proses yang membuka berkas `path` (Restart Manager), mis. brave.exe untuk .crdownload.
fn writer_of(path: &Path) -> Option<String> {
    use windows::core::{PCWSTR, PWSTR};
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::RestartManager::*;
    use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
    unsafe {
        let mut session = 0u32;
        let mut key = [0u16; 64];
        if RmStartSession(&mut session, None, PWSTR(key.as_mut_ptr())).is_err() {
            return None;
        }
        let wide: Vec<u16> = path.as_os_str().to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect();
        let mut result = None;
        if RmRegisterResources(session, Some(&[PCWSTR(wide.as_ptr())]), None, None).is_ok() {
            let (mut needed, mut count, mut reasons) = (0u32, 8u32, 0u32);
            let mut info = [RM_PROCESS_INFO::default(); 8];
            if RmGetList(session, &mut needed, &mut count, Some(info.as_mut_ptr()), &mut reasons).is_ok() {
                let me = std::process::id();
                for pi in info.iter().take(count as usize) {
                    let pid = pi.Process.dwProcessId;
                    if pid == me {
                        continue;
                    }
                    if let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
                        let mut buf = [0u16; 520];
                        let mut len = buf.len() as u32;
                        if QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok() {
                            result = Some(String::from_utf16_lossy(&buf[..len as usize]));
                        }
                        let _ = CloseHandle(h);
                    }
                    if result.is_some() {
                        break;
                    }
                }
            }
        }
        let _ = RmEndSession(session);
        result
    }
}

fn downloads_dir() -> Option<PathBuf> {
    std::env::var_os("NACRE_TEST_DOWNLOADS").map(PathBuf::from).or_else(crate::downloads::folder)
}

// ---------------------------------------------------------------------------------------------------------------
// Pekerja
// ---------------------------------------------------------------------------------------------------------------

/// Unduhan palsu untuk uji tampilan (`NACRE_FAKE_DOWNLOAD=store|browser`): naik dari 0 sampai selesai dalam ±16 detik.
fn fake(kind: &str, t: f32) -> Snapshot {
    let app = std::env::var("SystemRoot").map(|r| format!("{r}\\explorer.exe")).unwrap_or_default();
    let pct = (t / 16.0 * 100.0).min(100.0);
    let d = Download {
        source: if kind == "browser" { Source::Browser } else { Source::Store },
        app,
        name: "Unduhan Uji".into(),
        percent: (kind != "browser").then_some(pct),
        bytes: (pct as f64 * 3_500_000.0) as u64,
    };
    if pct >= 100.0 {
        Snapshot { active: vec![], finished: Some(Download { percent: Some(100.0), ..d }) }
    } else {
        Snapshot { active: vec![d], finished: None }
    }
}

pub fn spawn(notify: impl Fn() + Send + 'static) -> Watch {
    let (tx, rx) = channel::<Snapshot>();
    std::thread::spawn(move || {
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
        }
        if let Ok(kind) = std::env::var("NACRE_FAKE_DOWNLOAD") {
            let t0 = Instant::now();
            let mut sent_done = false;
            loop {
                std::thread::sleep(Duration::from_millis(500));
                let s = fake(&kind, t0.elapsed().as_secs_f32());
                if s.finished.is_some() {
                    if sent_done {
                        continue;
                    }
                    sent_done = true;
                }
                if tx.send(s).is_err() {
                    return;
                }
                notify();
            }
        }
        let store = store::Manager::new();
        // sekali di awal: memastikan pembacaan antrean Store berjalan (hanya status, tanpa nama app)
        let states: Vec<i32> = store.as_ref().map(|m| m.items().iter().map(|i| i.state).collect()).unwrap_or_default();
        crate::log::line("unduhan", &format!("pemantau mulai: Store {} (status antrean {states:?})", if store.is_some() { "terbaca" } else { "tidak tersedia" }));
        let mut steam_info: Option<Steam> = None;
        let mut steam_checked: Option<Instant> = None;
        let mut parts: HashMap<PathBuf, Part> = HashMap::new();
        let mut prev: HashMap<String, Download> = HashMap::new();
        let mut last = Snapshot::default();
        let mut tick = 0u64;
        loop {
            std::thread::sleep(Duration::from_millis(1000));
            tick += 1;
            let mut active: Vec<(String, Download)> = Vec::new();
            let mut finished: Option<Download> = None;

            // Store
            let store_items = store.as_ref().map(|m| m.items()).unwrap_or_default();
            for it in &store_items {
                if store::is_active(it.state) {
                    let d = Download { source: Source::Store, app: STORE_ICON.into(), name: it.key.clone(), percent: Some(it.percent.clamp(0.0, 100.0) as f32), bytes: it.bytes };
                    active.push((format!("store:{}", it.key), d));
                }
            }

            // Steam (periksa ulang daftar manifes tiap menit; baca manifes tiap 2 detik)
            if steam_checked.is_none_or(|t| t.elapsed() > Duration::from_secs(60)) {
                steam_info = steam();
                steam_checked = Some(Instant::now());
            }
            if let Some(st) = &steam_info {
                if tick % 2 == 0 {
                    for m in &st.manifests {
                        let Ok(acf) = std::fs::read_to_string(m) else { continue };
                        if let Some((name, done, total)) = steam_progress(&acf) {
                            let d = Download { source: Source::Steam, app: st.exe.clone(), name, percent: Some((done as f64 / total as f64 * 100.0) as f32), bytes: done };
                            active.push((format!("steam:{}", m.display()), d));
                        }
                    }
                } else {
                    // detik ganjil: pertahankan unduhan Steam yang sudah diketahui
                    active.extend(prev.iter().filter(|(k, _)| k.starts_with("steam:")).map(|(k, d)| (k.clone(), d.clone())));
                }
            }

            // Peramban
            let mut seen = Vec::new();
            if let Some(dir) = downloads_dir() {
                if let Ok(rd) = std::fs::read_dir(&dir) {
                    for e in rd.flatten().take(4000) {
                        let name = e.file_name().to_string_lossy().to_string();
                        let Some(suffix) = partial_suffix(&name) else { continue };
                        let path = e.path();
                        // ukuran dari berkasnya sendiri: entri folder NTFS baru diperbarui saat pengunduh menutup berkas
                        let Ok(md) = std::fs::metadata(&path) else { continue };
                        let size = md.len();
                        let part = parts.entry(path.clone()).or_insert(Part { size, changed: Instant::now(), writer: None, tries: 0 });
                        if part.size != size {
                            part.size = size;
                            part.changed = Instant::now();
                        }
                        if part.writer.is_none() && part.tries < 3 {
                            part.tries += 1;
                            part.writer = writer_of(&path);
                        }
                        seen.push(path.clone());
                        if part.changed.elapsed() < STALL {
                            let shown = name[..name.len() - suffix.len()].to_string();
                            let d = Download { source: Source::Browser, app: part.writer.clone().unwrap_or_default(), name: shown, percent: None, bytes: size };
                            active.push((format!("file:{}", path.display()), d));
                        }
                    }
                }
            }
            // berkas parsial hilang: selesai bila berkas akhirnya ada
            let gone: Vec<PathBuf> = parts.keys().filter(|p| !seen.contains(p)).cloned().collect();
            for p in gone {
                let part = parts.remove(&p);
                let key = format!("file:{}", p.display());
                let s = p.to_string_lossy().to_string();
                let final_path = partial_suffix(&s).map(|suf| PathBuf::from(&s[..s.len() - suf.len()]));
                if let (Some(fp), Some(part)) = (final_path, part) {
                    if fp.exists() && prev.contains_key(&key) {
                        let size = std::fs::metadata(&fp).map(|m| m.len()).unwrap_or(part.size);
                        finished = Some(Download { source: Source::Browser, app: part.writer.unwrap_or_default(), name: fp.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), percent: Some(100.0), bytes: size });
                    }
                }
            }
            // Store/Steam yang hilang dari daftar aktif: selesai bila sudah hampir penuh atau Store menandai Completed
            for (k, d) in &prev {
                if active.iter().any(|(ak, _)| ak == k) || k.starts_with("file:") {
                    continue;
                }
                let completed = match d.source {
                    Source::Store => store_items.iter().any(|it| format!("store:{}", it.key) == *k && it.state == store::COMPLETED) || d.percent.unwrap_or(0.0) >= 95.0,
                    Source::Steam => {
                        let path = k.trim_start_matches("steam:");
                        std::fs::read_to_string(path).ok().is_some_and(|acf| vdf_value(&acf, "StateFlags").and_then(|f| f.parse::<u64>().ok()).is_some_and(|f| f & 4 != 0))
                    }
                    Source::Browser => false,
                };
                if completed {
                    finished = Some(Download { percent: Some(100.0), ..d.clone() });
                }
            }

            prev = active.iter().cloned().collect();
            let snap = Snapshot { active: active.into_iter().map(|(_, d)| d).collect(), finished };
            if snap != last {
                if tx.send(snap.clone()).is_err() {
                    return;
                }
                notify();
                last = Snapshot { finished: None, ..snap };
            }
        }
    });
    Watch { snapshots: rx }
}
