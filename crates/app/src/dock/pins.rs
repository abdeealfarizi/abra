//! Daftar pin: satu path per baris di `pins.txt`. Ditulis atomik; berkas hilang = pin bawaan.

use nacre_core::dock::items::{name_from_path, Pin};
use std::io::Write;
use std::path::Path;

/// Nama ramah dari metadata exe, jatuh ke nama berkas bila tidak ada.
pub fn name_for(path: &str) -> String {
    let n = nacre_platform::wins::names::friendly(path);
    if n.is_empty() {
        name_from_path(path)
    } else {
        n
    }
}

fn default_pins() -> Vec<Pin> {
    let win = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
    let explorer = format!("{win}\\explorer.exe");
    vec![Pin { name: name_for(&explorer), path: explorer }]
}

pub fn load(path: &Path) -> Vec<Pin> {
    match std::fs::read_to_string(path) {
        // Notepad (dan banyak editor) menyimpan UTF-8 dengan BOM; baris pertama tidak boleh rusak karenanya.
        Ok(text) => text
            .trim_start_matches('\u{feff}')
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(|l| match l.split_once('\t') {
                Some((p, n)) if !n.trim().is_empty() => Pin { path: p.to_string(), name: n.trim().to_string() },
                _ => Pin { path: l.to_string(), name: name_for(l) },
            })
            .collect(),
        Err(_) => default_pins(),
    }
}

pub fn save(path: &Path, pins: &[Pin]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    let mut f = std::fs::File::create(&tmp)?;
    for p in pins {
        writeln!(f, "{}\t{}", p.path, p.name)?;
    }
    f.sync_all()?;
    drop(f);
    std::fs::rename(&tmp, path)
}



