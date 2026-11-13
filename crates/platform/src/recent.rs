//! Berkas yang baru dibuka (folder "Recent" Windows) untuk seksi "Terbaru" di Spotlight → Berkas.
//! Hanya membaca pintasan yang dibuat Windows sendiri; berkas tidak dibuka.

use std::path::PathBuf;
use std::time::SystemTime;

/// `n` berkas terbaru: (nama, path), terbaru lebih dulu. Hanya berkas yang masih ada (bukan folder).
/// Thread pemanggil harus sudah menginisialisasi COM (pembacaan `.lnk`).
pub fn list(n: usize) -> Vec<(String, String)> {
    let Some(appdata) = std::env::var_os("APPDATA") else { return Vec::new() };
    let dir = PathBuf::from(appdata).join("Microsoft").join("Windows").join("Recent");
    let Ok(rd) = std::fs::read_dir(&dir) else { return Vec::new() };
    let mut links: Vec<(SystemTime, PathBuf)> = rd
        .flatten()
        .take(2000)
        .filter(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("lnk")))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    links.sort_by(|a, b| b.0.cmp(&a.0));
    let mut out: Vec<(String, String)> = Vec::new();
    for (_, l) in links {
        if out.len() >= n {
            break;
        }
        let Some(target) = crate::pick::resolve_shortcut(&l.to_string_lossy()) else { continue };
        let p = PathBuf::from(&target);
        if !p.is_file() || out.iter().any(|(_, t)| t.eq_ignore_ascii_case(&target)) {
            continue;
        }
        let name = p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        out.push((name, target));
    }
    out
}
