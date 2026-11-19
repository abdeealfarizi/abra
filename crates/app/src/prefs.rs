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

pub fn update(f: impl FnOnce(&mut Settings)) {
    update_with(true, f);
}

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
    Dock,
    Notch,
    Themed,
    Dark,
}

pub fn bar_look() -> Look {
    settings::bar_look(&get(), sys::apps_dark())
}

pub fn look(role: Role) -> Look {
    let mut s = get();
    let dark_sys = sys::apps_dark();
    match role {
        Role::Dock => settings::look(&s, dark_sys, s.dock_material),
        Role::Notch => {
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

pub fn apply(base: GlassParams, l: &Look) -> GlassParams {
    GlassParams { blur_sigma: l.blur_sigma, saturation: l.saturation, tint: c(l.tint), rim: c(l.rim), solid: c(l.solid), highlight: l.highlight, liquify: l.liquify, ..base }
}

pub fn ui_scale() -> f32 {
    get().ui_scale.clamp(75, 200) as f32 / 100.0
}

pub fn round(role: Role) -> f32 {
    let s = get();
    settings::round_factor(if role == Role::Notch { s.notch_round } else { s.dock_round })
}

pub fn glass(size: (f32, f32), radius: f32, role: Role) -> GlassParams {
    apply(GlassParams::dark(size, radius), &look(role))
}

pub fn tr(src: &'static str) -> &'static str {
    let lang = get().lang;
    if lang != nacre_core::i18n::Lang::Id && !nacre_core::i18n::is_known(src) {
        note_missing(src);
    }
    nacre_core::i18n::tr(lang, src)
}

fn note_missing(src: &'static str) {
    static SEEN: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
    let mut g = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    if !g.contains(&src) && g.len() < 400 {
        g.push(src);
        nacre_platform::log::line("i18n", &format!("belum diterjemahkan: {src:?}"));
    }
}

pub fn accent() -> [u8; 3] {
    let a = get().accent;
    if a == settings::ACCENT_MULTI {
        return nacre_platform::sys::accent_color().unwrap_or(settings::ACCENTS[0]);
    }
    settings::ACCENTS[(a as usize).min(settings::ACCENTS.len() - 1)]
}

pub fn icon_dark() -> bool {
    match get().icon_variant {
        1 => false,
        2 => true,
        _ => look(Role::Themed).dark,
    }
}

pub fn accent_c(a: u8) -> Color {
    let c = accent();
    Color { A: a, R: c[0], G: c[1], B: c[2] }
}

pub fn fg(a: u8) -> Color {
    let l = look(Role::Themed);
    Color { A: a, R: l.fg[0], G: l.fg[1], B: l.fg[2] }
}
