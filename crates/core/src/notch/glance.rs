use crate::i18n::Lang;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    Greeting,
    Clock,
    Date,
    Battery,
    System,
    Wifi,
    Agenda,
}

pub const MODULES: usize = 7;

impl Item {
    pub const ALL: [Item; MODULES] = [Item::Greeting, Item::Clock, Item::Date, Item::Battery, Item::System, Item::Wifi, Item::Agenda];

    pub fn parse(s: &str) -> Option<Item> {
        match s.trim().to_ascii_lowercase().as_str() {
            "greeting" | "sapaan" => Some(Item::Greeting),
            "clock" | "jam" => Some(Item::Clock),
            "date" | "tanggal" => Some(Item::Date),
            "battery" | "baterai" => Some(Item::Battery),
            "system" | "sistem" => Some(Item::System),
            "wifi" | "wi-fi" => Some(Item::Wifi),
            "agenda" | "calendar" => Some(Item::Agenda),
            _ => None,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Item::Greeting => "greeting",
            Item::Clock => "clock",
            Item::Date => "date",
            Item::Battery => "battery",
            Item::System => "system",
            Item::Wifi => "wifi",
            Item::Agenda => "agenda",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Item::Greeting => "Sapaan",
            Item::Clock => "Jam",
            Item::Date => "Tanggal",
            Item::Battery => "Baterai",
            Item::System => "Status sistem",
            Item::Wifi => "Wi-Fi",
            Item::Agenda => "Agenda berikutnya",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capsule {
    pub order: [Item; MODULES],
    pub on: u8,
}

impl Default for Capsule {
    fn default() -> Capsule {
        Capsule { order: Item::ALL, on: 4 }
    }
}

impl Capsule {
    pub fn parse(s: &str) -> Capsule {
        let mut order: Vec<Item> = Vec::with_capacity(MODULES);
        for part in s.split(',') {
            if let Some(it) = Item::parse(part) {
                if !order.contains(&it) {
                    order.push(it);
                }
            }
        }
        let on = order.len() as u8;
        for it in Item::ALL {
            if !order.contains(&it) {
                order.push(it);
            }
        }
        let mut arr = Item::ALL;
        arr.copy_from_slice(&order);
        Capsule { order: arr, on }
    }

    pub fn text(&self) -> String {
        self.enabled().iter().map(|i| i.key()).collect::<Vec<_>>().join(",")
    }

    pub fn enabled(&self) -> &[Item] {
        &self.order[..(self.on as usize).min(MODULES)]
    }

    pub fn is_on(&self, i: usize) -> bool {
        i < self.on as usize
    }

    pub fn toggle(&mut self, i: usize) {
        let on = self.on as usize;
        if i >= MODULES {
            return;
        }
        if i < on {
            self.order[i..on].rotate_left(1);
            self.on -= 1;
        } else {
            self.order[on..=i].rotate_right(1);
            self.on += 1;
        }
    }

    pub fn shift(&mut self, i: usize, up: bool) {
        let on = self.on as usize;
        if i >= on {
            return;
        }
        if up && i > 0 {
            self.order.swap(i, i - 1);
        } else if !up && i + 1 < on {
            self.order.swap(i, i + 1);
        }
    }

    pub fn active(&self, available: impl Fn(Item) -> bool) -> Vec<Item> {
        self.enabled().iter().copied().filter(|i| available(*i)).collect()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Layout {
    #[default]
    Rotate,
    Split,
}

impl Layout {
    pub fn parse(s: &str) -> Option<Layout> {
        match s.trim().to_ascii_lowercase().as_str() {
            "rotate" | "bergantian" => Some(Layout::Rotate),
            "split" | "berdampingan" => Some(Layout::Split),
            _ => None,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Layout::Rotate => "rotate",
            Layout::Split => "split",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Decor {
    #[default]
    None,
    Dot,
    Edge,
}

impl Decor {
    pub fn parse(s: &str) -> Option<Decor> {
        match s.trim().to_ascii_lowercase().as_str() {
            "none" | "off" | "tidak" => Some(Decor::None),
            "dot" | "titik" => Some(Decor::Dot),
            "edge" | "glow" | "kilau" => Some(Decor::Edge),
            _ => None,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Decor::None => "none",
            Decor::Dot => "dot",
            Decor::Edge => "edge",
        }
    }
}

pub const SPLIT_MIN_SCALE: u8 = 90;

pub fn split_fits(layout: Layout, scale: u8, active: usize) -> bool {
    layout == Layout::Split && scale >= SPLIT_MIN_SCALE && active >= 2
}

pub fn shown(active: &[Item], idx: usize, split: bool) -> Vec<Item> {
    let n = active.len();
    if n == 0 {
        return Vec::new();
    }
    if split && n >= 2 {
        return vec![active[0], active[1 + idx % (n - 1)]];
    }
    vec![active[idx % n]]
}

pub fn turns(active: usize, split: bool) -> usize {
    if split && active >= 2 {
        active - 1
    } else {
        active
    }
}

pub const ROTATE_MS: u32 = 9000;

pub fn next(i: usize, n: usize) -> usize {
    if n == 0 {
        0
    } else {
        (i + 1) % n
    }
}

pub fn system_text(cpu: Option<u8>, ram: Option<u8>) -> String {
    match (cpu, ram) {
        (Some(c), Some(r)) => format!("CPU {c}% \u{00B7} RAM {r}%"),
        (None, Some(r)) => format!("RAM {r}%"),
        (Some(c), None) => format!("CPU {c}%"),
        (None, None) => String::new(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Daypart {
    Morning,
    Noon,
    Afternoon,
    Night,
}

pub fn daypart(hour: u32) -> Daypart {
    match hour % 24 {
        4..=10 => Daypart::Morning,
        11..=14 => Daypart::Noon,
        15..=17 => Daypart::Afternoon,
        _ => Daypart::Night,
    }
}

pub fn greeting(hour: u32, lang: Lang) -> &'static str {
    match (daypart(hour), lang) {
        (Daypart::Morning, Lang::Id) => "Selamat pagi",
        (Daypart::Noon, Lang::Id) => "Selamat siang",
        (Daypart::Afternoon, Lang::Id) => "Selamat sore",
        (Daypart::Night, Lang::Id) => "Selamat malam",
        (Daypart::Morning, Lang::En) => "Good morning",
        (Daypart::Noon | Daypart::Afternoon, Lang::En) => "Good afternoon",
        (Daypart::Night, Lang::En) => "Good evening",
        (Daypart::Morning, Lang::Es) => "Buenos días",
        (Daypart::Noon | Daypart::Afternoon, Lang::Es) => "Buenas tardes",
        (Daypart::Night, Lang::Es) => "Buenas noches",
    }
}

pub fn greeting_text(custom: &str, name: &str, hour: u32, lang: Lang) -> String {
    let custom = custom.trim();
    if !custom.is_empty() {
        return custom.to_string();
    }
    let name = name.trim();
    let g = greeting(hour, lang);
    if name.is_empty() {
        g.to_string()
    } else {
        format!("{g}, {name}")
    }
}

pub fn clock(hour: u32, minute: u32, h24: bool, lang: Lang) -> String {
    if h24 {
        let sep = if lang == Lang::Id { '.' } else { ':' };
        format!("{hour:02}{sep}{minute:02}")
    } else {
        let h = match hour % 12 {
            0 => 12,
            h => h,
        };
        format!("{h}:{minute:02} {}", crate::i18n::day_period(lang, hour))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_wraps_and_skips_missing_modules() {
        let c = Capsule::default();
        assert_eq!(c.active(|i| i != Item::Battery), vec![Item::Greeting, Item::Clock, Item::Date]);
        assert_eq!(c.active(|_| true).len(), 4);
        assert_eq!(next(2, 3), 0);
        assert_eq!(next(0, 4), 1);
        assert_eq!(next(5, 0), 0);
    }

    #[test]
    fn capsule_parses_round_trips_and_keeps_every_module_once() {
        let c = Capsule::parse("clock, jam ,system,bogus,battery");
        assert_eq!(c.enabled(), &[Item::Clock, Item::System, Item::Battery]);
        assert_eq!(c.text(), "clock,system,battery");
        assert_eq!(Capsule::parse(&c.text()), c);
        let mut seen: Vec<&str> = c.order.iter().map(|i| i.key()).collect();
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), MODULES);
        let empty = Capsule::parse("");
        assert_eq!(empty.on, 0);
        assert!(empty.enabled().is_empty());
        assert_eq!(Capsule::parse(&Capsule::default().text()), Capsule::default());
    }

    #[test]
    fn capsule_toggle_and_shift_keep_enabled_first() {
        let mut c = Capsule::default();
        c.toggle(1);
        assert_eq!(c.enabled(), &[Item::Greeting, Item::Date, Item::Battery]);
        assert_eq!(c.order[3], Item::Clock);
        c.toggle(5);
        assert_eq!(c.enabled(), &[Item::Greeting, Item::Date, Item::Battery, Item::Wifi]);
        c.shift(3, true);
        assert_eq!(c.enabled(), &[Item::Greeting, Item::Date, Item::Wifi, Item::Battery]);
        c.shift(0, true);
        c.shift(3, false);
        c.shift(5, true);
        assert_eq!(c.enabled(), &[Item::Greeting, Item::Date, Item::Wifi, Item::Battery]);
        for _ in 0..4 {
            c.toggle(0);
        }
        assert_eq!(c.on, 0);
        c.toggle(9);
        assert_eq!(c.on, 0);
    }

    #[test]
    fn split_keeps_the_first_module_and_rotates_the_rest() {
        let a = [Item::Clock, Item::Battery, Item::System];
        assert_eq!(shown(&a, 0, true), vec![Item::Clock, Item::Battery]);
        assert_eq!(shown(&a, 1, true), vec![Item::Clock, Item::System]);
        assert_eq!(shown(&a, 2, true), vec![Item::Clock, Item::Battery]);
        assert_eq!(shown(&a, 2, false), vec![Item::System]);
        assert_eq!(shown(&a[..1], 3, true), vec![Item::Clock]);
        assert!(shown(&[], 0, true).is_empty());
        assert_eq!((turns(3, true), turns(2, true), turns(3, false), turns(0, true)), (2, 1, 3, 0));
        assert!(split_fits(Layout::Split, 100, 2));
        assert!(!split_fits(Layout::Split, 85, 3));
        assert!(!split_fits(Layout::Split, 120, 1));
        assert!(!split_fits(Layout::Rotate, 120, 3));
        assert_eq!(system_text(Some(12), Some(48)), "CPU 12% \u{00B7} RAM 48%");
        assert_eq!(system_text(None, None), "");
        assert_eq!((Layout::parse("berdampingan"), Decor::parse("kilau")), (Some(Layout::Split), Some(Decor::Edge)));
    }

    #[test]
    fn dayparts_follow_local_hours() {
        assert_eq!(daypart(4), Daypart::Morning);
        assert_eq!(daypart(10), Daypart::Morning);
        assert_eq!(daypart(11), Daypart::Noon);
        assert_eq!(daypart(15), Daypart::Afternoon);
        assert_eq!(daypart(18), Daypart::Night);
        assert_eq!(daypart(3), Daypart::Night);
        assert_eq!(greeting(22, Lang::Id), "Selamat malam");
        assert_eq!(greeting(13, Lang::En), "Good afternoon");
    }

    #[test]
    fn custom_text_wins_and_name_is_optional() {
        assert_eq!(greeting_text("", "Kreativa", 22, Lang::Id), "Selamat malam, Kreativa");
        assert_eq!(greeting_text("  ", "", 8, Lang::En), "Good morning");
        assert_eq!(greeting_text("Semangat ngoding!", "Kreativa", 8, Lang::Id), "Semangat ngoding!");
    }

    #[test]
    fn clock_formats() {
        assert_eq!(clock(22, 5, true, Lang::Id), "22.05");
        assert_eq!(clock(9, 41, true, Lang::En), "09:41");
        assert_eq!(clock(0, 7, false, Lang::En), "12:07 AM");
        assert_eq!(clock(13, 30, false, Lang::Id), "1:30 PM");
    }
}
