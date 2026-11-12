//! Katalog modul Control Center dan tata letak grid 4 kolom (murni, diuji).

pub const COLS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Wifi,
    Bluetooth,
    Hotspot,
    /// Padanan AirDrop di Windows: Berbagi Terdekat (Nearby Sharing).
    Share,
    Airplane,
    NowPlaying,
    Stage,
    Mirror,
    Focus,
    DarkMode,
    Screenshot,
    Display,
    Sound,
    Battery,
    Lock,
    Mission,
    Apps,
    Calculator,
    Timer,
}

pub const ALL: [Kind; 19] = [
    Kind::Wifi,
    Kind::Bluetooth,
    Kind::Hotspot,
    Kind::Share,
    Kind::Airplane,
    Kind::NowPlaying,
    Kind::Stage,
    Kind::Mirror,
    Kind::Focus,
    Kind::DarkMode,
    Kind::Screenshot,
    Kind::Display,
    Kind::Sound,
    Kind::Battery,
    Kind::Lock,
    Kind::Mission,
    Kind::Apps,
    Kind::Calculator,
    Kind::Timer,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Connectivity,
    Display,
    Sound,
    Media,
    Battery,
    Utilities,
}

impl Category {
    pub const ALL: [Category; 6] = [Category::Connectivity, Category::Display, Category::Sound, Category::Media, Category::Battery, Category::Utilities];

    /// Kunci terjemahan (Indonesia).
    pub fn title(self) -> &'static str {
        match self {
            Category::Connectivity => "Konektivitas",
            Category::Display => "Layar & Kecerahan",
            Category::Sound => "Suara",
            Category::Media => "Sedang Diputar",
            Category::Battery => "Baterai",
            Category::Utilities => "Utilitas",
        }
    }
}

impl Kind {
    pub fn key(self) -> &'static str {
        match self {
            Kind::Wifi => "wifi",
            Kind::Bluetooth => "bluetooth",
            Kind::Hotspot => "hotspot",
            Kind::Share => "share",
            Kind::Airplane => "airplane",
            Kind::NowPlaying => "nowplaying",
            Kind::Stage => "stage",
            Kind::Mirror => "mirror",
            Kind::Focus => "focus",
            Kind::DarkMode => "dark",
            Kind::Screenshot => "screenshot",
            Kind::Display => "display",
            Kind::Sound => "sound",
            Kind::Battery => "battery",
            Kind::Lock => "lock",
            Kind::Mission => "mission",
            Kind::Apps => "apps",
            Kind::Calculator => "calculator",
            Kind::Timer => "timer",
        }
    }

    pub fn parse(s: &str) -> Option<Kind> {
        ALL.into_iter().find(|k| k.key() == s.trim())
    }

    /// Ukuran (kolom, baris).
    pub fn size(self) -> (usize, usize) {
        match self {
            Kind::Wifi | Kind::Bluetooth | Kind::Hotspot | Kind::Share | Kind::Focus | Kind::Battery => (2, 1),
            Kind::NowPlaying => (2, 2),
            Kind::Display | Kind::Sound => (4, 1),
            _ => (1, 1),
        }
    }

    pub fn category(self) -> Category {
        match self {
            Kind::Wifi | Kind::Bluetooth | Kind::Hotspot | Kind::Share | Kind::Airplane | Kind::Mirror => Category::Connectivity,
            Kind::Display | Kind::DarkMode => Category::Display,
            Kind::Sound => Category::Sound,
            Kind::NowPlaying => Category::Media,
            Kind::Battery => Category::Battery,
            _ => Category::Utilities,
        }
    }

    /// Judul (kunci terjemahan Indonesia).
    pub fn title(self) -> &'static str {
        match self {
            Kind::Wifi => "Wi-Fi",
            Kind::Bluetooth => "Bluetooth",
            Kind::Hotspot => "Hotspot",
            Kind::Share => "Berbagi Terdekat",
            Kind::Airplane => "Mode Pesawat",
            Kind::NowPlaying => "Sedang Diputar",
            Kind::Stage => "Stage Manager",
            Kind::Mirror => "Pencerminan Layar",
            Kind::Focus => "Fokus",
            Kind::DarkMode => "Mode Gelap",
            Kind::Screenshot => "Tangkapan Layar",
            Kind::Display => "Layar",
            Kind::Sound => "Suara",
            Kind::Battery => "Baterai",
            Kind::Lock => "Kunci Layar",
            Kind::Mission => "Mission Control",
            Kind::Apps => "Aplikasi",
            Kind::Calculator => "Kalkulator",
            Kind::Timer => "Timer",
        }
    }
}

/// Ukuran modul yang bisa dipilih pengguna (klik kanan di mode Edit): Kecil 1×1, Sedang 2×1, Besar 2×2.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Size {
    S,
    M,
    L,
}

impl Size {
    pub fn key(self) -> &'static str {
        match self {
            Size::S => "s",
            Size::M => "m",
            Size::L => "l",
        }
    }

    pub fn parse(s: &str) -> Option<Size> {
        match s.trim() {
            "s" => Some(Size::S),
            "m" => Some(Size::M),
            "l" => Some(Size::L),
            _ => None,
        }
    }
}

impl Kind {
    /// Slider dan Sedang Diputar punya ukuran tetap.
    pub fn resizable(self) -> bool {
        !matches!(self, Kind::Display | Kind::Sound | Kind::NowPlaying)
    }

    pub fn default_size(self) -> Size {
        match self.size() {
            (2, 1) => Size::M,
            _ => Size::S,
        }
    }

    /// Kolom × baris untuk ukuran pilihan.
    pub fn dims(self, s: Size) -> (usize, usize) {
        if !self.resizable() {
            return self.size();
        }
        match s {
            Size::S => (1, 1),
            Size::M => (2, 1),
            Size::L => (2, 2),
        }
    }
}

pub fn default_layout() -> Vec<Kind> {
    vec![Kind::Wifi, Kind::NowPlaying, Kind::Bluetooth, Kind::Share, Kind::Stage, Kind::Mission, Kind::DarkMode, Kind::Screenshot, Kind::Focus, Kind::Sound, Kind::Display]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placed {
    pub kind: Kind,
    pub col: usize,
    pub row: usize,
    pub w: usize,
    pub h: usize,
}

/// Tata letak mengalir: tiap modul menempati spot bebas pertama (baris demi baris) yang muat.
/// Mengembalikan modul yang tertata dan jumlah baris.
pub fn layout(kinds: &[Kind]) -> (Vec<Placed>, usize) {
    layout_sized(kinds, &|k| k.default_size())
}

/// Seperti `layout` tetapi dengan ukuran pilihan per modul.
pub fn layout_sized(kinds: &[Kind], size_of: &dyn Fn(Kind) -> Size) -> (Vec<Placed>, usize) {
    let mut occ: Vec<[bool; COLS]> = Vec::new();
    let mut out = Vec::new();
    for &k in kinds {
        let (w, h) = k.dims(size_of(k));
        let mut row = 0;
        'find: loop {
            while occ.len() < row + h {
                occ.push([false; COLS]);
            }
            for col in 0..=(COLS - w) {
                let free = (0..h).all(|dy| (0..w).all(|dx| !occ[row + dy][col + dx]));
                if free {
                    for dy in 0..h {
                        for dx in 0..w {
                            occ[row + dy][col + dx] = true;
                        }
                    }
                    out.push(Placed { kind: k, col, row, w, h });
                    break 'find;
                }
            }
            row += 1;
        }
    }
    (out, occ.len())
}

pub fn to_text(kinds: &[Kind]) -> String {
    kinds.iter().map(|k| k.key()).collect::<Vec<_>>().join(",")
}

/// Baca daftar modul; yang tidak dikenal dan duplikat diabaikan. Kosong/rusak ⇒ bawaan.
pub fn from_text(s: &str) -> Vec<Kind> {
    let mut v: Vec<Kind> = Vec::new();
    for part in s.split(',') {
        if let Some(k) = Kind::parse(part.split(':').next().unwrap_or("")) {
            if !v.contains(&k) {
                v.push(k);
            }
        }
    }
    if v.is_empty() { default_layout() } else { v }
}

/// Seperti `to_text` tetapi menyertakan ukuran yang berbeda dari bawaan ("wifi:l").
pub fn to_text_sized(kinds: &[Kind], sizes: &std::collections::HashMap<Kind, Size>) -> String {
    kinds
        .iter()
        .map(|k| match sizes.get(k) {
            Some(s) if *s != k.default_size() && k.resizable() => format!("{}:{}", k.key(), s.key()),
            _ => k.key().to_string(),
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// Baca daftar modul beserta ukurannya.
pub fn from_text_sized(s: &str) -> (Vec<Kind>, std::collections::HashMap<Kind, Size>) {
    let kinds = from_text(s);
    let mut sizes = std::collections::HashMap::new();
    for part in s.split(',') {
        let mut it = part.split(':');
        if let (Some(k), Some(sz)) = (it.next().and_then(Kind::parse), it.next().and_then(Size::parse)) {
            if k.resizable() {
                sizes.insert(k, sz);
            }
        }
    }
    (kinds, sizes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_layout_matches_reference_rows() {
        let (p, rows) = layout(&default_layout());
        let at = |k: Kind| p.iter().find(|x| x.kind == k).copied().unwrap();
        // baris 0: Wi-Fi (2×1) di kiri, Now Playing (2×2) di kanan
        assert_eq!((at(Kind::Wifi).col, at(Kind::Wifi).row), (0, 0));
        assert_eq!((at(Kind::NowPlaying).col, at(Kind::NowPlaying).row), (2, 0));
        assert_eq!((at(Kind::Bluetooth).col, at(Kind::Bluetooth).row), (0, 1));
        // Berbagi Terdekat (2×1) di baris 2 kiri, dua ubin 1×1 di sebelahnya
        assert_eq!((at(Kind::Share).col, at(Kind::Share).row), (0, 2));
        assert_eq!((at(Kind::Stage).col, at(Kind::Stage).row), (2, 2));
        assert_eq!((at(Kind::Mission).col, at(Kind::Mission).row), (3, 2));
        // slider selalu menempati satu baris penuh
        assert_eq!(at(Kind::Display).w, 4);
        assert_eq!(at(Kind::Display).row, at(Kind::Sound).row + 1); // Tahoe: Sound di atas Display
        assert!(rows >= 6);
    }

    #[test]
    fn sizes_change_dims_and_round_trip() {
        use std::collections::HashMap;
        assert_eq!(Kind::Focus.dims(Size::L), (2, 2));
        assert_eq!(Kind::Stage.dims(Size::M), (2, 1));
        assert_eq!(Kind::Sound.dims(Size::S), (4, 1), "slider tidak bisa diubah");
        let mut sizes = HashMap::new();
        sizes.insert(Kind::Focus, Size::L);
        sizes.insert(Kind::Stage, Size::M);
        let kinds = vec![Kind::Wifi, Kind::Focus, Kind::Stage];
        let text = to_text_sized(&kinds, &sizes);
        assert_eq!(text, "wifi,focus:l,stage:m");
        let (k2, s2) = from_text_sized(&text);
        assert_eq!(k2, kinds);
        assert_eq!(s2.get(&Kind::Focus), Some(&Size::L));
        let (p, _) = layout_sized(&kinds, &|k| *sizes.get(&k).unwrap_or(&k.default_size()));
        assert_eq!(p.iter().find(|x| x.kind == Kind::Focus).map(|x| (x.w, x.h)), Some((2, 2)));
    }

    #[test]
    fn no_overlap_and_inside_grid() {
        let (p, _) = layout(&ALL);
        for (i, a) in p.iter().enumerate() {
            assert!(a.col + a.w <= COLS);
            for b in &p[i + 1..] {
                let overlap = a.col < b.col + b.w && b.col < a.col + a.w && a.row < b.row + b.h && b.row < a.row + a.h;
                assert!(!overlap, "{:?} tumpang tindih {:?}", a.kind, b.kind);
            }
        }
        assert_eq!(p.len(), ALL.len());
    }

    #[test]
    fn text_round_trip_and_fallback() {
        let l = default_layout();
        assert_eq!(from_text(&to_text(&l)), l);
        assert_eq!(from_text(""), default_layout());
        assert_eq!(from_text("wifi,wifi,nope,sound"), vec![Kind::Wifi, Kind::Sound]);
    }
}
