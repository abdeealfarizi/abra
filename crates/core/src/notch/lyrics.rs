//! Lirik: pengurai format LRC (baris bercap waktu `[mm:ss.xx]`) dan lirik biasa tanpa waktu, plus pencarian baris yang
//! sedang dinyanyikan pada posisi lagu tertentu.
//!
//! Yang didukung: beberapa cap waktu per baris (`[00:12.30][01:40.00]teks`), pecahan 1–3 digit, bentuk `mm:ss:xx`,
//! tag `[offset:±ms]`, tag metadata (`[ar:…]`, `[ti:…]`, dibuang), cap kata `<mm:ss.xx>` di dalam teks (dibuang),
//! BOM, dan baris kosong bercap waktu (jeda instrumental, tampil kosong).

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    /// Waktu mulai (ms dari awal lagu); 0 untuk lirik tanpa waktu.
    pub at_ms: i64,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Lyrics {
    pub lines: Vec<Line>,
    /// true bila setiap baris punya waktu (bisa diikuti posisi lagu).
    pub synced: bool,
}

/// Cap waktu `mm:ss`, `mm:ss.f`, `mm:ss.ff`, `mm:ss.fff`, atau `mm:ss:ff` → ms.
fn stamp(tag: &str) -> Option<i64> {
    if tag.is_empty() || !tag.bytes().all(|b| b.is_ascii_digit() || b == b':' || b == b'.') {
        return None;
    }
    let parts: Vec<&str> = tag.split(':').collect();
    let (min, sec, frac) = match parts.as_slice() {
        [m, s] => match s.split_once('.') {
            Some((s, f)) => (*m, s, f),
            None => (*m, *s, ""),
        },
        [m, s, f] => (*m, *s, *f),
        _ => return None,
    };
    let min: i64 = min.parse().ok()?;
    let sec: i64 = sec.parse().ok()?;
    if sec >= 60 || frac.len() > 3 {
        return None;
    }
    let ms = if frac.is_empty() { 0 } else { frac.parse::<i64>().ok()? * 10i64.pow(3 - frac.len() as u32) };
    Some(min * 60_000 + sec * 1000 + ms)
}

/// Buang cap waktu per kata `<mm:ss.xx>` dari teks.
fn strip_word_stamps(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('<') {
        match rest[i + 1..].find('>') {
            Some(j) if stamp(&rest[i + 1..i + 1 + j]).is_some() => {
                out.push_str(&rest[..i]);
                rest = &rest[i + 2 + j..];
            }
            _ => {
                out.push_str(&rest[..=i]);
                rest = &rest[i + 1..];
            }
        }
    }
    out.push_str(rest);
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn parse(src: &str) -> Lyrics {
    let mut offset = 0i64;
    let mut timed: Vec<Line> = Vec::new();
    let mut plain: Vec<String> = Vec::new();
    for raw in src.trim_start_matches('\u{feff}').lines() {
        let mut rest = raw.trim();
        let mut stamps = Vec::new();
        let mut meta = false;
        while rest.starts_with('[') {
            let Some(end) = rest.find(']') else { break };
            let tag = &rest[1..end];
            if let Some(ms) = stamp(tag) {
                stamps.push(ms);
            } else if stamps.is_empty() {
                if let Some(v) = tag.strip_prefix("offset:") {
                    offset = v.trim().trim_start_matches('+').parse().unwrap_or(0);
                }
                meta = tag.contains(':');
            }
            rest = rest[end + 1..].trim_start();
            if meta {
                break;
            }
        }
        if meta && stamps.is_empty() {
            continue;
        }
        let text = strip_word_stamps(rest);
        if stamps.is_empty() {
            plain.push(text);
        } else {
            for ms in stamps {
                timed.push(Line { at_ms: ms, text: text.clone() });
            }
        }
    }
    if !timed.is_empty() {
        // offset positif = lirik tampil lebih awal
        for l in &mut timed {
            l.at_ms = (l.at_ms - offset).max(0);
        }
        timed.sort_by_key(|l| l.at_ms);
        return Lyrics { lines: timed, synced: true };
    }
    // lirik biasa: buang baris kosong di awal/akhir, rapatkan kosong berturut-turut
    let mut lines: Vec<Line> = Vec::new();
    for t in plain {
        if t.is_empty() && lines.last().is_none_or(|l| l.text.is_empty()) {
            continue;
        }
        lines.push(Line { at_ms: 0, text: t });
    }
    while lines.last().is_some_and(|l| l.text.is_empty()) {
        lines.pop();
    }
    Lyrics { lines, synced: false }
}

impl Lyrics {
    pub fn is_empty(&self) -> bool {
        self.lines.iter().all(|l| l.text.is_empty())
    }

    /// Indeks baris yang sedang berjalan pada `t_ms` (lirik tersinkron saja).
    pub fn index_at(&self, t_ms: i64) -> Option<usize> {
        if !self.synced {
            return None;
        }
        let p = self.lines.partition_point(|l| l.at_ms <= t_ms);
        p.checked_sub(1)
    }

    /// Teks baris yang sedang dinyanyikan; `None` sebelum baris pertama atau saat jeda instrumental.
    pub fn current(&self, t_ms: i64) -> Option<&str> {
        let i = self.index_at(t_ms)?;
        let t = self.lines[i].text.as_str();
        (!t.is_empty()).then_some(t)
    }

    /// Kapan baris berikutnya dimulai (untuk menjadwalkan pembaruan tepat waktu, tanpa polling).
    pub fn next_change_ms(&self, t_ms: i64) -> Option<i64> {
        if !self.synced {
            return None;
        }
        let p = self.lines.partition_point(|l| l.at_ms <= t_ms);
        self.lines.get(p).map(|l| l.at_ms)
    }

    /// Kemajuan di dalam baris aktif (0..1), untuk efek isian karaoke. Baris terakhir dianggap 4 detik.
    pub fn progress(&self, t_ms: i64) -> f32 {
        let Some(i) = self.index_at(t_ms) else { return 0.0 };
        let start = self.lines[i].at_ms;
        let end = self.lines.get(i + 1).map_or(start + 4000, |l| l.at_ms).max(start + 1);
        ((t_ms - start) as f32 / (end - start) as f32).clamp(0.0, 1.0)
    }

    /// Baris di sekitar baris aktif untuk kartu lirik: (indeks baris, apakah aktif). Sebelum baris pertama, mulai dari
    /// baris pertama tanpa yang aktif.
    pub fn window(&self, t_ms: i64, before: usize, after: usize) -> Vec<(usize, bool)> {
        let n = self.lines.len();
        if n == 0 {
            return Vec::new();
        }
        let (center, active) = match self.index_at(t_ms) {
            Some(i) => (i, true),
            None => (0, false),
        };
        let from = center.saturating_sub(before);
        let to = (center + after + 1).min(n);
        (from..to).map(|i| (i, active && i == center)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SONG: &str = "\u{feff}[ar:Penyanyi]\n[ti:Judul]\n[offset:+200]\n\n[00:05.00]Baris satu\n[00:09.5]Baris <00:10.00>dua  di sini\n[00:14.120][00:30.00]Reff\n[00:20.00]\n[00:25:50]Baris empat\n";

    #[test]
    fn stamps_in_every_common_form() {
        assert_eq!(stamp("01:02"), Some(62_000));
        assert_eq!(stamp("01:02.5"), Some(62_500));
        assert_eq!(stamp("01:02.05"), Some(62_050));
        assert_eq!(stamp("01:02.123"), Some(62_123));
        assert_eq!(stamp("01:02:30"), Some(62_300));
        assert_eq!(stamp("ar:x"), None);
        assert_eq!(stamp("01:75.00"), None);
        assert_eq!(stamp(""), None);
    }

    #[test]
    fn parses_synced_lyrics_with_offset_repeats_gaps_and_word_stamps() {
        let l = parse(SONG);
        assert!(l.synced);
        let got: Vec<(i64, &str)> = l.lines.iter().map(|x| (x.at_ms, x.text.as_str())).collect();
        assert_eq!(
            got,
            vec![(4800, "Baris satu"), (9300, "Baris dua di sini"), (13_920, "Reff"), (19_800, ""), (25_300, "Baris empat"), (29_800, "Reff")]
        );
    }

    #[test]
    fn current_line_follows_the_song_position() {
        let l = parse(SONG);
        assert_eq!(l.current(0), None, "sebelum baris pertama");
        assert_eq!(l.current(4800), Some("Baris satu"));
        assert_eq!(l.current(9299), Some("Baris satu"));
        assert_eq!(l.current(9300), Some("Baris dua di sini"));
        assert_eq!(l.current(21_000), None, "jeda instrumental tampil kosong");
        assert_eq!(l.current(99_000), Some("Reff"));
        assert_eq!(l.next_change_ms(10_000), Some(13_920));
        assert_eq!(l.next_change_ms(40_000), None);
        assert!((l.progress(4800 + 2250) - 0.5).abs() < 1e-3);
    }

    #[test]
    fn window_centres_on_the_active_line() {
        let l = parse(SONG);
        assert_eq!(l.window(10_000, 1, 1), vec![(0, false), (1, true), (2, false)]);
        assert_eq!(l.window(0, 1, 1), vec![(0, false), (1, false)]);
        assert_eq!(l.window(99_000, 1, 1), vec![(4, false), (5, true)]);
    }

    #[test]
    fn plain_lyrics_are_not_synced() {
        let l = parse("\n\nVerse one\nline two\n\n\n\nChorus\n\n");
        assert!(!l.synced);
        let t: Vec<&str> = l.lines.iter().map(|x| x.text.as_str()).collect();
        assert_eq!(t, vec!["Verse one", "line two", "", "Chorus"]);
        assert_eq!(l.current(5000), None);
        assert_eq!(l.next_change_ms(0), None);
        assert!(parse("[ar:x]\n[ti:y]\n").is_empty());
    }
}
