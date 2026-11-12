//! Spotlight ala macOS Tahoe: kapsul kaca di atas-tengah layar (Ctrl+Space atau ikon cari di menu bar) dengan empat mode:
//! Apps (Ctrl+1, juga Ctrl+Shift+Space), Berkas (Ctrl+2), Aksi (Ctrl+3, dengan quick keys), dan Papan Klip (Ctrl+4).
//! Saat kosong tampil kapsul + empat tombol bulat; saat mengetik berubah menjadi panel berisi hasil (aplikasi, kalkulator,
//! aksi, berkas, web) dengan chip filter. Dibuat saat dibuka dan dihancurkan saat ditutup.

use crate::popup::Popup;
use crate::{paths, prefs};
use nacre_core::appcat::{self, Cat};
use nacre_core::search;
use nacre_core::spotlight::{self as model, Mode, ACTIONS};
use nacre_core::symbols::{self, Sym};
use nacre_platform::apps::{self, App};
use nacre_platform::filesearch::{self, Hit};
use nacre_platform::icons::{self, Bitmap};
use nacre_platform::text::{measure, render, render_face, Style, ICON_FACE};
use nacre_platform::{clipboard, keys, launch, log, sys, time};
use nacre_ui::gfx::Gfx;
use nacre_ui::glass::{GlassPanel, GlassParams};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use windows::core::w;
use windows::UI::Color;
use windows::UI::Composition::{CompositionDrawingSurface, CompositionStretch, Compositor, ContainerVisual};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, RegisterHotKey, UnregisterHotKey, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, VK_BACK, VK_CONTROL, VK_DOWN, VK_ESCAPE, VK_LEFT, VK_RETURN, VK_RIGHT,
    VK_SPACE, VK_TAB, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows_numerics::{Vector2, Vector3};

const WM_APPS: u32 = WM_APP + 80;
const WM_CLOSE_SPOT: u32 = WM_APP + 81;
const WM_FILES: u32 = WM_APP + 84;
const HOTKEY_ID: i32 = 0xAB01;
const HOTKEY_APPS: i32 = 0xAB02;
const COLS: usize = 5;
const GRID_ROWS: usize = 4;

type Rect = (f32, f32, f32, f32);

#[derive(Clone, Copy, PartialEq, Debug)]
enum Filter {
    Apps,
    Files,
    Actions,
}

#[derive(Clone, PartialEq)]
enum RKind {
    App,
    Calc,
    Web,
    Action(usize),
    File,
    Clip,
}

#[derive(Clone)]
struct Row {
    kind: RKind,
    title: String,
    sub: String,
    path: String,
}

#[derive(Clone, Copy, PartialEq)]
enum HitKind {
    Mode(Mode),
    Filter(Filter),
    Cat(Option<Cat>),
    Row(usize),
    Cell(usize),
}

/// Apa yang dijalankan setelah Spotlight tertutup.
enum Run {
    Nothing,
    Open(String),
    Action(&'static str),
    Copy(String),
}

struct Spot {
    popup: Popup,
    gfx: Gfx,
    comp: Compositor,
    dpi: f32,
    screen: (i32, i32),
    mode: Mode,
    query: String,
    rows: Vec<Row>,
    sel: usize,
    scroll: usize,
    apps: Vec<App>,
    names: Vec<String>,
    cat: Option<Cat>,
    filter: Option<Filter>,
    files: Vec<Hit>,
    files_gen: u64,
    shared_files: Arc<Mutex<(u64, Vec<Hit>)>>,
    quick: HashMap<String, String>,
    /// Sedang mengisi quick key untuk aksi ini.
    editing: Option<usize>,
    edit_buf: String,
    hits: Vec<(Rect, HitKind)>,
    circles: Vec<GlassPanel>,
    text_cache: HashMap<String, (CompositionDrawingSurface, i32, i32)>,
    icon_cache: HashMap<String, Option<CompositionDrawingSurface>>,
}

struct Ctl {
    hwnd: HWND,
    comp: Compositor,
}

thread_local! {
    static SPOT: RefCell<Option<Box<Spot>>> = const { RefCell::new(None) };
    static CTL: RefCell<Option<Ctl>> = const { RefCell::new(None) };
}

fn with_spot<R>(f: impl FnOnce(&mut Spot) -> R) -> Option<R> {
    SPOT.with(|c| c.try_borrow_mut().ok().and_then(|mut b| b.as_mut().map(|b| f(b))))
}

fn recolor(b: &Bitmap, rgb: [u8; 3]) -> Vec<u8> {
    let mut out = b.bgra.clone();
    for px in out.chunks_exact_mut(4) {
        let a = px[3] as u32;
        px[0] = (rgb[2] as u32 * a / 255) as u8;
        px[1] = (rgb[1] as u32 * a / 255) as u8;
        px[2] = (rgb[0] as u32 * a / 255) as u8;
    }
    out
}

fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else if b == b' ' {
            out.push('+');
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn hit(r: Rect, x: f32, y: f32) -> bool {
    x >= r.0 && x < r.0 + r.2 && y >= r.1 && y < r.1 + r.3
}

fn quick_file() -> std::path::PathBuf {
    paths::config_dir().join("actions.txt")
}

fn action_title(a: &model::ActionDef) -> String {
    prefs::tr(a.title).to_string()
}

fn action_glyph(id: &str) -> (&'static str, [u8; 3]) {
    match id {
        "note" => ("\u{E70F}", [255, 204, 0]),
        "timer" => ("\u{E916}", [255, 149, 0]),
        "email" => ("\u{E715}", [10, 132, 255]),
        "screenshot" => ("\u{E722}", [99, 99, 108]),
        "lock" => ("\u{E72E}", [99, 99, 108]),
        "desktop" => ("\u{E7F4}", [90, 200, 250]),
        "dark" => ("\u{E790}", [88, 86, 214]),
        "stage" => ("\u{E8A0}", [48, 209, 88]),
        "mission" => ("\u{E7C4}", [10, 132, 255]),
        "calc" => ("\u{E8EF}", [255, 149, 0]),
        "taskmgr" => ("\u{E9D9}", [99, 99, 108]),
        "settings" => ("\u{E713}", [142, 142, 147]),
        _ => ("\u{E74D}", [255, 69, 58]),
    }
}

/// Daftarkan hot key (Ctrl+Space = Spotlight, Ctrl+Shift+Space = Apps) dan siapkan jendela kontrol.
pub fn init(comp: &Compositor) -> windows_core::Result<()> {
    unsafe {
        let hi: HINSTANCE = GetModuleHandleW(None)?.into();
        let class = w!("AbraSpotCtl");
        RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(ctl_proc), hInstance: hi, lpszClassName: class, ..Default::default() });
        let hwnd = CreateWindowExW(WINDOW_EX_STYLE(0), class, w!("abra-spot-ctl"), WS_POPUP, 0, 0, 0, 0, Some(HWND_MESSAGE), None, Some(hi), None)?;
        if RegisterHotKey(Some(hwnd), HOTKEY_ID, MOD_CONTROL | MOD_NOREPEAT, VK_SPACE.0 as u32).is_err() {
            log::line("spotlight", "Ctrl+Space sudah dipakai program lain; hot key tidak aktif");
        }
        if RegisterHotKey(Some(hwnd), HOTKEY_APPS, MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT, VK_SPACE.0 as u32).is_err() {
            log::line("spotlight", "Ctrl+Shift+Space dipakai program lain; hot key Apps tidak aktif");
        }
        let h = hwnd.0 as isize;
        CTL.with(|c| *c.borrow_mut() = Some(Ctl { hwnd, comp: comp.clone() }));
        prefs::listen(hwnd);
        clipboard::set_enabled(prefs::get().clipboard_history);
        apps::refresh(move || {
            let _ = PostMessageW(Some(HWND(h as *mut _)), WM_APPS, WPARAM(0), LPARAM(0));
        });
    }
    Ok(())
}

pub fn shutdown() {
    CTL.with(|c| {
        if let Some(ctl) = c.borrow().as_ref() {
            unsafe {
                let _ = UnregisterHotKey(Some(ctl.hwnd), HOTKEY_ID);
                let _ = UnregisterHotKey(Some(ctl.hwnd), HOTKEY_APPS);
            }
        }
    });
}

/// Buka bila tertutup, tutup bila terbuka.
pub fn toggle() {
    toggle_mode(Mode::Search)
}

/// Buka langsung di `mode`; bila sudah terbuka di mode itu, tutup; bila di mode lain, pindah.
pub fn toggle_mode(mode: Mode) {
    let open_mode = SPOT.with(|c| c.borrow().as_ref().map(|s| s.mode));
    match open_mode {
        Some(m) if m == mode => {
            close();
            return;
        }
        Some(_) => {
            with_spot(|s| s.set_mode(mode));
            return;
        }
        None => {}
    }
    let comp = CTL.with(|c| c.borrow().as_ref().map(|c| c.comp.clone()));
    let Some(comp) = comp else { return };
    match create(&comp, mode) {
        Ok(s) => {
            SPOT.with(|c| *c.borrow_mut() = Some(s));
            with_spot(|s| {
                s.set_mode(mode);
                s.popup.show_active();
                s.popup.fade_in(&s.comp, true);
            });
            // segarkan indeks di latar belakang (aplikasi baru terpasang)
            let h = CTL.with(|c| c.borrow().as_ref().map(|c| c.hwnd.0 as isize)).unwrap_or(0);
            apps::refresh(move || unsafe {
                let _ = PostMessageW(Some(HWND(h as *mut _)), WM_APPS, WPARAM(0), LPARAM(0));
            });
        }
        Err(e) => log::line("spotlight", &format!("gagal: {e}")),
    }
}

pub fn close() {
    let s = SPOT.with(|c| c.borrow_mut().take());
    if let Some(s) = s {
        let h = s.popup.hwnd;
        drop(s);
        unsafe {
            let _ = DestroyWindow(h);
        }
    }
}

fn create(comp: &Compositor, mode: Mode) -> windows_core::Result<Box<Spot>> {
    let screen = sys::screen_size();
    let glass = prefs::glass((10.0, 10.0), 27.0, prefs::Role::Dock);
    let mut popup = Popup::new_with(comp, w!("AbraSpotlight"), Some(spot_proc), WINDOW_EX_STYLE(0), glass, true)?;
    let dpi = unsafe { GetDpiForWindow(popup.hwnd) }.max(96) as f32 / 96.0;
    let tier = {
        let c = sys::conditions();
        if !c.transparency || c.battery_saver || c.remote_session { nacre_ui::glass::Tier::Solid } else { nacre_ui::glass::Tier::Full }
    };
    let _ = popup.panel.set_tier(tier);
    let gfx = Gfx::new(comp)?;
    let apps = apps::snapshot();
    let names = apps.iter().map(|a| a.name.clone()).collect();
    let quick = model::parse_quick_keys(&std::fs::read_to_string(quick_file()).unwrap_or_default());
    let _ = mode;
    Ok(Box::new(Spot {
        popup,
        gfx,
        comp: comp.clone(),
        dpi,
        screen,
        mode: Mode::Search,
        query: String::new(),
        rows: vec![],
        sel: 0,
        scroll: 0,
        apps,
        names,
        cat: None,
        filter: None,
        files: vec![],
        files_gen: 0,
        shared_files: Arc::new(Mutex::new((0, vec![]))),
        quick,
        editing: None,
        edit_buf: String::new(),
        hits: vec![],
        circles: vec![],
        text_cache: HashMap::new(),
        icon_cache: HashMap::new(),
    }))
}

impl Spot {
    fn look(&self) -> nacre_core::settings::Look {
        prefs::look(prefs::Role::Dock)
    }

    fn width(&self) -> f32 {
        (self.screen.0 as f32 * 0.5).clamp(620.0 * self.dpi, 860.0 * self.dpi).round()
    }

    fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        self.query.clear();
        self.sel = 0;
        self.scroll = 0;
        self.cat = None;
        self.filter = None;
        self.editing = None;
        self.edit_buf.clear();
        self.files.clear();
        self.rebuild_rows();
        self.render();
    }

    /// Mode panel (bukan kapsul + tombol): semua mode kecuali Cari yang masih kosong.
    fn compact(&self) -> bool {
        self.mode == Mode::Search && self.query.trim().is_empty()
    }

    fn request_files(&mut self) {
        let q = self.query.trim().to_string();
        self.files_gen += 1;
        if q.chars().count() < 2 {
            self.files.clear();
            return;
        }
        let (gen, shared) = (self.files_gen, self.shared_files.clone());
        let h = CTL.with(|c| c.borrow().as_ref().map(|c| c.hwnd.0 as isize)).unwrap_or(0);
        std::thread::spawn(move || {
            let hits = filesearch::search(&q);
            {
                // hasil dari ketikan yang lebih lama tidak boleh menimpa yang lebih baru
                let mut g = shared.lock().unwrap();
                if gen >= g.0 {
                    *g = (gen, hits);
                }
            }
            unsafe {
                let _ = PostMessageW(Some(HWND(h as *mut _)), WM_FILES, WPARAM(0), LPARAM(0));
            }
        });
    }

    fn take_files(&mut self) -> bool {
        let g = self.shared_files.lock().unwrap();
        if g.0 == self.files_gen && g.0 != 0 {
            self.files = g.1.clone();
            true
        } else {
            false
        }
    }

    fn rebuild_rows(&mut self) {
        self.rows.clear();
        self.sel = 0;
        self.scroll = 0;
        let q = self.query.trim().to_string();
        match self.mode {
            Mode::Search => {
                if q.is_empty() {
                    return;
                }
                let f = self.filter;
                if f.is_none() {
                    if let Some(v) = search::eval(&q) {
                        self.rows.push(Row { kind: RKind::Calc, title: format!("= {}", search::format_number(v)), sub: prefs::tr("Hasil kalkulator").to_string(), path: String::new() });
                    }
                }
                if f.is_none() || f == Some(Filter::Apps) {
                    for i in search::rank(&q, &self.names, if f.is_some() { 8 } else { 5 }) {
                        self.rows.push(Row { kind: RKind::App, title: self.apps[i].name.clone(), sub: prefs::tr("Aplikasi").to_string(), path: self.apps[i].path.clone() });
                    }
                }
                if f.is_none() || f == Some(Filter::Actions) {
                    let titles = |a: &model::ActionDef| action_title(a);
                    for i in model::rank_actions(&q, &self.quick, &titles).into_iter().take(if f.is_some() { 8 } else { 2 }) {
                        self.rows.push(self.action_row(i));
                    }
                }
                if f.is_none() || f == Some(Filter::Files) {
                    for h in self.files.iter().take(if f.is_some() { 8 } else { 3 }) {
                        self.rows.push(Row { kind: RKind::File, title: h.name.clone(), sub: h.path.clone(), path: h.path.clone() });
                    }
                }
                if f.is_none() {
                    self.rows.push(Row {
                        kind: RKind::Web,
                        title: format!("{} \"{}\"", prefs::tr("Cari di web:"), q),
                        sub: prefs::tr("Pencarian web").to_string(),
                        path: format!("https://www.google.com/search?q={}", url_encode(&q)),
                    });
                }
                self.rows.truncate(9);
            }
            Mode::Apps => {
                let idxs: Vec<usize> = if q.is_empty() { (0..self.apps.len()).collect() } else { search::rank(&q, &self.names, 200) };
                for i in idxs {
                    let a = &self.apps[i];
                    if self.cat.map(|c| appcat::guess(&a.name, &a.path) == c).unwrap_or(true) {
                        self.rows.push(Row { kind: RKind::App, title: a.name.clone(), sub: String::new(), path: a.path.clone() });
                    }
                }
            }
            Mode::Files => {
                for h in &self.files {
                    self.rows.push(Row { kind: RKind::File, title: h.name.clone(), sub: h.path.clone(), path: h.path.clone() });
                }
            }
            Mode::Actions => {
                let titles = |a: &model::ActionDef| action_title(a);
                for i in model::rank_actions(&q, &self.quick, &titles) {
                    self.rows.push(self.action_row(i));
                }
            }
            Mode::Clipboard => {
                let ql = q.to_lowercase();
                for e in clipboard::history() {
                    if ql.is_empty() || e.text.to_lowercase().contains(&ql) {
                        let first = e.text.lines().next().unwrap_or("").trim().to_string();
                        self.rows.push(Row { kind: RKind::Clip, title: first, sub: clip_time(e.at), path: e.text.clone() });
                    }
                }
            }
        }
    }

    fn action_row(&self, i: usize) -> Row {
        let a = &ACTIONS[i];
        Row { kind: RKind::Action(i), title: action_title(a), sub: prefs::tr(a.app).to_string(), path: a.id.to_string() }
    }

    // ------------------------------------------------------------------------------------------------------------
    // gambar
    // ------------------------------------------------------------------------------------------------------------

    fn text(&mut self, parent: &ContainerVisual, s: &str, x: f32, cy: f32, px: i32, weight: i32, rgb: [u8; 3], op: f32, max_w: f32) -> f32 {
        if s.is_empty() {
            return 0.0;
        }
        let key = format!("{px}|{weight}|{rgb:?}|{}|{s}", max_w as i32);
        let hit = self.text_cache.get(&key).cloned();
        let (surf, w, h) = match hit {
            Some(v) => v,
            None => {
                let style = Style { px, weight };
                let (tw, th) = measure(s, &style);
                let tw = (tw + 2).min(max_w as i32).max(2);
                let Some(bmp) = render(s, &style, tw, th) else { return 0.0 };
                let Ok(sf) = self.gfx.surface_from_bgra(bmp.w, bmp.h, &recolor(&bmp, rgb)) else { return 0.0 };
                if self.text_cache.len() > 300 {
                    self.text_cache.clear();
                }
                let v = (sf, tw, th);
                self.text_cache.insert(key, v.clone());
                v
            }
        };
        self.sprite(parent, &surf, x, cy - h as f32 / 2.0, w as f32, h as f32, op);
        w as f32
    }

    fn text_w(&self, s: &str, px: i32, weight: i32) -> f32 {
        measure(s, &Style { px, weight }).0 as f32 + 2.0
    }

    fn sprite(&self, parent: &ContainerVisual, s: &CompositionDrawingSurface, x: f32, y: f32, w: f32, h: f32, op: f32) {
        let go = || -> windows_core::Result<()> {
            let b = self.comp.CreateSurfaceBrushWithSurface(s)?;
            b.SetStretch(CompositionStretch::Uniform)?;
            let v = self.comp.CreateSpriteVisual()?;
            v.SetBrush(&b)?;
            v.SetSize(Vector2 { X: w, Y: h })?;
            v.SetOffset(Vector3 { X: x, Y: y, Z: 0.0 })?;
            v.SetOpacity(op)?;
            parent.Children()?.InsertAtTop(&v)
        };
        let _ = go();
    }

    fn rect(&self, parent: &ContainerVisual, r: Rect, radius: f32, c: Color) {
        let go = || -> windows_core::Result<()> {
            let v = self.comp.CreateSpriteVisual()?;
            v.SetSize(Vector2 { X: r.2, Y: r.3 })?;
            v.SetOffset(Vector3 { X: r.0, Y: r.1, Z: 0.0 })?;
            v.SetBrush(&self.comp.CreateColorBrushWithColor(c)?)?;
            if radius > 0.0 {
                let g = self.comp.CreateRoundedRectangleGeometry()?;
                g.SetSize(Vector2 { X: r.2, Y: r.3 })?;
                g.SetCornerRadius(Vector2 { X: radius, Y: radius })?;
                let clip = self.comp.CreateGeometricClip()?;
                clip.SetGeometry(&g)?;
                v.SetClip(&clip)?;
            }
            parent.Children()?.InsertAtTop(&v)
        };
        let _ = go();
    }

    fn icon_surface(&mut self, path: &str, px: i32) -> Option<CompositionDrawingSurface> {
        let key = format!("{px}|{path}");
        if let Some(c) = self.icon_cache.get(&key) {
            return c.clone();
        }
        let plates = prefs::get().dock_plates;
        let s = icons::load(path, px).and_then(|mut b| {
            // ikon dari paket/pilihan pengguna sudah berbentuk jadi: tanpa pelat dan gaya tambahan (sama dengan dock)
            if !icons::is_themed(path) {
                if plates && b.w == b.h {
                    b.bgra = nacre_core::iconplate::plate(&b.bgra, b.w);
                }
                let p = prefs::get();
                b.bgra = nacre_core::iconplate::style_icon(&b.bgra, p.icon_style, p.icon_tint);
            }
            self.gfx.surface_from_bgra(b.w, b.h, &b.bgra).ok()
        });
        if self.icon_cache.len() > 200 {
            self.icon_cache.clear();
        }
        self.icon_cache.insert(key, s.clone());
        s
    }

    fn sym(&mut self, parent: &ContainerVisual, s: Sym, cx: f32, cy: f32, size: f32, rgb: [u8; 3], op: f32) {
        let n = size.round().max(8.0) as u32;
        let key = format!("sym|{s:?}|{n}|{rgb:?}");
        let hit = self.text_cache.get(&key).cloned();
        let (surf, w, h) = match hit {
            Some(v) => v,
            None => {
                let data = symbols::render(s, n, rgb);
                let Some((cd, cw, ch)) = symbols::crop_ink(&data, n, n) else { return };
                let Ok(sf) = self.gfx.surface_from_bgra(cw, ch, &cd) else { return };
                let v = (sf, cw as i32, ch as i32);
                self.text_cache.insert(key, v.clone());
                v
            }
        };
        self.sprite(parent, &surf, cx - w as f32 / 2.0, cy - h as f32 / 2.0, w as f32, h as f32, op);
    }

    fn glyph(&mut self, parent: &ContainerVisual, g: &str, cx: f32, cy: f32, size: f32, rgb: [u8; 3], op: f32) {
        let px = size.round() as i32;
        let b = (size * 1.6).round() as i32;
        let key = format!("g|{px}|{rgb:?}|{g}");
        let hit = self.text_cache.get(&key).cloned();
        let (surf, w, h) = match hit {
            Some(v) => v,
            None => {
                let Some(bmp) = render_face(g, &Style { px, weight: 400 }, ICON_FACE, b, b) else { return };
                let Some((cd, cw, ch)) = symbols::crop_ink(&recolor(&bmp, rgb), bmp.w, bmp.h) else { return };
                let Ok(sf) = self.gfx.surface_from_bgra(cw, ch, &cd) else { return };
                let v = (sf, cw as i32, ch as i32);
                self.text_cache.insert(key, v.clone());
                v
            }
        };
        self.sprite(parent, &surf, cx - w as f32 / 2.0, cy - h as f32 / 2.0, w as f32, h as f32, op);
    }

    /// Ikon mode (kapsul kosong/tombol bulat/judul panel).
    fn mode_icon(&mut self, parent: &ContainerVisual, m: Mode, cx: f32, cy: f32, size: f32, rgb: [u8; 3], op: f32) {
        match m {
            Mode::Search => self.sym(parent, Sym::Search, cx, cy, size, rgb, op),
            Mode::Apps => self.sym(parent, Sym::Grid, cx, cy, size * 0.95, rgb, op),
            Mode::Files => self.glyph(parent, "\u{E8B7}", cx, cy, size * 0.95, rgb, op),
            Mode::Actions => self.sym(parent, Sym::Windows, cx, cy, size, rgb, op),
            Mode::Clipboard => self.glyph(parent, "\u{E77F}", cx, cy, size * 0.95, rgb, op),
        }
    }

    fn glass_circle(&self, parent: &ContainerVisual, r: Rect) -> Option<GlassPanel> {
        let base = GlassParams::dark((r.2, r.3), r.2 / 2.0);
        let p = GlassPanel::new(&self.comp, parent, prefs::apply(base, &prefs::look(prefs::Role::Dock))).ok()?;
        let _ = p.set_rect(r.0, r.1, r.2, r.3);
        let _ = p.set_radius(r.2 / 2.0);
        Some(p)
    }

    fn render(&mut self) {
        let content = self.popup.content.clone();
        if let Ok(k) = content.Children() {
            let _ = k.RemoveAll();
        }
        self.hits.clear();
        self.circles.clear();
        let d = self.dpi;
        let look = self.look();
        let fg = look.fg;
        let w = self.width();
        let head_h = 56.0 * d;
        let (x0, y0) = (((self.screen.0 as f32 - w) / 2.0) as i32, (self.screen.1 as f32 * 0.20) as i32);

        if self.compact() {
            // kapsul + empat tombol bulat (Apps, Berkas, Aksi, Papan Klip)
            let btn = 48.0 * d;
            let gap = 10.0 * d;
            let cap_w = w - 4.0 * (btn + gap);
            let _ = self.popup.set_rect(x0, y0, w.ceil() as i32, head_h.ceil() as i32);
            let _ = self.popup.panel.set_rect(0.0, 0.0, cap_w, head_h);
            let _ = self.popup.panel.set_radius(head_h / 2.0);
            self.sym(&content, Sym::Search, 30.0 * d, head_h / 2.0, 22.0 * d, fg, 0.8);
            self.text(&content, &prefs::tr("Cari Spotlight").to_string(), 60.0 * d, head_h / 2.0, (22.0 * d).round() as i32, 400, fg, 0.5, cap_w - 90.0 * d);
            let modes = [Mode::Apps, Mode::Files, Mode::Actions, Mode::Clipboard];
            for (i, m) in modes.iter().enumerate() {
                let r: Rect = (cap_w + gap + i as f32 * (btn + gap), (head_h - btn) / 2.0, btn, btn);
                if let Some(g) = self.glass_circle(&content, r) {
                    self.circles.push(g);
                }
                self.mode_icon(&content, *m, r.0 + btn / 2.0, r.1 + btn / 2.0, 22.0 * d, fg, 0.9);
                self.hits.push((r, HitKind::Mode(*m)));
            }
            return;
        }

        // ---------- panel ----------
        let has_chips = matches!(self.mode, Mode::Search | Mode::Apps);
        let chips_h = if has_chips { 42.0 * d } else { 0.0 };
        let row_h = 54.0 * d;
        let list_len = self.rows.len().max(1);
        let body_h = if self.mode == Mode::Apps {
            GRID_ROWS as f32 * self.cell_h() + 14.0 * d
        } else {
            (list_len.min(8)) as f32 * row_h + 12.0 * d
        };
        let h = head_h + chips_h + body_h;
        let _ = self.popup.set_rect(x0, y0, w.ceil() as i32, h.ceil() as i32);
        let _ = self.popup.panel.set_rect(0.0, 0.0, w, h);
        let _ = self.popup.panel.set_radius(28.0 * d);

        // kepala: ikon + teks (judul bila kosong) + keterangan mode
        let cy = head_h / 2.0;
        self.mode_icon(&content, self.mode, 30.0 * d, cy, 22.0 * d, fg, 0.85);
        let px = (22.0 * d).round() as i32;
        let tx = 60.0 * d;
        if self.query.is_empty() {
            let (title, op) = if self.mode == Mode::Apps { (prefs::tr(self.mode.title()).to_string(), 0.95) } else { (prefs::tr(self.mode.title()).to_string(), 0.5) };
            self.text(&content, &title, tx, cy, px, 400, fg, op, w - 160.0 * d);
        } else {
            let q = self.query.clone();
            let qw = self.text(&content, &q, tx, cy, px, 400, fg, 1.0, w - 200.0 * d);
            if self.mode == Mode::Search && !self.rows.is_empty() {
                let open = format!("\u{2014} {}", prefs::tr("Buka"));
                self.text(&content, &open, tx + qw + 8.0 * d, cy, (17.0 * d).round() as i32, 400, fg, 0.45, 160.0 * d);
            }
        }
        if self.mode != Mode::Search {
            // keycap "Ctrl 3" di kanan
            let label = format!("Ctrl+{}", self.mode.digit());
            let cw = self.text_w(&label, (12.0 * d).round() as i32, 500) + 18.0 * d;
            let r: Rect = (w - cw - 22.0 * d, cy - 12.0 * d, cw, 24.0 * d);
            self.rect(&content, r, 7.0 * d, Color { A: 38, R: fg[0], G: fg[1], B: fg[2] });
            self.text(&content, &label, r.0 + 9.0 * d, cy, (12.0 * d).round() as i32, 500, fg, 0.7, cw);
        } else if let Some(row) = self.rows.get(self.sel).cloned() {
            // ikon hasil terpilih di kanan atas
            if matches!(row.kind, RKind::App | RKind::File) {
                if let Some(s) = self.icon_surface(&row.path, (64.0 * d) as i32) {
                    self.sprite(&content, &s, w - 56.0 * d, cy - 17.0 * d, 34.0 * d, 34.0 * d, 1.0);
                }
            }
        }
        if !self.rows.is_empty() {
            self.rect(&content, (0.0, head_h, w, 1.0), 0.0, Color { A: 34, R: fg[0], G: fg[1], B: fg[2] });
        }

        // chip
        if has_chips {
            let mut x = 22.0 * d;
            let cy2 = head_h + chips_h / 2.0 + 1.0 * d;
            let cpx = (13.0 * d).round() as i32;
            let items: Vec<(String, HitKind, bool)> = if self.mode == Mode::Search {
                vec![
                    (prefs::tr("Aplikasi").to_string(), HitKind::Filter(Filter::Apps), self.filter == Some(Filter::Apps)),
                    (prefs::tr("Berkas Saya").to_string(), HitKind::Filter(Filter::Files), self.filter == Some(Filter::Files)),
                    (prefs::tr("Aksi").to_string(), HitKind::Filter(Filter::Actions), self.filter == Some(Filter::Actions)),
                ]
            } else {
                let mut v = vec![(prefs::tr("Semua").to_string(), HitKind::Cat(None), self.cat.is_none())];
                for c in appcat::ALL {
                    v.push((prefs::tr(c.title()).to_string(), HitKind::Cat(Some(c)), self.cat == Some(c)));
                }
                v
            };
            for (label, hk, active) in items {
                let cw = self.text_w(&label, cpx, 500) + 24.0 * d;
                let r: Rect = (x, cy2 - 14.0 * d, cw, 28.0 * d);
                let col = if active { crate::prefs::accent_c(90) } else { Color { A: 34, R: fg[0], G: fg[1], B: fg[2] } };
                self.rect(&content, r, 14.0 * d, col);
                self.text(&content, &label, r.0 + 12.0 * d, cy2, cpx, 500, fg, if active { 1.0 } else { 0.8 }, cw);
                self.hits.push((r, hk));
                x += cw + 8.0 * d;
            }
        }

        let top = head_h + chips_h;
        if self.mode == Mode::Apps {
            self.draw_grid(&content, w, top, fg);
        } else {
            self.draw_rows(&content, w, top, row_h, fg);
        }
    }

    fn cell_h(&self) -> f32 {
        112.0 * self.dpi
    }

    fn draw_grid(&mut self, content: &ContainerVisual, w: f32, top: f32, fg: [u8; 3]) {
        let d = self.dpi;
        let (cw, ch) = ((w - 40.0 * d) / COLS as f32, self.cell_h());
        let icon = (cw * 0.5).min(72.0 * d);
        let per = COLS * GRID_ROWS;
        let start = self.scroll * COLS;
        let n = self.rows.len();
        if n == 0 {
            self.text(content, &prefs::tr("Tidak ada aplikasi").to_string(), w / 2.0 - 60.0 * d, top + 60.0 * d, (15.0 * d).round() as i32, 400, fg, 0.6, w);
            return;
        }
        for i in 0..per {
            let idx = start + i;
            if idx >= n {
                break;
            }
            let (r, c) = (i / COLS, i % COLS);
            let cx = 20.0 * d + c as f32 * cw + cw / 2.0;
            let cy0 = top + 6.0 * d + r as f32 * ch;
            let row = self.rows[idx].clone();
            if idx == self.sel {
                self.rect(content, (cx - cw / 2.0 + 4.0 * d, cy0, cw - 8.0 * d, ch - 4.0 * d), 14.0 * d, crate::prefs::accent_c(60));
            }
            if let Some(s) = self.icon_surface(&row.path, (icon * 1.6) as i32) {
                self.sprite(content, &s, cx - icon / 2.0, cy0 + 8.0 * d, icon, icon, 1.0);
            } else {
                self.rect(content, (cx - icon / 2.0, cy0 + 8.0 * d, icon, icon), icon * 0.23, Color { A: 60, R: fg[0], G: fg[1], B: fg[2] });
            }
            let lw = self.text_w(&row.title, (13.0 * d).round() as i32, 400).min(cw - 10.0 * d);
            self.text(content, &row.title, cx - lw / 2.0, cy0 + 8.0 * d + icon + 14.0 * d, (13.0 * d).round() as i32, 400, fg, 0.95, cw - 10.0 * d);
            self.hits.push(((cx - cw / 2.0, cy0, cw, ch), HitKind::Cell(idx)));
        }
    }

    fn draw_rows(&mut self, content: &ContainerVisual, w: f32, top: f32, row_h: f32, fg: [u8; 3]) {
        let d = self.dpi;
        if self.rows.is_empty() {
            if self.mode != Mode::Search {
                let msg = match self.mode {
                    Mode::Clipboard => prefs::tr("Belum ada yang disalin dalam 8 jam terakhir."),
                    _ => prefs::tr("Tidak ada hasil"),
                };
                let hint = if self.mode == Mode::Files && self.query.trim().chars().count() < 2 { prefs::tr("Ketik untuk mencari berkas.") } else { msg };
                self.text(content, &hint.to_string(), 26.0 * d, top + row_h / 2.0, (15.0 * d).round() as i32, 400, fg, 0.6, w - 40.0 * d);
            }
            return;
        }
        let visible = 8usize;
        let first = self.scroll.min(self.rows.len().saturating_sub(1));
        for (k, i) in (first..self.rows.len().min(first + visible)).enumerate() {
            let ry = top + 4.0 * d + k as f32 * row_h;
            let row = self.rows[i].clone();
            let sel = i == self.sel;
            if sel {
                self.rect(content, (8.0 * d, ry, w - 16.0 * d, row_h - 2.0 * d), 12.0 * d, crate::prefs::accent_c(62));
            }
            let ic = 34.0 * d;
            let (icx, icy) = (22.0 * d + ic / 2.0, ry + (row_h - 2.0 * d) / 2.0);
            match &row.kind {
                RKind::App | RKind::File => {
                    if let Some(s) = self.icon_surface(&row.path, (ic * 2.0) as i32) {
                        self.sprite(content, &s, icx - ic / 2.0, icy - ic / 2.0, ic, ic, 1.0);
                    }
                }
                RKind::Calc => {
                    self.rect(content, (icx - ic / 2.0, icy - ic / 2.0, ic, ic), 8.0 * d, Color { A: 255, R: 255, G: 149, B: 0 });
                    self.glyph(content, "\u{E8EF}", icx, icy, 18.0 * d, [255, 255, 255], 1.0);
                }
                RKind::Web => {
                    self.rect(content, (icx - ic / 2.0, icy - ic / 2.0, ic, ic), 8.0 * d, crate::prefs::accent_c(255));
                    self.glyph(content, "\u{E774}", icx, icy, 18.0 * d, [255, 255, 255], 1.0);
                }
                RKind::Action(_) => {
                    let (g, col) = action_glyph(&row.path);
                    self.rect(content, (icx - ic / 2.0, icy - ic / 2.0, ic, ic), 8.0 * d, Color { A: 255, R: col[0], G: col[1], B: col[2] });
                    self.glyph(content, g, icx, icy, 18.0 * d, [255, 255, 255], 1.0);
                }
                RKind::Clip => {
                    self.rect(content, (icx - ic / 2.0, icy - ic / 2.0, ic, ic), 8.0 * d, Color { A: 255, R: 142, G: 142, B: 147 });
                    self.glyph(content, "\u{E77F}", icx, icy, 18.0 * d, [255, 255, 255], 1.0);
                }
            }
            let tx = 22.0 * d + ic + 14.0 * d;
            let max_w = w - tx - 150.0 * d;
            self.text(content, &row.title, tx, icy - 9.0 * d, (16.5 * d).round() as i32, 400, fg, 1.0, max_w);
            if !row.sub.is_empty() {
                self.text(content, &row.sub, tx, icy + 11.0 * d, (12.5 * d).round() as i32, 400, fg, 0.58, max_w);
            }
            // sisi kanan: quick key aksi / chip
            if let RKind::Action(ai) = row.kind {
                let key = model::key_of(&ACTIONS[ai], &self.quick).to_string();
                let editing = self.editing == Some(ai);
                let label = if editing { format!("{}|", self.edit_buf) } else if key.is_empty() { prefs::tr("Tambah kunci cepat").to_string() } else { key };
                let cpx = (12.0 * d).round() as i32;
                let cw = self.text_w(&label, cpx, 500) + 18.0 * d;
                let r: Rect = (w - cw - 24.0 * d, icy - 11.0 * d, cw, 22.0 * d);
                self.rect(content, r, 6.0 * d, Color { A: if editing { 80 } else { 36 }, R: fg[0], G: fg[1], B: fg[2] });
                self.text(content, &label, r.0 + 9.0 * d, icy, cpx, 500, fg, 0.75, cw);
            }
            self.hits.push(((0.0, ry, w, row_h), HitKind::Row(i)));
        }
    }

    // ------------------------------------------------------------------------------------------------------------
    // aksi
    // ------------------------------------------------------------------------------------------------------------

    fn take_run(&mut self) -> Run {
        let Some(r) = self.rows.get(self.sel).cloned() else { return Run::Nothing };
        match r.kind {
            RKind::App | RKind::Web | RKind::File => Run::Open(r.path),
            RKind::Calc => Run::Nothing,
            RKind::Action(i) => Run::Action(ACTIONS[i].id),
            RKind::Clip => Run::Copy(r.path),
        }
    }

    fn move_sel(&mut self, dx: i32, dy: i32) {
        let n = self.rows.len();
        if n == 0 {
            return;
        }
        if self.mode == Mode::Apps {
            let cur = self.sel as i32;
            let next = (cur + dx + dy * COLS as i32).clamp(0, n as i32 - 1) as usize;
            self.sel = next;
            let row = self.sel / COLS;
            if row < self.scroll {
                self.scroll = row;
            } else if row >= self.scroll + GRID_ROWS {
                self.scroll = row + 1 - GRID_ROWS;
            }
        } else {
            let step = if dy != 0 { dy } else { dx };
            self.sel = ((self.sel as i32 + step).rem_euclid(n as i32)) as usize;
            if self.sel < self.scroll {
                self.scroll = self.sel;
            } else if self.sel >= self.scroll + 8 {
                self.scroll = self.sel + 1 - 8;
            }
        }
        self.render();
    }

    fn start_edit_key(&mut self) {
        if let Some(RKind::Action(i)) = self.rows.get(self.sel).map(|r| r.kind.clone()) {
            self.editing = Some(i);
            self.edit_buf = model::key_of(&ACTIONS[i], &self.quick).to_string();
            self.render();
        }
    }

    fn finish_edit_key(&mut self, save: bool) {
        if let (Some(i), true) = (self.editing, save) {
            let k = self.edit_buf.trim().to_lowercase();
            self.quick.insert(ACTIONS[i].id.to_string(), k);
            let _ = std::fs::write(quick_file(), model::quick_keys_to_text(&self.quick));
        }
        self.editing = None;
        self.edit_buf.clear();
        self.rebuild_rows();
        self.render();
    }
}

fn clip_time(at: i64) -> String {
    // jam lokal dari selisih terhadap sekarang (tanpa tabel zona waktu): pakai jam lokal sekarang dikurangi umur
    let now_local = time::local_now();
    let now_secs = (now_local.hour * 3600 + now_local.minute * 60 + now_local.second) as i64;
    let age = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0) - at;
    let t = (now_secs - age).rem_euclid(86_400);
    let (h, m) = ((t / 3600) as u32, ((t % 3600) / 60) as u32);
    if prefs::get().clock_24h {
        format!("{h:02}.{m:02}")
    } else {
        format!("{}:{m:02} {}", if h % 12 == 0 { 12 } else { h % 12 }, if h < 12 { "AM" } else { "PM" })
    }
}

fn run_action(id: &str) {
    match id {
        "note" => launch::run("notepad.exe", ""),
        "timer" => launch::open("ms-clock:"),
        "email" => launch::open("mailto:"),
        "screenshot" => keys::chord(&[keys::WIN, keys::SHIFT, 0x53]),
        "lock" => launch::run("rundll32.exe", "user32.dll,LockWorkStation"),
        "desktop" => keys::chord(&[keys::WIN, 0x44]),
        "dark" => prefs::update(|s| {
            s.theme = if s.theme == nacre_core::settings::Theme::Dark { nacre_core::settings::Theme::Light } else { nacre_core::settings::Theme::Dark }
        }),
        "stage" => prefs::update(|s| s.stage = !s.stage),
        "mission" => crate::mission::toggle(),
        "calc" => launch::run("calc.exe", ""),
        "taskmgr" => launch::run("taskmgr.exe", ""),
        "settings" => launch::open("ms-settings:"),
        "clearclip" => clipboard::clear(),
        _ => {}
    }
}

fn perform(r: Run) {
    match r {
        Run::Nothing => {}
        Run::Open(p) => launch::open(&p),
        Run::Action(id) => run_action(id),
        Run::Copy(t) => {
            clipboard::copy_text(&t);
        }
    }
}

unsafe extern "system" fn ctl_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_HOTKEY => {
            if wp.0 as i32 == HOTKEY_APPS {
                toggle_mode(Mode::Apps);
            } else {
                toggle();
            }
            LRESULT(0)
        }
        WM_APPS => {
            // indeks siap: segarkan daftar pada Spotlight yang sedang terbuka
            with_spot(|s| {
                s.apps = apps::snapshot();
                s.names = s.apps.iter().map(|a| a.name.clone()).collect();
                s.rebuild_rows();
                s.render();
            });
            LRESULT(0)
        }
        WM_FILES => {
            with_spot(|s| {
                if s.take_files() && matches!(s.mode, Mode::Search | Mode::Files) {
                    let (sel, scroll) = (s.sel, s.scroll);
                    s.rebuild_rows();
                    s.sel = sel.min(s.rows.len().saturating_sub(1));
                    s.scroll = scroll;
                    s.render();
                }
            });
            LRESULT(0)
        }
        m if m == prefs::WM_PREFS => {
            clipboard::set_enabled(prefs::get().clipboard_history);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

fn ctrl_down() -> bool {
    unsafe { (GetKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0 }
}

unsafe extern "system" fn spot_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_CHAR => {
            let c = wp.0 as u32;
            if c >= 32 && c != 127 && !ctrl_down() {
                if let Some(ch) = char::from_u32(c) {
                    with_spot(|s| {
                        if s.editing.is_some() {
                            if ch.is_ascii_alphanumeric() && s.edit_buf.len() < 6 {
                                s.edit_buf.push(ch.to_ascii_lowercase());
                                s.render();
                            }
                            return;
                        }
                        s.query.push(ch);
                        if matches!(s.mode, Mode::Search | Mode::Files) {
                            s.request_files();
                        }
                        s.rebuild_rows();
                        s.render();
                    });
                }
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            let vk = wp.0 as u16;
            if ctrl_down() && (0x31..=0x34).contains(&vk) {
                if let Some(m) = Mode::from_digit((vk - 0x30) as u32) {
                    with_spot(|s| s.set_mode(m));
                }
                return LRESULT(0);
            }
            if vk == VK_ESCAPE.0 {
                let handled = with_spot(|s| {
                    if s.editing.is_some() {
                        s.finish_edit_key(false);
                        return true;
                    }
                    if !s.query.is_empty() {
                        s.query.clear();
                        s.files.clear();
                        s.rebuild_rows();
                        s.render();
                        return true;
                    }
                    if s.mode != Mode::Search {
                        s.set_mode(Mode::Search);
                        return true;
                    }
                    false
                })
                .unwrap_or(false);
                if !handled {
                    close();
                }
            } else if vk == VK_BACK.0 {
                with_spot(|s| {
                    if s.editing.is_some() {
                        s.edit_buf.pop();
                        s.render();
                        return;
                    }
                    s.query.pop();
                    if matches!(s.mode, Mode::Search | Mode::Files) {
                        s.request_files();
                    }
                    s.rebuild_rows();
                    s.render();
                });
            } else if vk == VK_TAB.0 {
                with_spot(|s| {
                    if s.mode == Mode::Actions || s.mode == Mode::Search {
                        s.start_edit_key();
                    }
                });
            } else if vk == VK_DOWN.0 || vk == VK_UP.0 || vk == VK_LEFT.0 || vk == VK_RIGHT.0 {
                with_spot(|s| {
                    let (dx, dy) = match vk {
                        v if v == VK_DOWN.0 => (0, 1),
                        v if v == VK_UP.0 => (0, -1),
                        v if v == VK_LEFT.0 => (-1, 0),
                        _ => (1, 0),
                    };
                    // kiri/kanan hanya bermakna pada grid Apps
                    if (dx != 0 && s.mode != Mode::Apps) || s.editing.is_some() {
                        return;
                    }
                    s.move_sel(dx, dy);
                });
            } else if vk == VK_RETURN.0 {
                let run = with_spot(|s| {
                    if s.editing.is_some() {
                        s.finish_edit_key(true);
                        return Run::Nothing;
                    }
                    s.take_run()
                })
                .unwrap_or(Run::Nothing);
                if !matches!(run, Run::Nothing) {
                    close();
                    perform(run);
                }
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = (wp.0 >> 16) as i16;
            with_spot(|s| {
                let step = if s.mode == Mode::Apps { 1 } else { 2 };
                let per = if s.mode == Mode::Apps { COLS } else { 1 };
                let max_scroll = if s.mode == Mode::Apps { s.rows.len().div_ceil(per).saturating_sub(GRID_ROWS) } else { s.rows.len().saturating_sub(8) };
                s.scroll = if delta < 0 { (s.scroll + step).min(max_scroll) } else { s.scroll.saturating_sub(step) };
                s.render();
            });
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let (x, y) = ((lp.0 as i16) as f32, ((lp.0 >> 16) as i16) as f32);
            with_spot(|s| {
                let h = s.hits.iter().find(|(r, k)| hit(*r, x, y) && matches!(k, HitKind::Row(_) | HitKind::Cell(_))).map(|(_, k)| *k);
                let idx = match h {
                    Some(HitKind::Row(i)) | Some(HitKind::Cell(i)) => Some(i),
                    _ => None,
                };
                if let Some(i) = idx {
                    if i != s.sel {
                        s.sel = i;
                        s.render();
                    }
                }
            });
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let (x, y) = ((lp.0 as i16) as f32, ((lp.0 >> 16) as i16) as f32);
            let run = with_spot(|s| {
                let Some(k) = s.hits.iter().find(|(r, _)| hit(*r, x, y)).map(|(_, k)| *k) else { return Run::Nothing };
                match k {
                    HitKind::Mode(m) => {
                        s.set_mode(m);
                        Run::Nothing
                    }
                    HitKind::Filter(f) => {
                        s.filter = if s.filter == Some(f) { None } else { Some(f) };
                        s.rebuild_rows();
                        s.render();
                        Run::Nothing
                    }
                    HitKind::Cat(c) => {
                        s.cat = c;
                        s.rebuild_rows();
                        s.render();
                        Run::Nothing
                    }
                    HitKind::Row(i) | HitKind::Cell(i) => {
                        s.sel = i;
                        s.take_run()
                    }
                }
            })
            .unwrap_or(Run::Nothing);
            if !matches!(run, Run::Nothing) {
                close();
                perform(run);
            }
            LRESULT(0)
        }
        WM_ACTIVATE => {
            // kehilangan fokus (klik di luar, alt-tab): tutup
            if (wp.0 & 0xFFFF) == 0 {
                let _ = PostMessageW(Some(hwnd), WM_CLOSE_SPOT, WPARAM(0), LPARAM(0));
            }
            LRESULT(0)
        }
        m if m == WM_CLOSE_SPOT => {
            close();
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
