//! Berkas terbaru di folder Unduhan pengguna untuk "stack" dock. Hanya membaca nama dan waktu ubah; berkas tidak dibuka.

use std::path::PathBuf;
use std::time::SystemTime;

/// Folder Unduhan pengguna (profil pengguna + "Downloads"), bila ada.
pub fn folder() -> Option<PathBuf> {
    let p = PathBuf::from(std::env::var_os("USERPROFILE")?).join("Downloads");
    p.is_dir().then_some(p)
}

/// Berkas yang masih diunduh atau berkas sistem tidak ditampilkan.
fn skip(name: &str) -> bool {
    let l = name.to_lowercase();
    name.starts_with('.') || l == "desktop.ini" || [".crdownload", ".part", ".tmp", ".download", ".partial"].iter().any(|e| l.ends_with(e))
}

/// `n` berkas/folder terbaru: (nama, path). Pemindaian dibatasi 3000 entri agar folder besar tetap cepat.
pub fn latest(n: usize) -> Vec<(String, String)> {
    let Some(dir) = folder() else { return Vec::new() };
    let Ok(rd) = std::fs::read_dir(&dir) else { return Vec::new() };
    let mut v: Vec<(SystemTime, String, String)> = Vec::new();
    for e in rd.flatten().take(3000) {
        let name = e.file_name().to_string_lossy().to_string();
        if skip(&name) {
            continue;
        }
        let Ok(md) = e.metadata() else { continue };
        let t = md.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        v.push((t, name, e.path().to_string_lossy().to_string()));
    }
    v.sort_by(|a, b| b.0.cmp(&a.0));
    v.into_iter().take(n).map(|(_, n, p)| (n, p)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_downloads_are_hidden() {
        assert!(skip("a.crdownload") && skip("desktop.ini") && skip(".hidden"));
        assert!(!skip("laporan.pdf"));
    }
}
