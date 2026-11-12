//! Paket ikon opsional (mis. folder PNG ikon macOS) yang dipasang pengguna di `%APPDATA%\ABRA\iconpack`.
//!
//! ABRA tidak membawa ikon apa pun; ia hanya mencocokkan app Windows dengan berkas bernama mirip di folder itu
//! (Calculator, Finder, Terminal, ...). Ikon yang dipilih manual lewat "Ubah ikon…" selalu menang atas paket.
//! App yang tidak punya padanan jelas (mis. browser pihak ketiga) tidak pernah diganti.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Batas aman saat memindai paket: kedalaman folder, jumlah berkas, dan ukuran satu gambar.
const MAX_DEPTH: usize = 3;
const MAX_FILES: usize = 2000;
const MAX_BYTES: u64 = 12 * 1024 * 1024;

/// Nama biner (tanpa `.exe`, huruf kecil) → nama berkas ikon di paket.
const EXE_ALIASES: &[(&str, &str)] = &[
    ("explorer", "Finder"),
    ("calc", "Calculator"),
    ("calculatorapp", "Calculator"),
    ("notepad", "TextEdit"),
    ("wordpad", "TextEdit"),
    ("taskmgr", "Activity Monitor"),
    ("perfmon", "Activity Monitor"),
    ("resmon", "Activity Monitor"),
    ("cmd", "Terminal"),
    ("powershell", "Terminal"),
    ("pwsh", "Terminal"),
    ("windowsterminal", "Terminal"),
    ("wt", "Terminal"),
    ("systemsettings", "System Settings"),
    ("snippingtool", "Screenshot"),
    ("screenclippinghost", "Screenshot"),
    ("magnify", "Magnifier"),
    ("charmap", "Font Book"),
    ("msinfo32", "System Information"),
    ("stikynot", "Stickies"),
    ("wmplayer", "Music"),
    ("quickassist", "Screen Sharing"),
    ("soundrecorder", "Voice Memos"),
];

/// Potongan AUMID app Store/UWP (huruf kecil) → nama berkas ikon.
const UWP_ALIASES: &[(&str, &str)] = &[
    ("windowscalculator", "Calculator"),
    ("windowsalarms", "Clock"),
    ("windowslive.mail", "Mail"),
    ("windowslive.calendar", "Calendar"),
    ("windowsmaps", "Maps"),
    ("windows.photos", "Photos"),
    ("zunemusic", "Music"),
    ("zunevideo", "TV"),
    ("bingweather", "Weather"),
    ("bingnews", "News"),
    ("windowsstore", "App Store"),
    ("windowssoundrecorder", "Voice Memos"),
    ("getstarted", "Tips"),
    ("windowscamera", "Photo Booth"),
    ("stickynotes", "Stickies"),
    ("yourphone", "iPhone Mirroring"),
    ("windowsterminal", "Terminal"),
    ("windowsnotepad", "TextEdit"),
    ("immersivecontrolpanel", "System Settings"),
    ("screensketch", "Screenshot"),
    ("todos", "Reminders"),
    ("people", "Contacts"),
    ("xboxapp", "Games"),
    ("gamingapp", "Games"),
];

/// Tombol dock bawaan ABRA (kunci internal) → nama berkas ikon.
const SPECIAL: &[(&str, &str)] = &[("abra:launchpad", "Apps")];

#[derive(Default)]
struct State {
    enabled: bool,
    dir: Option<PathBuf>,
    /// nama berkas tanpa ekstensi (huruf kecil) → path
    index: HashMap<String, PathBuf>,
    /// path app (huruf kecil) → hasil pencocokan, agar pencocokan tidak diulang
    cache: HashMap<String, Option<String>>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut g = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(State::default))
}

fn scan(dir: &Path) -> HashMap<String, PathBuf> {
    let mut out: HashMap<String, PathBuf> = HashMap::new();
    let mut level = vec![dir.to_path_buf()];
    let mut seen = 0usize;
    for _ in 0..=MAX_DEPTH {
        let mut next = Vec::new();
        for d in level {
            let Ok(rd) = std::fs::read_dir(&d) else { continue };
            for e in rd.flatten() {
                seen += 1;
                if seen > MAX_FILES {
                    return out;
                }
                // tautan simbolik dilewati agar paket tidak bisa menunjuk ke luar foldernya
                let Ok(md) = std::fs::symlink_metadata(e.path()) else { continue };
                if md.file_type().is_symlink() {
                    continue;
                }
                let p = e.path();
                if md.is_dir() {
                    next.push(p);
                } else if md.is_file() && md.len() <= MAX_BYTES && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("png")) {
                    if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                        // folder yang lebih dangkal lebih dulu, jadi yang pertama dipertahankan
                        out.entry(stem.to_lowercase()).or_insert(p);
                    }
                }
            }
        }
        level = next;
    }
    out
}

/// Atur folder paket dan apakah dipakai. Indeks dibangun ulang hanya bila folder berubah.
pub fn configure(dir: &Path, enabled: bool) {
    with(|s| {
        if s.dir.as_deref() != Some(dir) {
            s.index = if dir.is_dir() { scan(dir) } else { HashMap::new() };
            s.dir = Some(dir.to_path_buf());
            s.cache.clear();
        }
        if s.enabled != enabled {
            s.enabled = enabled;
            s.cache.clear();
        }
    });
}

/// Banyak ikon yang terbaca di paket.
pub fn count() -> usize {
    with(|s| s.index.len())
}

fn exe_stem(path: &str) -> String {
    let file = path.rsplit(['\\', '/']).next().unwrap_or(path).to_lowercase();
    file.strip_suffix(".exe").unwrap_or(&file).to_string()
}

fn resolve(s: &State, app: &str) -> Option<String> {
    let path = app.split('?').next().unwrap_or(app).to_lowercase();
    let want = if let Some((_, n)) = SPECIAL.iter().find(|(k, _)| *k == path) {
        *n
    } else if let Some(rest) = path.strip_prefix("shell:appsfolder\\") {
        UWP_ALIASES.iter().find(|(tok, _)| rest.contains(tok))?.1
    } else if path.starts_with("shell:") {
        return None;
    } else {
        let stem = exe_stem(&path);
        EXE_ALIASES.iter().find(|(e, _)| *e == stem)?.1
    };
    s.index.get(&want.to_lowercase()).map(|p| p.to_string_lossy().to_string())
}

/// Berkas ikon paket untuk app `app` (path exe, `shell:AppsFolder\AUMID`, atau kunci internal), bila ada padanannya.
pub fn lookup(app: &str) -> Option<String> {
    with(|s| {
        if !s.enabled || s.index.is_empty() {
            return None;
        }
        let k = app.to_lowercase();
        if let Some(hit) = s.cache.get(&k) {
            return hit.clone();
        }
        let r = resolve(s, app);
        if s.cache.len() > 512 {
            s.cache.clear();
        }
        s.cache.insert(k, r.clone());
        r
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_with(names: &[&str]) -> State {
        let mut s = State { enabled: true, ..Default::default() };
        for n in names {
            s.index.insert(n.to_lowercase(), PathBuf::from(format!("C:\\pack\\{n}.png")));
        }
        s
    }

    #[test]
    fn exe_and_uwp_map_to_matching_icons() {
        let s = state_with(&["Finder", "Calculator", "Terminal", "Mail", "Apps"]);
        assert!(resolve(&s, "C:\\Windows\\explorer.exe").unwrap().ends_with("Finder.png"));
        assert!(resolve(&s, "C:\\Windows\\System32\\calc.exe").unwrap().ends_with("Calculator.png"));
        assert!(resolve(&s, "shell:AppsFolder\\Microsoft.WindowsCalculator_8wekyb3d8bbwe!App").unwrap().ends_with("Calculator.png"));
        assert!(resolve(&s, "shell:AppsFolder\\microsoft.windowscommunicationsapps_8wekyb3d8bbwe!microsoft.windowslive.mail").unwrap().ends_with("Mail.png"));
        assert!(resolve(&s, "abra:launchpad").unwrap().ends_with("Apps.png"));
    }

    #[test]
    fn third_party_apps_are_never_replaced() {
        let s = state_with(&["Finder", "Safari", "Notes"]);
        assert!(resolve(&s, "C:\\Program Files\\BraveSoftware\\Brave-Browser\\Application\\brave.exe").is_none());
        assert!(resolve(&s, "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe").is_none());
        assert!(resolve(&s, "C:\\Apps\\Notes.exe").is_none());
    }

    #[test]
    fn missing_icon_in_pack_gives_none() {
        let s = state_with(&["Finder"]);
        assert!(resolve(&s, "C:\\Windows\\System32\\calc.exe").is_none());
    }
}
