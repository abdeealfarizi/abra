pub const CATS: usize = 4;
pub const CUSTOM: usize = 3;
pub const ROUNDS: u8 = 4;
pub const STEP_MIN: u8 = 5;
pub const MIN_MIN: u8 = 5;
pub const MAX_MIN: u8 = 180;
pub const MAX_BREAK: u8 = 60;
pub const TITLES: [&str; CATS] = ["Kerja", "Belajar", "Baca", "Kustom"];
pub const COLORS: [[u8; 3]; CATS] = [[127, 119, 221], [93, 202, 165], [239, 159, 39], [237, 147, 177]];
pub const DEFAULT_FOCUS: [u8; CATS] = [25, 50, 30, 25];
pub const DEFAULT_BREAK: [u8; CATS] = [5, 10, 5, 5];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Stage {
    #[default]
    Focus,
    Break,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Session {
    pub cat: usize,
    pub stage: Stage,
    pub round: u8,
}

impl Default for Session {
    fn default() -> Self {
        Session { cat: 0, stage: Stage::Focus, round: 1 }
    }
}

pub enum Next {
    StartBreak(i64),
    Ready(i64),
}

impl Session {
    pub fn finish(&mut self, focus_min: u8, break_min: u8) -> Next {
        match self.stage {
            Stage::Focus if break_min > 0 => {
                self.stage = Stage::Break;
                Next::StartBreak(break_min as i64 * 60)
            }
            Stage::Focus => {
                self.round = self.round % ROUNDS + 1;
                Next::Ready(focus_min as i64 * 60)
            }
            Stage::Break => {
                self.stage = Stage::Focus;
                self.round = self.round % ROUNDS + 1;
                Next::Ready(focus_min as i64 * 60)
            }
        }
    }
}

pub fn step(min: u8, up: bool, is_break: bool) -> u8 {
    let (lo, hi) = if is_break { (0, MAX_BREAK) } else { (MIN_MIN, MAX_MIN) };
    if up {
        min.saturating_add(STEP_MIN).min(hi)
    } else {
        min.saturating_sub(STEP_MIN).max(lo)
    }
}

pub fn parse_list(v: &str, fallback: [u8; CATS], is_break: bool) -> [u8; CATS] {
    let (lo, hi) = if is_break { (0, MAX_BREAK) } else { (MIN_MIN, MAX_MIN) };
    let mut out = fallback;
    for (i, part) in v.split(',').take(CATS).enumerate() {
        if let Ok(n) = part.trim().parse::<u32>() {
            out[i] = n.clamp(lo as u32, hi as u32) as u8;
        }
    }
    out
}

pub fn list_text(v: [u8; CATS]) -> String {
    v.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(",")
}

pub type Day = (i32, u32, u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub day: Day,
    pub cat: usize,
    pub minutes: u32,
}

pub fn day_key(d: Day) -> String {
    format!("{:04}-{:02}-{:02}", d.0, d.1, d.2)
}

fn parse_day(s: &str) -> Option<Day> {
    let mut it = s.split('-');
    let y = it.next()?.parse().ok()?;
    let m = it.next()?.parse().ok()?;
    let d = it.next()?.parse().ok()?;
    ((1..=12).contains(&m) && (1..=31).contains(&d)).then_some((y, m, d))
}

pub fn parse_log(text: &str) -> Vec<Entry> {
    text.lines()
        .filter_map(|l| {
            let mut it = l.split('\t');
            let day = parse_day(it.next()?.trim())?;
            let cat = it.next()?.trim().parse::<usize>().ok().filter(|c| *c < CATS)?;
            let minutes = it.next()?.trim().parse::<u32>().ok().filter(|m| *m > 0 && *m <= 24 * 60)?;
            Some(Entry { day, cat, minutes })
        })
        .collect()
}

pub fn log_line(e: Entry) -> String {
    format!("{}\t{}\t{}\n", day_key(e.day), e.cat, e.minutes)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Today {
    pub minutes: u32,
    pub sessions: u32,
    pub top: Option<usize>,
}

pub fn today(entries: &[Entry], day: Day) -> Today {
    let mut per = [0u32; CATS];
    let mut t = Today::default();
    for e in entries.iter().filter(|e| e.day == day) {
        t.minutes += e.minutes;
        t.sessions += 1;
        per[e.cat] += e.minutes;
    }
    t.top = per.iter().enumerate().filter(|(_, m)| **m > 0).max_by_key(|(i, m)| (**m, std::cmp::Reverse(*i))).map(|(i, _)| i);
    t
}

pub fn week(entries: &[Entry], days: &[Day; 7]) -> [u32; 7] {
    let mut out = [0u32; 7];
    for e in entries {
        if let Some(i) = days.iter().position(|d| *d == e.day) {
            out[i] += e.minutes;
        }
    }
    out
}

pub fn trim(entries: &mut Vec<Entry>, keep: usize) {
    if entries.len() > keep {
        let cut = entries.len() - keep;
        entries.drain(..cut);
    }
}

pub fn duration_label(minutes: u32, en: bool) -> String {
    duration_label_lang(minutes, if en { crate::i18n::Lang::En } else { crate::i18n::Lang::Id })
}

pub fn duration_label_lang(minutes: u32, lang: crate::i18n::Lang) -> String {
    let (h, m) = (minutes / 60, minutes % 60);
    match (h, lang) {
        (0, _) => format!("{m}m"),
        (_, crate::i18n::Lang::Id) => format!("{h}j {m}m"),
        (_, crate::i18n::Lang::En) => format!("{h}h {m}m"),
        (_, crate::i18n::Lang::Es) => format!("{h} h {m} min"),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Saved {
    pub session: Session,
    pub running: bool,
    pub remaining_ms: i64,
    pub deadline_epoch_ms: i64,
}

pub fn saved_text(s: Saved) -> String {
    let stage = if s.session.stage == Stage::Break { 1 } else { 0 };
    format!("{} {} {} {} {} {}\n", s.session.cat, stage, s.session.round, s.running as u8, s.remaining_ms, s.deadline_epoch_ms)
}

pub fn parse_saved(text: &str) -> Option<Saved> {
    let v: Vec<i64> = text.split_whitespace().map(|p| p.parse::<i64>()).collect::<Result<_, _>>().ok()?;
    let [cat, stage, round, running, remaining_ms, deadline] = v[..] else { return None };
    if !(0..CATS as i64).contains(&cat) || !(1..=ROUNDS as i64).contains(&round) || remaining_ms < 0 {
        return None;
    }
    Some(Saved {
        session: Session { cat: cat as usize, stage: if stage == 1 { Stage::Break } else { Stage::Focus }, round: round as u8 },
        running: running == 1,
        remaining_ms,
        deadline_epoch_ms: deadline,
    })
}

pub fn left_after_restart(s: Saved, now_epoch_ms: i64) -> i64 {
    if s.running {
        (s.deadline_epoch_ms - now_epoch_ms).max(0)
    } else {
        s.remaining_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_then_break_then_next_round() {
        let mut s = Session::default();
        assert!(matches!(s.finish(25, 5), Next::StartBreak(300)));
        assert_eq!(s.stage, Stage::Break);
        assert!(matches!(s.finish(25, 5), Next::Ready(1500)));
        assert_eq!((s.stage, s.round), (Stage::Focus, 2));
    }

    #[test]
    fn zero_break_skips_straight_to_next_round_and_rounds_wrap() {
        let mut s = Session { round: 4, ..Session::default() };
        assert!(matches!(s.finish(30, 0), Next::Ready(1800)));
        assert_eq!((s.stage, s.round), (Stage::Focus, 1));
    }

    #[test]
    fn steps_are_clamped() {
        assert_eq!(step(5, false, false), 5);
        assert_eq!(step(180, true, false), 180);
        assert_eq!(step(25, true, false), 30);
        assert_eq!(step(0, false, true), 0);
        assert_eq!(step(5, false, true), 0);
        assert_eq!(step(60, true, true), 60);
    }

    #[test]
    fn duration_lists_parse_with_fallback_and_clamp() {
        assert_eq!(parse_list("15,90,x,400", DEFAULT_FOCUS, false), [15, 90, 30, 180]);
        assert_eq!(parse_list("0,99", DEFAULT_BREAK, true), [0, 60, 5, 5]);
        assert_eq!(parse_list(&list_text([10, 20, 30, 40]), DEFAULT_FOCUS, false), [10, 20, 30, 40]);
    }

    #[test]
    fn log_round_trip_and_bad_lines() {
        let e = Entry { day: (2026, 11, 20), cat: 3, minutes: 25 };
        let text = format!("{}junk\n2026-13-01\t0\t5\n2026-11-20\t9\t5\n2026-11-20\t1\t0\n", log_line(e));
        assert_eq!(parse_log(&text), vec![e]);
    }

    #[test]
    fn today_and_week_totals() {
        let d = (2026, 11, 20);
        let y = (2026, 11, 19);
        let list = vec![
            Entry { day: d, cat: 0, minutes: 25 },
            Entry { day: d, cat: 1, minutes: 50 },
            Entry { day: d, cat: 0, minutes: 25 },
            Entry { day: y, cat: 2, minutes: 30 },
        ];
        assert_eq!(today(&list, d), Today { minutes: 100, sessions: 3, top: Some(0) });
        assert_eq!(today(&list, (2026, 1, 1)), Today::default());
        let days = [(2026, 11, 14), (2026, 11, 15), (2026, 11, 16), (2026, 11, 17), (2026, 11, 18), y, d];
        assert_eq!(week(&list, &days), [0, 0, 0, 0, 0, 30, 100]);
    }

    #[test]
    fn ties_pick_the_first_category() {
        let d = (2026, 11, 20);
        let list = vec![Entry { day: d, cat: 2, minutes: 30 }, Entry { day: d, cat: 1, minutes: 30 }];
        assert_eq!(today(&list, d).top, Some(1));
    }

    #[test]
    fn saved_state_survives_a_restart() {
        let s = Saved { session: Session { cat: 2, stage: Stage::Break, round: 3 }, running: true, remaining_ms: 0, deadline_epoch_ms: 10_000 };
        let back = parse_saved(&saved_text(s)).unwrap();
        assert_eq!(back, s);
        assert_eq!(left_after_restart(back, 4_000), 6_000);
        assert_eq!(left_after_restart(back, 20_000), 0);
        let paused = Saved { running: false, remaining_ms: 90_000, ..s };
        assert_eq!(left_after_restart(paused, 99_999_999), 90_000);
        assert!(parse_saved("9 0 1 0 0 0").is_none());
        assert!(parse_saved("garbage").is_none());
    }

    #[test]
    fn labels_and_trim() {
        assert_eq!(duration_label(85, false), "1j 25m");
        assert_eq!(duration_label(85, true), "1h 25m");
        assert_eq!(duration_label(40, false), "40m");
        let mut v: Vec<Entry> = (0..10).map(|i| Entry { day: (2026, 1, 1), cat: 0, minutes: i + 1 }).collect();
        trim(&mut v, 4);
        assert_eq!(v.len(), 4);
        assert_eq!(v[0].minutes, 7);
    }
}
