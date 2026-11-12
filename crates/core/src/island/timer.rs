//! Timer hitung mundur (Pomodoro). Waktu dioper dari luar (ms monoton) supaya mudah diuji.

pub const MAX_SECONDS: i64 = 180 * 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Running,
    Paused,
    Finished,
}

#[derive(Clone, Copy, Debug)]
pub struct Timer {
    /// Durasi yang dipilih (detik); dipakai ulang saat reset.
    pub duration: i64,
    remaining_ms: i64,
    deadline_ms: i64,
    pub phase: Phase,
}

impl Timer {
    pub fn new(duration_secs: i64) -> Timer {
        let d = duration_secs.clamp(1, MAX_SECONDS);
        Timer { duration: d, remaining_ms: d * 1000, deadline_ms: 0, phase: Phase::Idle }
    }

    pub fn set_duration(&mut self, secs: i64) {
        *self = Timer::new(secs);
    }

    /// Tambah/kurangi durasi hanya saat tidak berjalan.
    pub fn adjust(&mut self, delta_secs: i64) {
        if matches!(self.phase, Phase::Idle | Phase::Finished) {
            self.set_duration(self.duration + delta_secs);
        }
    }

    pub fn start(&mut self, now_ms: i64) {
        match self.phase {
            Phase::Finished => *self = Timer::new(self.duration),
            Phase::Running => return,
            _ => {}
        }
        self.deadline_ms = now_ms + self.remaining_ms;
        self.phase = Phase::Running;
    }

    pub fn pause(&mut self, now_ms: i64) {
        if self.phase == Phase::Running {
            self.remaining_ms = (self.deadline_ms - now_ms).max(0);
            self.phase = Phase::Paused;
        }
    }

    pub fn reset(&mut self) {
        *self = Timer::new(self.duration);
    }

    /// Mengembalikan `true` tepat sekali saat hitungan mencapai nol.
    pub fn tick(&mut self, now_ms: i64) -> bool {
        if self.phase == Phase::Running && now_ms >= self.deadline_ms {
            self.remaining_ms = 0;
            self.phase = Phase::Finished;
            return true;
        }
        false
    }

    /// Sisa detik, dibulatkan ke atas (00:01 tetap tampil sampai benar-benar nol).
    pub fn remaining_secs(&self, now_ms: i64) -> i64 {
        let ms = if self.phase == Phase::Running { (self.deadline_ms - now_ms).max(0) } else { self.remaining_ms };
        (ms + 999) / 1000
    }
}

pub fn format_mmss(secs: i64) -> String {
    let s = secs.max(0);
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_down_and_finishes_once() {
        let mut t = Timer::new(10);
        t.start(1000);
        assert_eq!(t.remaining_secs(1000), 10);
        assert_eq!(t.remaining_secs(5500), 6);
        assert!(!t.tick(10_999));
        assert!(t.tick(11_000));
        assert!(!t.tick(12_000), "selesai hanya dilaporkan sekali");
        assert_eq!(t.phase, Phase::Finished);
        assert_eq!(t.remaining_secs(99_999), 0);
    }

    #[test]
    fn pause_keeps_remaining_and_resume_continues() {
        let mut t = Timer::new(60);
        t.start(0);
        t.pause(20_000);
        assert_eq!(t.remaining_secs(50_000), 40);
        t.start(50_000);
        assert_eq!(t.remaining_secs(60_000), 30);
    }

    #[test]
    fn start_after_finish_restarts_from_chosen_duration() {
        let mut t = Timer::new(5);
        t.start(0);
        t.tick(5000);
        t.start(9000);
        assert_eq!(t.phase, Phase::Running);
        assert_eq!(t.remaining_secs(9000), 5);
    }

    #[test]
    fn duration_is_clamped_and_adjust_only_when_not_running() {
        assert_eq!(Timer::new(10_000_000).duration, MAX_SECONDS);
        assert_eq!(Timer::new(-5).duration, 1);
        let mut t = Timer::new(300);
        t.adjust(60);
        assert_eq!(t.duration, 360);
        t.start(0);
        t.adjust(60);
        assert_eq!(t.duration, 360, "tidak boleh berubah saat berjalan");
    }

    #[test]
    fn formatting() {
        assert_eq!(format_mmss(65), "01:05");
        assert_eq!(format_mmss(3725), "1:02:05");
        assert_eq!(format_mmss(-3), "00:00");
    }
}
