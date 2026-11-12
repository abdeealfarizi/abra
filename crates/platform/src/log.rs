//! Log berkas minimal (tanpa dependensi). Satu baris per kejadian, dibatasi ukurannya.

use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;

static PATH: OnceLock<PathBuf> = OnceLock::new();
const MAX_BYTES: u64 = 256 * 1024;

pub fn init(path: PathBuf) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // Potong bila terlalu besar: cukup mulai ulang, ini log diagnostik, bukan arsip.
    if std::fs::metadata(&path).map(|m| m.len() > MAX_BYTES).unwrap_or(false) {
        let _ = std::fs::remove_file(&path);
    }
    let _ = PATH.set(path);
}

pub fn line(what: &str, detail: &str) {
    let Some(path) = PATH.get() else { return };
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{secs} pid={} {what}: {detail}", std::process::id());
    }
}
