//! Tata letak dock satu baris dengan magnifikasi ala macOS.
//!
//! Fungsi murni: diberi daftar slot dan posisi kursor (atau `None`), mengembalikan posisi
//! kiri + skala tiap slot. Pusat tiap slot untuk perhitungan jarak diambil dari posisi
//! *tanpa* magnifikasi, sehingga hasilnya stabil (tidak ada umpan balik antar frame).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Item,
    /// Garis pemisah: lebar tetap, tidak ikut membesar.
    Separator,
}

#[derive(Clone, Copy, Debug)]
pub struct Metrics {
    pub icon: f32,
    pub gap: f32,
    /// Skala maksimum di bawah kursor (1.0 = tanpa magnifikasi).
    pub max_scale: f32,
    /// Jarak (px) dari kursor sampai skala kembali ke 1.
    pub reach: f32,
    pub separator: f32,
}

impl Metrics {
    /// Metrik dari pengaturan: ukuran ikon (dp), jarak antar ikon (dp), zoom maksimum (1.0 = tanpa zoom).
    pub fn custom(icon: f32, gap: f32, zoom: f32, scale: f32) -> Self {
        Metrics { icon: icon * scale, gap: gap * scale, max_scale: zoom.max(1.0), reach: 2.6 * icon * scale, separator: 10.0 * scale }
    }

    pub fn new(icon: f32, scale: f32) -> Self {
        Metrics {
            icon: icon * scale,
            gap: 6.0 * scale,
            max_scale: 1.7,
            reach: 2.6 * icon * scale,
            separator: 10.0 * scale,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    /// Tepi kiri kotak yang sudah diskalakan.
    pub x: f32,
    /// Lebar kotak yang sudah diskalakan.
    pub w: f32,
    pub scale: f32,
}

#[derive(Clone, Debug)]
pub struct Layout {
    pub cells: Vec<Placed>,
    /// Lebar isi (tanpa padding kartu).
    pub total: f32,
}

fn base_width(slots: &[Slot], m: &Metrics) -> f32 {
    let mut w = 0.0;
    for (i, s) in slots.iter().enumerate() {
        if i > 0 {
            w += m.gap;
        }
        w += match s {
            Slot::Item => m.icon,
            Slot::Separator => m.separator,
        };
    }
    w
}

/// Bentuk kosinus: 1 di bawah kursor, 0 di jarak `reach`, mulus di kedua ujung.
fn falloff(distance: f32, reach: f32) -> f32 {
    let t = (distance / reach).clamp(0.0, 1.0);
    0.5 * (1.0 + (std::f32::consts::PI * t).cos())
}

pub fn layout(slots: &[Slot], cursor_x: Option<f32>, area_w: f32, m: &Metrics) -> Layout {
    let base_total = base_width(slots, m);
    let base_left = (area_w - base_total) / 2.0;

    // skala tiap slot dari jarak ke kursor, memakai pusat tanpa magnifikasi
    let mut scales = Vec::with_capacity(slots.len());
    let mut x = base_left;
    for s in slots {
        let w = match s {
            Slot::Item => m.icon,
            Slot::Separator => m.separator,
        };
        let scale = match (s, cursor_x) {
            (Slot::Item, Some(c)) => 1.0 + (m.max_scale - 1.0) * falloff((c - (x + w / 2.0)).abs(), m.reach),
            _ => 1.0,
        };
        scales.push(scale);
        x += w + m.gap;
    }

    let widths: Vec<f32> = slots
        .iter()
        .zip(&scales)
        .map(|(s, sc)| match s {
            Slot::Item => m.icon * sc,
            Slot::Separator => m.separator,
        })
        .collect();
    let total = widths.iter().sum::<f32>() + m.gap * (slots.len().saturating_sub(1)) as f32;
    let mut x = (area_w - total) / 2.0;
    let mut cells = Vec::with_capacity(slots.len());
    for (w, sc) in widths.iter().zip(&scales) {
        cells.push(Placed { x, w: *w, scale: *sc });
        x += w + m.gap;
    }
    Layout { cells, total }
}

/// Indeks slot `Item` di bawah `x` (celah antar ikon dibagi dua ke tetangga terdekat).
pub fn hit(slots: &[Slot], l: &Layout, x: f32, m: &Metrics) -> Option<usize> {
    let half = m.gap / 2.0;
    for (i, c) in l.cells.iter().enumerate() {
        if slots[i] == Slot::Item && x >= c.x - half && x <= c.x + c.w + half {
            return Some(i);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m() -> Metrics {
        Metrics::new(48.0, 1.0)
    }
    fn items(n: usize) -> Vec<Slot> {
        vec![Slot::Item; n]
    }

    #[test]
    fn without_cursor_everything_is_unscaled_and_centered() {
        let s = items(5);
        let l = layout(&s, None, 800.0, &m());
        assert!(l.cells.iter().all(|c| c.scale == 1.0));
        let expected = 5.0 * 48.0 + 4.0 * 6.0;
        assert!((l.total - expected).abs() < 1e-3);
        assert!((l.cells[0].x - (800.0 - expected) / 2.0).abs() < 1e-3);
    }

    #[test]
    fn cursor_on_middle_item_peaks_there_and_is_symmetric() {
        let s = items(7);
        let base = layout(&s, None, 900.0, &m());
        let mid = base.cells[3].x + base.cells[3].w / 2.0;
        let l = layout(&s, Some(mid), 900.0, &m());
        assert!((l.cells[3].scale - m().max_scale).abs() < 1e-3);
        assert!((l.cells[2].scale - l.cells[4].scale).abs() < 1e-3);
        assert!(l.cells[3].scale > l.cells[2].scale && l.cells[2].scale > l.cells[1].scale);
        assert!(l.total > base.total, "dock melebar saat magnifikasi");
    }

    #[test]
    fn items_beyond_reach_stay_at_scale_one() {
        let s = items(12);
        let base = layout(&s, None, 1200.0, &m());
        let c = base.cells[0].x + 24.0;
        let l = layout(&s, Some(c), 1200.0, &m());
        assert!((l.cells[11].scale - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cells_never_overlap_and_stay_ordered() {
        let s = items(9);
        let base = layout(&s, None, 1000.0, &m());
        for step in 0..40 {
            let c = base.cells[0].x + step as f32 * 12.0;
            let l = layout(&s, Some(c), 1000.0, &m());
            for w in l.cells.windows(2) {
                assert!(w[1].x >= w[0].x + w[0].w - 1e-3, "tumpang tindih di c={c}");
            }
        }
    }

    #[test]
    fn separator_keeps_fixed_width_and_scale() {
        let s = [Slot::Item, Slot::Separator, Slot::Item];
        let base = layout(&s, None, 400.0, &m());
        let c = base.cells[1].x;
        let l = layout(&s, Some(c), 400.0, &m());
        assert_eq!(l.cells[1].scale, 1.0);
        assert_eq!(l.cells[1].w, m().separator);
    }

    #[test]
    fn hit_test_finds_items_and_ignores_separators() {
        let s = [Slot::Item, Slot::Separator, Slot::Item];
        let l = layout(&s, None, 400.0, &m());
        assert_eq!(hit(&s, &l, l.cells[0].x + 10.0, &m()), Some(0));
        assert_eq!(hit(&s, &l, l.cells[1].x + 2.0, &m()), None);
        assert_eq!(hit(&s, &l, l.cells[2].x + 10.0, &m()), Some(2));
        assert_eq!(hit(&s, &l, -50.0, &m()), None);
    }

    #[test]
    fn empty_dock_is_safe() {
        let l = layout(&[], Some(10.0), 300.0, &m());
        assert!(l.cells.is_empty());
        assert_eq!(l.total, 0.0);
    }
}
