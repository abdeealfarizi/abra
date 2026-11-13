//! Notch diam "sapaan & info": isi pil berganti pelan antara sapaan, jam, tanggal, dan baterai. Tiap item = ikon di kiri
//! dan nilainya di kanan. Murni: jam dan bahasa dioper dari luar.

use crate::i18n::Lang;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    Greeting,
    Clock,
    Date,
    Battery,
}

/// Selang pergantian item (ms).
pub const ROTATE_MS: u32 = 9000;

/// Item yang ditampilkan, berurutan. Baterai hanya bila perangkat punya baterai.
pub fn items(has_battery: bool) -> Vec<Item> {
    let mut v = vec![Item::Greeting, Item::Clock, Item::Date];
    if has_battery {
        v.push(Item::Battery);
    }
    v
}

/// Indeks berikutnya (melingkar).
pub fn next(i: usize, n: usize) -> usize {
    if n == 0 {
        0
    } else {
        (i + 1) % n
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Daypart {
    Morning,
    Noon,
    Afternoon,
    Night,
}

/// Bagian hari dari jam lokal: pagi 04–10, siang 11–14, sore 15–17, malam 18–03.
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
    }
}

/// Teks sapaan: teks kustom dipakai apa adanya; kosong → sapaan menurut jam + nama (nama kosong → sapaan saja).
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

/// Jam "22.41" (Indonesia, 24 jam), "22:41" (Inggris 24 jam), atau "10:41 PM" (12 jam).
pub fn clock(hour: u32, minute: u32, h24: bool, lang: Lang) -> String {
    if h24 {
        let sep = if lang == Lang::Id { '.' } else { ':' };
        format!("{hour:02}{sep}{minute:02}")
    } else {
        let h = match hour % 12 {
            0 => 12,
            h => h,
        };
        format!("{h}:{minute:02} {}", if hour < 12 { "AM" } else { "PM" })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_skips_battery_on_desktops_and_wraps() {
        assert_eq!(items(false), vec![Item::Greeting, Item::Clock, Item::Date]);
        assert_eq!(items(true).len(), 4);
        assert_eq!(next(2, 3), 0);
        assert_eq!(next(0, 4), 1);
        assert_eq!(next(5, 0), 0);
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
        assert_eq!(greeting_text("", "Rani", 22, Lang::Id), "Selamat malam, Rani");
        assert_eq!(greeting_text("  ", "", 8, Lang::En), "Good morning");
        assert_eq!(greeting_text("Semangat ngoding!", "Rani", 8, Lang::Id), "Semangat ngoding!");
    }

    #[test]
    fn clock_formats() {
        assert_eq!(clock(22, 5, true, Lang::Id), "22.05");
        assert_eq!(clock(9, 41, true, Lang::En), "09:41");
        assert_eq!(clock(0, 7, false, Lang::En), "12:07 AM");
        assert_eq!(clock(13, 30, false, Lang::Id), "1:30 PM");
    }
}
