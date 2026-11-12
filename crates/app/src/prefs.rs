//! Hub pengaturan ABRA: satu salinan `Settings` untuk seluruh proses, disimpan ke `settings.cfg`,
//! dan setiap perubahan disiarkan (`WM_PREFS`) ke window yang mendaftar (dock, island, HUD, Settings).

use crate::paths;
use nacre_core::settings::{self, Look, Settings};
use nacre_platform::{log, sys};
use nacre_ui::glass::GlassParams;
use std::sync::Mutex;
use windows::UI::Color;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

pub const WM_PREFS: u32 = WM_APP + 40;

static CUR: Mutex<Option<Settings>> = Mutex::new(None);
static DIRTY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static LISTENERS: Mutex<Vec<isize>> = Mutex::new(Vec::new());

fn file() -> std::path::PathBuf {
    paths::config_dir().join("settings.cfg")
}

pub fn init() {
    let s = std::fs::read_to_string(file()).map(|t| Settings::parse(&t)).unwrap_or_default();
    log::line("prefs", &format!("dimuat: {s:?}"));
    *CUR.lock().unwrap() = Some(s);
}

pub fn get() -> Settings {
    let mut g = CUR.lock().unwrap();
    *g.get_or_insert_with(Settings::default)
}

pub fn listen(h: HWND) {
    LISTENERS.lock().unwrap().push(h.0 as isize);
}

pub fn unlisten(h: HWND) {
    LISTENERS.lock().unwrap().retain(|x| *x != h.0 as isize);
}

/// Ubah pengaturan, simpan, lalu siarkan. Tidak melakukan apa pun bila tidak ada perubahan.
pub fn update(f: impl FnOnce(&mut Settings)) {
    update_with(true, f);
}

/// Seperti `update`, tetapi `save=false` hanya menyiarkan (untuk drag slider; simpan saat dilepas).
pub fn update_with(save: bool, f: impl FnOnce(&mut Settings)) {
    let before = get();
    let mut s = before;
    f(&mut s);
    let dirty = DIRTY.load(std::sync::atomic::Ordering::Relaxed);
    if s == before && !(save && dirty) {
        return;
    }
    *CUR.lock().unwrap() = Some(s);
    DIRTY.store(!save, std::sync::atomic::Ordering::Relaxed);
    if save {
        let _ = std::fs::create_dir_all(paths::config_dir());
        if let Err(e) = std::fs::write(file(), s.serialize()) {
            log::line("prefs", &format!("simpan gagal: {e}"));
        }
    }
    broadcast();
}

pub fn broadcast() {
    for h in LISTENERS.lock().unwrap().iter() {
        unsafe {
            let _ = PostMessageW(Some(HWND(*h as *mut _)), WM_PREFS, WPARAM(0), LPARAM(0));
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Role {
    /// Bar dock: material dan tema pilihan pengguna.
    Dock,
    /// Notch/island: material pilihan (bawaan hitam).
    Notch,
    /// Jendela Settings: kaca mengikuti tema.
    Themed,
    /// Popup, HUD: kaca gelap (teksnya putih) tetapi blur/transparansi/gaya mengikuti pengaturan.
    Dark,
}

/// Tampilan latar menu bar (kaca/terang/gelap sesuai pengaturan).
pub fn bar_look() -> Look {
    settings::bar_look(&get(), sys::apps_dark())
}

pub fn look(role: Role) -> Look {
    let mut s = get();
    let dark_sys = sys::apps_dark();
    match role {
        Role::Dock => settings::look(&s, dark_sys, s.dock_material),
        Role::Notch => {
            // Teks island putih: notch selalu varian gelap; materialnya (Liquid Glass atau Hitam buram) dipilih sendiri di Tampilan.
            s.theme = settings::Theme::Dark;
            settings::look(&s, dark_sys, s.notch_material)
        }
        Role::Themed => settings::look(&s, dark_sys, s.dock_material),
        Role::Dark => {
            s.theme = settings::Theme::Dark;
            settings::look(&s, dark_sys, s.dock_material)
        }
    }
}

fn c(a: [u8; 4]) -> Color {
    Color { A: a[0], R: a[1], G: a[2], B: a[3] }
}

/// Terapkan `Look` ke parameter kaca (ukuran dan radius dipertahankan dari `base`).
pub fn apply(base: GlassParams, l: &Look) -> GlassParams {
    GlassParams { blur_sigma: l.blur_sigma, saturation: l.saturation, tint: c(l.tint), rim: c(l.rim), solid: c(l.solid), highlight: l.highlight, ..base }
}

/// Faktor skala radius sudut untuk permukaan (dock/notch) dari pengaturan.
pub fn round(role: Role) -> f32 {
    let s = get();
    settings::round_factor(if role == Role::Notch { s.notch_round } else { s.dock_round })
}

pub fn glass(size: (f32, f32), radius: f32, role: Role) -> GlassParams {
    apply(GlassParams::dark(size, radius), &look(role))
}

/// Terjemahkan teks sumber (Indonesia) ke bahasa pilihan.
pub fn tr(src: &'static str) -> &'static str {
    nacre_core::i18n::tr(get().lang, src)
}

/// Warna aksen sistem terpilih (RGB).
pub fn accent() -> [u8; 3] {
    settings::ACCENTS[(get().accent as usize).min(settings::ACCENTS.len() - 1)]
}

/// Warna aksen dengan alfa `a`.
pub fn accent_c(a: u8) -> Color {
    let c = accent();
    Color { A: a, R: c[0], G: c[1], B: c[2] }
}

pub fn fg(a: u8) -> Color {
    let l = look(Role::Themed);
    Color { A: a, R: l.fg[0], G: l.fg[1], B: l.fg[2] }
}
