#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Civil {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
}

pub fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y } as i64;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

impl Civil {
    pub fn minutes(&self) -> i64 {
        days_from_civil(self.year, self.month, self.day) * 1440 + self.hour as i64 * 60 + self.minute as i64
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Task {
    pub id: u32,
    pub text: String,
    pub done: bool,
    pub due: Option<i64>,
    pub fired: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct List {
    pub tasks: Vec<Task>,
}

fn clean(s: &str) -> String {
    s.chars().map(|c| if c == '\t' || c == '\n' || c == '\r' { ' ' } else { c }).collect::<String>().trim().chars().take(200).collect()
}

fn hhmm(tok: &str) -> Option<(u32, u32)> {
    let (h, m) = tok.split_once([':', '.'])?;
    let (h, m) = (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?);
    (h < 24 && m < 60 && !tok.ends_with(['.', ':'])).then_some((h, m))
}

fn span(tok: &str) -> Option<i64> {
    let lower = tok.to_lowercase();
    let digits: String = lower.chars().take_while(|c| c.is_ascii_digit()).collect();
    let unit = &lower[digits.len()..];
    let n = digits.parse::<i64>().ok().filter(|n| *n > 0 && *n <= 10_000)?;
    match unit {
        "m" | "mnt" | "min" | "menit" => Some(n),
        "j" | "jam" | "h" | "hr" | "hour" | "hours" => Some(n * 60),
        _ => None,
    }
}

pub fn parse(input: &str, now: Civil) -> (String, Option<i64>) {
    let mut words: Vec<&str> = input.split_whitespace().collect();
    if words.first().is_some_and(|w| matches!(w.to_lowercase().as_str(), "ingatkan" | "remind" | "reminder" | "pengingat")) {
        words.remove(0);
    }
    let now_m = now.minutes();
    let mut due = None;
    let first = words.first().map(|w| w.to_lowercase());
    match first.as_deref() {
        Some("besok") | Some("tomorrow") => {
            if let Some((h, m)) = words.get(1).and_then(|w| hhmm(w)) {
                due = Some((days_from_civil(now.year, now.month, now.day) + 1) * 1440 + h as i64 * 60 + m as i64);
                words.drain(..2);
            }
        }
        Some(w) => {
            if let Some((h, m)) = hhmm(w) {
                let mut at = days_from_civil(now.year, now.month, now.day) * 1440 + h as i64 * 60 + m as i64;
                if at <= now_m {
                    at += 1440;
                }
                due = Some(at);
                words.remove(0);
            } else if let Some(mins) = span(w) {
                due = Some(now_m + mins);
                words.remove(0);
            }
        }
        None => {}
    }
    if due.is_some() && words.first().is_some_and(|w| matches!(w.to_lowercase().as_str(), "-" | "–")) {
        words.remove(0);
    }
    let text = clean(&words.join(" "));
    if text.is_empty() {
        return (clean(input), None);
    }
    (text, due)
}

pub fn due_label(due: i64, now: Civil, english: bool) -> String {
    due_label_lang(due, now, if english { crate::i18n::Lang::En } else { crate::i18n::Lang::Id })
}

pub fn due_label_lang(due: i64, now: Civil, lang: crate::i18n::Lang) -> String {
    let day = due.div_euclid(1440);
    let (h, m) = ((due.rem_euclid(1440)) / 60, due.rem_euclid(60));
    let today = days_from_civil(now.year, now.month, now.day);
    let t = format!("{h:02}:{m:02}");
    match day - today {
        0 => t,
        1 => format!("{} {t}", match lang { crate::i18n::Lang::Id => "Besok", crate::i18n::Lang::En => "Tomorrow", crate::i18n::Lang::Es => "Mañana", crate::i18n::Lang::Vi => "Ngày mai" }),
        -1 => format!("{} {t}", match lang { crate::i18n::Lang::Id => "Kemarin", crate::i18n::Lang::En => "Yesterday", crate::i18n::Lang::Es => "Ayer", crate::i18n::Lang::Vi => "Hôm qua" }),
        _ => {
            let (y, mo, d) = civil_from_days(day);
            let _ = y;
            format!("{d}/{mo} {t}")
        }
    }
}

pub fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    ((if m <= 2 { y + 1 } else { y }) as i32, m, d)
}

impl List {
    pub fn add(&mut self, input: &str, now: Civil) -> Option<u32> {
        let (text, due) = parse(input, now);
        if text.is_empty() {
            return None;
        }
        let id = self.tasks.iter().map(|t| t.id).max().unwrap_or(0) + 1;
        self.tasks.push(Task { id, text, done: false, due, fired: false });
        Some(id)
    }

    pub fn toggle(&mut self, id: u32) -> bool {
        let Some(t) = self.tasks.iter_mut().find(|t| t.id == id) else { return false };
        t.done = !t.done;
        true
    }

    pub fn remove(&mut self, id: u32) -> bool {
        let before = self.tasks.len();
        self.tasks.retain(|t| t.id != id);
        before != self.tasks.len()
    }

    pub fn clear_done(&mut self) -> usize {
        let before = self.tasks.len();
        self.tasks.retain(|t| !t.done);
        before - self.tasks.len()
    }

    pub fn snooze(&mut self, id: u32, now_m: i64, mins: i64) -> bool {
        let Some(t) = self.tasks.iter_mut().find(|t| t.id == id) else { return false };
        t.due = Some(now_m + mins);
        t.fired = false;
        t.done = false;
        true
    }

    pub fn due_now(&self, now_m: i64) -> Vec<u32> {
        self.tasks.iter().filter(|t| !t.done && !t.fired && t.due.is_some_and(|d| d <= now_m)).map(|t| t.id).collect()
    }

    pub fn mark_fired(&mut self, id: u32) {
        if let Some(t) = self.tasks.iter_mut().find(|t| t.id == id) {
            t.fired = true;
        }
    }

    pub fn next_due(&self) -> Option<i64> {
        self.tasks.iter().filter(|t| !t.done && !t.fired).filter_map(|t| t.due).min()
    }

    pub fn open_count(&self) -> usize {
        self.tasks.iter().filter(|t| !t.done).count()
    }

    pub fn get(&self, id: u32) -> Option<&Task> {
        self.tasks.iter().find(|t| t.id == id)
    }

    pub fn ordered(&self) -> Vec<&Task> {
        let mut v: Vec<&Task> = self.tasks.iter().collect();
        v.sort_by_key(|t| (t.done, t.due.unwrap_or(i64::MAX), t.id));
        v
    }

    pub fn to_text(&self) -> String {
        let mut out = String::from("# ABRA tasks v1\n");
        for t in &self.tasks {
            let due = t.due.map_or("-".to_string(), |d| d.to_string());
            out.push_str(&format!("{}\t{}\t{}\t{}\t{}\n", t.id, t.done as u8, due, t.fired as u8, clean(&t.text)));
        }
        out
    }

    pub fn from_text(s: &str) -> List {
        let mut tasks = Vec::new();
        for line in s.trim_start_matches('\u{feff}').lines() {
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }
            let f: Vec<&str> = line.splitn(5, '\t').collect();
            if f.len() != 5 {
                continue;
            }
            let (Ok(id), Some(text)) = (f[0].parse::<u32>(), Some(clean(f[4]))) else { continue };
            if text.is_empty() || tasks.iter().any(|t: &Task| t.id == id) {
                continue;
            }
            tasks.push(Task { id, text, done: f[1] == "1", due: f[2].parse::<i64>().ok(), fired: f[3] == "1" });
        }
        List { tasks }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> Civil {
        Civil { year: 2026, month: 11, day: 16, hour: 14, minute: 0 }
    }

    #[test]
    fn civil_round_trip() {
        for (y, m, d) in [(1970, 1, 1), (2000, 2, 29), (2026, 11, 16), (2026, 12, 31)] {
            assert_eq!(civil_from_days(days_from_civil(y, m, d)), (y, m, d));
        }
        assert_eq!(days_from_civil(1970, 1, 1), 0);
    }

    #[test]
    fn parses_times_spans_and_tomorrow() {
        let n = now();
        let base = days_from_civil(2026, 11, 16) * 1440;
        assert_eq!(parse("15:30 rapat tim", n), ("rapat tim".into(), Some(base + 15 * 60 + 30)));
        assert_eq!(parse("ingatkan 09.15 minum obat", n), ("minum obat".into(), Some(base + 1440 + 9 * 60 + 15)));
        assert_eq!(parse("10m angkat jemuran", n), ("angkat jemuran".into(), Some(n.minutes() + 10)));
        assert_eq!(parse("2j istirahat", n), ("istirahat".into(), Some(n.minutes() + 120)));
        assert_eq!(parse("remind 30min stretch", n), ("stretch".into(), Some(n.minutes() + 30)));
        assert_eq!(parse("besok 08:00 kirim laporan", n), ("kirim laporan".into(), Some(base + 1440 + 8 * 60)));
        assert_eq!(parse("beli susu", n), ("beli susu".into(), None));
        assert_eq!(parse("15:30", n), ("15:30".into(), None));
        assert_eq!(parse("25:00 bukan jam", n), ("25:00 bukan jam".into(), None));
        assert_eq!(parse("  ", n), (String::new(), None));
    }

    #[test]
    fn labels() {
        let n = now();
        let base = days_from_civil(2026, 11, 16) * 1440;
        assert_eq!(due_label(base + 15 * 60 + 30, n, false), "15:30");
        assert_eq!(due_label(base + 1440 + 8 * 60, n, false), "Besok 08:00");
        assert_eq!(due_label(base + 1440 + 8 * 60, n, true), "Tomorrow 08:00");
        assert_eq!(due_label(base + 3 * 1440, n, false), "19/11 00:00");
    }

    #[test]
    fn list_lifecycle_and_reminders() {
        let n = now();
        let mut l = List::default();
        let a = l.add("10m minum", n).unwrap();
        let b = l.add("beli susu", n).unwrap();
        assert_eq!(l.next_due(), Some(n.minutes() + 10));
        assert!(l.due_now(n.minutes() + 9).is_empty());
        assert_eq!(l.due_now(n.minutes() + 10), vec![a]);
        l.mark_fired(a);
        assert!(l.due_now(n.minutes() + 11).is_empty());
        assert_eq!(l.next_due(), None);
        assert!(l.snooze(a, n.minutes() + 11, 10));
        assert_eq!(l.next_due(), Some(n.minutes() + 21));
        assert!(l.toggle(b));
        assert_eq!(l.open_count(), 1);
        assert_eq!(l.ordered()[0].id, a);
        assert_eq!(l.clear_done(), 1);
        assert!(l.remove(a));
        assert!(l.tasks.is_empty());
        assert!(l.add("   ", n).is_none());
    }

    #[test]
    fn text_round_trip_and_junk() {
        let n = now();
        let mut l = List::default();
        l.add("15:30 rapat\ttim", n);
        l.add("beli susu", n);
        l.toggle(2);
        let back = List::from_text(&l.to_text());
        assert_eq!(back, l);
        let junk = List::from_text("\u{feff}# x\nzzz\n1\t0\t-\t0\t\n2\t1\t99\t0\tok\n2\t0\t-\t0\tdup\n");
        assert_eq!(junk.tasks.len(), 1);
        assert_eq!(junk.tasks[0].due, Some(99));
    }
}
