//! Pengaturan pengguna ABRA: model murni + format berkas teks sederhana (`kunci=nilai`) + turunan tampilan kaca.
//! Tanpa Win32 sehingga seluruh pemetaan bisa diuji.

use crate::i18n::Lang;
use crate::visibility::policy::Mode;

/// Perilaku permukaan di tepi layar (dock atau notch).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Behavior {
    /// Tetap: mencadangkan ruang, jendela app berhenti di tepinya (seperti Dock macOS).
    Fixed,
    /// Menimpa: mengambang di atas jendela, tidak mencadangkan ruang.
    Overlay,
    /// Otomatis sembunyi saat ada jendela yang bersinggungan.
    AutoHide,
    /// Selalu tersembunyi; muncul bila tepi layar disentuh.
    Peek,
}

impl Behavior {
    pub fn parse(s: &str) -> Option<Behavior> {
        match s.trim().to_ascii_lowercase().as_str() {
            "fixed" | "tetap" | "permanent" => Some(Behavior::Fixed),
            "overlay" | "menimpa" => Some(Behavior::Overlay),
            "autohide" | "auto-hide" | "auto" => Some(Behavior::AutoHide),
            "peek" | "hidden" => Some(Behavior::Peek),
            _ => None,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Behavior::Fixed => "fixed",
            Behavior::Overlay => "overlay",
            Behavior::AutoHide => "autohide",
            Behavior::Peek => "peek",
        }
    }

    /// Kebijakan tampil/sembunyi yang dipakai `visibility::policy`.
    pub fn mode(self) -> Mode {
        match self {
            Behavior::Fixed | Behavior::Overlay => Mode::Always,
            // Seperti macOS "Sembunyikan dan tampilkan Dock secara otomatis": selalu tersembunyi, muncul saat kursor
            // menyentuh tepi layar, hilang lagi saat kursor pergi.
            Behavior::AutoHide => Mode::OnDemand,
            // Pintar: hanya menyingkir bila ada jendela yang menimpa permukaan.
            Behavior::Peek => Mode::AutoHide,
        }
    }

    /// Apakah ruang layar dicadangkan (jendela app tidak boleh berada di bawah permukaan).
    pub fn reserves(self) -> bool {
        self == Behavior::Fixed
    }

    pub const ALL: [Behavior; 4] = [Behavior::Fixed, Behavior::Overlay, Behavior::AutoHide, Behavior::Peek];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Theme {
    Auto,
    Light,
    Dark,
}

impl Theme {
    pub fn parse(s: &str) -> Option<Theme> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" | "system" => Some(Theme::Auto),
            "light" | "terang" => Some(Theme::Light),
            "dark" | "gelap" => Some(Theme::Dark),
            _ => None,
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Theme::Auto => "auto",
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }
    pub const ALL: [Theme; 3] = [Theme::Auto, Theme::Light, Theme::Dark];
}

/// Gaya blur kaca.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlurStyle {
    /// Buram lembut, saturasi sedang (bawaan, mirip Liquid Glass).
    Frosted,
    /// Bening: blur tipis, warna latar tetap jelas.
    Clear,
    /// Hidup: blur dengan saturasi tinggi.
    Vivid,
}

impl BlurStyle {
    pub fn parse(s: &str) -> Option<BlurStyle> {
        match s.trim().to_ascii_lowercase().as_str() {
            "frosted" | "buram" => Some(BlurStyle::Frosted),
            "clear" | "bening" => Some(BlurStyle::Clear),
            "vivid" | "hidup" => Some(BlurStyle::Vivid),
            _ => None,
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            BlurStyle::Frosted => "frosted",
            BlurStyle::Clear => "clear",
            BlurStyle::Vivid => "vivid",
        }
    }
    pub const ALL: [BlurStyle; 3] = [BlurStyle::Frosted, BlurStyle::Clear, BlurStyle::Vivid];
}

/// Material permukaan (dock/notch). Dua permukaan memakai pilihan yang sama.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Material {
    /// Blur: kabut tebal (blur kuat, tint tipis, tepi cahaya halus) seperti contoh Tahoe; mengikuti tema.
    Blur,
    /// Liquid Glass: kaca berbias, mengikuti tema.
    Glass,
    /// Hitam buram seperti notch MacBook: hitam pekat dengan blur halus.
    Black,
}

impl Material {
    pub fn parse(s: &str) -> Option<Material> {
        match s.trim().to_ascii_lowercase().as_str() {
            "blur" | "buram" | "frosted" => Some(Material::Blur),
            "glass" | "liquid" | "liquidglass" => Some(Material::Glass),
            "black" | "hitam" => Some(Material::Black),
            // Material "transparan" sudah dihapus (terlalu mirip Liquid Glass); config lama dipetakan ke Liquid Glass.
            "clear" | "transparan" | "transparent" => Some(Material::Glass),
            _ => None,
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Material::Blur => "blur",
            Material::Glass => "glass",
            Material::Black => "black",
        }
    }
    pub const ALL: [Material; 3] = [Material::Blur, Material::Glass, Material::Black];
}

/// Bahan latar menu bar bila latar aktif (`menubar_glass`): kaca mengikuti tema, atau terang/gelap yang lebih pekat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarStyle {
    Light,
    Dark,
}

impl BarStyle {
    pub fn parse(s: &str) -> Option<BarStyle> {
        match s.trim().to_ascii_lowercase().as_str() {
            "light" | "terang" => Some(BarStyle::Light),
            "dark" | "gelap" => Some(BarStyle::Dark),
            _ => None,
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            BarStyle::Light => "light",
            BarStyle::Dark => "dark",
        }
    }
}

/// Warna aksen sistem ala Tahoe: Biru, Ungu, Merah Muda, Merah, Oranye, Kuning, Hijau, Grafit.
/// Nilai `accent` khusus: Multiwarna (ikut warna aksen Windows), seperti Multicolor di macOS.
pub const ACCENT_MULTI: u8 = 255;

pub const ACCENTS: [[u8; 3]; 8] = [[10, 132, 255], [175, 82, 222], [255, 55, 95], [255, 69, 58], [255, 149, 10], [255, 204, 0], [48, 209, 88], [142, 142, 147]];

/// Gaya ikon ala Tahoe (Pengaturan Sistem > Tampilan > Gaya ikon dan widget).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconStyle {
    Default,
    Dark,
    Clear,
    Tinted,
}

impl IconStyle {
    pub fn parse(s: &str) -> Option<IconStyle> {
        match s.trim().to_ascii_lowercase().as_str() {
            "default" | "bawaan" => Some(IconStyle::Default),
            "dark" | "gelap" => Some(IconStyle::Dark),
            "clear" | "bening" => Some(IconStyle::Clear),
            "tinted" | "berwarna" => Some(IconStyle::Tinted),
            _ => None,
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            IconStyle::Default => "default",
            IconStyle::Dark => "dark",
            IconStyle::Clear => "clear",
            IconStyle::Tinted => "tinted",
        }
    }
}

/// Faktor skala radius sudut dari pengaturan 0..=100 (50 = bawaan, 0 = hampir kotak, 100 = bulat penuh).
pub fn round_factor(round: u8) -> f32 {
    0.2 + 1.6 * (round.min(100) as f32 / 100.0)
}

/// Aksi hot corner (kursor menyentuh sudut layar).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotAction {
    None,
    Mission,
    Launchpad,
    Desktop,
    Stage,
    Notifications,
    Lock,
}

impl HotAction {
    pub const ALL: [HotAction; 7] =
        [HotAction::None, HotAction::Mission, HotAction::Launchpad, HotAction::Desktop, HotAction::Stage, HotAction::Notifications, HotAction::Lock];

    pub fn parse(s: &str) -> Option<HotAction> {
        Self::ALL.into_iter().find(|a| a.key() == s.trim().to_ascii_lowercase())
    }
    pub fn key(self) -> &'static str {
        match self {
            HotAction::None => "none",
            HotAction::Mission => "mission",
            HotAction::Launchpad => "launchpad",
            HotAction::Desktop => "desktop",
            HotAction::Stage => "stage",
            HotAction::Notifications => "notifications",
            HotAction::Lock => "lock",
        }
    }
    /// Aksi berikutnya (melingkar) untuk kontrol yang berputar saat diklik.
    pub fn next(self) -> HotAction {
        let i = Self::ALL.iter().position(|a| *a == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }
}

/// Teks pendek yang tetap `Copy` (Settings disalin ke banyak tempat): UTF-8 paling banyak 96 byte, satu baris.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ShortText {
    len: u8,
    buf: [u8; 96],
}

impl ShortText {
    pub const EMPTY: ShortText = ShortText { len: 0, buf: [0; 96] };

    /// Rapikan (buang karakter kendali dan spasi di tepi) lalu potong di batas karakter bila lebih dari 96 byte.
    pub fn new(s: &str) -> ShortText {
        let clean: String = s.chars().filter(|c| !c.is_control()).collect();
        let clean = clean.trim();
        let mut end = clean.len().min(96);
        while !clean.is_char_boundary(end) {
            end -= 1;
        }
        let mut buf = [0u8; 96];
        buf[..end].copy_from_slice(&clean.as_bytes()[..end]);
        ShortText { len: end as u8, buf }
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.buf[..self.len as usize]).unwrap_or("")
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl Default for ShortText {
    fn default() -> Self {
        ShortText::EMPTY
    }
}

impl std::fmt::Debug for ShortText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    /// Mode penuh: dock aktif ⇒ taskbar Windows disembunyikan permanen.
    pub takeover: bool,
    pub dock: Behavior,
    pub notch: Behavior,
    /// 0..=100: seberapa kuat blur.
    pub blur: u8,
    pub style: BlurStyle,
    /// 0..=100: seberapa tembus pandang kaca.
    pub transparency: u8,
    pub theme: Theme,
    pub lang: Lang,
    pub dock_material: Material,
    pub dock_round: u8,
    pub notch_material: Material,
    pub notch_round: u8,
    /// Jalankan ABRA saat login Windows.
    pub autostart: bool,
    /// Ukuran ikon dock (dp), 32..=72.
    pub dock_size: u8,
    /// Jarak antar ikon (dp), 0..=24.
    pub dock_gap: u8,
    /// Zoom maksimum saat hover, persen 100..=220.
    pub dock_zoom: u8,
    /// Jarak dock dari tepi bawah layar (dp), 0..=60.
    pub dock_margin: u8,
    /// Animasi jendela ala macOS (minimize, tutup, buka dari dock).
    pub anim: bool,
    /// Refraksi tepi pada kaca dock (Liquid Glass).
    pub refraction: bool,
    /// Dock dan notch bisa dinonaktifkan sendiri-sendiri.
    pub dock_enabled: bool,
    pub notch_enabled: bool,
    /// Notch terbuka otomatis saat kursor diarahkan ke pil (tanpa klik).
    pub notch_hover_open: bool,
    /// Bayangan halus di bawah jendela yang aktif agar tampak melayang.
    pub win_shadow: bool,
    /// Menu bar ala macOS di atas layar (mati bawaan; menempati area notch bila notch aktif).
    pub menubar: bool,
    /// Jam 24 jam (bawaan) atau 12 jam AM/PM.
    pub clock_24h: bool,
    /// Latar kaca di belakang menu bar (mati bawaan: transparan seperti macOS Tahoe).
    pub menubar_glass: bool,
    /// Latar menu bar memakai material global (Blur/Liquid Glass/Hitam) sebagai strip, bukan transparan atau warna rata.
    pub menubar_material: bool,
    /// Bahan latar menu bar saat `menubar_glass` aktif.
    pub menubar_style: BarStyle,
    /// Hot corner: kiri-atas, kanan-atas, kiri-bawah, kanan-bawah.
    pub hot: [HotAction; 4],
    /// Stage Manager aktif.
    pub stage: bool,
    /// Saat Stage Manager aktif, Alt+Tab diganti pengalih app ABRA.
    pub switcher: bool,
    /// Stage Manager: daftar jendela di sisi kanan (bawaan kiri).
    pub stage_right: bool,
    /// Lebar kartu daftar (dp), jarak antar kartu (dp), jumlah grup maksimum.
    pub stage_size: u8,
    pub stage_gap: u8,
    pub stage_count: u8,
    /// Kecepatan animasi Stage Manager (persen; 100 = normal, lebih besar = lebih cepat).
    pub stage_speed: u8,
    /// Klik wallpaper menyimpan semua jendela ke daftar.
    pub stage_desktop_click: bool,
    /// Jeda media app yang keluar panggung, lanjutkan saat kembali.
    pub stage_media: bool,
    /// Ikon desktop: 0 selalu sembunyi, 1 otomatis (tampil saat panggung kosong), 2 selalu tampil.
    pub stage_icons: u8,
    /// Judul jendela muncul saat kartu di-hover.
    pub stage_titles: bool,
    /// Sembunyikan semua ikon desktop selama ABRA berjalan (dikembalikan saat ABRA keluar).
    pub hide_desktop_icons: bool,
    /// Ikon dock diseragamkan ala macOS (badan squircle + bayangan; ikon bentuk bebas di atas pelat putih).
    pub dock_plates: bool,
    /// Dock, menu bar, dan notch tetap terlihat di atas jendela layar penuh (bukan menyingkir).
    pub above_fullscreen: bool,
    /// Riwayat papan klip 8 jam (hanya di memori) untuk mode Papan Klip di Spotlight.
    pub clipboard_history: bool,
    pub icon_style: IconStyle,
    /// Indeks warna tint (`iconplate::TINTS`) untuk gaya Tinted.
    pub icon_tint: u8,
    /// Indeks warna aksen (`ACCENTS`).
    pub accent: u8,
    /// Varian gaya ikon: 0 Otomatis (ikut tema), 1 Terang, 2 Gelap.
    pub icon_variant: u8,
    /// Efek minimize: 0 Genie, 1 Skala (seperti pilihan macOS).
    pub min_effect: u8,
    /// Notch diam: 0 = sapaan & info, 1 = kosong, 2 = musik & lirik (pemutar lebar selama ada lagu).
    pub notch_idle: u8,
    /// Nama di sapaan notch (kosong = nama akun Windows).
    pub greet_name: ShortText,
    /// Teks sapaan kustom (kosong = otomatis menurut jam).
    pub greet_text: ShortText,
    /// Baris lirik yang sedang dinyanyikan tampil di notch tertutup.
    pub lyrics_compact: bool,
    /// Cari lirik online di LRCLIB bila tidak ada berkas .lrc lokal (satu-satunya akses jaringan ABRA).
    pub lyrics_online: bool,
    /// Unduhan yang berjalan (Store, Steam, peramban) tampil di notch.
    pub notch_downloads: bool,
    /// Zona "Kirim" di notch: kirim berkas ke HP lewat QR + browser (server lokal sementara).
    pub notch_send: bool,
    /// Pintasan ala Mac global (Alt+` ganti jendela app, Ctrl+Alt+H/M/Q).
    pub mac_shortcuts: bool,
    /// Tumpukan Unduhan di dock (klik menampilkan berkas terbaru).
    pub downloads_stack: bool,
    /// Tiga app terakhir dipakai (sudah ditutup) tetap tampil di dock setelah app yang berjalan.
    pub recent_apps: bool,
    /// Pakai paket ikon di %APPDATA%\ABRA\iconpack bila ada (app yang punya padanan jelas saja).
    pub icon_pack: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            takeover: true,
            dock: Behavior::Fixed,
            notch: Behavior::Fixed,
            blur: 60,
            style: BlurStyle::Clear,
            transparency: 50,
            theme: Theme::Auto,
            lang: Lang::Id,
            dock_material: Material::Glass,
            dock_round: 50,
            notch_material: Material::Black,
            notch_round: 50,
            autostart: false,
            dock_size: 48,
            dock_gap: 14,
            dock_zoom: 200,
            dock_margin: 8,
            anim: true,
            refraction: true,
            dock_enabled: true,
            notch_enabled: true,
            notch_hover_open: true,
            win_shadow: true,
            menubar: false,
            clock_24h: true,
            menubar_glass: false,
            menubar_material: false,
            menubar_style: BarStyle::Light,
            hot: [HotAction::None; 4],
            stage: false,
            switcher: true,
            stage_right: false,
            stage_size: 170,
            stage_gap: 20,
            stage_count: 6,
            stage_speed: 100,
            stage_desktop_click: true,
            stage_media: false,
            stage_icons: 2,
            stage_titles: true,
            hide_desktop_icons: true,
            dock_plates: true,
            above_fullscreen: true,
            clipboard_history: true,
            icon_style: IconStyle::Default,
            icon_tint: 0,
            accent: 0,
            icon_variant: 0,
            min_effect: 0,
            notch_idle: 0,
            greet_name: ShortText::EMPTY,
            greet_text: ShortText::EMPTY,
            lyrics_compact: true,
            lyrics_online: true,
            notch_downloads: true,
            notch_send: true,
            mac_shortcuts: true,
            downloads_stack: true,
            recent_apps: true,
            icon_pack: true,
        }
    }
}

impl Settings {
    /// Baca teks `kunci=nilai`. Baris tak dikenal/rusak diabaikan; nilai di luar rentang dijepit.
    pub fn parse(text: &str) -> Settings {
        let mut s = Settings::default();
        let mut legacy_glass = false;
        for line in text.trim_start_matches('\u{feff}').lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim(), v.trim());
            match k {
                "takeover" => {
                    if let Some(b) = parse_bool(v) {
                        s.takeover = b;
                    }
                }
                "dock" => s.dock = Behavior::parse(v).unwrap_or(s.dock),
                "notch" => s.notch = Behavior::parse(v).unwrap_or(s.notch),
                "blur" => s.blur = v.parse::<i32>().map(|n| n.clamp(0, 100) as u8).unwrap_or(s.blur),
                // Gaya blur tidak bisa dipilih lagi: selalu Clear (nilai lama di berkas diabaikan).
                "style" => {}
                "transparency" => s.transparency = v.parse::<i32>().map(|n| n.clamp(0, 100) as u8).unwrap_or(s.transparency),
                "theme" => s.theme = Theme::parse(v).unwrap_or(s.theme),
                "lang" => s.lang = Lang::parse(v).unwrap_or(s.lang),
                "dock_material" => s.dock_material = Material::parse(v).unwrap_or(s.dock_material),
                "notch_material" => s.notch_material = Material::parse(v).unwrap_or(s.notch_material),
                "dock_round" => s.dock_round = v.parse::<i32>().map(|n| n.clamp(0, 100) as u8).unwrap_or(s.dock_round),
                "notch_round" => s.notch_round = v.parse::<i32>().map(|n| n.clamp(0, 100) as u8).unwrap_or(s.notch_round),
                "dock_size" => s.dock_size = v.parse::<i32>().map(|n| n.clamp(32, 72) as u8).unwrap_or(s.dock_size),
                "dock_gap" => s.dock_gap = v.parse::<i32>().map(|n| n.clamp(0, 24) as u8).unwrap_or(s.dock_gap),
                "dock_zoom" => s.dock_zoom = v.parse::<i32>().map(|n| n.clamp(100, 250) as u8).unwrap_or(s.dock_zoom),
                "dock_margin" => s.dock_margin = v.parse::<i32>().map(|n| n.clamp(0, 60) as u8).unwrap_or(s.dock_margin),
                "dock_enabled" => {
                    if let Some(b) = parse_bool(v) {
                        s.dock_enabled = b;
                    }
                }
                "menubar" => {
                    if let Some(b) = parse_bool(v) {
                        s.menubar = b;
                    }
                }
                "hot_tl" => s.hot[0] = HotAction::parse(v).unwrap_or(s.hot[0]),
                "hot_tr" => s.hot[1] = HotAction::parse(v).unwrap_or(s.hot[1]),
                "hot_bl" => s.hot[2] = HotAction::parse(v).unwrap_or(s.hot[2]),
                "hot_br" => s.hot[3] = HotAction::parse(v).unwrap_or(s.hot[3]),
                "switcher" => {
                    if let Some(b) = parse_bool(v) {
                        s.switcher = b;
                    }
                }
                "stage" => {
                    if let Some(b) = parse_bool(v) {
                        s.stage = b;
                    }
                }
                "stage_right" => {
                    if let Some(b) = parse_bool(v) {
                        s.stage_right = b;
                    }
                }
                "stage_desktop_click" => {
                    if let Some(b) = parse_bool(v) {
                        s.stage_desktop_click = b;
                    }
                }
                "icon_style" => s.icon_style = IconStyle::parse(v).unwrap_or(s.icon_style),
                "downloads_stack" => {
                    if let Some(b) = parse_bool(v) {
                        s.downloads_stack = b;
                    }
                }
                "icon_pack" => {
                    if let Some(b) = parse_bool(v) {
                        s.icon_pack = b;
                    }
                }
                "recent_apps" => {
                    if let Some(b) = parse_bool(v) {
                        s.recent_apps = b;
                    }
                }
                "mac_shortcuts" => {
                    if let Some(b) = parse_bool(v) {
                        s.mac_shortcuts = b;
                    }
                }
                "accent" => s.accent = v.parse::<i32>().map(|n| if n == ACCENT_MULTI as i32 { ACCENT_MULTI } else { n.clamp(0, 7) as u8 }).unwrap_or(s.accent),
                "min_effect" => s.min_effect = v.parse::<i32>().map(|n| n.clamp(0, 1) as u8).unwrap_or(s.min_effect),
                "notch_idle" => s.notch_idle = v.parse::<i32>().map(|n| n.clamp(0, 2) as u8).unwrap_or(s.notch_idle),
                "greet_name" => s.greet_name = ShortText::new(v),
                "greet_text" => s.greet_text = ShortText::new(v),
                "lyrics_compact" => {
                    if let Some(b) = parse_bool(v) {
                        s.lyrics_compact = b;
                    }
                }
                "lyrics_online" => {
                    if let Some(b) = parse_bool(v) {
                        s.lyrics_online = b;
                    }
                }
                "notch_downloads" => {
                    if let Some(b) = parse_bool(v) {
                        s.notch_downloads = b;
                    }
                }
                "notch_send" => {
                    if let Some(b) = parse_bool(v) {
                        s.notch_send = b;
                    }
                }
                "icon_variant" => s.icon_variant = v.parse::<i32>().map(|n| n.clamp(0, 2) as u8).unwrap_or(s.icon_variant),
                "icon_tint" => s.icon_tint = v.parse::<i32>().map(|n| n.clamp(0, 4) as u8).unwrap_or(s.icon_tint),
                "clipboard_history" => {
                    if let Some(b) = parse_bool(v) {
                        s.clipboard_history = b;
                    }
                }
                "above_fullscreen" => {
                    if let Some(b) = parse_bool(v) {
                        s.above_fullscreen = b;
                    }
                }
                "dock_plates" => {
                    if let Some(b) = parse_bool(v) {
                        s.dock_plates = b;
                    }
                }
                "hide_desktop_icons" => {
                    if let Some(b) = parse_bool(v) {
                        s.hide_desktop_icons = b;
                    }
                }
                "stage_media" => {
                    if let Some(b) = parse_bool(v) {
                        s.stage_media = b;
                    }
                }
                "stage_titles" => {
                    if let Some(b) = parse_bool(v) {
                        s.stage_titles = b;
                    }
                }
                "stage_size" => s.stage_size = v.parse::<i32>().map(|n| n.clamp(70, 260) as u8).unwrap_or(s.stage_size),
                "stage_gap" => s.stage_gap = v.parse::<i32>().map(|n| n.clamp(0, 60) as u8).unwrap_or(s.stage_gap),
                "stage_count" => s.stage_count = v.parse::<i32>().map(|n| n.clamp(2, 10) as u8).unwrap_or(s.stage_count),
                "stage_speed" => s.stage_speed = v.parse::<i32>().map(|n| n.clamp(50, 200) as u8).unwrap_or(s.stage_speed),
                "stage_icons" => s.stage_icons = v.parse::<i32>().map(|n| n.clamp(0, 2) as u8).unwrap_or(s.stage_icons),
                "menubar_style" => {
                    // mode Kaca sudah dihapus: konfigurasi lama dengan latar kaca menjadi latar transparan
                    if matches!(v.to_ascii_lowercase().as_str(), "glass" | "kaca") {
                        legacy_glass = true;
                    } else {
                        s.menubar_style = BarStyle::parse(v).unwrap_or(s.menubar_style);
                    }
                }
                "menubar_material" => {
                    if let Some(b) = parse_bool(v) {
                        s.menubar_material = b;
                    }
                }
                "menubar_glass" => {
                    if let Some(b) = parse_bool(v) {
                        s.menubar_glass = b;
                    }
                }
                "clock_24h" => {
                    if let Some(b) = parse_bool(v) {
                        s.clock_24h = b;
                    }
                }
                "win_shadow" => {
                    if let Some(b) = parse_bool(v) {
                        s.win_shadow = b;
                    }
                }
                "notch_hover_open" => {
                    if let Some(b) = parse_bool(v) {
                        s.notch_hover_open = b;
                    }
                }
                "notch_enabled" => {
                    if let Some(b) = parse_bool(v) {
                        s.notch_enabled = b;
                    }
                }
                "refraction" => {
                    if let Some(b) = parse_bool(v) {
                        s.refraction = b;
                    }
                }
                "anim" => {
                    if let Some(b) = parse_bool(v) {
                        s.anim = b;
                    }
                }
                "autostart" => {
                    if let Some(b) = parse_bool(v) {
                        s.autostart = b;
                    }
                }
                _ => {}
            }
        }
        if legacy_glass {
            s.menubar_glass = false;
        }
        s
    }

    pub fn serialize(&self) -> String {
        format!(
            "# ABRA settings\ntakeover={}\ndock={}\nnotch={}\nblur={}\nstyle={}\ntransparency={}\ntheme={}\nlang={}\ndock_material={}\ndock_round={}\nnotch_material={}\nnotch_round={}\nautostart={}\ndock_size={}\ndock_gap={}\ndock_zoom={}\ndock_margin={}\nanim={}\nrefraction={}\ndock_enabled={}\nnotch_enabled={}\nnotch_hover_open={}\nwin_shadow={}\nmenubar={}\nclock_24h={}\nmenubar_glass={}\nhot_tl={}\nhot_tr={}\nhot_bl={}\nhot_br={}\nstage={}\nswitcher={}\nstage_right={}\nstage_size={}\nstage_gap={}\nstage_count={}\nstage_speed={}\nstage_desktop_click={}\nstage_media={}\nstage_icons={}\nstage_titles={}\nhide_desktop_icons={}\nmenubar_style={}\ndock_plates={}\nabove_fullscreen={}\nclipboard_history={}\nicon_style={}\nicon_tint={}\naccent={}\nmac_shortcuts={}\ndownloads_stack={}\nrecent_apps={}\nicon_pack={}\nmenubar_material={}\nicon_variant={}\nmin_effect={}\nnotch_idle={}\ngreet_name={}\ngreet_text={}\nlyrics_compact={}\nlyrics_online={}\nnotch_downloads={}\nnotch_send={}\n",
            self.takeover,
            self.dock.key(),
            self.notch.key(),
            self.blur,
            self.style.key(),
            self.transparency,
            self.theme.key(),
            self.lang.key(),
            self.dock_material.key(),
            self.dock_round,
            self.notch_material.key(),
            self.notch_round,
            self.autostart,
            self.dock_size,
            self.dock_gap,
            self.dock_zoom,
            self.dock_margin,
            self.anim,
            self.refraction,
            self.dock_enabled,
            self.notch_enabled,
            self.notch_hover_open,
            self.win_shadow,
            self.menubar,
            self.clock_24h,
            self.menubar_glass,
            self.hot[0].key(),
            self.hot[1].key(),
            self.hot[2].key(),
            self.hot[3].key(),
            self.stage,
            self.switcher,
            self.stage_right,
            self.stage_size,
            self.stage_gap,
            self.stage_count,
            self.stage_speed,
            self.stage_desktop_click,
            self.stage_media,
            self.stage_icons,
            self.stage_titles,
            self.hide_desktop_icons,
            self.menubar_style.key(),
            self.dock_plates,
            self.above_fullscreen,
            self.clipboard_history,
            self.icon_style.key(),
            self.icon_tint,
            self.accent,
            self.mac_shortcuts,
            self.downloads_stack,
            self.recent_apps,
            self.icon_pack,
            self.menubar_material,
            self.icon_variant,
            self.min_effect,
            self.notch_idle,
            self.greet_name.as_str(),
            self.greet_text.as_str(),
            self.lyrics_compact,
            self.lyrics_online,
            self.notch_downloads,
            self.notch_send
        )
    }
}

fn parse_bool(v: &str) -> Option<bool> {
    match v.to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Some(true),
        "false" | "0" | "no" | "off" => Some(false),
        _ => None,
    }
}

/// Turunan tampilan kaca (semua dalam ARGB 0..255), tidak bergantung pada Win32.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    pub dark: bool,
    pub blur_sigma: f32,
    pub saturation: f32,
    pub tint: [u8; 4],
    pub rim: [u8; 4],
    pub solid: [u8; 4],
    /// Kekuatan sorotan spekular 0..1.
    pub highlight: f32,
    /// Warna teks/ikon utama.
    pub fg: [u8; 3],
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// `system_dark`: tema aplikasi sistem sedang gelap (dipakai saat `Theme::Auto`).
/// `material`: material permukaan; `Black` selalu gelap, `Glass` dan `Clear` mengikuti tema.
pub fn look(s: &Settings, system_dark: bool, material: Material) -> Look {
    let dark = match (material, s.theme) {
        (Material::Black, _) => true,
        (_, Theme::Auto) => system_dark,
        (_, Theme::Dark) => true,
        (_, Theme::Light) => false,
    };
    let b = s.blur.min(100) as f32 / 100.0;
    let t = s.transparency.min(100) as f32 / 100.0;
    let (sigma_k, sat) = match s.style {
        BlurStyle::Frosted => (1.0, 1.25),
        BlurStyle::Clear => (0.45, 1.05),
        BlurStyle::Vivid => (0.85, 1.9),
    };
    let mut blur_sigma = lerp(1.0, 40.0, b) * sigma_k;
    let mut saturation: f32 = sat;
    let (tr, tg, tb, a_max, a_min) = if dark { (20u8, 20u8, 24u8, 232.0, 40.0) } else { (255, 255, 255, 215.0, 24.0) };
    let mut a = lerp(a_max, a_min, t);
    let mut highlight = 1.0f32;
    let (mut rim, mut solid, fg) = if dark {
        ([70u8, 255, 255, 255], [235u8, 28, 28, 32], [255u8, 255, 255])
    } else {
        ([150, 255, 255, 255], [235, 238, 240, 246], [24, 24, 28])
    };
    let (mut tr, mut tg, mut tb) = (tr, tg, tb);
    match material {
        Material::Glass => {
            // Liquid Glass ala macOS Tahoe: hampir bening (tint sangat tipis), blur sedang, tepi menangkap cahaya.
            a = if dark { lerp(120.0, 22.0, t) } else { lerp(80.0, 18.0, t) };
            if !dark {
                // kaca terang lembut: putih kebiruan redup (bukan putih murni) agar dock tidak silau
                (tr, tg, tb) = (226, 231, 240);
            }
            blur_sigma = lerp(3.0, 24.0, b) * sigma_k;
            saturation = saturation.min(1.25);
            rim = if dark { [150, 255, 255, 255] } else { [170, 255, 255, 255] };
        }
        Material::Blur => {
            // Blur ala contoh Tahoe: kabut tebal. Blur kuat (tidak mengikuti pengali gaya), sedikit pudar warnanya,
            // tint tipis, dan tepi cahaya halus; bentuk datar (sorotan kecil) supaya terlihat seperti kaca berkabut.
            // Jernih tapi tetap berkabut: tint tipis supaya latar terlihat tembus, blur sedang-kuat, tepi cahaya jelas.
            a = if dark { lerp(100.0, 30.0, t) } else { lerp(120.0, 40.0, t) };
            (tr, tg, tb) = if dark { (26, 28, 30) } else { (240, 242, 246) };
            blur_sigma = lerp(8.0, 36.0, b);
            saturation = 1.1;
            // garis tepi sangat tipis saja (kesan "timbul"); strip menu bar menghilangkannya sendiri
            rim = if dark { [34, 255, 255, 255] } else { [60, 255, 255, 255] };
            highlight = 0.2;
        }
        Material::Black => {
            // Hitam pekat ala notch: transparansi hanya melonggarkan sedikit.
            (tr, tg, tb) = (10, 10, 12);
            // Hitam rata tanpa sorotan/tepi terang: bentuknya utuh seperti notch sungguhan.
            a = lerp(255.0, 225.0, t);
            saturation = 1.0;
            blur_sigma *= 0.8;
            rim = [0, 255, 255, 255];
            solid = [255, 10, 10, 12];
            highlight = 0.0;
        }
    }
    Look { dark, blur_sigma, saturation, tint: [a.round() as u8, tr, tg, tb], rim, solid, highlight, fg }
}

/// Tampilan latar menu bar: `Light`/`Dark` memaksa terang/gelap (di aplikasi
/// digambar sebagai warna rata putih/hitam tanpa kaca; `Look` di sini hanya dipakai untuk menentukan warna teks).
pub fn bar_look(s: &Settings, system_dark: bool) -> Look {
    let mut t = s.clone();
    match s.menubar_style {
        BarStyle::Light => t.theme = Theme::Light,
        BarStyle::Dark => t.theme = Theme::Dark,
    }
    let mut l = look(&t, system_dark, Material::Glass);
    l.tint[0] = 238;
    l.blur_sigma = l.blur_sigma.min(14.0);
    l.saturation = 1.0;
    l.highlight = 0.25;
    l.rim[0] = l.rim[0].min(60);
    l
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_styles_force_theme_and_are_denser_than_glass() {
        let mut s = Settings::default();
        s.menubar_style = BarStyle::Light;
        let light = bar_look(&s, true);
        s.menubar_style = BarStyle::Dark;
        let dark = bar_look(&s, false);
        assert!(!light.dark && dark.dark);
        assert!(light.tint[0] >= 230 && dark.tint[0] >= 230);
        assert_eq!(BarStyle::parse("terang"), Some(BarStyle::Light));
        let again = Settings::parse(&s.serialize());
        assert_eq!(again.menubar_style, BarStyle::Dark);
    }

    #[test]
    fn legacy_glass_bar_becomes_transparent_and_light_glass_is_brighter() {
        let s = Settings::parse("menubar_glass=true
menubar_style=glass
");
        assert!(!s.menubar_glass, "latar kaca lama menjadi transparan");
        let mut st = Settings::default();
        st.theme = Theme::Light;
        let light = look(&st, false, Material::Glass);
        st.theme = Theme::Dark;
        let dark = look(&st, false, Material::Glass);
        assert!(light.tint[0] >= 40 && light.tint[0] <= 60, "kaca terang cukup terang: {}", light.tint[0]);
        assert!(dark.tint[0] >= 60);
    }

    #[test]
    fn roundtrip() {
        let s = Settings { takeover: false, dock: Behavior::AutoHide, notch: Behavior::Overlay, blur: 12, style: BlurStyle::Clear, transparency: 90, theme: Theme::Light, lang: Lang::En, dock_material: Material::Black, dock_round: 80, notch_material: Material::Glass, notch_round: 10, autostart: true, dock_size: 60, dock_gap: 10, dock_zoom: 200, dock_margin: 30, anim: false, refraction: false, dock_enabled: false, notch_enabled: false, notch_hover_open: false, win_shadow: false, menubar: true, clock_24h: false, menubar_glass: true, hot: [HotAction::Mission, HotAction::Launchpad, HotAction::Lock, HotAction::Stage], stage: true, switcher: false, ..Default::default() };
        assert_eq!(Settings::parse(&s.serialize()), s);
    }

    #[test]
    fn defaults_are_full_takeover() {
        let d = Settings::default();
        assert!(d.takeover && d.dock.reserves() && d.notch.reserves());
    }

    #[test]
    fn dock_metrics_clamped() {
        let s = Settings::parse("dock_size=500
dock_zoom=5
dock_margin=900");
        assert_eq!((s.dock_size, s.dock_zoom, s.dock_margin), (72, 100, 60));
    }

    #[test]
    fn junk_is_ignored_and_clamped() {
        let s = Settings::parse("\u{feff}blur=999\ntransparency=-5\ndock=banana\nwat\ntheme=gelap\n#x=1");
        assert_eq!(s.blur, 100);
        assert_eq!(s.transparency, 0);
        assert_eq!(s.dock, Behavior::Fixed);
        assert_eq!(s.theme, Theme::Dark);
    }

    #[test]
    fn behavior_maps_to_policy() {
        assert_eq!(Behavior::Fixed.mode(), Mode::Always);
        assert!(Behavior::Fixed.reserves());
        assert!(!Behavior::Overlay.reserves());
        assert_eq!(Behavior::Overlay.mode(), Mode::Always);
        assert_eq!(Behavior::AutoHide.mode(), Mode::OnDemand, "auto-hide ala macOS: selalu sembunyi sampai tepi disentuh");
        assert_eq!(Behavior::Peek.mode(), Mode::AutoHide);
    }

    #[test]
    fn materials_differ() {
        let s = Settings::default();
        let g = look(&s, false, Material::Glass);
        let k = look(&s, false, Material::Black);
        assert!(!g.dark && k.dark, "hitam selalu gelap");
        assert!(g.tint[0] < k.tint[0], "alfa: glass < black");
        // config lama dengan material transparan jatuh ke Liquid Glass
        assert_eq!(Material::parse("clear"), Some(Material::Glass));
    }

    #[test]
    fn round_factor_scales() {
        assert!((round_factor(50) - 1.0).abs() < 1e-6);
        assert!(round_factor(0) < 0.5 && round_factor(100) > 1.5);
    }

    #[test]
    fn blur_material_is_foggier_than_glass_and_follows_theme() {
        let s = Settings::default();
        let glass = look(&s, true, Material::Glass);
        let blur = look(&s, true, Material::Blur);
        assert!(blur.blur_sigma > glass.blur_sigma * 1.5, "blur {} vs glass {}", blur.blur_sigma, glass.blur_sigma);
        assert!(blur.dark && !look(&s, false, Material::Blur).dark);
        assert!(blur.tint[0] >= 60 && blur.rim[0] > 0 && blur.rim[0] < 60, "rim Blur sangat tipis");
        assert_eq!(Material::parse("blur"), Some(Material::Blur));
        assert_eq!(Material::Blur.key(), "blur");
        assert_eq!(Material::ALL.len(), 3);
    }

    #[test]
    fn notch_greeting_settings_round_trip() {
        let s = Settings::parse("notch_idle=1\ngreet_name=  Rani =)  \ngreet_text=Semangat, \u{1F680}!\n");
        assert_eq!(s.notch_idle, 1);
        assert_eq!(s.greet_name.as_str(), "Rani =)");
        let again = Settings::parse(&s.serialize());
        assert_eq!((again.notch_idle, again.greet_name, again.greet_text), (1, s.greet_name, s.greet_text));
        assert_eq!(Settings::default().notch_idle, 0);
        assert_eq!(Settings::parse(&Settings::parse("notch_idle=2\n").serialize()).notch_idle, 2, "musik & lirik");
        assert_eq!(Settings::parse("notch_idle=9\n").notch_idle, 2);
        let d = Settings::default();
        assert!(d.lyrics_compact && d.lyrics_online, "lirik online aktif bawaan (keputusan pengguna)");
        let off = Settings::parse(&Settings::parse("lyrics_online=false\nlyrics_compact=0\nnotch_downloads=0\n").serialize());
        assert!(!off.lyrics_online && !off.lyrics_compact && !off.notch_downloads);
        assert!(d.notch_downloads, "unduhan di notch aktif bawaan");
        assert!(d.notch_send, "zona Kirim aktif bawaan");
        assert!(!Settings::parse(&Settings::parse("notch_send=0\n").serialize()).notch_send);
        assert!(Settings::default().greet_name.is_empty());
        // baris baru dibuang, teks panjang dipotong di batas karakter
        assert_eq!(ShortText::new("a\nb").as_str(), "ab");
        let long = "é".repeat(80);
        let t = ShortText::new(&long);
        assert!(t.as_str().len() <= 96 && t.as_str().chars().all(|c| c == 'é'));
    }

    #[test]
    fn multicolor_accent_and_icon_variant_round_trip() {
        let s = Settings::parse("accent=255\nicon_variant=2\n");
        assert_eq!(s.accent, ACCENT_MULTI);
        assert_eq!(s.icon_variant, 2);
        let again = Settings::parse(&s.serialize());
        assert_eq!((again.accent, again.icon_variant), (ACCENT_MULTI, 2));
        assert_eq!(Settings::parse("accent=9\n").accent, 7);
    }

    #[test]
    fn blur_style_option_is_gone_and_always_clear() {
        let s = Settings::parse("style=vivid
menubar_material=true
");
        assert_eq!(s.style, BlurStyle::Clear);
        assert!(s.menubar_material);
        assert!(Settings::default().style == BlurStyle::Clear);
    }

    #[test]
    fn look_responds_to_controls() {
        let mut s = Settings::default();
        let base = look(&s, true, Material::Glass);
        s.blur = 100;
        assert!(look(&s, true, Material::Glass).blur_sigma > base.blur_sigma);
        s = Settings::default();
        s.transparency = 100;
        assert!(look(&s, true, Material::Glass).tint[0] < base.tint[0]);
        s = Settings::default();
        s.theme = Theme::Light;
        assert!(!look(&s, true, Material::Glass).dark);
        s.theme = Theme::Auto;
        assert!(look(&s, true, Material::Glass).dark && !look(&s, false, Material::Glass).dark);
    }
}
