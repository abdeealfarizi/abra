//! Garis waktu tampil/sembunyi HUD: muncul saat nilai berubah atau tepi disentuh, bertahan sebentar,
//! tetap selama dipegang (kursor di atasnya / sedang di-drag), lalu hilang. Waktu dioper dari luar.

pub const LINGER_MS: i64 = 1500;

#[derive(Clone, Copy, Debug, Default)]
pub struct Timeline {
    visible_until: i64,
    held: bool,
}

impl Timeline {
    /// Nilai berubah atau tepi disentuh: tampil dan perpanjang hitungan.
    pub fn poke(&mut self, now: i64) {
        self.visible_until = now + LINGER_MS;
    }

    /// Kursor di HUD atau slider sedang di-drag: tampil tanpa batas sampai dilepas.
    pub fn hold(&mut self, held: bool, now: i64) {
        self.held = held;
        if !held {
            self.visible_until = now + LINGER_MS;
        }
    }

    pub fn visible(&self, now: i64) -> bool {
        self.held || now < self.visible_until
    }

    /// Sisa ms sampai sembunyi bila tak ada yang memperpanjang (untuk menjadwalkan satu timer).
    pub fn remaining(&self, now: i64) -> Option<i64> {
        if self.held || now >= self.visible_until {
            None
        } else {
            Some(self.visible_until - now)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appears_on_poke_and_lingers_then_hides() {
        let mut t = Timeline::default();
        assert!(!t.visible(0));
        t.poke(1000);
        assert!(t.visible(1000) && t.visible(2499));
        assert!(!t.visible(2500));
    }

    #[test]
    fn repeated_changes_extend_the_window() {
        let mut t = Timeline::default();
        t.poke(0);
        t.poke(1200);
        assert!(t.visible(2600));
        assert!(!t.visible(2700));
        assert_eq!(t.remaining(2000), Some(700));
    }

    #[test]
    fn held_keeps_it_visible_and_release_restarts_the_linger() {
        let mut t = Timeline::default();
        t.poke(0);
        t.hold(true, 100);
        assert!(t.visible(99_999));
        assert_eq!(t.remaining(99_999), None);
        t.hold(false, 100_000);
        assert!(t.visible(101_000) && !t.visible(101_500));
    }
}
