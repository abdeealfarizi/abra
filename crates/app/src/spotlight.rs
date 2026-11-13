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
use windows::UI::Composition::{CompositionDrawingSurface, CompositionObject, CompositionStretch, Compositor, ContainerVisual, InsetClip};
use windows::Win32::Graphics::Gdi::{CreateRectRgn, SetWindowRgn};
use windows_core::{Interface, HSTRING};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT, RegisterHotKey, UnregisterHotKey, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, VK_BACK, VK_CONTROL, VK_DOWN, VK_ESCAPE, VK_LEFT, VK_RETURN, VK_RIGHT,
    VK_SPACE, VK_TAB, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows_numerics::{Vector2, Vector3};

const WM_APPS: u32 = WM_APP + 80;
const WM_CLOSE_SPOT: u32 = WM_APP + 81;
const WM_FILES: u32 = WM_APP + 84;
const WM_THUMBS: u32 = WM_APP + 85;
const WM_MOUSELEAVE_SPOT: u32 = 0x02A3;
/// Timer pada jendela Spotlight: tahap 2 morph kapsul → panel, dan merapikan region klik setelah animasi.
const T_MORPH: usize = 1;
const T_REGION: usize = 2;
/// Timer pada jendela kontrol: menghancurkan jendela setelah fade-out tutup.
const T_FINISH: usize = 9;
const HOTKEY_ID: i32 = 0xAB01;
const HOTKEY_APPS: i32 = 0xAB02;
const COLS: usize = 5;
/// Chip jenis berkas di mode Berkas (padanan chip app di video): label + ekstensi.
const FTYPES: [(&str, &[&str]); 4] = [
    ("Gambar", &["png", "jpg", "jpeg", "gif", "webp", "heic", "bmp", "svg", "tif", "tiff"]),
    ("Video", &["mp4", "mov", "mkv", "avi", "webm", "wmv", "m4v"]),
    ("Dokumen", &["pdf", "doc", "docx", "txt", "md", "rtf", "odt", "xls", "xlsx", "csv", "ppt", "pptx"]),
    ("Musik", &["mp3", "wav", "flac", "m4a", "aac", "ogg", "wma"]),
];

fn of_type(path: &str, t: Option<usize>) -> bool {
    let Some(t) = t else { return true };
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    FTYPES[t].1.contains(&ext.as_str())
}
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
    /// "Lihat Semua" pada seksi ke-n.
    ShowMore(usize),
    /// Tombol "⋯" di kepala panel dan item menunya.
    More,
    MoreItem,
    /// Tombol salin di baris Papan Klip.
    CopyRow(usize),
    /// Chip jenis berkas ke-n.
    FType(usize),
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
    geo: Geo,
    /// Persegi badan kaca saat ini (koordinat jendela).
    cur: Rect,
    /// Tombol mode tampil di samping kapsul (kursor di atas Spotlight).
    buttons: bool,
    buttons_intro: bool,
    side_fading: bool,
    hover_mode: Option<Mode>,
    tracking: bool,
    clip: InsetClip,
    /// Lapisan tombol mode (tidak ikut klip/geser isi).
    side: ContainerVisual,
    /// Seksi grid (Apps/Berkas): judul + rentang indeks `rows`.
    sections: Vec<(String, std::ops::Range<usize>)>,
    expanded: Vec<bool>,
    pscroll: f32,
    content_h: f32,
    view: (f32, f32),
    cell_y: Vec<(usize, f32, f32)>,
    more_open: bool,
    /// Judul seksi daftar (Aksi): sebelum baris ke-n.
    headers: Vec<(usize, String)>,
    recents: Arc<Mutex<Vec<(String, String)>>>,
    thumbs: Arc<Mutex<HashMap<String, Bitmap>>>,
    thumb_surf: HashMap<String, (CompositionDrawingSurface, u32, u32)>,
    recents_requested: bool,
    morphing: bool,
    ftype: Option<usize>,
}

/// Ukuran Spotlight (piksel perangkat), diturunkan dari video referensi: kapsul ±54 pt; saat tombol mode muncul
/// kapsul memendek ±15 % dan empat tombol bulat setinggi kapsul berjajar di kanannya; panel mode selebar
/// kapsul pendek + empat tombol; kapsul berada di sekitar 23 % tinggi layar.
#[derive(Clone, Copy)]
struct Geo {
    h: f32,
    ws: f32,
    cap: f32,
    btn: f32,
    gap: f32,
    gw: f32,
    max_h: f32,
    win: (i32, i32, i32, i32),
}

fn geo(dpi: f32, screen: (i32, i32)) -> Geo {
    let d = dpi;
    let (sw, sh) = (screen.0 as f32, screen.1 as f32);
    let h = (54.0 * d).round();
    let btn = h;
    let gap = (12.0 * d).round();
    let ws = (620.0 * d).min(sw * 0.55).round();
    let cap = (ws * 0.85).round();
    let gw = cap + 4.0 * (btn + gap);
    let max_h = (sh * 0.6).min(600.0 * d).max(h * 4.0).round();
    let top = (sh * 0.23 - h / 2.0).round();
    Geo { h, ws, cap, btn, gap, gw, max_h, win: (((sw - gw) / 2.0).round() as i32, top as i32, gw.ceil() as i32, max_h.ceil() as i32) }
}

fn union(a: Rect, b: Rect) -> Rect {
    let (l, t) = (a.0.min(b.0), a.1.min(b.1));
    let (r, bt) = ((a.0 + a.2).max(b.0 + b.2), (a.1 + a.3).max(b.1 + b.3));
    (l, t, r - l, bt - t)
}

/// Pegas compositor pada satu properti skalar.
fn spring(comp: &Compositor, obj: &CompositionObject, prop: &str, v: f32, ratio: f32, period_ms: u32) {
    let go = || -> windows_core::Result<()> {
        let a = comp.CreateSpringScalarAnimation()?;
        a.SetDampingRatio(ratio)?;
        a.SetPeriod(windows::Foundation::TimeSpan { Duration: period_ms as i64 * 10_000 })?;
        a.SetFinalValue(&windows::Foundation::PropertyValue::CreateSingle(v)?.cast::<windows::Foundation::IReference<f32>>()?)?;
        obj.StartAnimation(&HSTRING::from(prop), &a)
    };
    let _ = go();
}

struct Ctl {
    hwnd: HWND,
    comp: Compositor,
}

thread_local! {
    static SPOT: RefCell<Option<Box<Spot>>> = const { RefCell::new(None) };
    /// Spotlight yang sedang memudar keluar (dihancurkan oleh timer `T_FINISH`).
    static CLOSING: RefCell<Option<Box<Spot>>> = const { RefCell::new(None) };
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
            // kapsul → morph; panel → ganti isi
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
                // Seperti video: muncul murni memudar ±80 ms, tanpa skala atau geser.
                s.reset_state(mode);
                s.render();
                let root = s.popup.root().clone();
                let _ = root.SetOpacity(0.0);
                s.popup.show_active();
                let _ = crate::motion::scalar(&s.comp, &root, "Opacity", 0.0, 1.0, 80);
                let _ = root.SetOpacity(1.0);
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

/// Tutup dengan memudar ±90 ms (seperti video); jendela dihancurkan sesudahnya oleh timer.
pub fn close() {
    let s = SPOT.with(|c| c.borrow_mut().take());
    if let Some(s) = s {
        finish_closing();
        let root = s.popup.root().clone();
        let _ = crate::motion::scalar(&s.comp, &root, "Opacity", 1.0, 0.0, 90);
        let _ = root.SetOpacity(0.0);
        CLOSING.with(|c| *c.borrow_mut() = Some(s));
        let ctl = CTL.with(|c| c.borrow().as_ref().map(|c| c.hwnd));
        if let Some(h) = ctl {
            unsafe {
                SetTimer(Some(h), T_FINISH, 110, None);
            }
        }
    }
}

fn finish_closing() {
    let s = CLOSING.with(|c| c.borrow_mut().take());
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
    // Jendela dibuat sekali seukuran panel terbesar; badan kaca di dalamnya yang dianimasikan, dan region membatasi
    // area klik ke badan itu. Dengan begitu kapsul bisa memendek/melebar/tumbuh tanpa jendela berkedip.
    let g = geo(dpi, screen);
    popup.set_rect(g.win.0, g.win.1, g.win.2, g.win.3)?;
    let idle: Rect = ((g.gw - g.ws) / 2.0, 0.0, g.ws, g.h);
    popup.panel.set_rect(idle.0, idle.1, idle.2, idle.3)?;
    popup.panel.set_radius(g.h / 2.0)?;
    // InsetClip memotong ke ukuran visual: kontainer isi harus seukuran jendela, kalau tidak semua isi terpotong habis.
    popup.content.SetSize(Vector2 { X: g.win.2 as f32, Y: g.win.3 as f32 })?;
    let clip = comp.CreateInsetClip()?;
    popup.content.SetClip(&clip)?;
    let side = comp.CreateContainerVisual()?;
    popup.root().Children()?.InsertAtTop(&side)?;
    let gfx = Gfx::new(comp)?;
    let apps = apps::snapshot();
    let names = apps.iter().map(|a| a.name.clone()).collect();
    let quick = model::parse_quick_keys(&std::fs::read_to_string(quick_file()).unwrap_or_default());
    let _ = mode;
    let s = Box::new(Spot {
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
        geo: g,
        cur: idle,
        buttons: false,
        buttons_intro: false,
        side_fading: false,
        hover_mode: None,
        tracking: false,
        clip,
        side,
        sections: vec![],
        expanded: vec![],
        pscroll: 0.0,
        content_h: 0.0,
        view: (0.0, 0.0),
        cell_y: vec![],
        more_open: false,
        headers: vec![],
        recents: Arc::new(Mutex::new(vec![])),
        thumbs: Arc::new(Mutex::new(HashMap::new())),
        thumb_surf: HashMap::new(),
        recents_requested: false,
        morphing: false,
        ftype: None,
    });
    s.set_region(idle);
    Ok(s)
}

impl Spot {
    fn look(&self) -> nacre_core::settings::Look {
        prefs::look(prefs::Role::Dock)
    }

    fn idle_rect(&self) -> Rect {
        let g = self.geo;
        ((g.gw - g.ws) / 2.0, 0.0, g.ws, g.h)
    }

    fn hover_rect(&self) -> Rect {
        let g = self.geo;
        (0.0, 0.0, g.cap, g.h)
    }

    /// Tinggi panel: Cari mengikuti jumlah hasil; mode lain setinggi tetap seperti di video.
    fn panel_h(&self) -> f32 {
        let (g, d) = (self.geo, self.dpi);
        if self.mode != Mode::Search {
            return g.max_h;
        }
        let n = self.rows.len().clamp(1, 8) as f32;
        (g.h + 42.0 * d + n * 54.0 * d + 12.0 * d).min(g.max_h)
    }

    fn panel_rect(&self) -> Rect {
        let h = self.panel_h();
        if self.mode == Mode::Search {
            let i = self.idle_rect();
            (i.0, 0.0, i.2, h)
        } else {
            (0.0, 0.0, self.geo.gw, h)
        }
    }

    /// Region klik jendela. Pada kapsul: seluruh baris (kapsul + area tombol) agar hover di sana terdeteksi.
    fn set_region(&self, r: Rect) {
        let g = self.geo;
        let r = if r.3 <= g.h + 0.5 { (0.0, 0.0, g.gw, g.h) } else { r };
        unsafe {
            let rgn = CreateRectRgn(r.0.floor() as i32, r.1.floor() as i32, (r.0 + r.2).ceil() as i32, (r.1 + r.3).ceil() as i32);
            SetWindowRgn(self.popup.hwnd, Some(rgn), true);
        }
    }

    /// Pindah/ubah ukuran badan kaca ke `t` (koordinat jendela). `anim` = pegas (rasio redam, periode ms) di
    /// compositor; klip isi ikut dengan pegas yang sama, region dilebarkan ke gabungan lama+baru lalu dirapikan.
    fn place(&mut self, t: Rect, anim: Option<(f32, u32)>) {
        let g = self.geo;
        let old = self.cur;
        let changed = (old.0 - t.0).abs() > 0.5 || (old.1 - t.1).abs() > 0.5 || (old.2 - t.2).abs() > 0.5 || (old.3 - t.3).abs() > 0.5;
        if !changed {
            return;
        }
        let radius = if t.3 <= g.h + 0.5 { t.3 / 2.0 } else { 26.0 * self.dpi };
        let (ww, wh) = (g.win.2 as f32, g.win.3 as f32);
        // kapsul tidak dipotong (teks bergeser bersama badan); panel dipotong ke badan kaca
        let ins = if t.3 <= g.h + 0.5 { (0.0, 0.0, 0.0, 0.0) } else { (t.0, t.1, (ww - t.0 - t.2).max(0.0), (wh - t.1 - t.3).max(0.0)) };
        match anim {
            Some((ratio, period)) => {
                let _ = self.popup.panel.animate_rect(t.0, t.1, t.2, t.3, radius, ratio, period);
                if let Ok(o) = self.clip.cast::<CompositionObject>() {
                    for (p, v) in [("LeftInset", ins.0), ("TopInset", ins.1), ("RightInset", ins.2), ("BottomInset", ins.3)] {
                        spring(&self.comp, &o, p, v, ratio, period);
                    }
                }
                self.set_region(union(old, t));
                unsafe {
                    SetTimer(Some(self.popup.hwnd), T_REGION, 380, None);
                }
            }
            None => {
                let _ = self.popup.panel.set_rect(t.0, t.1, t.2, t.3);
                let _ = self.popup.panel.set_radius(radius);
                let _ = self.clip.SetLeftInset(ins.0);
                let _ = self.clip.SetTopInset(ins.1);
                let _ = self.clip.SetRightInset(ins.2);
                let _ = self.clip.SetBottomInset(ins.3);
                self.set_region(t);
            }
        }
        self.cur = t;
    }

    /// Kosongkan kueri dan pilihan untuk `mode` (tanpa menggambar).
    fn reset_state(&mut self, mode: Mode) {
        self.mode = mode;
        self.query.clear();
        self.sel = 0;
        self.scroll = 0;
        self.cat = None;
        self.filter = None;
        self.editing = None;
        self.edit_buf.clear();
        self.files.clear();
        self.pscroll = 0.0;
        self.more_open = false;
        self.ftype = None;
        if mode == Mode::Files {
            self.request_recents();
        }
        self.rebuild_rows();
    }

    fn set_mode(&mut self, mode: Mode) {
        if self.compact() && mode != Mode::Search {
            self.begin_morph(mode);
            return;
        }
        self.reset_state(mode);
        self.render();
    }

    /// Mode panel (bukan kapsul + tombol): semua mode kecuali Cari yang masih kosong.
    fn compact(&self) -> bool {
        self.mode == Mode::Search && self.query.trim().is_empty()
    }

    /// Kapsul → panel mode seperti video: (1) kapsul melebar menelan tombol (±100 ms), (2) badan tumbuh ke bawah,
    /// (3) isi memudar masuk. Tahap 2 dan 3 dimulai oleh timer `T_MORPH`.
    fn begin_morph(&mut self, mode: Mode) {
        self.reset_state(mode);
        self.buttons = false;
        self.hover_mode = None;
        let t = self.panel_rect();
        self.place((t.0, 0.0, t.2, self.geo.h), Some((1.0, 55)));
        let content = self.popup.content.clone();
        let _ = crate::motion::scalar(&self.comp, &content, "Opacity", 1.0, 0.0, 90);
        let _ = content.SetOpacity(0.0);
        let _ = crate::motion::scalar(&self.comp, &self.side, "Opacity", 1.0, 0.0, 90);
        let _ = self.side.SetOpacity(0.0);
        self.morphing = true;
        unsafe {
            SetTimer(Some(self.popup.hwnd), T_MORPH, 100, None);
        }
    }

    fn finish_morph(&mut self) {
        unsafe {
            let _ = KillTimer(Some(self.popup.hwnd), T_MORPH);
        }
        if !self.morphing {
            return;
        }
        self.morphing = false;
        if let Ok(k) = self.side.Children() {
            let _ = k.RemoveAll();
        }
        self.render_with(true);
    }

    /// Muat daftar berkas terbaru Windows + thumbnail-nya di latar belakang (sekali per pembukaan).
    fn request_recents(&mut self) {
        if self.recents_requested {
            return;
        }
        self.recents_requested = true;
        let (rec, th) = (self.recents.clone(), self.thumbs.clone());
        let h = CTL.with(|c| c.borrow().as_ref().map(|c| c.hwnd.0 as isize)).unwrap_or(0);
        let px = (180.0 * self.dpi) as i32;
        std::thread::spawn(move || unsafe {
            use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
            let com = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
            let list = nacre_platform::recent::list(15);
            *rec.lock().unwrap() = list.clone();
            let _ = PostMessageW(Some(HWND(h as *mut _)), WM_THUMBS, WPARAM(0), LPARAM(0));
            for (_, p) in list {
                if let Some(b) = icons::thumbnail(&p, px) {
                    th.lock().unwrap().insert(p, b);
                    let _ = PostMessageW(Some(HWND(h as *mut _)), WM_THUMBS, WPARAM(0), LPARAM(0));
                }
            }
            if com {
                CoUninitialize();
            }
        });
    }

    /// Thumbnail berkas `path` yang muat di kotak `box_px` (lebar, tinggi tampil).
    fn thumb_for(&mut self, path: &str, box_px: f32) -> Option<(CompositionDrawingSurface, f32, f32)> {
        if !self.thumb_surf.contains_key(path) {
            let b = self.thumbs.lock().unwrap().get(path).cloned()?;
            let s = self.gfx.surface_from_bgra(b.w, b.h, &b.bgra).ok()?;
            self.thumb_surf.insert(path.to_string(), (s, b.w, b.h));
        }
        let (s, w, h) = self.thumb_surf.get(path)?.clone();
        let k = (box_px / w as f32).min(box_px / h as f32);
        Some((s, w as f32 * k, h as f32 * k))
    }

    /// Saran Apps: app di dock (sematan lalu terakhir dipakai) yang terpasang, dilengkapi app lain hingga 5.
    fn suggestions(&self) -> Vec<usize> {
        let mut out: Vec<usize> = Vec::new();
        let key = |p: &str| p.replace('/', "\\").to_lowercase();
        for f in ["pins.txt", "recents.txt"] {
            let text = std::fs::read_to_string(paths::config_dir().join(f)).unwrap_or_default();
            for line in text.lines() {
                let p = key(line.split('\t').next().unwrap_or("").trim());
                if p.is_empty() {
                    continue;
                }
                if let Some(i) = self.apps.iter().position(|a| key(&a.path) == p) {
                    if !out.contains(&i) {
                        out.push(i);
                    }
                }
                if out.len() >= COLS {
                    return out;
                }
            }
        }
        let mut rest: Vec<usize> = (0..self.apps.len()).filter(|i| !out.contains(i)).collect();
        rest.sort_by_key(|&i| self.apps[i].name.to_lowercase());
        out.extend(rest.into_iter().take(COLS - out.len()));
        out
    }

    fn rebuild_rows(&mut self) {
        self.rows.clear();
        self.headers.clear();
        self.sections.clear();
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
            Mode::Apps if q.is_empty() => {
                // seperti video: Saran, lalu seksi per kategori; tiap seksi satu baris kecuali "Lihat Semua"
                let push = |rows: &mut Vec<Row>, sections: &mut Vec<(String, std::ops::Range<usize>)>, title: String, idxs: Vec<usize>, apps: &[App]| {
                    if idxs.is_empty() {
                        return;
                    }
                    let start = rows.len();
                    for i in idxs {
                        rows.push(Row { kind: RKind::App, title: apps[i].name.clone(), sub: String::new(), path: apps[i].path.clone() });
                    }
                    sections.push((title, start..rows.len()));
                };
                let (mut rows, mut sections) = (Vec::new(), Vec::new());
                let sugg = self.suggestions();
                push(&mut rows, &mut sections, prefs::tr("Saran").to_string(), sugg, &self.apps);
                for c in appcat::ALL {
                    let mut v: Vec<usize> = (0..self.apps.len()).filter(|&i| appcat::guess(&self.apps[i].name, &self.apps[i].path) == c).collect();
                    v.sort_by_key(|&i| self.apps[i].name.to_lowercase());
                    push(&mut rows, &mut sections, prefs::tr(c.title()).to_string(), v, &self.apps);
                }
                self.rows = rows;
                self.sections = sections;
                if self.expanded.len() != self.sections.len() {
                    self.expanded = vec![false; self.sections.len()];
                }
            }
            Mode::Apps => {
                for i in search::rank(&q, &self.names, 200) {
                    let a = &self.apps[i];
                    self.rows.push(Row { kind: RKind::App, title: a.name.clone(), sub: String::new(), path: a.path.clone() });
                }
            }
            Mode::Files if q.is_empty() => {
                let rec = self.recents.lock().unwrap().clone();
                for (n, p) in rec.into_iter().filter(|(_, p)| of_type(p, self.ftype)) {
                    self.rows.push(Row { kind: RKind::File, title: n, sub: p.clone(), path: p });
                }
                self.sections = vec![(prefs::tr("Terbaru").to_string(), 0..self.rows.len())];
                self.expanded = vec![true];
            }
            Mode::Files => {
                for h in self.files.iter().filter(|h| of_type(&h.path, self.ftype)) {
                    self.rows.push(Row { kind: RKind::File, title: h.name.clone(), sub: h.path.clone(), path: h.path.clone() });
                }
            }
            Mode::Actions => {
                if q.is_empty() {
                    // seperti video: Saran, lalu dikelompokkan per app
                    self.headers.push((0, prefs::tr("Saran").to_string()));
                    let n = ACTIONS.len().min(5);
                    for i in 0..n {
                        self.rows.push(self.action_row(i));
                    }
                    let mut rest: Vec<usize> = (n..ACTIONS.len()).collect();
                    rest.sort_by_key(|&i| prefs::tr(ACTIONS[i].app).to_lowercase());
                    let mut last = String::new();
                    for i in rest {
                        let app = prefs::tr(ACTIONS[i].app).to_string();
                        if app != last {
                            self.headers.push((self.rows.len(), app.clone()));
                            last = app;
                        }
                        self.rows.push(self.action_row(i));
                    }
                } else {
                    let titles = |a: &model::ActionDef| action_title(a);
                    for i in model::rank_actions(&q, &self.quick, &titles) {
                        self.rows.push(self.action_row(i));
                    }
                }
            }
            Mode::Clipboard => {
                let ql = q.to_lowercase();
                for e in clipboard::history() {
                    if ql.is_empty() || e.text.to_lowercase().contains(&ql) {
                        let first = e.text.lines().next().unwrap_or("").trim().to_string();
                        let sub = format!("{} \u{00B7} {} {}", prefs::tr("teks"), prefs::tr("Disalin"), clip_time(e.at));
                        self.rows.push(Row { kind: RKind::Clip, title: first, sub, path: e.text.clone() });
                    }
                }
            }
        }
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
                b.bgra = nacre_core::iconplate::style_icon_v(&b.bgra, p.icon_style, p.icon_tint, prefs::icon_dark());
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
        self.render_with(false);
    }

    fn render_with(&mut self, intro: bool) {
        if self.morphing {
            // tahap 1 morph sedang berjalan; gambar penuh menyusul dari timer
            return;
        }
        let content = self.popup.content.clone();
        if let Ok(k) = content.Children() {
            let _ = k.RemoveAll();
        }
        self.hits.clear();
        self.circles.clear();
        if self.compact() {
            self.render_compact(&content);
        } else {
            self.render_panel(&content, intro);
        }
    }

    /// Kapsul diam ("Spotlight Search") atau kapsul pendek + empat tombol mode saat kursor di atasnya.
    fn render_compact(&mut self, content: &ContainerVisual) {
        let (g, d) = (self.geo, self.dpi);
        let fg = self.look().fg;
        let _ = content.SetOpacity(1.0);
        let t = if self.buttons { self.hover_rect() } else { self.idle_rect() };
        let was_panel = self.cur.3 > g.h + 0.5;
        let old_x = self.cur.0;
        self.place(t, Some(if was_panel { (0.95, 70) } else { (0.82, 75) }));
        // isi kapsul bergeser bersama badannya (pegas yang sama)
        let dx = old_x - t.0;
        if dx.abs() > 0.5 && !was_panel {
            let _ = content.SetOffset(Vector3 { X: dx, Y: 0.0, Z: 0.0 });
            if let Ok(o) = content.cast::<CompositionObject>() {
                spring(&self.comp, &o, "Offset.X", 0.0, 0.82, 75);
            }
        } else if was_panel {
            let _ = content.SetOffset(Vector3 { X: 0.0, Y: 0.0, Z: 0.0 });
        }
        let cy = g.h / 2.0;
        self.sym(content, Sym::Search, t.0 + 28.0 * d, cy, 21.0 * d, fg, 0.75);
        let label = match self.hover_mode {
            Some(m) => prefs::tr(m.title()).to_string(),
            None => prefs::tr("Cari Spotlight").to_string(),
        };
        let mut avail = t.2 - 72.0 * d;
        if let Some(m) = self.hover_mode {
            // seperti ⌘1 di video: keycap pintasan di ujung kanan kapsul
            let key = format!("Ctrl {}", m.digit());
            let kpx = (12.0 * d).round() as i32;
            let kw = self.text_w(&key, kpx, 500) + 14.0 * d;
            let r: Rect = (t.0 + t.2 - kw - 16.0 * d, cy - 11.0 * d, kw, 22.0 * d);
            self.rect(content, r, 6.0 * d, Color { A: 30, R: fg[0], G: fg[1], B: fg[2] });
            self.text(content, &key, r.0 + 7.0 * d, cy, kpx, 500, fg, 0.65, kw);
            avail -= kw + 12.0 * d;
        }
        self.text(content, &label, t.0 + 52.0 * d, cy, (21.0 * d).round() as i32, 400, fg, 0.5, avail);

        let side = self.side.clone();
        if self.buttons {
            if let Ok(k) = side.Children() {
                let _ = k.RemoveAll();
            }
            let _ = side.SetOpacity(1.0);
            self.side_fading = false;
            for (i, m) in [Mode::Apps, Mode::Files, Mode::Actions, Mode::Clipboard].into_iter().enumerate() {
                let r: Rect = (g.cap + g.gap + i as f32 * (g.btn + g.gap), 0.0, g.btn, g.btn);
                let Ok(cv) = self.comp.CreateContainerVisual() else { continue };
                let _ = side.Children().and_then(|k| k.InsertAtTop(&cv));
                if let Some(gp) = self.glass_circle(&cv, r) {
                    self.circles.push(gp);
                }
                if self.hover_mode == Some(m) {
                    self.rect(&cv, r, g.btn / 2.0, Color { A: 36, R: fg[0], G: fg[1], B: fg[2] });
                }
                self.mode_icon(&cv, m, r.0 + g.btn / 2.0, r.1 + g.btn / 2.0, 21.0 * d, fg, 0.9);
                if self.buttons_intro {
                    // tombol muncul satu per satu: membesar dari kecil sambil memudar masuk
                    let center = Vector3 { X: r.0 + g.btn / 2.0, Y: r.1 + g.btn / 2.0, Z: 0.0 };
                    let _ = crate::motion::pop_in(&self.comp, &cv, center, 0.55, 1.05, 20 + i as u32 * 28, 300);
                }
                self.hits.push((r, HitKind::Mode(m)));
            }
            self.buttons_intro = false;
        } else if !self.side_fading && side.Children().map(|k| k.Count().unwrap_or(0)).unwrap_or(0) > 0 {
            // tombol menghilang memudar saat kursor pergi
            let _ = crate::motion::scalar(&self.comp, &side, "Opacity", 1.0, 0.0, 130);
            let _ = side.SetOpacity(0.0);
            self.side_fading = true;
        }
    }

    fn render_panel(&mut self, content: &ContainerVisual, intro: bool) {
        let (g, d) = (self.geo, self.dpi);
        let fg = self.look().fg;
        let was_compact = self.cur.3 <= g.h + 0.5;
        let t = self.panel_rect();
        self.place(t, Some(if intro { (0.9, 90) } else if was_compact { (0.92, 80) } else { (1.0, 55) }));
        let _ = content.SetOffset(Vector3 { X: 0.0, Y: 0.0, Z: 0.0 });
        if intro {
            // tahap 3 morph: isi baru memudar masuk setelah badan hampir selesai tumbuh
            let go = || -> windows_core::Result<()> {
                let e = crate::motion::ease(&self.comp)?;
                let a = self.comp.CreateScalarKeyFrameAnimation()?;
                a.InsertKeyFrame(0.0, 0.0)?;
                a.InsertKeyFrame(0.35, 0.0)?;
                a.InsertKeyFrameWithEasingFunction(1.0, 1.0, &e)?;
                a.SetDuration(windows::Foundation::TimeSpan { Duration: 320 * 10_000 })?;
                content.cast::<CompositionObject>()?.StartAnimation(&HSTRING::from("Opacity"), &a)
            };
            let _ = go();
        }
        let _ = content.SetOpacity(1.0);
        if let Ok(k) = self.side.Children() {
            let _ = k.RemoveAll();
        }
        let Ok(body) = self.comp.CreateContainerVisual() else { return };
        let _ = body.SetOffset(Vector3 { X: t.0, Y: 0.0, Z: 0.0 });
        let _ = content.Children().and_then(|k| k.InsertAtTop(&body));
        let hits0 = self.hits.len();
        let (w, ph) = (t.2, t.3);

        // ---------- kepala: ikon mode + teks; "⋯" di kanan (kecuali Cari dan Aksi) ----------
        let cy = g.h / 2.0;
        self.mode_icon(&body, self.mode, 28.0 * d, cy, 21.0 * d, fg, 0.8);
        let px = (21.0 * d).round() as i32;
        let tx = 52.0 * d;
        let right_pad = 70.0 * d;
        if self.query.is_empty() {
            self.text(&body, prefs::tr(self.mode.title()), tx, cy, px, 400, fg, 0.5, w - tx - right_pad);
        } else {
            let q = self.query.clone();
            let qw = self.text(&body, &q, tx, cy, px, 400, fg, 1.0, w - tx - right_pad - 90.0 * d);
            if self.mode == Mode::Search && !self.rows.is_empty() {
                let open = format!("\u{2014} {}", prefs::tr("Buka"));
                self.text(&body, &open, tx + qw + 8.0 * d, cy, (16.0 * d).round() as i32, 400, fg, 0.42, 160.0 * d);
            }
        }
        let has_more = matches!(self.mode, Mode::Apps | Mode::Files | Mode::Clipboard);
        if self.mode == Mode::Search {
            if let Some(row) = self.rows.get(self.sel).cloned() {
                if matches!(row.kind, RKind::App | RKind::File) {
                    if let Some(s) = self.icon_surface(&row.path, (64.0 * d) as i32) {
                        self.sprite(&body, &s, w - 52.0 * d, cy - 16.0 * d, 32.0 * d, 32.0 * d, 1.0);
                    }
                }
            }
        } else if has_more {
            let r: Rect = (w - 46.0 * d, cy - 14.0 * d, 28.0 * d, 28.0 * d);
            self.rect(&body, r, 14.0 * d, Color { A: if self.more_open { 64 } else { 26 }, R: fg[0], G: fg[1], B: fg[2] });
            self.glyph(&body, "\u{E712}", r.0 + 14.0 * d, cy, 13.0 * d, fg, 0.85);
            self.hits.push((r, HitKind::More));
        }
        if !(self.mode == Mode::Search && self.rows.is_empty()) {
            self.rect(&body, (16.0 * d, g.h - 1.0, w - 32.0 * d, 1.0), 0.0, Color { A: 30, R: fg[0], G: fg[1], B: fg[2] });
        }
        let mut top = g.h + 2.0 * d;

        // ---------- chip filter (Cari: jenis hasil; Berkas: jenis berkas) ----------
        if matches!(self.mode, Mode::Search | Mode::Files) {
            let chips_h = 42.0 * d;
            let mut x = 20.0 * d;
            let cy2 = top + chips_h / 2.0;
            let cpx = (13.0 * d).round() as i32;
            let items: Vec<(String, HitKind, bool)> = if self.mode == Mode::Search {
                [(prefs::tr("Aplikasi"), Filter::Apps), (prefs::tr("Berkas Saya"), Filter::Files), (prefs::tr("Aksi"), Filter::Actions)]
                    .into_iter()
                    .map(|(l, f)| (l.to_string(), HitKind::Filter(f), self.filter == Some(f)))
                    .collect()
            } else {
                FTYPES.iter().enumerate().map(|(i, (l, _))| (prefs::tr(l).to_string(), HitKind::FType(i), self.ftype == Some(i))).collect()
            };
            for (label, f, active) in items {
                let cw = self.text_w(&label, cpx, 500) + 24.0 * d;
                let r: Rect = (x, cy2 - 13.0 * d, cw, 26.0 * d);
                let col = if active { crate::prefs::accent_c(90) } else { Color { A: 30, R: fg[0], G: fg[1], B: fg[2] } };
                self.rect(&body, r, 13.0 * d, col);
                self.text(&body, &label, r.0 + 12.0 * d, cy2, cpx, 500, fg, if active { 1.0 } else { 0.78 }, cw);
                self.hits.push((r, f));
                x += cw + 8.0 * d;
            }
            top += chips_h;
        }

        // ---------- area gulir (dipotong di bawah kepala) ----------
        let Ok(area) = self.comp.CreateContainerVisual() else { return };
        let _ = area.SetSize(Vector2 { X: w, Y: ph });
        if let Ok(ac) = self.comp.CreateInsetClip() {
            let _ = ac.SetTopInset(top);
            let _ = area.SetClip(&ac);
        }
        let _ = body.Children().and_then(|k| k.InsertAtTop(&area));
        let empty_q = self.query.trim().is_empty();
        match self.mode {
            Mode::Apps if empty_q => self.draw_sections(&area, w, top, ph, fg, false),
            Mode::Apps => self.draw_grid(&area, w, top, fg),
            Mode::Files if empty_q => self.draw_sections(&area, w, top, ph, fg, true),
            _ => self.draw_rows(&area, w, top, 54.0 * d, ph, fg),
        }

        // ---------- menu "⋯" (di atas isi) ----------
        if self.more_open && has_more {
            let label = match self.mode {
                Mode::Clipboard => prefs::tr("Hapus Riwayat"),
                Mode::Apps => prefs::tr(if self.expanded.iter().all(|e| *e) { "Ringkas Semua" } else { "Tampilkan Semua" }),
                _ => prefs::tr("Buka Folder Terbaru"),
            };
            let mpx = (13.0 * d).round() as i32;
            let mw = self.text_w(label, mpx, 500) + 28.0 * d;
            let r: Rect = (w - mw - 18.0 * d, g.h - 4.0 * d, mw, 32.0 * d);
            if let Some(gp) = self.glass_circle(&body, r) {
                let _ = gp.set_radius(10.0 * d);
                self.circles.push(gp);
            }
            self.text(&body, label, r.0 + 14.0 * d, r.1 + r.3 / 2.0, mpx, 500, fg, 0.95, mw);
            self.hits.insert(hits0, (r, HitKind::MoreItem));
        }

        // hit dari koordinat panel ke koordinat jendela
        for h in &mut self.hits[hits0..] {
            h.0 .0 += t.0;
        }
    }

    /// Seksi grid (Apps: Saran + kategori; Berkas: Terbaru dengan thumbnail): judul + "Lihat Semua", grid 5
    /// kolom, garis pemisah antar seksi, digulir per piksel.
    #[allow(clippy::too_many_arguments)]
    fn draw_sections(&mut self, area: &ContainerVisual, w: f32, top: f32, ph: f32, fg: [u8; 3], thumbs: bool) {
        let d = self.dpi;
        let cw = (w - 32.0 * d) / COLS as f32;
        let (icon, ch) = if thumbs { (80.0 * d, 124.0 * d) } else { (56.0 * d, 98.0 * d) };
        let hh = 34.0 * d;
        self.view = (top, ph);
        self.cell_y.clear();
        if self.rows.is_empty() {
            let msg = if thumbs { prefs::tr("Belum ada berkas terbaru.") } else { prefs::tr("Tidak ada aplikasi") };
            self.text(area, msg, 24.0 * d, top + 40.0 * d, (15.0 * d).round() as i32, 400, fg, 0.6, w - 40.0 * d);
            self.content_h = 0.0;
            return;
        }
        let vis = |y0: f32, h: f32| y0 + h > top && y0 < ph;
        let mut y = top + 4.0 * d - self.pscroll;
        let secs = self.sections.clone();
        for (si, (title, range)) in secs.iter().enumerate() {
            let expanded = self.expanded.get(si).copied().unwrap_or(false);
            let total = range.len();
            let shown = if expanded { total } else { total.min(COLS) };
            if vis(y, hh) {
                self.text(area, title, 18.0 * d, y + hh / 2.0, (13.0 * d).round() as i32, 600, fg, 0.62, w * 0.6);
                if total > COLS && !thumbs {
                    let label = prefs::tr(if expanded { "Lebih Sedikit" } else { "Lihat Semua" });
                    let lpx = (12.5 * d).round() as i32;
                    let lw = self.text_w(label, lpx, 500);
                    let r: Rect = (w - lw - 24.0 * d, y + 4.0 * d, lw + 8.0 * d, hh - 8.0 * d);
                    self.text(area, label, r.0 + 4.0 * d, y + hh / 2.0, lpx, 500, fg, 0.55, lw + 4.0 * d);
                    self.hits.push((r, HitKind::ShowMore(si)));
                }
            }
            y += hh;
            for k in 0..shown {
                let idx = range.start + k;
                let (r, c) = (k / COLS, k % COLS);
                let cx = 16.0 * d + c as f32 * cw + cw / 2.0;
                let y0 = y + r as f32 * ch;
                self.cell_y.push((idx, y0 + self.pscroll, ch));
                if !vis(y0, ch) {
                    continue;
                }
                let row = self.rows[idx].clone();
                if idx == self.sel {
                    self.rect(area, (cx - cw / 2.0 + 4.0 * d, y0, cw - 8.0 * d, ch - 4.0 * d), 14.0 * d, Color { A: 26, R: fg[0], G: fg[1], B: fg[2] });
                }
                let iy = y0 + 6.0 * d;
                if thumbs {
                    if let Some((s, tw, th)) = self.thumb_for(&row.path, icon) {
                        self.sprite(area, &s, cx - tw / 2.0, iy + (icon - th) / 2.0, tw, th, 1.0);
                    } else if let Some(s) = self.icon_surface(&row.path, (icon * 1.2) as i32) {
                        self.sprite(area, &s, cx - icon * 0.35, iy + icon * 0.15, icon * 0.7, icon * 0.7, 1.0);
                    }
                } else if let Some(s) = self.icon_surface(&row.path, (icon * 1.6) as i32) {
                    self.sprite(area, &s, cx - icon / 2.0, iy, icon, icon, 1.0);
                } else {
                    self.rect(area, (cx - icon / 2.0, iy, icon, icon), icon * 0.23, Color { A: 60, R: fg[0], G: fg[1], B: fg[2] });
                }
                let lpx = (12.0 * d).round() as i32;
                let lw = self.text_w(&row.title, lpx, 400).min(cw - 10.0 * d);
                self.text(area, &row.title, cx - lw / 2.0, iy + icon + 13.0 * d, lpx, 400, fg, 0.92, cw - 10.0 * d);
                self.hits.push(((cx - cw / 2.0, y0, cw, ch), HitKind::Cell(idx)));
            }
            y += shown.div_ceil(COLS) as f32 * ch + 6.0 * d;
            if si + 1 < secs.len() && vis(y, 1.0) {
                self.rect(area, (16.0 * d, y, w - 32.0 * d, 1.0), 0.0, Color { A: 26, R: fg[0], G: fg[1], B: fg[2] });
            }
            y += 8.0 * d;
        }
        self.content_h = y + self.pscroll - top;
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
                self.rect(content, (cx - cw / 2.0 + 4.0 * d, cy0, cw - 8.0 * d, ch - 4.0 * d), 14.0 * d, Color { A: 26, R: fg[0], G: fg[1], B: fg[2] });
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

    /// Daftar baris (Cari, Berkas dengan kueri, Aksi, Papan Klip) dengan judul seksi opsional.
    #[allow(clippy::too_many_arguments)]
    fn draw_rows(&mut self, area: &ContainerVisual, w: f32, top: f32, row_h: f32, bottom: f32, fg: [u8; 3]) {
        let d = self.dpi;
        if self.rows.is_empty() {
            if self.mode != Mode::Search {
                let msg = match self.mode {
                    Mode::Clipboard => prefs::tr("Belum ada yang disalin dalam 8 jam terakhir."),
                    _ => prefs::tr("Tidak ada hasil"),
                };
                let hint = if self.mode == Mode::Files && self.query.trim().chars().count() < 2 { prefs::tr("Ketik untuk mencari berkas.") } else { msg };
                self.text(area, hint, 26.0 * d, top + row_h / 2.0, (15.0 * d).round() as i32, 400, fg, 0.6, w - 40.0 * d);
            }
            return;
        }
        let first = self.scroll.min(self.rows.len().saturating_sub(1));
        let headers = self.headers.clone();
        let mut ry = top + 4.0 * d;
        for i in first..self.rows.len() {
            if ry > bottom {
                break;
            }
            if let Some((_, title)) = headers.iter().find(|(at, _)| *at == i) {
                self.text(area, title, 20.0 * d, ry + 15.0 * d, (13.0 * d).round() as i32, 600, fg, 0.6, w * 0.6);
                ry += 30.0 * d;
            }
            let row = self.rows[i].clone();
            if i == self.sel {
                self.rect(area, (8.0 * d, ry, w - 16.0 * d, row_h - 2.0 * d), 12.0 * d, Color { A: 28, R: fg[0], G: fg[1], B: fg[2] });
            }
            let ic = 32.0 * d;
            let (icx, icy) = (22.0 * d + ic / 2.0, ry + (row_h - 2.0 * d) / 2.0);
            match &row.kind {
                RKind::App | RKind::File => {
                    if let Some(s) = self.icon_surface(&row.path, (ic * 2.0) as i32) {
                        self.sprite(area, &s, icx - ic / 2.0, icy - ic / 2.0, ic, ic, 1.0);
                    }
                }
                RKind::Calc => {
                    self.rect(area, (icx - ic / 2.0, icy - ic / 2.0, ic, ic), 8.0 * d, Color { A: 255, R: 255, G: 149, B: 0 });
                    self.glyph(area, "\u{E8EF}", icx, icy, 17.0 * d, [255, 255, 255], 1.0);
                }
                RKind::Web => {
                    self.rect(area, (icx - ic / 2.0, icy - ic / 2.0, ic, ic), 8.0 * d, crate::prefs::accent_c(255));
                    self.glyph(area, "\u{E774}", icx, icy, 17.0 * d, [255, 255, 255], 1.0);
                }
                RKind::Action(_) => {
                    let (g, col) = action_glyph(&row.path);
                    self.rect(area, (icx - ic / 2.0, icy - ic / 2.0, ic, ic), 8.0 * d, Color { A: 255, R: col[0], G: col[1], B: col[2] });
                    self.glyph(area, g, icx, icy, 17.0 * d, [255, 255, 255], 1.0);
                }
                RKind::Clip => {
                    // ikon lembar dokumen seperti di video
                    self.rect(area, (icx - ic * 0.36, icy - ic / 2.0, ic * 0.72, ic), 4.0 * d, Color { A: 240, R: 250, G: 250, B: 252 });
                    self.glyph(area, "\u{E8A5}", icx, icy, 14.0 * d, [120, 120, 128], 1.0);
                }
            }
            let tx = 22.0 * d + ic + 14.0 * d;
            let max_w = w - tx - if matches!(row.kind, RKind::Clip) { 64.0 * d } else { 150.0 * d };
            self.text(area, &row.title, tx, icy - 9.0 * d, (15.0 * d).round() as i32, 400, fg, 1.0, max_w);
            if !row.sub.is_empty() {
                self.text(area, &row.sub, tx, icy + 11.0 * d, (12.0 * d).round() as i32, 400, fg, 0.55, max_w);
            }
            if let RKind::Action(ai) = row.kind {
                let key = model::key_of(&ACTIONS[ai], &self.quick).to_string();
                let editing = self.editing == Some(ai);
                let label = if editing { format!("{}|", self.edit_buf) } else if key.is_empty() { prefs::tr("Tambah kunci cepat").to_string() } else { key };
                let cpx = (12.0 * d).round() as i32;
                let cw = self.text_w(&label, cpx, 500) + 18.0 * d;
                let r: Rect = (w - cw - 24.0 * d, icy - 11.0 * d, cw, 22.0 * d);
                self.rect(area, r, 6.0 * d, Color { A: if editing { 80 } else { 36 }, R: fg[0], G: fg[1], B: fg[2] });
                self.text(area, &label, r.0 + 9.0 * d, icy, cpx, 500, fg, 0.75, cw);
            }
            if matches!(row.kind, RKind::Clip) {
                // tombol salin bulat di kanan
                let r: Rect = (w - 48.0 * d, icy - 13.0 * d, 26.0 * d, 26.0 * d);
                self.rect(area, r, 13.0 * d, Color { A: 30, R: fg[0], G: fg[1], B: fg[2] });
                self.glyph(area, "\u{E8C8}", r.0 + 13.0 * d, icy, 12.0 * d, fg, 0.8);
                self.hits.push((r, HitKind::CopyRow(i)));
            }
            self.hits.push(((0.0, ry, w, row_h), HitKind::Row(i)));
            ry += row_h;
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
        let empty_q = self.query.trim().is_empty();
        if matches!(self.mode, Mode::Apps | Mode::Files) && empty_q {
            // seksi grid: berpindah di antara sel yang tampil; gulir agar sel terpilih terlihat
            let vis: Vec<usize> = self.cell_y.iter().map(|c| c.0).collect();
            if vis.is_empty() {
                return;
            }
            let pos = vis.iter().position(|&i| i == self.sel).unwrap_or(0) as i32;
            let np = (pos + dx + dy * COLS as i32).clamp(0, vis.len() as i32 - 1) as usize;
            self.sel = vis[np];
            if let Some(&(_, y, h)) = self.cell_y.iter().find(|c| c.0 == self.sel) {
                let (top, bottom) = self.view;
                let yv = y - self.pscroll;
                if yv < top {
                    self.pscroll -= top - yv + 6.0 * self.dpi;
                } else if yv + h > bottom {
                    self.pscroll += yv + h - bottom + 6.0 * self.dpi;
                }
                self.pscroll = self.pscroll.clamp(0.0, (self.content_h - (bottom - top)).max(0.0));
            }
        } else if self.mode == Mode::Apps {
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
        WM_TIMER if wp.0 == T_FINISH => {
            let _ = KillTimer(Some(hwnd), T_FINISH);
            finish_closing();
            LRESULT(0)
        }
        WM_THUMBS => {
            // daftar terbaru / thumbnail baru siap: segarkan panel Berkas tanpa animasi
            with_spot(|s| {
                if s.mode == Mode::Files && s.query.trim().is_empty() && !s.morphing {
                    let sel = s.sel;
                    s.rebuild_rows();
                    s.sel = sel.min(s.rows.len().saturating_sub(1));
                    s.render();
                }
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
                if matches!(s.mode, Mode::Apps | Mode::Files) && s.query.trim().is_empty() {
                    let max = (s.content_h - (s.view.1 - s.view.0)).max(0.0);
                    s.pscroll = (s.pscroll + if delta < 0 { 72.0 } else { -72.0 } * s.dpi).clamp(0.0, max);
                    s.render();
                    return;
                }
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
                if !s.tracking {
                    let mut t = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
                    if TrackMouseEvent(&mut t).is_ok() {
                        s.tracking = true;
                    }
                }
                if s.morphing {
                    return;
                }
                if s.compact() {
                    // kursor di atas Spotlight: kapsul memendek dan empat tombol mode muncul
                    if !s.buttons {
                        s.buttons = true;
                        s.buttons_intro = true;
                        s.hover_mode = None;
                        s.render();
                        return;
                    }
                    let hm = s.hits.iter().find(|(r, k)| hit(*r, x, y) && matches!(k, HitKind::Mode(_))).and_then(|(_, k)| match k {
                        HitKind::Mode(m) => Some(*m),
                        _ => None,
                    });
                    if hm != s.hover_mode {
                        s.hover_mode = hm;
                        s.render();
                    }
                    return;
                }
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
        WM_MOUSELEAVE_SPOT => {
            with_spot(|s| {
                s.tracking = false;
                if s.compact() && s.buttons && !s.morphing {
                    s.buttons = false;
                    s.hover_mode = None;
                    s.render();
                }
            });
            LRESULT(0)
        }
        WM_TIMER => {
            let id = wp.0;
            if id == T_MORPH {
                with_spot(|s| s.finish_morph());
            } else if id == T_REGION {
                let _ = KillTimer(Some(hwnd), T_REGION);
                with_spot(|s| s.set_region(s.cur));
            }
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
                    HitKind::ShowMore(si) => {
                        if let Some(e) = s.expanded.get_mut(si) {
                            *e = !*e;
                        }
                        s.render();
                        Run::Nothing
                    }
                    HitKind::More => {
                        s.more_open = !s.more_open;
                        s.render();
                        Run::Nothing
                    }
                    HitKind::MoreItem => {
                        s.more_open = false;
                        match s.mode {
                            Mode::Clipboard => {
                                clipboard::clear();
                                s.rebuild_rows();
                                s.render();
                                Run::Nothing
                            }
                            Mode::Apps => {
                                let all = s.expanded.iter().all(|e| *e);
                                for e in s.expanded.iter_mut() {
                                    *e = !all;
                                }
                                s.render();
                                Run::Nothing
                            }
                            _ => Run::Open("shell:recent".to_string()),
                        }
                    }
                    HitKind::FType(i) => {
                        s.ftype = if s.ftype == Some(i) { None } else { Some(i) };
                        s.rebuild_rows();
                        s.render();
                        Run::Nothing
                    }
                    HitKind::CopyRow(i) => s.rows.get(i).map(|r| Run::Copy(r.path.clone())).unwrap_or(Run::Nothing),
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
