//! Keadaan dan ukuran island. Murni: dipakai kontroler untuk menentukan target animasi.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panel {
    NowPlaying,
    Calendar,
    Controls,
}

#[derive(Clone, Copy, Debug)]
pub struct State {
    pub open: Option<Panel>,
    pub hover: bool,
    /// Ada sesi media (sayap tampil di pil tertutup).
    pub media: bool,
    pub calendar_rows: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Size {
    pub w: f32,
    pub h: f32,
    /// Radius sudut bawah.
    pub radius: f32,
}

/// Tinggi baris pager (ikon pindah panel + pengaturan) di dasar panel terbuka, dalam dp.
pub const PAGER_H: f32 = 32.0;

/// Ukuran target (px perangkat) untuk `s`.
pub fn target(s: &State, dpi: f32) -> Size {
    let (w, h, r) = match s.open {
        Some(Panel::NowPlaying) => (384.0, 140.0 + PAGER_H, 26.0),
        Some(Panel::Controls) => (360.0, 214.0 + PAGER_H, 26.0),
        Some(Panel::Calendar) => {
            // judul bulan + hari pekan + baris tanggal + jarak + baris timer + padding
            let rows = s.calendar_rows.clamp(4, 6) as f32;
            (340.0, 154.0 + rows * 26.0 + PAGER_H, 26.0)
        }
        None => {
            let base = if s.media { 264.0 } else { 200.0 };
            if s.hover {
                (base + 14.0, 34.0, 17.0)
            } else {
                (base, 32.0, 16.0)
            }
        }
    };
    Size { w: w * dpi, h: h * dpi, radius: r * dpi }
}

/// Panel yang bisa dipilih: Kalender dan Kontrol selalu ada; Now Playing hanya bila ada sesi media.
pub fn panels(media: bool) -> Vec<Panel> {
    if media {
        vec![Panel::Controls, Panel::Calendar, Panel::NowPlaying]
    } else {
        vec![Panel::Controls, Panel::Calendar]
    }
}

/// Panel yang terbuka saat pil diklik: selalu Control Center. Kalender dan Now Playing lewat scroll/pager.
pub fn initial_panel(_media: bool) -> Panel {
    Panel::Controls
}

/// Panel berikutnya (`dir` = +1/−1) dalam daftar, melingkar. Bila `cur` tidak ada, panel awal.
pub fn cycle(list: &[Panel], cur: Panel, dir: i32) -> Panel {
    match list.iter().position(|p| *p == cur) {
        Some(i) => list[(i as i32 + dir).rem_euclid(list.len() as i32) as usize],
        None => *list.first().unwrap_or(&Panel::Calendar),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(open: Option<Panel>, hover: bool, media: bool) -> State {
        State { open, hover, media, calendar_rows: 5 }
    }

    #[test]
    fn collapsed_is_slim_and_wings_widen_it() {
        let a = target(&st(None, false, false), 1.0);
        let b = target(&st(None, false, true), 1.0);
        assert_eq!((a.w, a.h), (200.0, 32.0));
        assert!(b.w > a.w && b.h == a.h);
    }

    #[test]
    fn hover_grows_a_little_but_open_panels_grow_a_lot() {
        let rest = target(&st(None, false, true), 1.0);
        let hov = target(&st(None, true, true), 1.0);
        let open = target(&st(Some(Panel::NowPlaying), true, true), 1.0);
        assert!(hov.w > rest.w && hov.w - rest.w < 30.0);
        assert!(open.h > hov.h * 3.0 && open.w > hov.w);
    }

    #[test]
    fn calendar_height_follows_week_rows() {
        let five = target(&State { open: Some(Panel::Calendar), hover: true, media: false, calendar_rows: 5 }, 1.0);
        let six = target(&State { calendar_rows: 6, ..State { open: Some(Panel::Calendar), hover: true, media: false, calendar_rows: 5 } }, 1.0);
        assert_eq!(six.h - five.h, 26.0);
    }

    #[test]
    fn sizes_scale_with_dpi() {
        let a = target(&st(Some(Panel::NowPlaying), true, true), 1.0);
        let b = target(&st(Some(Panel::NowPlaying), true, true), 1.5);
        assert_eq!(b.w, a.w * 1.5);
        assert_eq!(b.radius, a.radius * 1.5);
    }

    #[test]
    fn cycling_wraps_and_skips_missing_now_playing() {
        let all = panels(true);
        assert_eq!(cycle(&all, Panel::Controls, 1), Panel::Calendar);
        assert_eq!(cycle(&all, Panel::Calendar, 1), Panel::NowPlaying);
        assert_eq!(cycle(&all, Panel::NowPlaying, 1), Panel::Controls);
        assert_eq!(cycle(&all, Panel::Controls, -1), Panel::NowPlaying);
        let only = panels(false);
        assert_eq!(cycle(&only, Panel::Calendar, 1), Panel::Controls);
        assert_eq!(cycle(&only, Panel::Controls, 1), Panel::Calendar);
        // sesi media hilang saat Now Playing terbuka: jatuh ke panel pertama yang masih ada
        assert_eq!(cycle(&only, Panel::NowPlaying, 1), Panel::Controls);
        // klik pil selalu membuka Control Center
        assert_eq!(initial_panel(true), Panel::Controls);
        assert_eq!(initial_panel(false), Panel::Controls);
    }
}

