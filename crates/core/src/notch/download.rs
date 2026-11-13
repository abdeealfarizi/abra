//! Aktivitas unduhan di notch (murni, dites): model unduhan, angka di bawah busur, busur pengukur, dan pembaca
//! manifes Steam. Busur mulai ±jam 8 dan berputar searah jarum jam sampai ±jam 4 (celah di bawah untuk angka).

/// Asal unduhan; menentukan apakah persentase diketahui.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Store,
    Steam,
    /// Peramban (.crdownload / .part): ukuran total tidak diketahui, busur berputar dan angka = ukuran terunduh.
    Browser,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Download {
    pub source: Source,
    /// Path exe pengunduh (untuk ikon), kosong bila tidak diketahui.
    pub app: String,
    /// Nama berkas / app / game.
    pub name: String,
    /// 0..=100 bila diketahui.
    pub percent: Option<f32>,
    /// Byte yang sudah diunduh.
    pub bytes: u64,
}

/// Sudut awal busur (derajat, 0 = jam 12, searah jarum jam) dan panjangnya.
pub const ARC_START_DEG: f32 = 232.0;
pub const ARC_SWEEP_DEG: f32 = 256.0;
/// Lama unduhan selesai tetap tampil (busur penuh + centang).
pub const DONE_LINGER_MS: u32 = 3500;

/// Angka di bawah busur: persen tanpa tanda "%" (dibulatkan ke bawah agar 100 hanya saat benar-benar selesai), atau
/// ukuran terunduh singkat untuk peramban.
pub fn label(d: &Download) -> String {
    match d.percent {
        Some(p) => format!("{}", p.clamp(0.0, 100.0).floor() as u32),
        None => size_short(d.bytes),
    }
}

/// Ukuran singkat untuk ruang sempit: "640K", "12M", "1.4G".
pub fn size_short(b: u64) -> String {
    const K: f64 = 1000.0;
    let f = b as f64;
    if f < K * K {
        format!("{}K", (f / K).round() as u64)
    } else if f < K * K * K {
        format!("{}M", (f / (K * K)).round() as u64)
    } else {
        let g = f / (K * K * K);
        if g < 10.0 {
            format!("{:.1}G", g)
        } else {
            format!("{}G", g.round() as u64)
        }
    }
}

/// Bitmap BGRA ter-premultiply `n`×`n` berisi busur pengukur dari `from` ke `to` (0..1 sepanjang busur), berpusat di
/// tengah, jari-jari `r` dan tebal `w` (px), ujung bulat. Dipakai untuk jalur (0..1), isi (0..persen), dan putaran.
pub fn arc_bitmap(n: u32, r: f32, w: f32, from: f32, to: f32, rgb: [u8; 3], alpha: u8) -> Vec<u8> {
    let mut out = vec![0u8; (n * n * 4) as usize];
    let (from, to) = (from.clamp(0.0, 1.0), to.clamp(0.0, 1.0));
    if to <= from {
        return out;
    }
    let c = n as f32 / 2.0;
    let a0 = ARC_START_DEG + from * ARC_SWEEP_DEG;
    let a1 = ARC_START_DEG + to * ARC_SWEEP_DEG;
    let at = |deg: f32| {
        let t = deg.to_radians();
        (c + r * t.sin(), c - r * t.cos())
    };
    let (e0, e1) = (at(a0), at(a1));
    let span = a1 - a0;
    for y in 0..n {
        for x in 0..n {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let (dx, dy) = (px - c, py - c);
            let ang = dx.atan2(-dy).to_degrees().rem_euclid(360.0);
            let rel = (ang - a0).rem_euclid(360.0);
            let d = if rel <= span {
                ((dx * dx + dy * dy).sqrt() - r).abs() - w / 2.0
            } else {
                let d0 = ((px - e0.0).powi(2) + (py - e0.1).powi(2)).sqrt();
                let d1 = ((px - e1.0).powi(2) + (py - e1.1).powi(2)).sqrt();
                d0.min(d1) - w / 2.0
            };
            let cov = (0.5 - d).clamp(0.0, 1.0);
            if cov <= 0.0 {
                continue;
            }
            let a = (cov * alpha as f32).round() as u32;
            let i = ((y * n + x) * 4) as usize;
            out[i] = (rgb[2] as u32 * a / 255) as u8;
            out[i + 1] = (rgb[1] as u32 * a / 255) as u8;
            out[i + 2] = (rgb[0] as u32 * a / 255) as u8;
            out[i + 3] = a as u8;
        }
    }
    out
}

/// Nilai kunci pertama `key` dalam teks VDF Steam (`"kunci"  "nilai"`).
pub fn vdf_value<'a>(text: &'a str, key: &'a str) -> Option<&'a str> {
    vdf_values(text, key).next()
}

/// Semua nilai untuk `key` (mis. setiap "path" di libraryfolders.vdf).
pub fn vdf_values<'a>(text: &'a str, key: &'a str) -> impl Iterator<Item = &'a str> + 'a {
    text.lines().filter_map(move |line| {
        let mut parts = line.split('"').skip(1).step_by(2);
        let k = parts.next()?;
        if !k.eq_ignore_ascii_case(key) {
            return None;
        }
        parts.next()
    })
}

/// Folder perpustakaan Steam dari isi libraryfolders.vdf ("\\" di VDF = satu backslash).
pub fn steam_libraries(vdf: &str) -> Vec<String> {
    vdf_values(vdf, "path").map(|p| p.replace("\\\\", "\\")).collect()
}

/// Bendera EAppState Steam yang berarti unduhan/pembaruan sedang berjalan, dan "dijeda".
const STEAM_RUNNING: u64 = 256 | 1024 | 1_048_576;
const STEAM_PAUSED: u64 = 512;

/// Unduhan aktif dari isi appmanifest_*.acf: (nama game, terunduh, total). `None` bila tidak sedang mengunduh.
pub fn steam_progress(acf: &str) -> Option<(String, u64, u64)> {
    let flags: u64 = vdf_value(acf, "StateFlags")?.parse().ok()?;
    if flags & STEAM_RUNNING == 0 || flags & STEAM_PAUSED != 0 {
        return None;
    }
    let total: u64 = vdf_value(acf, "BytesToDownload")?.parse().ok()?;
    let done: u64 = vdf_value(acf, "BytesDownloaded").and_then(|v| v.parse().ok()).unwrap_or(0);
    if total == 0 || done >= total {
        return None;
    }
    Some((vdf_value(acf, "name").unwrap_or("Steam").to_string(), done, total))
}

/// Pilih unduhan yang ditampilkan: yang persennya diketahui dan paling jauh dari selesai kalah dari yang paling baru
/// bergerak? Sederhana dan stabil: utamakan sumber berpersen (Store/Steam), lalu byte terbanyak.
pub fn primary(list: &[Download]) -> Option<&Download> {
    list.iter().max_by(|a, b| a.percent.is_some().cmp(&b.percent.is_some()).then(a.bytes.cmp(&b.bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dl(percent: Option<f32>, bytes: u64) -> Download {
        Download { source: if percent.is_some() { Source::Store } else { Source::Browser }, app: String::new(), name: "x".into(), percent, bytes }
    }

    #[test]
    fn labels() {
        assert_eq!(label(&dl(Some(38.7), 0)), "38");
        assert_eq!(label(&dl(Some(99.99), 0)), "99");
        assert_eq!(label(&dl(Some(100.0), 0)), "100");
        assert_eq!(label(&dl(Some(-3.0), 0)), "0");
        assert_eq!(label(&dl(None, 640_000)), "640K");
        assert_eq!(label(&dl(None, 12_400_000)), "12M");
        assert_eq!(label(&dl(None, 1_440_000_000)), "1.4G");
        assert_eq!(label(&dl(None, 23_000_000_000)), "23G");
    }

    #[test]
    fn arc_covers_start_not_gap() {
        let n = 40;
        let r = 15.0;
        let bmp = arc_bitmap(n, r, 3.0, 0.0, 1.0, [255, 255, 255], 255);
        let alpha = |deg: f32| {
            let t = (deg as f32).to_radians();
            let (x, y) = (20.0 + r * t.sin(), 20.0 - r * t.cos());
            bmp[((y as u32 * n + x as u32) * 4 + 3) as usize]
        };
        assert!(alpha(0.0) > 200, "jam 12 terisi");
        assert!(alpha(270.0) > 200, "jam 9 terisi");
        assert!(alpha(90.0) > 200, "jam 3 terisi");
        assert_eq!(alpha(180.0), 0, "jam 6 kosong (celah angka)");
        // isi 25%: dari jam ±8 sampai sedikit lewat jam 10
        let part = arc_bitmap(n, r, 3.0, 0.0, 0.25, [52, 199, 89], 255);
        let pa = |deg: f32| {
            let t = deg.to_radians();
            let (x, y) = (20.0 + r * t.sin(), 20.0 - r * t.cos());
            part[((y as u32 * n + x as u32) * 4 + 3) as usize]
        };
        assert!(pa(270.0) > 200);
        assert_eq!(pa(0.0), 0);
        assert!(arc_bitmap(n, r, 3.0, 0.5, 0.5, [0, 0, 0], 255).iter().all(|b| *b == 0));
    }

    #[test]
    fn steam_manifest() {
        let acf = "\"AppState\"\n{\n\t\"appid\"\t\t\"570\"\n\t\"name\"\t\t\"Game Uji\"\n\t\"StateFlags\"\t\t\"1026\"\n\t\"BytesToDownload\"\t\t\"2000\"\n\t\"BytesDownloaded\"\t\t\"500\"\n}\n";
        assert_eq!(steam_progress(acf), Some(("Game Uji".to_string(), 500, 2000)));
        assert_eq!(steam_progress(&acf.replace("1026", "4")), None, "terpasang penuh");
        assert_eq!(steam_progress(&acf.replace("1026", "1538")), None, "dijeda");
        assert_eq!(steam_progress(&acf.replace("\"500\"", "\"2000\"")), None, "selesai");
        let lib = "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"C:\\\\Program Files (x86)\\\\Steam\"\n\t}\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"D:\\\\SteamLibrary\"\n\t}\n}\n";
        assert_eq!(steam_libraries(lib), vec!["C:\\Program Files (x86)\\Steam".to_string(), "D:\\SteamLibrary".to_string()]);
    }

    #[test]
    fn primary_prefers_known_percent() {
        let list = vec![dl(None, 9_000_000), dl(Some(40.0), 100), dl(Some(10.0), 50)];
        assert_eq!(primary(&list).unwrap().percent, Some(40.0));
        assert!(primary(&[]).is_none());
    }
}
