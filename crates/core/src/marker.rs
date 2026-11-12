//! Berkas penanda "taskbar sedang disembunyikan".
//!
//! Ditulis sebelum taskbar Windows disembunyikan dan dihapus hanya setelah
//! pemulihan terverifikasi. Berisi segala yang diperlukan untuk memulihkan
//! taskbar tanpa bantuan proses utama (watchdog, atau start berikutnya setelah crash).
//!
//! Format teks per baris, mudah dibaca manusia:
//! ```text
//! abstate=2
//! pid=1234
//! primary=0,1040,1920,1080
//! secondary=65890,1920,1040,3840,1080
//! ```

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    /// Persegi yang benar-benar terpasang di layar. Taskbar yang kita parkir
    /// berada jauh di luar (−10000), dan taskbar Explorer yang baru dibuat bisa
    /// melapor rect kosong; keduanya tidak boleh disimpan sebagai posisi asli.
    pub fn is_placed(&self) -> bool {
        self.right > self.left && self.bottom > self.top && self.left > -9000 && self.top > -9000
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Marker {
    /// State AppBar taskbar sebelum kita ubah (bit 1 = auto-hide, bit 2 = selalu di atas).
    /// Negatif = tidak diketahui.
    pub abstate: i32,
    pub pid: u32,
    pub primary: Option<Rect>,
    /// (handle window, rect) tiap taskbar monitor sekunder.
    pub secondaries: Vec<(isize, Rect)>,
}

impl Marker {
    pub fn new(abstate: i32, pid: u32) -> Self {
        Marker { abstate, pid, primary: None, secondaries: Vec::new() }
    }

    pub fn serialize(&self) -> String {
        let mut s = format!("abstate={}\npid={}\n", self.abstate, self.pid);
        if let Some(r) = self.primary {
            s.push_str(&format!("primary={},{},{},{}\n", r.left, r.top, r.right, r.bottom));
        }
        let mut sec = self.secondaries.clone();
        sec.sort_by_key(|(h, _)| *h);
        for (h, r) in sec {
            s.push_str(&format!("secondary={h},{},{},{},{}\n", r.left, r.top, r.right, r.bottom));
        }
        s
    }

    /// `None` bila berkas rusak atau tanpa pid. Marker format lama/rusak tidak
    /// boleh membuat pemulihan salah menebak posisi.
    pub fn parse(text: &str) -> Option<Marker> {
        let mut abstate = -1;
        let mut pid = None;
        let mut primary = None;
        let mut secondaries = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if let Some(v) = line.strip_prefix("abstate=") {
                abstate = v.parse().ok()?;
            } else if let Some(v) = line.strip_prefix("pid=") {
                pid = Some(v.parse().ok()?);
            } else if let Some(v) = line.strip_prefix("primary=") {
                primary = Some(parse_rect(v.split(',').collect::<Vec<_>>().as_slice())?);
            } else if let Some(v) = line.strip_prefix("secondary=") {
                let parts: Vec<&str> = v.split(',').collect();
                let (h, rest) = parts.split_first()?;
                secondaries.push((h.parse().ok()?, parse_rect(rest)?));
            }
        }
        Some(Marker { abstate, pid: pid?, primary, secondaries })
    }

    pub fn secondary_rect(&self, hwnd: isize) -> Option<Rect> {
        self.secondaries.iter().find(|(h, _)| *h == hwnd).map(|(_, r)| *r)
    }
}

fn parse_rect(parts: &[&str]) -> Option<Rect> {
    if parts.len() != 4 {
        return None;
    }
    let n: Vec<i32> = parts.iter().map(|p| p.trim().parse().ok()).collect::<Option<_>>()?;
    Some(Rect { left: n[0], top: n[1], right: n[2], bottom: n[3] })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Marker {
        Marker {
            abstate: 2,
            pid: 4242,
            primary: Some(Rect { left: 0, top: 1032, right: 1920, bottom: 1080 }),
            secondaries: vec![
                (900, Rect { left: 1920, top: 1032, right: 3840, bottom: 1080 }),
                (100, Rect { left: -1920, top: 1032, right: 0, bottom: 1080 }),
            ],
        }
    }

    #[test]
    fn round_trip_preserves_everything() {
        let m = sample();
        let back = Marker::parse(&m.serialize()).unwrap();
        assert_eq!(back.abstate, 2);
        assert_eq!(back.pid, 4242);
        assert_eq!(back.primary, m.primary);
        assert_eq!(back.secondary_rect(100), Some(Rect { left: -1920, top: 1032, right: 0, bottom: 1080 }));
        assert_eq!(back.secondary_rect(900), m.secondary_rect(900));
        assert_eq!(back.secondaries.len(), 2);
    }

    #[test]
    fn serialization_is_stable_regardless_of_secondary_order() {
        let a = sample();
        let mut b = sample();
        b.secondaries.reverse();
        assert_eq!(a.serialize(), b.serialize());
    }

    #[test]
    fn marker_without_pid_is_rejected() {
        assert!(Marker::parse("abstate=1\n").is_none());
    }

    #[test]
    fn damaged_lines_are_rejected_not_guessed() {
        assert!(Marker::parse("abstate=x\npid=1\n").is_none());
        assert!(Marker::parse("abstate=1\npid=1\nprimary=1,2,3\n").is_none());
        assert!(Marker::parse("abstate=1\npid=1\nsecondary=5,1,2,3\n").is_none());
        assert!(Marker::parse("").is_none());
    }

    #[test]
    fn unknown_state_defaults_to_negative() {
        let m = Marker::parse("pid=7\n").unwrap();
        assert_eq!(m.abstate, -1);
        assert!(m.primary.is_none());
    }

    #[test]
    fn parked_or_empty_rects_are_not_placed() {
        assert!(!Rect { left: -10000, top: -10000, right: -9900, bottom: -9950 }.is_placed());
        assert!(!Rect { left: 0, top: 0, right: 0, bottom: 0 }.is_placed());
        assert!(Rect { left: 0, top: 1032, right: 1920, bottom: 1080 }.is_placed());
        // monitor di sebelah kiri/atas primer: koordinat negatif tetap sah
        assert!(Rect { left: -1920, top: 1032, right: 0, bottom: 1080 }.is_placed());
    }
}
