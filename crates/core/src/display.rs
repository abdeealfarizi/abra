#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Area {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Area {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Area {
        Area { x, y, w, h }
    }

    pub fn right(&self) -> i32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }

    pub fn size(&self) -> (i32, i32) {
        (self.w, self.h)
    }

    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && px < self.right() && py >= self.y && py < self.bottom()
    }

    pub fn clamp_x(&self, x: i32, w: i32, pad: i32) -> i32 {
        x.clamp(self.x + pad, (self.right() - w - pad).max(self.x + pad))
    }

    pub fn clamp_y(&self, y: i32, h: i32, pad: i32) -> i32 {
        y.clamp(self.y + pad, (self.bottom() - h - pad).max(self.y + pad))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Monitor {
    pub area: Area,
    pub work: Area,
    pub dpi: u32,
    pub primary: bool,
}

impl Monitor {
    pub fn scale(&self) -> f32 {
        self.dpi.max(96) as f32 / 96.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Screen {
    #[default]
    Primary,
    Active,
    All,
    Nth(u8),
}

pub const MAX_SCREENS: u8 = 4;

impl Screen {
    pub fn parse(s: &str) -> Option<Screen> {
        match s.trim().to_ascii_lowercase().as_str() {
            "primary" | "utama" | "main" => Some(Screen::Primary),
            "active" | "cursor" | "kursor" | "follow" => Some(Screen::Active),
            "all" | "semua" | "todos" => Some(Screen::All),
            v => v.parse::<u8>().ok().filter(|n| (1..=MAX_SCREENS).contains(n)).map(Screen::Nth),
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Screen::Primary => "primary",
            Screen::Active => "active",
            Screen::All => "all",
            Screen::Nth(1) => "1",
            Screen::Nth(2) => "2",
            Screen::Nth(3) => "3",
            Screen::Nth(_) => "4",
        }
    }
}

pub fn ordered(mut list: Vec<Monitor>) -> Vec<Monitor> {
    list.sort_by_key(|m| (m.area.x, m.area.y));
    list
}

pub fn primary(list: &[Monitor]) -> Option<Monitor> {
    list.iter().copied().find(|m| m.primary).or_else(|| list.first().copied())
}

pub fn resolve(screen: Screen, list: &[Monitor]) -> Option<Monitor> {
    match screen {
        Screen::Primary | Screen::Active | Screen::All => primary(list),
        Screen::Nth(n) => ordered(list.to_vec()).get(n as usize - 1).copied().or_else(|| primary(list)),
    }
}

pub fn at_point(list: &[Monitor], x: i32, y: i32) -> Option<Monitor> {
    list.iter().copied().find(|m| m.area.contains(x, y)).or_else(|| {
        list.iter()
            .copied()
            .min_by_key(|m| {
                let dx = (m.area.x - x).max(0).max(x - m.area.right());
                let dy = (m.area.y - y).max(0).max(y - m.area.bottom());
                dx as i64 * dx as i64 + dy as i64 * dy as i64
            })
    })
}

pub fn parse_fake(spec: &str) -> Vec<Monitor> {
    spec.split(';')
        .filter_map(|part| {
            let (rect, dpi) = match part.split_once('@') {
                Some((r, d)) => (r, d.trim().parse::<u32>().ok()?),
                None => (part, 96),
            };
            let n: Vec<i32> = rect.split(',').map(|v| v.trim().parse::<i32>()).collect::<Result<_, _>>().ok()?;
            let [x, y, w, h] = n[..] else { return None };
            (w > 0 && h > 0).then_some(Area::new(x, y, w, h)).map(|a| (a, dpi.clamp(96, 480)))
        })
        .enumerate()
        .map(|(i, (area, dpi))| Monitor { area, work: area, dpi, primary: i == 0 })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mon(x: i32, y: i32, w: i32, h: i32, primary: bool) -> Monitor {
        Monitor { area: Area::new(x, y, w, h), work: Area::new(x, y, w, h), dpi: 96, primary }
    }

    #[test]
    fn screen_keys_round_trip() {
        for s in [Screen::Primary, Screen::Active, Screen::All, Screen::Nth(1), Screen::Nth(2), Screen::Nth(3), Screen::Nth(4)] {
            assert_eq!(Screen::parse(s.key()), Some(s));
        }
        assert_eq!(Screen::parse("utama"), Some(Screen::Primary));
        assert_eq!(Screen::parse("0"), None);
        assert_eq!(Screen::parse("9"), None);
    }

    #[test]
    fn numbering_goes_left_to_right_then_top_to_bottom() {
        let list = vec![mon(0, 0, 1920, 1080, true), mon(-1280, 0, 1280, 1024, false), mon(0, -1080, 1920, 1080, false)];
        let o = ordered(list);
        assert_eq!(o[0].area.x, -1280);
        assert_eq!((o[1].area.x, o[1].area.y), (0, -1080));
        assert_eq!((o[2].area.x, o[2].area.y), (0, 0));
    }

    #[test]
    fn missing_screen_falls_back_to_primary() {
        let list = vec![mon(1920, 0, 1920, 1080, false), mon(0, 0, 1920, 1080, true)];
        assert_eq!(resolve(Screen::Nth(2), &list).unwrap().area.x, 1920);
        assert_eq!(resolve(Screen::Nth(3), &list).unwrap().area.x, 0);
        assert_eq!(resolve(Screen::Primary, &list).unwrap().area.x, 0);
        assert!(resolve(Screen::Primary, &[]).is_none());
    }

    #[test]
    fn point_lookup_uses_nearest_screen_when_outside() {
        let list = vec![mon(0, 0, 1920, 1080, true), mon(1920, 0, 1280, 720, false)];
        assert_eq!(at_point(&list, 2000, 100).unwrap().area.x, 1920);
        assert_eq!(at_point(&list, 2500, 900).unwrap().area.x, 1920);
        assert_eq!(at_point(&list, -50, 500).unwrap().area.x, 0);
    }

    #[test]
    fn fake_spec_parses_and_marks_first_primary() {
        let list = parse_fake("0,0,960,1080;960,0,960,1080@144; bad ;0,0,0,5");
        assert_eq!(list.len(), 2);
        assert!(list[0].primary && !list[1].primary);
        assert_eq!(list[1].dpi, 144);
        assert_eq!(list[1].area, Area::new(960, 0, 960, 1080));
    }

    #[test]
    fn common_setups_resolve_and_clamp_inside_each_screen() {
        let setups = [
            "0,0,1920,1080;1920,0,2560,1440@144",
            "0,0,2560,1600@240;-1920,0,1920,1080",
            "0,0,1920,1080@120;1920,-420,1080,1920",
            "0,0,1366,768;0,-1080,1920,1080@144;1366,0,1280,1024@120",
            "0,0,3840,2160@288",
        ];
        for spec in setups {
            let list = parse_fake(spec);
            assert!(!list.is_empty(), "{spec}");
            for n in 1..=MAX_SCREENS {
                let m = resolve(Screen::Nth(n), &list).unwrap();
                assert!(list.contains(&m), "{spec} layar {n}");
                let (w, h) = ((600.0 * m.scale()) as i32, (226.0 * m.scale()) as i32);
                let x = m.area.clamp_x(m.area.x + m.area.w / 2 - w / 2, w, 0);
                let y = m.area.clamp_y(m.area.y, h, 0);
                assert!(x >= m.area.x && y >= m.area.y, "{spec}");
                assert!(w > m.area.w || x + w <= m.area.right(), "{spec}");
                assert_eq!(at_point(&list, m.area.x + m.area.w / 2, m.area.y + m.area.h / 2), Some(m));
            }
        }
        let portrait = parse_fake("0,0,1920,1080;1920,-420,1080,1920@144");
        let p = resolve(Screen::Nth(2), &portrait).unwrap();
        assert!(p.area.h > p.area.w && p.area.y < 0 && p.scale() == 1.5);
        let tiny = Area::new(0, 0, 300, 200);
        assert_eq!((tiny.clamp_x(50, 600, 8), tiny.clamp_y(50, 400, 8)), (8, 8), "jendela lebih besar dari layar tetap menempel di kiri atas");
        assert_eq!(parse_fake("0,0,800,600@20")[0].dpi, 96);
        assert_eq!(parse_fake("0,0,800,600@9999")[0].dpi, 480);
    }

    #[test]
    fn clamping_stays_inside_the_area() {
        let a = Area::new(1920, 0, 1280, 720);
        assert_eq!(a.clamp_x(1900, 200, 4), 1924);
        assert_eq!(a.clamp_x(3150, 200, 4), 2996);
        assert_eq!(a.clamp_y(700, 100, 4), 616);
    }
}
