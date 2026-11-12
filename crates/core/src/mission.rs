//! Tata letak Mission Control: menaruh thumbnail jendela (rasio dijaga) dalam grid yang memenuhi area. Murni.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// `sizes`: ukuran asli tiap jendela (w, h). `area`: ruang yang tersedia. `gap`: jarak antar sel.
/// `label_h`: tinggi tambahan di bawah tiap thumbnail untuk judul. Mengembalikan persegi thumbnail
/// (tanpa label) satu per jendela, urutan sama dengan `sizes`.
pub fn grid(sizes: &[(f32, f32)], area: Rect, gap: f32, label_h: f32) -> Vec<Rect> {
    let n = sizes.len();
    if n == 0 {
        return vec![];
    }
    // pilih jumlah kolom yang memberi thumbnail terbesar
    let mut best: Option<(f32, usize)> = None;
    for cols in 1..=n {
        let rows = n.div_ceil(cols);
        let cw = (area.w - gap * (cols as f32 - 1.0)) / cols as f32;
        let ch = (area.h - gap * (rows as f32 - 1.0)) / rows as f32 - label_h;
        if cw <= 0.0 || ch <= 0.0 {
            continue;
        }
        // ukuran thumbnail terkecil yang dihasilkan (jendela rata-rata 16:10)
        let thumb = (cw / 1.6).min(ch);
        if best.map(|b| thumb > b.0).unwrap_or(true) {
            best = Some((thumb, cols));
        }
    }
    let cols = best.map(|b| b.1).unwrap_or(n);
    let rows = n.div_ceil(cols);
    let cw = (area.w - gap * (cols as f32 - 1.0)) / cols as f32;
    let ch = (area.h - gap * (rows as f32 - 1.0)) / rows as f32 - label_h;
    let total_h = rows as f32 * (ch + label_h) + gap * (rows as f32 - 1.0);
    let y0 = area.y + (area.h - total_h).max(0.0) / 2.0;
    let mut out = Vec::with_capacity(n);
    for (i, (sw, sh)) in sizes.iter().enumerate() {
        let (r, c) = (i / cols, i % cols);
        // baris terakhir yang tidak penuh dipusatkan
        let in_row = if r == rows - 1 { n - r * cols } else { cols };
        let row_w = in_row as f32 * cw + gap * (in_row as f32 - 1.0);
        let x0 = area.x + (area.w - row_w) / 2.0 + c as f32 * (cw + gap);
        let (sw, sh) = (sw.max(1.0), sh.max(1.0));
        let k = (cw / sw).min(ch / sh).min(1.0);
        let (w, h) = (sw * k, sh * k);
        out.push(Rect { x: x0 + (cw - w) / 2.0, y: y0 + r as f32 * (ch + label_h + gap) + (ch - h), w, h });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: Rect = Rect { x: 0.0, y: 0.0, w: 1920.0, h: 1080.0 };

    #[test]
    fn keeps_aspect_and_stays_inside() {
        let sizes = [(1600.0, 900.0), (800.0, 600.0), (1000.0, 1000.0), (400.0, 300.0), (1920.0, 1080.0)];
        let g = grid(&sizes, AREA, 24.0, 28.0);
        assert_eq!(g.len(), 5);
        for (r, (sw, sh)) in g.iter().zip(sizes.iter()) {
            assert!((r.w / r.h - sw / sh).abs() < 0.01, "rasio");
            assert!(r.x >= 0.0 && r.y >= 0.0 && r.x + r.w <= 1920.01 && r.y + r.h <= 1080.01);
        }
    }

    #[test]
    fn never_enlarges_small_windows() {
        let g = grid(&[(300.0, 200.0)], AREA, 24.0, 28.0);
        assert_eq!((g[0].w, g[0].h), (300.0, 200.0));
    }

    #[test]
    fn cells_do_not_overlap() {
        let sizes = vec![(1280.0, 720.0); 7];
        let g = grid(&sizes, AREA, 24.0, 28.0);
        for i in 0..g.len() {
            for j in i + 1..g.len() {
                let (a, b) = (g[i], g[j]);
                let overlap = a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
                assert!(!overlap, "{i} dan {j}");
            }
        }
    }

    #[test]
    fn empty_is_empty() {
        assert!(grid(&[], AREA, 24.0, 28.0).is_empty());
    }
}
