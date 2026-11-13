//! Pembantu tata letak teks lirik: pemenggalan baris berdasarkan lebar terukur, dan blur kotak untuk baris lirik yang
//! sedang tidak dinyanyikan (redup dan sedikit buram, sehingga baris aktif menonjol).

/// Penggal `text` menjadi paling banyak `max_lines` baris yang lebarnya ≤ `max` menurut `measure`. Kata yang terlalu
/// panjang dibiarkan utuh di barisnya sendiri; sisa kata setelah baris terakhir diberi "…".
pub fn wrap(text: &str, max: f32, max_lines: usize, measure: impl Fn(&str) -> f32) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut i = 0;
    while i < words.len() {
        let w = words[i];
        let cand = if cur.is_empty() { w.to_string() } else { format!("{cur} {w}") };
        if cur.is_empty() || measure(&cand) <= max {
            cur = cand;
            i += 1;
            continue;
        }
        lines.push(std::mem::take(&mut cur));
        if lines.len() == max_lines {
            break;
        }
    }
    if lines.len() < max_lines {
        if !cur.is_empty() {
            lines.push(cur);
        }
    } else if i < words.len() || !cur.is_empty() {
        if let Some(last) = lines.last_mut() {
            last.push('\u{2026}');
        }
    }
    lines
}

/// Blur kotak dua lintasan (horizontal lalu vertikal) dengan radius `r` px pada BGRA ter-premultiply `w`×`h`.
pub fn box_blur(bgra: &mut [u8], w: usize, h: usize, r: usize) {
    if r == 0 || w == 0 || h == 0 || bgra.len() < w * h * 4 {
        return;
    }
    let mut tmp = vec![0u8; bgra.len()];
    let pass = |src: &[u8], dst: &mut [u8], len: usize, lines: usize, step: usize, stride: usize| {
        for l in 0..lines {
            for c in 0..4 {
                let at = |k: usize| src[(l * stride + k * step) * 4 + c] as u32;
                let mut acc: u32 = 0;
                let win = (2 * r + 1) as u32;
                // jendela awal berpusat di 0 (tepi dianggap 0)
                for k in 0..=r.min(len - 1) {
                    acc += at(k);
                }
                for k in 0..len {
                    dst[(l * stride + k * step) * 4 + c] = (acc / win) as u8;
                    if k + r + 1 < len {
                        acc += at(k + r + 1);
                    }
                    if k >= r {
                        acc -= at(k - r);
                    }
                }
            }
        }
    };
    pass(bgra, &mut tmp, w, h, 1, w);
    pass(&tmp, bgra, h, w, w, 1);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(s: &str) -> f32 {
        s.chars().count() as f32
    }

    #[test]
    fn wraps_by_measured_width_and_marks_overflow() {
        assert_eq!(wrap("Treat her better, boy", 12.0, 2, m), vec!["Treat her", "better, boy"]);
        assert_eq!(wrap("short", 12.0, 2, m), vec!["short"]);
        assert_eq!(wrap("one two three four five six", 9.0, 2, m), vec!["one two", "three\u{2026}"]);
        assert_eq!(wrap("Supercalifragilistic word", 6.0, 2, m), vec!["Supercalifragilistic", "word"]);
        assert!(wrap("   ", 10.0, 2, m).is_empty());
    }

    #[test]
    fn blur_spreads_a_dot_and_keeps_energy_roughly() {
        let (w, h) = (9usize, 9usize);
        let mut b = vec![0u8; w * h * 4];
        let c = (4 * w + 4) * 4;
        b[c..c + 4].copy_from_slice(&[0, 0, 0, 255]);
        box_blur(&mut b, w, h, 1);
        let a = |x: usize, y: usize| b[(y * w + x) * 4 + 3];
        assert!(a(4, 4) > 0 && a(4, 4) < 255);
        assert!(a(3, 4) > 0 && a(5, 5) > 0, "menyebar ke tetangga");
        assert_eq!(a(0, 0), 0);
        let total: u32 = (0..w * h).map(|i| b[i * 4 + 3] as u32).sum();
        assert!((200..=260).contains(&total), "energi kira-kira tetap: {total}");
    }
}
