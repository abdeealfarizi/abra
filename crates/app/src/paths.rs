//! Lokasi berkas. `NACRE_CONFIG_DIR` menimpa folder config (dipakai tes agar tidak menyentuh data asli).
use std::path::PathBuf;

pub fn config_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("NACRE_CONFIG_DIR") {
        return PathBuf::from(d);
    }
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let dir = base.join("ABRA");
    migrate_legacy(&base.join("Nacre"), &dir);
    dir
}

/// Pertama kali dijalankan sebagai ABRA: salin data lama (pins, pengaturan) dari folder "Nacre" bila ada.
fn migrate_legacy(old: &std::path::Path, new: &std::path::Path) {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        if new.exists() || !old.exists() {
            return;
        }
        let _ = std::fs::create_dir_all(new);
        for f in ["pins.txt", "settings.cfg"] {
            let _ = std::fs::copy(old.join(f), new.join(f));
        }
    });
}

pub fn marker() -> PathBuf {
    config_dir().join("taskbar_hidden.flag")
}

pub fn log() -> PathBuf {
    config_dir().join("abra.log")
}

/// `abra-guard.exe` ada di folder yang sama dengan exe utama.
pub fn guard_exe() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("abra-guard.exe")))
        .unwrap_or_else(|| PathBuf::from("abra-guard.exe"))
}
