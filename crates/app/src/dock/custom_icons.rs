//! Ikon kustom per app: `icons.txt` (`jalur-app<TAB>jalur-gambar`). Gambar yang dipilih pengguna disalin ke
//! `%APPDATA%\ABRA\icons\` agar tetap ada walau berkas aslinya dipindah/dihapus.

use crate::paths;
use nacre_core::dock::items::key_of;
use nacre_platform::{icons, log};
use std::path::PathBuf;

fn list_path() -> PathBuf {
    paths::config_dir().join("icons.txt")
}

fn dir() -> PathBuf {
    paths::config_dir().join("icons")
}

fn read() -> Vec<(String, String)> {
    std::fs::read_to_string(list_path())
        .map(|t| {
            t.trim_start_matches('\u{feff}')
                .lines()
                .filter_map(|l| l.split_once('\t').map(|(a, b)| (a.trim().to_string(), b.trim().to_string())))
                .filter(|(a, b)| !a.is_empty() && !b.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn write(rows: &[(String, String)]) {
    let _ = std::fs::create_dir_all(paths::config_dir());
    let text: String = rows.iter().map(|(a, b)| format!("{a}\t{b}\n")).collect();
    let tmp = list_path().with_extension("tmp");
    if std::fs::write(&tmp, text).is_ok() {
        let _ = std::fs::rename(&tmp, list_path());
    }
}

/// Daftarkan semua ikon kustom tersimpan ke pemuat ikon (dipanggil saat start).
pub fn install() {
    for (app, img) in read() {
        if std::path::Path::new(&img).exists() {
            icons::set_override(&app, Some(img));
        }
    }
}

fn fnv(s: &str) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Pakai `image` sebagai ikon `app`. Mengembalikan false bila gambar tak bisa disalin/dibaca.
pub fn set(app: &str, image: &str) -> bool {
    let ext = std::path::Path::new(image).extension().and_then(|e| e.to_str()).unwrap_or("png").to_ascii_lowercase();
    let dest = dir().join(format!("{:016x}.{ext}", fnv(&key_of(app))));
    let _ = std::fs::create_dir_all(dir());
    if let Err(e) = std::fs::copy(image, &dest) {
        log::line("icons", &format!("salin ikon gagal: {e}"));
        return false;
    }
    let dest_s = dest.to_string_lossy().to_string();
    // Pastikan gambar bisa didekode sebelum dipakai.
    if nacre_platform::wic::decode_file_fit(&dest_s, 16).is_none() {
        let _ = std::fs::remove_file(&dest);
        return false;
    }
    let mut rows = read();
    rows.retain(|(a, _)| key_of(a) != key_of(app));
    rows.push((app.to_string(), dest_s.clone()));
    write(&rows);
    icons::set_override(app, Some(dest_s));
    true
}

pub fn reset(app: &str) {
    let mut rows = read();
    if let Some((_, img)) = rows.iter().find(|(a, _)| key_of(a) == key_of(app)) {
        let _ = std::fs::remove_file(img);
    }
    rows.retain(|(a, _)| key_of(a) != key_of(app));
    write(&rows);
    icons::set_override(app, None);
}

pub fn has(app: &str) -> bool {
    icons::override_for(app).is_some()
}
