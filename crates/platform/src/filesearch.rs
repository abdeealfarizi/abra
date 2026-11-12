//! Pencarian berkas sederhana untuk mode Berkas di Spotlight: menelusuri folder pribadi pengguna (Desktop, Dokumen, Unduhan,
//! Gambar, Video, Musik, dan OneDrive) dengan batas kedalaman, jumlah entri, dan waktu. Bukan indeks seluruh disk.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}

/// Folder akar pencarian.
pub fn roots() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(home) = std::env::var_os("USERPROFILE").map(PathBuf::from) {
        for d in ["Desktop", "Documents", "Downloads", "Pictures", "Videos", "Music"] {
            let p = home.join(d);
            if p.is_dir() {
                v.push(p);
            }
        }
    }
    if let Some(od) = std::env::var_os("OneDrive").map(PathBuf::from) {
        if od.is_dir() && !v.iter().any(|p| p.starts_with(&od)) {
            v.push(od);
        }
    }
    v
}

fn skip_dir(name: &str) -> bool {
    name.starts_with('.') || matches!(name, "node_modules" | "target" | "AppData" | "$RECYCLE.BIN" | "System Volume Information")
}

/// Semua kata `query` (huruf kecil) harus ada pada `name` (huruf kecil).
fn matches(words: &[String], name: &str) -> bool {
    let n = name.to_lowercase();
    words.iter().all(|w| n.contains(w.as_str()))
}

/// Cari di `roots`; `limit` hasil terbaik (nama diawali query lebih dulu, lalu nama lebih pendek).
pub fn search_in(roots: &[PathBuf], query: &str, budget: Duration, limit: usize, max_depth: usize) -> Vec<Hit> {
    let words: Vec<String> = query.split_whitespace().map(|w| w.to_lowercase()).collect();
    if words.is_empty() {
        return vec![];
    }
    let start = Instant::now();
    let mut found: Vec<Hit> = Vec::new();
    let mut visited = 0usize;
    let mut queue: std::collections::VecDeque<(PathBuf, usize)> = roots.iter().map(|r| (r.clone(), 0)).collect();
    while let Some((dir, depth)) = queue.pop_front() {
        if start.elapsed() > budget || visited > 60_000 || found.len() >= limit * 6 {
            break;
        }
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            visited += 1;
            let name = e.file_name().to_string_lossy().to_string();
            let Ok(ft) = e.file_type() else { continue };
            let is_dir = ft.is_dir();
            if matches(&words, &name) {
                found.push(Hit { name: name.clone(), path: e.path().to_string_lossy().to_string(), is_dir });
            }
            if is_dir && depth < max_depth && !skip_dir(&name) {
                queue.push_back((e.path(), depth + 1));
            }
        }
    }
    let q = words[0].clone();
    found.sort_by_key(|h| {
        let n = h.name.to_lowercase();
        (!n.starts_with(&q), !h.is_dir, n.len())
    });
    found.truncate(limit);
    found
}

pub fn search(query: &str) -> Vec<Hit> {
    search_in(&roots(), query, Duration::from_millis(450), 20, 5)
}

/// Apakah `p` ada (untuk tes dan pembersihan).
#[allow(dead_code)]
fn exists(p: &Path) -> bool {
    p.exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_by_all_words_case_insensitive_and_prefers_prefix() {
        let base = std::env::temp_dir().join(format!("abra-fs-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("a/b")).unwrap();
        std::fs::write(base.join("Report Final.txt"), b"x").unwrap();
        std::fs::write(base.join("a/b/final report notes.md"), b"x").unwrap();
        std::fs::write(base.join("a/other.txt"), b"x").unwrap();
        std::fs::create_dir_all(base.join("node_modules/zzz")).unwrap();
        std::fs::write(base.join("node_modules/zzz/final report.js"), b"x").unwrap();
        let hits = search_in(&[base.clone()], "report FINAL", Duration::from_secs(2), 10, 5);
        let names: Vec<&str> = hits.iter().map(|h| h.name.as_str()).collect();
        assert!(names.contains(&"Report Final.txt"));
        assert!(names.contains(&"final report notes.md"));
        assert!(!names.contains(&"other.txt"));
        assert!(!names.contains(&"final report.js"), "node_modules dilewati");
        assert!(search_in(&[base.clone()], "", Duration::from_secs(1), 10, 5).is_empty());
        let _ = std::fs::remove_dir_all(&base);
    }
}
