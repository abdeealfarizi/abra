use crate::popup::Popup;
use crate::{motion, paths, prefs, services};
use crate::dock::menu::{Item as MItem, Menu, MENU_CLOSE, MENU_PICK};
use nacre_core::controls::{self, Category, Kind, Placed, Size};
use nacre_core::symbols::{self, Sym};
use nacre_platform::icons::Bitmap;
use nacre_platform::media::{self, ArtUpdate, Command};
use nacre_platform::radio::{self, Cmd as RadioCmd, Kind as RK};
use nacre_platform::text::{measure, render, render_face, Style, ICON_FACE};
use nacre_platform::{audio, brightness, keys, launch, wifi};
use nacre_ui::glass::{GlassPanel, GlassParams, Tier};
use nacre_ui::gfx::Gfx;
use std::cell::RefCell;
use std::collections::HashMap;
use windows::core::w;
use windows::UI::Color;
use windows::UI::Composition::{CompositionDrawingSurface, CompositionStretch, Compositor, ContainerVisual, SpriteVisual};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT, VK_BACK, VK_ESCAPE};
use windows::Win32::UI::WindowsAndMessaging::*;
const WM_MOUSELEAVE: u32 = 0x02A3;
use windows_core::Interface;
use windows_numerics::{Vector2, Vector3};

const WM_CLOSE_CC: u32 = WM_APP + 130;
static PAD: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn pad() -> f32 {
    f32::from_bits(PAD.load(std::sync::atomic::Ordering::Relaxed))
}

fn pxy(lp: LPARAM) -> (f32, f32) {
    let (x, y) = xy(lp);
    (x - pad(), y - pad())
}
const WM_RADIO: u32 = WM_APP + 131;
const BTN_DISC: u8 = 20;
const WM_MEDIA: u32 = WM_APP + 132;
const WM_VALUES: u32 = WM_APP + 133;

type Rect = (f32, f32, f32, f32);

fn r_sep_visible(cy: f32, gh: f32, dpi: f32) -> bool {
    cy > 0.0 && cy < gh - 60.0 * dpi
}

fn hit(r: Rect, x: f32, y: f32) -> bool {
    x >= r.0 && x < r.0 + r.2 && y >= r.1 && y < r.1 + r.3
}

fn glyph_of(k: Kind) -> &'static str {
    match k {
        Kind::Wifi => "\u{E701}",
        Kind::Bluetooth => "\u{E702}",
        Kind::Hotspot => "\u{E704}",
        Kind::Share => "\u{E72D}",
        Kind::Airplane => "\u{E709}",
        Kind::NowPlaying => "\u{E8D6}",
        Kind::Stage => "\u{E8A0}",
        Kind::Mirror => "\u{EC15}",
        Kind::Focus => "\u{E708}",
        Kind::DarkMode => "\u{E790}",
        Kind::Screenshot => "\u{E722}",
        Kind::Display => "\u{E706}",
        Kind::Sound => "\u{E767}",
        Kind::Battery => "\u{E83F}",
        Kind::Lock => "\u{E72E}",
        Kind::Mission => "\u{E7C4}",
        Kind::Apps => "\u{E71D}",
        Kind::Calculator => "\u{E8EF}",
        Kind::Timer => "\u{E916}",
    }
}

struct Mod {
    placed: Placed,
    rect: Rect,
    track: Option<Rect>,
    fill: Option<SpriteVisual>,
    thumb: Vec<SpriteVisual>,
    btns: Vec<(Rect, u8)>,
    badge: Rect,
    grip: Rect,
    vertical: bool,
    cont: Option<ContainerVisual>,
}

struct Panel {
    popup: Popup,
    dpi: f32,
    cell: f32,
    gap: f32,
    mods: Vec<Mod>,
    edit_btn: Rect,
    _panels: Vec<GlassPanel>,
    drag: Option<(usize, f32, f32)>,
    slider: Option<usize>,
    w: f32,
    chip: Option<(Rect, bool)>,
    hover: Option<usize>,
    preview: Vec<Kind>,
    shift: HashMap<Kind, (f32, f32)>,
    lift: (f32, f32),
    resize: Option<(usize, Size)>,
    ghost: Option<ContainerVisual>,
}

fn grid_rect(p: &Placed, cell: f32, gap: f32, top: f32) -> Rect {
    (p.col as f32 * (cell + gap), top + p.row as f32 * (cell + gap), p.w as f32 * cell + (p.w - 1) as f32 * gap, p.h as f32 * cell + (p.h - 1) as f32 * gap)
}

fn center_of(r: Rect) -> Vector3 {
    Vector3 { X: r.0 + r.2 / 2.0, Y: r.1 + r.3 / 2.0, Z: 0.0 }
}

struct Gallery {
    popup: Popup,
    dpi: f32,
    cat: Option<Category>,
    query: String,
    cards: Vec<(Kind, Rect)>,
    cats: Vec<(Option<Category>, Rect)>,
    done: Rect,
    scroll: f32,
    content_h: f32,
    _panels: Vec<GlassPanel>,
    conts: Vec<(Rect, ContainerVisual, f32)>,
    hover: Option<usize>,
    tracking: bool,
}

struct Cc {
    ctl: HWND,
    comp: Compositor,
    gfx: Option<Gfx>,
    layout: Vec<Kind>,
    sizes: HashMap<Kind, Size>,
    menu: Option<Menu>,
    menu_kind: Option<Kind>,
    edit: bool,
    panel: Option<Panel>,
    gallery: Option<Gallery>,
    radios: Option<radio::Radios>,
    wifi: Option<bool>,
    bt: Option<bool>,
    ssid: Option<String>,
    media: Option<media::Media>,
    track: Option<(String, String, bool, bool)>,
    art: Option<Bitmap>,
    cache: HashMap<String, (CompositionDrawingSurface, i32, i32)>,
    top: i32,
    intro: bool,
    intro_items: Vec<(ContainerVisual, Vector3, u32)>,
    closing: bool,
    pending_remove: Option<Kind>,
    pick_up: Option<usize>,
}

const T_CLOSE: usize = 1;
const T_INTRO: usize = 2;
const T_REMOVE: usize = 3;

thread_local! {
    static CC: RefCell<Option<Cc>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut Cc) -> R) -> Option<R> {
    CC.with(|c| c.try_borrow_mut().ok().and_then(|mut b| b.as_mut().map(f)))
}

fn layout_file() -> std::path::PathBuf {
    paths::config_dir().join("cc_layout.txt")
}

pub fn init(comp: &Compositor) -> windows_core::Result<()> {
    unsafe {
        let hi: HINSTANCE = GetModuleHandleW(None)?.into();
        let class = w!("AbraCcCtl");
        RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(ctl_proc), hInstance: hi, lpszClassName: class, ..Default::default() });
        let hwnd = CreateWindowExW(WINDOW_EX_STYLE(0), class, w!("abra-cc-ctl"), WS_POPUP, 0, 0, 0, 0, Some(HWND_MESSAGE), None, Some(hi), None)?;
        let (layout, sizes) = controls::from_text_sized(&std::fs::read_to_string(layout_file()).unwrap_or_default());
        CC.with(|c| {
            *c.borrow_mut() = Some(Cc {
                ctl: hwnd,
                comp: comp.clone(),
                gfx: None,
                layout,
                sizes,
                menu: None,
                menu_kind: None,
                edit: false,
                panel: None,
                gallery: None,
                radios: None,
                wifi: None,
                bt: None,
                ssid: None,
                media: None,
                track: None,
                art: None,
                cache: HashMap::new(),
                top: 30,
                intro: false,
                intro_items: Vec::new(),
                closing: false,
                pending_remove: None,
                pick_up: None,
            })
        });
    }
    Ok(())
}

pub fn toggle(top: i32) {
    let open = with(|c| c.panel.is_some() && !c.closing).unwrap_or(false);
    if open {
        close();
    } else {
        with(|c| {
            c.finish_close();
            c.open(top)
        });
    }
}

static SLID_AT: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

pub fn adjusting() -> bool {
    SLID_AT.lock().ok().and_then(|g| *g).is_some_and(|t| t.elapsed() < std::time::Duration::from_millis(900))
}

pub fn is_open() -> bool {
    with(|c| c.panel.is_some() && !c.closing).unwrap_or(false)
}

pub fn close() {
    with(|c| c.close());
}

pub fn test_open_picker() {
    let target = with(|c| {
        let p = c.panel.as_ref()?;
        let m = p.mods.iter().find(|m| m.placed.kind == Kind::Wifi)?;
        let (x, y) = ((m.rect.0 + m.rect.2 * 0.8 + pad()) as i32, (m.rect.1 + m.rect.3 / 2.0 + pad()) as i32);
        Some((p.popup.hwnd.0 as isize, ((y as u32) << 16 | (x as u32 & 0xFFFF)) as isize))
    })
    .flatten();
    if let Some((h, lp)) = target {
        unsafe {
            let h = HWND(h as *mut _);
            let _ = PostMessageW(Some(h), WM_LBUTTONDOWN, WPARAM(1), LPARAM(lp));
            let _ = PostMessageW(Some(h), WM_LBUTTONUP, WPARAM(0), LPARAM(lp));
        }
    }
}

pub fn test_enter_edit() {
    with(|c| c.enter_edit());
}

pub fn refresh_radios() {
    with(|c| {
        if let Some(r) = &c.radios {
            let _ = r.commands.send(RadioCmd::Refresh);
        }
    });
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

const WHITE: [u8; 3] = [255, 255, 255];

impl Cc {
    fn save_layout(&self) {
        let _ = std::fs::write(layout_file(), controls::to_text_sized(&self.layout, &self.sizes));
    }

    fn open(&mut self, top: i32) {
        self.top = top;
        self.edit = false;
        if self.gfx.is_none() {
            self.gfx = Gfx::shared(&self.comp).ok();
        }
        let ctl = self.ctl.0 as isize;
        if self.radios.is_none() {
            self.radios = Some(radio::spawn(move || unsafe {
                let _ = PostMessageW(Some(HWND(ctl as *mut _)), WM_RADIO, WPARAM(0), LPARAM(0));
            }));
        }
        if self.media.is_none() && self.layout.contains(&Kind::NowPlaying) {
            self.media = Some(media::spawn(move || unsafe {
                let _ = PostMessageW(Some(HWND(ctl as *mut _)), WM_MEDIA, WPARAM(0), LPARAM(0));
            }));
        }
        self.ssid = wifi::ssid();
        self.intro = true;
        self.build();
        self.intro = false;
        if let Some(p) = &self.panel {
            let _ = p.popup.root().SetOpacity(0.0);
            p.popup.show();
        }
        unsafe {
            SetTimer(Some(self.ctl), T_INTRO, 60, None);
        }
    }

    fn start_intro(&mut self) {
        unsafe {
            let _ = KillTimer(Some(self.ctl), T_INTRO);
        }
        let items = std::mem::take(&mut self.intro_items);
        if self.closing {
            return;
        }
        let Some(p) = self.panel.as_ref() else { return };
        let _ = motion::pop_in(&self.comp, p.popup.root(), Vector3 { X: pad() + p.w - 24.0 * p.dpi, Y: pad(), Z: 0.0 }, 0.45, 1.025, 0, motion::CC_OPEN_MS);
        for (v, center, delay) in items {
            let _ = motion::pop_in(&self.comp, &v, center, 0.7, 1.04, delay, 360);
        }
    }

    fn close(&mut self) {
        if self.closing || self.panel.is_none() {
            return;
        }
        nacre_platform::input::end_dismiss();
        unsafe {
            let _ = KillTimer(Some(self.ctl), T_INTRO);
        }
        self.intro_items.clear();
        self.edit = false;
        if let Some(g) = self.gallery.take() {
            let h = g.popup.hwnd;
            drop(g);
            unsafe {
                let _ = DestroyWindow(h);
            }
        }
        let Some(p) = self.panel.as_ref() else { return };
        if motion::pop_out(&self.comp, p.popup.root(), Vector3 { X: pad() + p.w - 24.0 * p.dpi, Y: pad(), Z: 0.0 }, 0.55, motion::CC_CLOSE_MS).is_ok() {
            self.closing = true;
            unsafe {
                SetTimer(Some(self.ctl), T_CLOSE, motion::CC_CLOSE_MS + 20, None);
            }
        } else {
            self.finish_close();
        }
    }

    fn finish_close(&mut self) {
        unsafe {
            let _ = KillTimer(Some(self.ctl), T_CLOSE);
        }
        self.closing = false;
        self.edit = false;
        if let Some(p) = self.panel.take() {
            let h = p.popup.hwnd;
            drop(p);
            unsafe {
                let _ = DestroyWindow(h);
            }
            self.cache.clear();
            nacre_platform::sys::trim_later(5000);
        }
    }

    fn text_surf(&mut self, text: &str, px: i32, weight: i32, rgb: [u8; 3], maxw: i32) -> Option<(CompositionDrawingSurface, i32, i32)> {
        let key = format!("t|{px}|{weight}|{rgb:?}|{maxw}|{text}");
        if let Some(v) = self.cache.get(&key) {
            return Some(v.clone());
        }
        let style = Style { px, weight };
        let (tw, th) = measure(text, &style);
        let tw = (tw + 2).min(maxw.max(8));
        let bmp = render(text, &style, tw, th)?;
        let s = self.gfx.as_ref()?.surface_from_bgra(bmp.w, bmp.h, &recolor(&bmp, rgb)).ok()?;
        let v = (s, tw, th);
        self.cache.insert(key, v.clone());
        Some(v)
    }

    fn glyph_surf(&mut self, g: &str, px: i32, rgb: [u8; 3]) -> Option<(CompositionDrawingSurface, i32, i32)> {
        let key = format!("g|{px}|{rgb:?}|{g}");
        if let Some(v) = self.cache.get(&key) {
            return Some(v.clone());
        }
        let b = (px as f32 * 1.6) as i32;
        let bmp = render_face(g, &Style { px, weight: 400 }, ICON_FACE, b, b)?;
        let (w, h) = (bmp.w as usize, bmp.h as usize);
        let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0usize, 0usize);
        for y in 0..h {
            for x in 0..w {
                if bmp.bgra[(y * w + x) * 4 + 3] > 8 {
                    x0 = x0.min(x);
                    x1 = x1.max(x);
                    y0 = y0.min(y);
                    y1 = y1.max(y);
                }
            }
        }
        if x0 > x1 {
            return None;
        }
        let (gw, gh) = (x1 - x0 + 1, y1 - y0 + 1);
        let rec = recolor(&bmp, rgb);
        let mut data = vec![0u8; gw * gh * 4];
        for y in 0..gh {
            for x in 0..gw {
                let s = ((y0 + y) * w + x0 + x) * 4;
                let d = (y * gw + x) * 4;
                data[d..d + 4].copy_from_slice(&rec[s..s + 4]);
            }
        }
        let s = self.gfx.as_ref()?.surface_from_bgra(gw as u32, gh as u32, &data).ok()?;
        let v = (s, gw as i32, gh as i32);
        self.cache.insert(key, v.clone());
        Some(v)
    }

    fn symbol(&mut self, parent: &ContainerVisual, s: Sym, px: i32, rgb: [u8; 3], cx: f32, cy: f32, op: f32) {
        let n = px.max(8) as u32;
        let key = format!("sym|{s:?}|{n}|{rgb:?}");
        let hit = self.cache.get(&key).cloned();
        let (surf, w, h) = match hit {
            Some(v) => v,
            None => {
                let data = symbols::render(s, n, rgb);
                let Some((cd, cw, ch)) = symbols::crop_ink(&data, n, n) else { return };
                let Some(Ok(sf)) = self.gfx.as_ref().map(|g| g.surface_from_bgra(cw, ch, &cd)) else { return };
                let v = (sf, cw as i32, ch as i32);
                self.cache.insert(key, v.clone());
                v
            }
        };
        self.put(parent, &surf, cx - w as f32 / 2.0, cy - h as f32 / 2.0, w as f32, h as f32, op);
    }

    fn put(&self, parent: &ContainerVisual, s: &CompositionDrawingSurface, x: f32, y: f32, w: f32, h: f32, op: f32) {
        let go = || -> windows_core::Result<()> {
            let b = self.comp.CreateSurfaceBrushWithSurface(s)?;
            b.SetStretch(CompositionStretch::None)?;
            let v = self.comp.CreateSpriteVisual()?;
            v.SetBrush(&b)?;
            v.SetSize(Vector2 { X: w, Y: h })?;
            v.SetOffset(Vector3 { X: x, Y: y, Z: 0.0 })?;
            v.SetOpacity(op)?;
            parent.Children()?.InsertAtTop(&v)
        };
        let _ = go();
    }

    fn text(&mut self, parent: &ContainerVisual, t: &str, px: i32, weight: i32, rgb: [u8; 3], x: f32, y: f32, maxw: f32, op: f32) {
        if let Some((s, w, h)) = self.text_surf(t, px, weight, rgb, maxw as i32) {
            self.put(parent, &s, x, y, w as f32, h as f32, op);
        }
    }

    fn glyph(&mut self, parent: &ContainerVisual, g: &str, px: i32, rgb: [u8; 3], cx: f32, cy: f32, op: f32) {
        if let Some((s, w, h)) = self.glyph_surf(g, px, rgb) {
            self.put(parent, &s, cx - w as f32 / 2.0, cy - h as f32 / 2.0, w as f32, h as f32, op);
        }
    }

    fn rrect(&self, parent: &ContainerVisual, r: Rect, radius: f32, c: Color) -> Option<SpriteVisual> {
        let go = || -> windows_core::Result<SpriteVisual> {
            let v = self.comp.CreateSpriteVisual()?;
            v.SetSize(Vector2 { X: r.2, Y: r.3 })?;
            v.SetOffset(Vector3 { X: r.0, Y: r.1, Z: 0.0 })?;
            v.SetBrush(&self.comp.CreateColorBrushWithColor(c)?)?;
            let g = self.comp.CreateRoundedRectangleGeometry()?;
            g.SetSize(Vector2 { X: r.2, Y: r.3 })?;
            g.SetCornerRadius(Vector2 { X: radius, Y: radius })?;
            let clip = self.comp.CreateGeometricClip()?;
            clip.SetGeometry(&g)?;
            v.SetClip(&clip)?;
            parent.Children()?.InsertAtTop(&v)?;
            Ok(v)
        };
        go().ok()
    }

    fn glass(&self, parent: &ContainerVisual, r: Rect, radius: f32) -> Option<GlassPanel> {
        let look = prefs::look(prefs::Role::Dark);
        let base = GlassParams::dark((r.2, r.3), radius);
        let p = GlassPanel::new(&self.comp, parent, prefs::apply(base, &look)).ok()?;
        let _ = p.set_rect(r.0, r.1, r.2, r.3);
        let _ = p.set_radius(radius);
        Some(p)
    }

    fn is_on(&self, k: Kind) -> bool {
        match k {
            Kind::Wifi => self.wifi == Some(true),
            Kind::Bluetooth => self.bt == Some(true),
            Kind::Stage => prefs::get().stage,
            Kind::DarkMode => prefs::look(prefs::Role::Dark).dark && prefs::get().theme == nacre_core::settings::Theme::Dark,
            _ => false,
        }
    }

    fn status(&self, k: Kind) -> String {
        let on = prefs::tr("Aktif");
        let off = prefs::tr("Mati");
        match k {
            Kind::Wifi => match (self.wifi, &self.ssid) {
                (Some(true), Some(s)) => s.clone(),
                (Some(true), None) => on.to_string(),
                (Some(false), _) => off.to_string(),
                _ => "-".to_string(),
            },
            Kind::Bluetooth => match self.bt {
                Some(true) => on.to_string(),
                Some(false) => off.to_string(),
                _ => "-".to_string(),
            },
            Kind::Hotspot | Kind::Share => prefs::tr("Buka Pengaturan").to_string(),
            Kind::Battery => match nacre_platform::sys::battery() {
                Some((p, ch)) => format!("{p}%{}", if ch { " \u{26A1}" } else { "" }),
                None => "-".to_string(),
            },
            Kind::Focus => prefs::tr("Fokus").to_string(),
            _ => String::new(),
        }
    }

    fn build(&mut self) {
        let old = self.panel.take();
        let dpi;
        let popup = match old {
            Some(mut p) => {
                if let Ok(k) = p.popup.content.Children() {
                    let _ = k.RemoveAll();
                }
                p._panels.clear();
                dpi = p.dpi;
                p.popup
            }
            None => {
                let base = GlassParams::dark((10.0, 10.0), 0.0);
                let Ok(popup) = Popup::new(&self.comp, w!("AbraControlCenter"), Some(panel_proc), WINDOW_EX_STYLE(0), base) else { return };
                dpi = unsafe { GetDpiForWindow(popup.hwnd) }.max(96) as f32 / 96.0 * prefs::ui_scale();
                popup
            }
        };
        let mut popup = popup;
        let (cell, gap) = (82.0 * dpi, 14.0 * dpi);
        let sizes = self.sizes.clone();
        let (placed, rows) = controls::layout_sized(&self.layout, &|k| *sizes.get(&k).unwrap_or(&k.default_size()));
        let rows = if self.edit { rows + 2 } else { rows };
        let (gw, gh) = (4.0 * cell + 3.0 * gap, rows as f32 * cell + (rows.max(1) - 1) as f32 * gap);
        let btn_h = 34.0 * dpi;
        let inuse = if self.edit { Vec::new() } else { nacre_platform::privacy::active() };
        let top = if inuse.is_empty() { 0.0 } else { 36.0 * dpi };
        let (pw, ph) = (gw, top + gh + 14.0 * dpi + btn_h + 4.0 * dpi);
        let mon = crate::services::rail_area().unwrap_or_else(|| nacre_platform::monitors::under_cursor().area);
        let x = (mon.right() as f32 - pw - 10.0 * dpi) as i32;
        let y = self.top + (6.0 * dpi) as i32;
        let m = (8.0 * dpi).round();
        PAD.store(m.to_bits(), std::sync::atomic::Ordering::Relaxed);
        let _ = popup.set_rect(x - m as i32, y - m as i32, (pw + 2.0 * m).ceil() as i32, (ph + 2.0 * m).ceil() as i32);
        let _ = popup.panel.set_rect(0.0, 0.0, 0.0, 0.0);
        let _ = popup.content.SetOffset(Vector3 { X: m, Y: m, Z: 0.0 });
        let content = popup.content.clone();

        let mut mods = Vec::new();
        let mut panels = Vec::new();
        let mut groups: Vec<(ContainerVisual, Rect)> = Vec::new();
        let mut occupied: Vec<(usize, usize)> = Vec::new();
        for p in &placed {
            let r: Rect = grid_rect(p, cell, gap, top);
            for dy in 0..p.h {
                for dx in 0..p.w {
                    occupied.push((p.col + dx, p.row + dy));
                }
            }
            let radius = if p.w == 1 && p.h == 1 { r.2 / 2.0 } else if p.h >= 2 || p.w >= 3 { CARD_R * cell } else { r.3 / 2.0 };
            let mc = self.comp.CreateContainerVisual().ok();
            if let Some(mc) = &mc {
                let _ = content.Children().and_then(|k| k.InsertAtTop(mc));
                groups.push((mc.clone(), r));
            }
            let parent = mc.as_ref().unwrap_or(&content);
            if let Some(g) = self.glass(parent, r, radius) {
                panels.push(g);
            }
            let mut m = self.draw_module(parent, p, r, dpi, radius);
            m.cont = mc.clone();
            mods.push(m);
        }
        if self.edit {
            for row in 0..rows {
                for col in 0..4 {
                    if occupied.contains(&(col, row)) {
                        continue;
                    }
                    let r: Rect = (col as f32 * (cell + gap), row as f32 * (cell + gap), cell, cell);
                    let _ = self.rrect(&content, r, cell / 2.0, Color { A: 40, R: 255, G: 255, B: 255 });
                }
            }
        }
        let label = if self.edit { prefs::tr("Selesai") } else { prefs::tr("Edit Kontrol…") };
        let bw = 128.0 * dpi;
        let bx = (pw - bw) / 2.0;
        let edit_btn: Rect = (bx, top + gh + 14.0 * dpi, bw, btn_h);
        let ec = self.comp.CreateContainerVisual().ok();
        if let Some(ec) = &ec {
            let _ = content.Children().and_then(|k| k.InsertAtTop(ec));
            groups.push((ec.clone(), edit_btn));
        }
        let eparent = ec.as_ref().unwrap_or(&content);
        if let Some(g) = self.glass(eparent, edit_btn, btn_h / 2.0) {
            panels.push(g);
        }
        let (lw, _) = measure(label, &Style { px: (13.0 * dpi) as i32, weight: 600 });
        self.text(eparent, label, (13.0 * dpi) as i32, 600, WHITE, bx + ((bw - lw as f32) / 2.0).max(8.0 * dpi), edit_btn.1 + 8.0 * dpi, bw - 16.0 * dpi, 1.0);
        let chip = self.draw_chip(&content, &inuse, pw, dpi, &mut groups, &mut panels);
        self.intro_items.clear();
        if self.intro {
            let icon = (pw - 24.0 * dpi, 0.0f32);
            let mut order: Vec<usize> = (0..groups.len()).collect();
            let dist = |r: &Rect| ((r.0 + r.2 / 2.0 - icon.0).powi(2) + (r.1 + r.3 / 2.0 - icon.1).powi(2)).sqrt();
            order.sort_by(|a, b| dist(&groups[*a].1).total_cmp(&dist(&groups[*b].1)));
            for (rank, i) in order.into_iter().enumerate() {
                let (v, r) = &groups[i];
                let center = Vector3 { X: r.0 + r.2 / 2.0, Y: r.1 + r.3 / 2.0, Z: 0.0 };
                let _ = v.SetOpacity(0.0);
                self.intro_items.push((v.clone(), center, 40 + rank as u32 * motion::CC_STAGGER_MS));
            }
        }

        let hwnd = popup.hwnd;
        let rect = popup.rect;
        self.panel = Some(Panel { popup, dpi, cell, gap, mods, edit_btn, _panels: panels, drag: None, slider: None, w: pw, chip, hover: None, preview: Vec::new(), shift: HashMap::new(), lift: (0.0, 0.0), resize: None, ghost: None });
        if self.edit {
            nacre_platform::input::end_dismiss();
        } else {
            nacre_platform::input::begin_dismiss((rect.0, rect.1, rect.0 + rect.2, rect.1 + rect.3), hwnd.0 as isize, WM_CLOSE_CC);
        }
    }

    fn draw_chip(&mut self, content: &ContainerVisual, inuse: &[nacre_platform::privacy::InUse], pw: f32, dpi: f32, groups: &mut Vec<(ContainerVisual, Rect)>, panels: &mut Vec<GlassPanel>) -> Option<(Rect, bool)> {
        if inuse.is_empty() {
            return None;
        }
        let mut apps: Vec<&str> = Vec::new();
        for u in inuse {
            if !apps.contains(&u.app.as_str()) {
                apps.push(&u.app);
            }
        }
        let (mic, cam) = (inuse.iter().any(|u| !u.camera), inuse.iter().any(|u| u.camera));
        let label = apps.iter().take(2).copied().collect::<Vec<_>>().join(", ");
        let style = Style { px: (12.5 * dpi) as i32, weight: 600 };
        let (lw, _) = measure(&label, &style);
        let ch = 28.0 * dpi;
        let dots = (mic as u32 + cam as u32) as f32 * 13.0 * dpi;
        let cw = (12.0 * dpi + dots + 3.0 * dpi + lw as f32 + 14.0 * dpi).min(pw);
        let cr: Rect = (pw - cw, 0.0, cw, ch);
        let cc = self.comp.CreateContainerVisual().ok()?;
        let _ = content.Children().and_then(|k| k.InsertAtTop(&cc));
        groups.push((cc.clone(), cr));
        if let Some(g) = self.glass(&cc, cr, ch / 2.0) {
            panels.push(g);
        }
        let mut x = cr.0 + 12.0 * dpi;
        let d = 8.0 * dpi;
        for (on, col) in [(mic, Color { A: 255, R: 255, G: 149, B: 0 }), (cam, Color { A: 255, R: 52, G: 199, B: 89 })] {
            if on {
                let _ = self.rrect(&cc, (x, cr.1 + (ch - d) / 2.0, d, d), d / 2.0, col);
                x += 13.0 * dpi;
            }
        }
        self.text(&cc, &label, style.px, 600, WHITE, x + 3.0 * dpi, cr.1 + 6.0 * dpi, lw as f32 + 4.0, 1.0);
        Some((cr, cam && !mic))
    }

    fn draw_module(&mut self, c: &ContainerVisual, p: &Placed, r: Rect, dpi: f32, radius: f32) -> Mod {
        let k = p.kind;
        let on = self.is_on(k);
        let px = |v: f32| (v * dpi) as i32;
        let rad = radius.min(r.2 / 2.0).min(r.3 / 2.0);
        let (gt, grc) = (17.0 * dpi, rad + 2.5 * dpi);
        let (gcx, gcy) = (r.0 + r.2 - rad, r.1 + r.3 - rad);
        let grip_box: Rect = (gcx - gt, gcy - gt, grc + 2.0 * gt, grc + 2.0 * gt);
        let mut m = Mod { placed: *p, rect: r, track: None, fill: None, thumb: vec![], btns: vec![], badge: (r.0 - 6.0 * dpi, r.1 - 6.0 * dpi, 24.0 * dpi, 24.0 * dpi), grip: grip_box, vertical: false, cont: None };
        let small = p.w == 1 && p.h == 1;
        match k {
            Kind::Display | Kind::Sound if small => {
                let sym = if k == Kind::Display { Sym::Sun } else { Sym::SpeakerWave };
                self.symbol(c, sym, px(20.0), WHITE, r.0 + r.2 / 2.0, r.1 + r.3 / 2.0, 0.95);
                m.btns.push((r, 10));
            }
            Kind::Display | Kind::Sound if p.w == 1 => {
                let tw = 12.0 * dpi;
                let (top, bot) = (r.1 + 16.0 * dpi, r.1 + r.3 - 40.0 * dpi);
                let tx = r.0 + (r.2 - tw) / 2.0;
                let th = (bot - top).max(20.0 * dpi);
                let _ = self.rrect(c, (tx, top, tw, th), tw / 2.0, Color { A: 45, R: 255, G: 255, B: 255 });
                let frac = if k == Kind::Display { brightness::latest().map(|v| v as f32 / 100.0).unwrap_or(0.5) } else { audio::latest().level };
                let (kw, kh) = (KNOB.1 * dpi, KNOB.0 * dpi);
                let ky = nacre_core::hud::slider::pos_of(frac, top + kh / 2.0, (th - kh).max(1.0), true);
                let fill = self.rrect(c, (tx, ky, tw, (top + th - ky).max(tw)), tw / 2.0, Color { A: 240, R: 255, G: 255, B: 255 });
                let sym = if k == Kind::Display { Sym::Sun } else { Sym::SpeakerWave };
                self.symbol(c, sym, px(17.0), WHITE, r.0 + r.2 / 2.0, r.1 + r.3 - 21.0 * dpi, 0.85);
                let mut thumb = Vec::new();
                let sh = 1.0 * dpi;
                let [s0, s1, s2] = DROP_IDLE;
                for (inset, col) in [(-sh, s0), (0.0, s1), (1.5 * dpi, s2)] {
                    let (w, h) = (kw - 2.0 * inset, kh - 2.0 * inset);
                    if let Some(v) = self.rrect(c, (r.0 + r.2 / 2.0 - w / 2.0, ky - h / 2.0, w, h), w / 2.0, col) {
                        thumb.push(v);
                    }
                }
                m.thumb = thumb;
                m.track = Some((tx - 14.0 * dpi, top, tw + 28.0 * dpi, th));
                m.fill = fill;
                m.vertical = true;
            }
            Kind::Display | Kind::Sound => {
                let title = prefs::tr(k.title());
                let titled = p.w >= 3 || p.h >= 2;
                if titled {
                    self.text(c, title, px(15.5), 600, WHITE, r.0 + 20.0 * dpi, r.1 + 12.0 * dpi, r.2 - 40.0 * dpi, 0.95);
                }
                let th = 10.0 * dpi;
                let out = if k == Kind::Sound && r.2 > 250.0 * dpi { 36.0 * dpi } else { 0.0 };
                let right = if out > 0.0 { 108.0 * dpi } else { 40.0 * dpi };
                let cy = if p.h >= 2 { r.1 + r.3 - 36.0 * dpi } else if titled { r.1 + r.3 * 0.65 } else { r.1 + r.3 / 2.0 };
                let (tx, ty) = (r.0 + 42.0 * dpi, cy - th / 2.0);
                let tw = r.2 - 42.0 * dpi - right;
                let _ = self.rrect(c, (tx, ty, tw, th), th / 2.0, Color { A: 45, R: 255, G: 255, B: 255 });
                let frac = if k == Kind::Display { brightness::latest().map(|v| v as f32 / 100.0).unwrap_or(0.5) } else { audio::latest().level };
                let (kw, kh) = (KNOB.0 * dpi, KNOB.1 * dpi);
                let kx = nacre_core::hud::slider::pos_of(frac, tx + kw / 2.0, (tw - kw).max(1.0), false);
                let fill = self.rrect(c, (tx, ty, (kx - tx).max(th), th), th / 2.0, Color { A: 240, R: 255, G: 255, B: 255 });
                let (lo, hi, lo_px, hi_px) = if k == Kind::Display { (Sym::SunSmall, Sym::Sun, 15, 19) } else { (Sym::Speaker, Sym::SpeakerWave, 19, 25) };
                self.symbol(c, lo, px(lo_px as f32), WHITE, r.0 + 23.0 * dpi, cy, 0.85);
                let hi_x = if out > 0.0 { r.0 + r.2 - 86.0 * dpi } else { r.0 + r.2 - 21.0 * dpi };
                self.symbol(c, hi, px(hi_px as f32), WHITE, hi_x, cy, 0.85);
                if out > 0.0 {
                    let b: Rect = (r.0 + r.2 - 39.0 * dpi - out / 2.0, cy - out / 2.0, out, out);
                    let _ = self.rrect(c, b, out / 2.0, Color { A: 45, R: 255, G: 255, B: 255 });
                    self.symbol(c, Sym::Headphones, px(19.0), WHITE, b.0 + out / 2.0, cy, 0.9);
                    m.btns.push((b, 10));
                }
                let mut thumb = Vec::new();
                let sh = 1.0 * dpi;
                let [s0, s1, s2] = DROP_IDLE;
                for (inset, col) in [(-sh, s0), (0.0, s1), (1.5 * dpi, s2)] {
                    let (w, h) = (kw - 2.0 * inset, kh - 2.0 * inset);
                    if let Some(v) = self.rrect(c, (kx - w / 2.0, cy - h / 2.0, w, h), h / 2.0, col) {
                        thumb.push(v);
                    }
                }
                m.thumb = thumb;
                m.track = Some((tx, ty - 10.0 * dpi, tw, th + 20.0 * dpi));
                m.fill = fill;
            }
            Kind::NowPlaying => {
                let (title, artist, has, playing) = self.track.clone().unwrap_or_default();
                let pad = 14.0 * dpi;
                let play = if playing { Sym::Pause } else { Sym::Play };
                if !has {
                    let (cx, cy) = (r.0 + r.2 / 2.0, r.1 + r.3 / 2.0);
                    self.glyph(c, "\u{E8D6}", px(if small { 20.0 } else { 26.0 }), WHITE, cx, if small || p.h == 1 { cy } else { cy - 10.0 * dpi }, 0.6);
                    if !small && p.h >= 2 {
                        self.text(c, prefs::tr("Tidak diputar"), px(12.0), 500, WHITE, r.0 + 16.0 * dpi, cy + 14.0 * dpi, r.2 - 20.0 * dpi, 0.7);
                    }
                } else if small {
                    let (cx, cy) = (r.0 + r.2 / 2.0, r.1 + r.3 / 2.0);
                    self.symbol(c, play, px(22.0), WHITE, cx, cy, 1.0);
                    m.btns.push((r, 1));
                } else if p.h == 1 {
                    let art = r.3 - 2.0 * pad;
                    self.art_at(c, r.0 + pad, r.1 + pad, art, dpi);
                    let ctrl: Vec<(Sym, u8)> = if p.w >= 3 { vec![(Sym::Backward, 0), (play, 1), (Sym::Forward, 2)] } else { vec![(play, 1)] };
                    let bw = 38.0 * dpi;
                    let cy = r.1 + r.3 / 2.0;
                    let x_end = r.0 + r.2 - pad;
                    for (j, (sy, id)) in ctrl.iter().enumerate() {
                        let cx = x_end - bw * (ctrl.len() - j) as f32 + bw / 2.0;
                        self.symbol(c, *sy, px(if *id == 1 { 20.0 } else { 16.0 }), WHITE, cx, cy, 1.0);
                        m.btns.push(((cx - bw / 2.0, cy - bw / 2.0, bw, bw), *id));
                    }
                    let tx = r.0 + pad + art + 10.0 * dpi;
                    let tw = x_end - bw * ctrl.len() as f32 - tx - 6.0 * dpi;
                    if tw > 30.0 * dpi {
                        self.text(c, &title, px(13.0), 600, WHITE, tx, cy - 18.0 * dpi, tw, 1.0);
                        self.text(c, &artist, px(12.0), 400, WHITE, tx, cy + 1.0 * dpi, tw, 0.65);
                    }
                } else if p.w == 1 {
                    let art = r.2 - 2.0 * pad;
                    self.art_at(c, r.0 + pad, r.1 + pad, art, dpi);
                    let cy = r.1 + r.3 - 30.0 * dpi;
                    self.symbol(c, play, px(20.0), WHITE, r.0 + r.2 / 2.0, cy, 1.0);
                    m.btns.push(((r.0, cy - 20.0 * dpi, r.2, 40.0 * dpi), 1));
                } else if p.w >= 3 {
                    let art = (r.3 - 2.0 * pad).min(r.2 * 0.45);
                    self.art_at(c, r.0 + pad, r.1 + pad, art, dpi);
                    let tx = r.0 + pad + art + 14.0 * dpi;
                    let tw = r.0 + r.2 - pad - tx;
                    self.text(c, &title, px(15.0), 600, WHITE, tx, r.1 + pad + 4.0 * dpi, tw, 1.0);
                    self.text(c, &artist, px(13.0), 400, WHITE, tx, r.1 + pad + 26.0 * dpi, tw, 0.65);
                    let cy = r.1 + r.3 - pad - 22.0 * dpi;
                    let syms = [Sym::Backward, play, Sym::Forward];
                    for (i, sy) in syms.iter().enumerate() {
                        let cx = tx + tw * (0.18 + i as f32 * 0.32);
                        self.symbol(c, *sy, px(if i == 1 { 24.0 } else { 19.0 }), WHITE, cx, cy, 1.0);
                        m.btns.push(((cx - 22.0 * dpi, cy - 22.0 * dpi, 44.0 * dpi, 44.0 * dpi), i as u8));
                    }
                } else {
                    let art = (r.3 - 120.0 * dpi).min(r.2 - 2.0 * pad).max(40.0 * dpi).min(r.2 * 0.24 * p.h as f32);
                    self.art_at(c, r.0 + pad, r.1 + pad, art, dpi);
                    self.text(c, &title, px(13.0), 600, WHITE, r.0 + 16.0 * dpi, r.1 + 18.0 * dpi + art, r.2 - 28.0 * dpi, 1.0);
                    self.text(c, &artist, px(12.0), 400, WHITE, r.0 + 16.0 * dpi, r.1 + 38.0 * dpi + art, r.2 - 28.0 * dpi, 0.65);
                    let cy = r.1 + r.3 - 24.0 * dpi;
                    let cxs = [r.0 + r.2 * 0.22, r.0 + r.2 * 0.5, r.0 + r.2 * 0.78];
                    let syms = [Sym::Backward, play, Sym::Forward];
                    for (i, (cx, sy)) in cxs.iter().zip(syms).enumerate() {
                        self.symbol(c, sy, px(if i == 1 { 20.0 } else { 17.0 }), WHITE, *cx, cy, 1.0);
                        m.btns.push(((cx - 20.0 * dpi, cy - 20.0 * dpi, 40.0 * dpi, 40.0 * dpi), i as u8));
                    }
                }
            }
            _ if p.h >= 2 => {
                let d = 46.0 * dpi;
                let disc: Rect = (r.0 + 12.0 * dpi, r.1 + 12.0 * dpi, d, d);
                let (bg, fg) = if on { (Color { A: 245, R: 255, G: 255, B: 255 }, prefs::accent()) } else { (Color { A: 70, R: 0, G: 0, B: 0 }, WHITE) };
                let _ = self.rrect(c, disc, d / 2.0, bg);
                self.glyph(c, glyph_of(k), px(20.0), fg, disc.0 + d / 2.0, disc.1 + d / 2.0, 1.0);
                m.btns.push((disc, BTN_DISC));
                let title = prefs::tr(k.title());
                self.text(c, title, px(14.0), 600, WHITE, r.0 + 16.0 * dpi, r.1 + r.3 - 48.0 * dpi, r.2 - 28.0 * dpi, 1.0);
                let status = self.status(k);
                if !status.is_empty() {
                    self.text(c, &status, px(12.0), 400, WHITE, r.0 + 16.0 * dpi, r.1 + r.3 - 28.0 * dpi, r.2 - 28.0 * dpi, 0.65);
                }
            }
            _ if p.w == 1 && p.h == 1 => {
                if on {
                    let _ = self.rrect(c, r, r.2 / 2.0, prefs::accent_c(245));
                }
                self.glyph(c, glyph_of(k), px(20.0), WHITE, r.0 + r.2 / 2.0, r.1 + r.3 / 2.0, 1.0);
            }
            _ => {
                let d = r.3 * 0.58;
                let off = (r.3 - d) / 2.0;
                let disc: Rect = (r.0 + off, r.1 + off, d, d);
                let (bg, fg) = if on { (Color { A: 245, R: 255, G: 255, B: 255 }, prefs::accent()) } else { (Color { A: 70, R: 0, G: 0, B: 0 }, WHITE) };
                let _ = self.rrect(c, disc, d / 2.0, bg);
                self.glyph(c, glyph_of(k), px(16.0), fg, disc.0 + d / 2.0, disc.1 + d / 2.0, 1.0);
                m.btns.push((disc, BTN_DISC));
                let tx = disc.0 + d + 11.0 * dpi;
                let title = prefs::tr(k.title());
                let status = self.status(k);
                let ty = r.1 + r.3 / 2.0 - if status.is_empty() { 9.5 } else { 19.0 } * dpi;
                self.text(c, title, px(14.0), 600, WHITE, tx, ty, r.2 - d - off - 22.0 * dpi, 1.0);
                if !status.is_empty() {
                    self.text(c, &status, px(12.0), 400, WHITE, tx, ty + 19.0 * dpi, r.2 - d - off - 22.0 * dpi, 0.65);
                }
            }
        }
        if self.edit {
            let b = m.badge;
            let _ = self.rrect(c, b, b.2 / 2.0, Color { A: 150, R: 70, G: 70, B: 74 });
            let _ = self.rrect(c, b, b.2 / 2.0, Color { A: 46, R: 255, G: 255, B: 255 });
            self.glass_ring(c, b, dpi);
            let _ = self.rrect(c, (b.0 + 6.0 * dpi, b.1 + b.3 / 2.0 - 1.0 * dpi, b.2 - 12.0 * dpi, 2.2 * dpi), dpi, Color { A: 255, R: 255, G: 255, B: 255 });
            let (bw, bh) = (grip_box.2.ceil() as u32, grip_box.3.ceil() as u32);
            let data = nacre_core::logo::render_grip(bw, bh, (gt, gt), grc, gt, 14.0, 76.0);
            if let Some(Ok(sf)) = self.gfx.as_ref().map(|g| g.surface_from_bgra(bw, bh, &data)) {
                self.put(c, &sf, grip_box.0, grip_box.1, bw as f32, bh as f32, 1.0);
            }
        }
        m
    }

    fn glass_ring(&self, c: &ContainerVisual, b: Rect, dpi: f32) {
        let n = b.2.round().max(4.0) as u32;
        let data = nacre_core::widgets::ring(n, 1.4 * dpi, 1.0, [255, 255, 255], [0, 0, 0, 0]);
        if let Some(Ok(sf)) = self.gfx.as_ref().map(|g| g.surface_from_bgra(n, n, &data)) {
            self.put(c, &sf, b.0, b.1, n as f32, n as f32, 0.6);
        }
    }

    fn art_at(&self, c: &ContainerVisual, x: f32, y: f32, art: f32, dpi: f32) {
        let Some(a) = self.art.clone() else {
            let _ = self.rrect(c, (x, y, art, art), (art * 0.18).min(12.0 * dpi), Color { A: 60, R: 255, G: 255, B: 255 });
            return;
        };
        let Some(gfx) = self.gfx.as_ref() else { return };
        let go = || -> windows_core::Result<()> {
            let s = gfx.surface_from_bgra(a.w, a.h, &a.bgra)?;
            let b = self.comp.CreateSurfaceBrushWithSurface(&s)?;
            b.SetStretch(CompositionStretch::UniformToFill)?;
            let v = self.comp.CreateSpriteVisual()?;
            v.SetBrush(&b)?;
            v.SetSize(Vector2 { X: art, Y: art })?;
            v.SetOffset(Vector3 { X: x, Y: y, Z: 0.0 })?;
            let g = self.comp.CreateRoundedRectangleGeometry()?;
            g.SetSize(Vector2 { X: art, Y: art })?;
            let rad = (art * 0.18).min(14.0 * dpi);
            g.SetCornerRadius(Vector2 { X: rad, Y: rad })?;
            let cl = self.comp.CreateGeometricClip()?;
            cl.SetGeometry(&g)?;
            v.SetClip(&cl)?;
            c.Children()?.InsertAtTop(&v)
        };
        let _ = go();
    }

    fn start_resize(&mut self, i: usize) {
        let sizes = self.sizes.clone();
        let layout = self.layout.clone();
        let Some(p) = self.panel.as_mut() else { return };
        let Some(m) = p.mods.get(i) else { return };
        let k = m.placed.kind;
        let cur = k.clamp(m.placed.w, m.placed.h);
        let _ = sizes;
        p.resize = Some((i, cur));
        p.preview = layout;
        p.shift.clear();
        self.show_ghost(i, cur);
    }

    fn show_ghost(&mut self, i: usize, size: Size) {
        let comp = self.comp.clone();
        let Some(p) = self.panel.as_ref() else { return };
        let (cell, gap, dpi) = (p.cell, p.gap, p.dpi);
        let Some(m) = p.mods.get(i) else { return };
        let origin = (m.rect.0 + p.shift.get(&m.placed.kind).map(|s| s.0).unwrap_or(0.0), m.rect.1 + p.shift.get(&m.placed.kind).map(|s| s.1).unwrap_or(0.0));
        let (w, h) = (size.w as f32 * cell + (size.w - 1) as f32 * gap, size.h as f32 * cell + (size.h - 1) as f32 * gap);
        let radius = if size.w == 1 && size.h == 1 { w / 2.0 } else if size.h >= 2 || size.w >= 3 { CARD_R * cell } else { h / 2.0 };
        let content = p.popup.content.clone();
        let old = p.ghost.clone();
        if let Some(g) = old {
            if let Ok(k) = content.Children() {
                let _ = k.Remove(&g);
            }
        }
        let Ok(g) = self.comp.CreateContainerVisual() else { return };
        let _ = self.rrect(&g, (origin.0, origin.1, w, h), radius, Color { A: 60, R: 255, G: 255, B: 255 });
        let t = 2.0 * dpi;
        let _ = self.rrect(&g, (origin.0 + t, origin.1 + t, w - 2.0 * t, h - 2.0 * t), (radius - t).max(0.0), Color { A: 30, R: 0, G: 0, B: 0 });
        let _ = content.Children().and_then(|k| k.InsertAtTop(&g));
        let center = Vector3 { X: origin.0 + w / 2.0, Y: origin.1 + h / 2.0, Z: 0.0 };
        let _ = g.SetCenterPoint(center);
        let _ = g.SetScale(Vector3 { X: 0.93, Y: 0.93, Z: 1.0 });
        let _ = motion::hover_pop(&comp, &g, center, 1.0);
        if let Some(p) = self.panel.as_mut() {
            p.ghost = Some(g);
        }
    }

    fn resize_move(&mut self, x: f32, y: f32) {
        let comp = self.comp.clone();
        let Some(p) = self.panel.as_ref() else { return };
        let Some((i, cur)) = p.resize else { return };
        let Some(m) = p.mods.get(i) else { return };
        let k = m.placed.kind;
        let (cell, gap) = (p.cell, p.gap);
        let origin = (m.rect.0 + p.shift.get(&k).map(|s| s.0).unwrap_or(0.0), m.rect.1 + p.shift.get(&k).map(|s| s.1).unwrap_or(0.0));
        let want = k.clamp(controls::cells_for(x - origin.0, cell, gap), controls::cells_for(y - origin.1, cell, gap));
        if want == cur {
            return;
        }
        let mut sizes = self.sizes.clone();
        sizes.insert(k, want);
        let preview = p.preview.clone();
        let (placed, _) = controls::layout_sized(&preview, &|kk| *sizes.get(&kk).unwrap_or(&kk.default_size()));
        let mut moves = Vec::new();
        for mm in &p.mods {
            let kk = mm.placed.kind;
            let Some(q) = placed.iter().find(|q| q.kind == kk) else { continue };
            let r = grid_rect(q, cell, gap, 0.0);
            let to = (r.0 - mm.rect.0, r.1 - mm.rect.1);
            let from = p.shift.get(&kk).copied().unwrap_or((0.0, 0.0));
            if (to.0 - from.0).abs() > 0.5 || (to.1 - from.1).abs() > 0.5 {
                moves.push((kk, mm.cont.clone(), from, to));
            }
        }
        if let Some(p) = self.panel.as_mut() {
            p.resize = Some((i, want));
            for (kk, cont, from, to) in moves {
                if let Some(c) = cont {
                    let _ = motion::slide_pop(&comp, &c, Vector3 { X: from.0, Y: from.1, Z: 0.0 }, Vector3 { X: to.0, Y: to.1, Z: 0.0 });
                }
                p.shift.insert(kk, to);
            }
        }
        self.show_ghost(i, want);
    }

    fn end_resize(&mut self) {
        let from = self.origins();
        let Some(p) = self.panel.as_mut() else { return };
        let Some((i, want)) = p.resize.take() else { return };
        let k = p.mods[i].placed.kind;
        p.preview.clear();
        p.shift.clear();
        p.ghost = None;
        if want != k.clamp(p.mods[i].placed.w, p.mods[i].placed.h) {
            self.sizes.insert(k, want);
            self.save_layout();
        }
        self.build_flip(from, Some(k), None);
    }

    fn set_hover(&mut self, hit: Option<usize>) {
        let comp = self.comp.clone();
        let Some(p) = self.panel.as_mut() else { return };
        if p.hover == hit {
            return;
        }
        for (i, on) in [(p.hover, false), (hit, true)] {
            let Some(m) = i.and_then(|i| p.mods.get(i)) else { continue };
            let Some(c) = &m.cont else { continue };
            let r = m.rect;
            let to = if !on { 1.0 } else if m.placed.w == 1 && m.placed.h == 1 { motion::HOVER_BUTTON } else { motion::HOVER_ROW };
            let _ = motion::hover_pop(&comp, c, Vector3 { X: r.0 + r.2 / 2.0, Y: r.1 + r.3 / 2.0, Z: 0.0 }, to);
        }
        p.hover = hit;
    }

    fn set_slider(&mut self, i: usize, x: f32, y: f32, commit: bool) {
        let Some(p) = self.panel.as_ref() else { return };
        let Some(m) = p.mods.get(i) else { return };
        let Some(t) = m.track else { return };
        let frac = slider_value(t, m.vertical, x, y, p.dpi);
        if let Some(f) = &m.fill {
            if m.vertical {
                place_slider_v(f, &m.thumb, t, frac, p.dpi);
            } else {
                place_slider(f, &m.thumb, t, frac, p.dpi);
            }
        }
        let kind = m.placed.kind;
        if let Ok(mut g) = SLID_AT.lock() {
            *g = Some(std::time::Instant::now());
        }
        if commit {
            match kind {
                Kind::Display => services::set_brightness(frac),
                _ => services::set_volume(frac),
            }
        }
    }

    fn open_size_menu(&mut self, i: usize, x: f32, y: f32) {
        let Some(p) = self.panel.as_ref() else { return };
        let kind = p.mods[i].placed.kind;
        if !kind.resizable() {
            return;
        }
        let cur = *self.sizes.get(&kind).unwrap_or(&kind.default_size());
        let (rect, dpi, hwnd) = (p.popup.rect, p.dpi, p.popup.hwnd);
        let Some(gfx) = self.gfx.as_ref() else { return };
        if self.menu.is_none() {
            match Menu::new(&self.comp, hwnd, dpi, Tier::Full) {
                Ok(m) => self.menu = Some(m),
                Err(_) => return,
            }
        }
        let mk = |id: usize, label: &'static str, s: Size| MItem::action(id, &format!("{}{}", if cur == s { "\u{2713} " } else { "" }, prefs::tr(label)), true);
        let items = vec![mk(1, "Kecil", Size::S), mk(2, "Sedang", Size::M), mk(3, "Besar", Size::L)];
        self.menu_kind = Some(kind);
        if let Some(m) = self.menu.as_mut() {
            let _ = m.show(&self.comp, gfx, items, rect.0 + (x + pad()) as i32, rect.1 + (y + pad()) as i32);
        }
    }

    fn pick_size(&mut self, id: usize) {
        if let Some(m) = &self.menu {
            m.hide();
        }
        let Some(kind) = self.menu_kind.take() else { return };
        let s = match id {
            1 => Size::S,
            2 => Size::M,
            _ => Size::L,
        };
        let from = self.origins();
        self.sizes.insert(kind, s);
        self.build_flip(from, Some(kind), None);
        self.save_layout();
    }

    fn origins(&self) -> HashMap<Kind, (f32, f32)> {
        let Some(p) = self.panel.as_ref() else { return HashMap::new() };
        let dragged = p.drag.map(|(i, _, _)| p.mods[i].placed.kind);
        p.mods
            .iter()
            .map(|m| {
                let k = m.placed.kind;
                let (dx, dy) = if Some(k) == dragged { p.lift } else { p.shift.get(&k).copied().unwrap_or((0.0, 0.0)) };
                (k, (m.rect.0 + dx, m.rect.1 + dy))
            })
            .collect()
    }

    fn build_flip(&mut self, from: HashMap<Kind, (f32, f32)>, pop: Option<Kind>, lifted: Option<Kind>) {
        self.build();
        let comp = self.comp.clone();
        let Some(p) = self.panel.as_ref() else { return };
        for m in &p.mods {
            let Some(c) = &m.cont else { continue };
            let k = m.placed.kind;
            let center = center_of(m.rect);
            if Some(k) == pop || (!from.is_empty() && !from.contains_key(&k)) {
                let _ = motion::pop_in(&comp, c, center, 0.5, 1.06, 0, 360);
                continue;
            }
            if let Some(o) = from.get(&k) {
                let (dx, dy) = (o.0 - m.rect.0, o.1 - m.rect.1);
                if dx.abs() > 0.5 || dy.abs() > 0.5 {
                    let _ = motion::slide_pop(&comp, c, Vector3 { X: dx, Y: dy, Z: 0.0 }, Vector3 { X: 0.0, Y: 0.0, Z: 0.0 });
                }
            }
            if Some(k) == lifted {
                let _ = c.SetCenterPoint(center);
                let _ = c.SetScale(Vector3 { X: 1.08, Y: 1.08, Z: 1.0 });
                let _ = motion::hover_pop(&comp, c, center, 1.0);
            }
        }
    }

    fn pop_all(&self) {
        let Some(p) = self.panel.as_ref() else { return };
        for (rank, m) in p.mods.iter().enumerate() {
            if let Some(c) = &m.cont {
                let _ = c.SetOpacity(0.0);
                let _ = motion::pop_in(&self.comp, c, center_of(m.rect), 0.86, 1.05, rank as u32 * 14, 340);
            }
        }
    }

    fn start_drag(&mut self, i: usize, x: f32, y: f32) {
        let comp = self.comp.clone();
        let layout = self.layout.clone();
        let Some(p) = self.panel.as_mut() else { return };
        p.drag = Some((i, x, y));
        p.lift = (0.0, 0.0);
        p.preview = layout;
        p.shift.clear();
        let content = p.popup.content.clone();
        if let Some(m) = p.mods.get(i) {
            if let Some(c) = &m.cont {
                if let Ok(k) = content.Children() {
                    let _ = k.Remove(c);
                    let _ = k.InsertAtTop(c);
                }
                let _ = motion::hover_pop(&comp, c, center_of(m.rect), 1.08);
            }
        }
    }

    fn drag_move(&mut self, x: f32, y: f32) {
        let comp = self.comp.clone();
        let sizes = self.sizes.clone();
        let size_of = |k: Kind| *sizes.get(&k).unwrap_or(&k.default_size());
        let Some(p) = self.panel.as_mut() else { return };
        let Some((i, x0, y0)) = p.drag else { return };
        let Some(kind) = p.mods.get(i).map(|m| m.placed.kind) else { return };
        p.lift = (x - x0, y - y0);
        if let Some(c) = &p.mods[i].cont {
            if let Ok(o) = c.cast::<windows::UI::Composition::CompositionObject>() {
                let _ = o.StopAnimation(&windows_core::HSTRING::from("Offset"));
            }
            let _ = c.SetOffset(Vector3 { X: p.lift.0, Y: p.lift.1, Z: 0.0 });
        }
        let (cell, gap) = (p.cell, p.gap);
        let (placed, _) = controls::layout_sized(&p.preview, &size_of);
        let Some(over) = placed.iter().filter(|q| q.kind != kind).find(|q| hit(grid_rect(q, cell, gap, 0.0), x, y)).map(|q| q.kind) else { return };
        let (Some(a), Some(b)) = (p.preview.iter().position(|k| *k == kind), p.preview.iter().position(|k| *k == over)) else { return };
        let k = p.preview.remove(a);
        p.preview.insert(b, k);
        let (placed, _) = controls::layout_sized(&p.preview, &size_of);
        for m in &p.mods {
            let k = m.placed.kind;
            if k == kind {
                continue;
            }
            let Some(q) = placed.iter().find(|q| q.kind == k) else { continue };
            let r = grid_rect(q, cell, gap, 0.0);
            let to = (r.0 - m.rect.0, r.1 - m.rect.1);
            let cur = p.shift.get(&k).copied().unwrap_or((0.0, 0.0));
            if (to.0 - cur.0).abs() > 0.5 || (to.1 - cur.1).abs() > 0.5 {
                if let Some(c) = &m.cont {
                    let _ = motion::slide_pop(&comp, c, Vector3 { X: cur.0, Y: cur.1, Z: 0.0 }, Vector3 { X: to.0, Y: to.1, Z: 0.0 });
                }
                p.shift.insert(k, to);
            }
        }
    }

    fn end_drag(&mut self) {
        let from = self.origins();
        let Some(p) = self.panel.as_mut() else { return };
        let Some((i, _, _)) = p.drag.take() else { return };
        let kind = p.mods[i].placed.kind;
        let preview = std::mem::take(&mut p.preview);
        p.shift.clear();
        if !preview.is_empty() && preview != self.layout {
            self.layout = preview;
            self.save_layout();
        }
        self.build_flip(from, None, Some(kind));
    }

    fn finish_remove(&mut self) {
        unsafe {
            let _ = KillTimer(Some(self.ctl), T_REMOVE);
        }
        let Some(kind) = self.pending_remove.take() else { return };
        let from = self.origins();
        self.layout.retain(|k| *k != kind);
        self.build_flip(from, None, None);
        self.refresh_gallery();
    }

    fn set_gallery_hover(&mut self, i: Option<usize>) {
        let comp = self.comp.clone();
        let Some(g) = self.gallery.as_mut() else { return };
        if g.hover == i {
            return;
        }
        for (k, on) in [(g.hover, false), (i, true)] {
            if let Some((r, c, to)) = k.and_then(|k| g.conts.get(k)) {
                let _ = motion::hover_pop(&comp, c, center_of(*r), if on { *to } else { 1.0 });
            }
        }
        g.hover = i;
    }

    fn open_picker(&mut self, i: usize) {
        let Some(p) = self.panel.as_ref() else { return };
        let Some(m) = p.mods.get(i) else { return };
        let which = match m.placed.kind {
            Kind::Wifi => crate::netmenu::Which::Wifi,
            Kind::Bluetooth => crate::netmenu::Which::Bluetooth,
            _ => return,
        };
        let (rect, dpi) = (p.popup.rect, p.dpi);
        self.close();
        let m = pad() as i32;
        crate::netmenu::show(&self.comp, which, crate::netmenu::Anchor::Corner(rect.0 + rect.2 - m, rect.1 + m), dpi);
    }

    fn click(&mut self, i: usize, x: f32, y: f32) {
        let Some(p) = self.panel.as_ref() else { return };
        let m = &p.mods[i];
        let kind = m.placed.kind;
        let btn = m.btns.iter().find(|(r, _)| hit(*r, x, y)).map(|(_, b)| *b);
        let send = |cmd: RadioCmd, s: &Cc| {
            if let Some(r) = &s.radios {
                let _ = r.commands.send(cmd);
            }
        };
        let small = m.placed.w == 1 && m.placed.h == 1;
        match kind {
            Kind::Wifi | Kind::Bluetooth if !small && btn != Some(BTN_DISC) => self.pick_up = Some(i),
            Kind::Wifi => send(RadioCmd::Set(RK::Wifi, !(self.wifi == Some(true))), self),
            Kind::Bluetooth => send(RadioCmd::Set(RK::Bluetooth, !(self.bt == Some(true))), self),
            Kind::NowPlaying => {
                if let (Some(b), Some(md)) = (btn, &self.media) {
                    let _ = md.commands.send(match b {
                        0 => Command::Prev,
                        1 => Command::PlayPause,
                        _ => Command::Next,
                    });
                }
            }
            Kind::Stage => {
                prefs::update(|s| s.stage = !s.stage);
                self.build();
            }
            Kind::DarkMode => {
                prefs::update(|s| {
                    s.theme = if s.theme == nacre_core::settings::Theme::Dark { nacre_core::settings::Theme::Light } else { nacre_core::settings::Theme::Dark }
                });
                self.build();
            }
            Kind::Display | Kind::Sound => {
                if btn == Some(10) {
                    self.close();
                    if kind == Kind::Sound {
                        keys::chord(&[0x5B, 0x11, 0x56]);
                    } else {
                        launch::open("ms-settings:display");
                    }
                }
            }
            other => {
                self.close();
                match other {
                    Kind::Hotspot => launch::open("ms-settings:network-mobilehotspot"),
                    Kind::Share => launch::open("ms-settings:crossdevice"),
                    Kind::Airplane => launch::open("ms-settings:network-airplanemode"),
                    Kind::Mirror => keys::chord(&[keys::WIN, 0x4B]),
                    Kind::Focus => launch::open("ms-settings:notifications"),
                    Kind::Screenshot => keys::chord(&[keys::WIN, keys::SHIFT, 0x53]),
                    Kind::Battery => launch::open("ms-settings:batterysaver"),
                    Kind::Lock => launch::run("rundll32.exe", "user32.dll,LockWorkStation"),
                    Kind::Mission => crate::mission::toggle(),
                    Kind::Apps => crate::launchpad::toggle(),
                    Kind::Calculator => launch::show("calc.exe", ""),
                    Kind::Timer => launch::open("ms-clock:"),
                    _ => {}
                }
            }
        }
    }

    fn enter_edit(&mut self) {
        self.edit = true;
        self.build();
        self.pop_all();
        self.open_gallery();
    }

    fn leave_edit(&mut self) {
        self.save_layout();
        self.edit = false;
        if let Some(g) = self.gallery.take() {
            let h = g.popup.hwnd;
            drop(g);
            unsafe {
                let _ = DestroyWindow(h);
            }
        }
        self.build();
        self.pop_all();
    }

    fn remove(&mut self, kind: Kind) {
        if self.pending_remove.is_some() {
            self.finish_remove();
        }
        if let Some(m) = self.panel.as_ref().and_then(|p| p.mods.iter().find(|m| m.placed.kind == kind)) {
            if let Some(c) = &m.cont {
                let _ = motion::pop_out(&self.comp, c, center_of(m.rect), 0.4, 180);
            }
        }
        self.pending_remove = Some(kind);
        unsafe {
            SetTimer(Some(self.ctl), T_REMOVE, 190, None);
        }
    }

    fn add(&mut self, kind: Kind) {
        if !self.layout.contains(&kind) {
            let from = self.origins();
            self.layout.push(kind);
            if kind == Kind::NowPlaying && self.media.is_none() {
                let ctl = self.ctl.0 as isize;
                self.media = Some(media::spawn(move || unsafe {
                    let _ = PostMessageW(Some(HWND(ctl as *mut _)), WM_MEDIA, WPARAM(0), LPARAM(0));
                }));
            }
            self.build_flip(from, Some(kind), None);
            self.refresh_gallery();
        }
    }

    fn open_gallery(&mut self) {
        let Some(p) = self.panel.as_ref() else { return };
        let dpi = p.dpi;
        let base = GlassParams::dark((10.0, 10.0), 26.0 * dpi);
        let look = prefs::look(prefs::Role::Dark);
        let Ok(mut popup) = Popup::new_with(&self.comp, w!("AbraCcGallery"), Some(gallery_proc), WINDOW_EX_STYLE(0), prefs::apply(base, &look), true) else { return };
        let (gw, gh) = (620.0 * dpi, 560.0 * dpi);
        let rect = p.popup.rect;
        let x = (rect.0 as f32 + pad() - gw - 16.0 * dpi).max(10.0 * dpi);
        let _ = popup.set_rect(x as i32, rect.1 + pad() as i32, gw as i32, gh as i32);
        let _ = popup.panel.set_radius(26.0 * dpi);
        self.gallery = Some(Gallery { popup, dpi, cat: None, query: String::new(), cards: vec![], cats: vec![], done: (0.0, 0.0, 0.0, 0.0), scroll: 0.0, content_h: 0.0, _panels: vec![], conts: vec![], hover: None, tracking: false });
        self.refresh_gallery();
        if let Some(g) = &self.gallery {
            g.popup.show();
            let _ = motion::pop_in(&self.comp, g.popup.root(), Vector3 { X: g.popup.rect.2 as f32, Y: 0.0, Z: 0.0 }, 0.82, 1.02, 0, 340);
        }
    }

    fn refresh_gallery(&mut self) {
        let Some(g) = self.gallery.as_ref() else { return };
        let (dpi, cat, query, scroll) = (g.dpi, g.cat, g.query.to_lowercase(), g.scroll);
        let content = g.popup.content.clone();
        let (gw, gh) = (g.popup.rect.2 as f32, g.popup.rect.3 as f32);
        if let Ok(k) = content.Children() {
            let _ = k.RemoveAll();
        }
        let side = 190.0 * dpi;
        let mut cats = Vec::new();
        let mut conts: Vec<(Rect, ContainerVisual, f32)> = Vec::new();
        let sb: Rect = (16.0 * dpi, 16.0 * dpi, side - 8.0 * dpi, 36.0 * dpi);
        let _ = self.rrect(&content, sb, 10.0 * dpi, Color { A: 60, R: 255, G: 255, B: 255 });
        let q = if query.is_empty() { prefs::tr("Cari Kontrol").to_string() } else { g.query.clone() };
        self.text(&content, &q, (13.0 * dpi) as i32, 400, WHITE, sb.0 + 12.0 * dpi, sb.1 + 9.0 * dpi, sb.2 - 20.0 * dpi, if query.is_empty() { 0.55 } else { 1.0 });
        let mut y = 66.0 * dpi;
        let mut entries: Vec<(Option<Category>, &str)> = vec![(None, "Semua Kontrol")];
        entries.extend(Category::ALL.iter().map(|c| (Some(*c), c.title())));
        for (c, title) in entries {
            let r: Rect = (12.0 * dpi, y, side, 36.0 * dpi);
            let Ok(row) = self.comp.CreateContainerVisual() else { continue };
            let _ = content.Children().and_then(|k| k.InsertAtTop(&row));
            conts.push((r, row.clone(), motion::HOVER_ROW));
            let content = row;
            if c == cat {
                let _ = self.rrect(&content, r, 10.0 * dpi, Color { A: 70, R: 255, G: 255, B: 255 });
            }
            let (gl, col): (&str, [u8; 3]) = match c {
                None => ("\u{E71D}", [99, 99, 108]),
                Some(Category::Connectivity) => ("\u{E701}", [10, 132, 255]),
                Some(Category::Display) => ("\u{E706}", [10, 132, 255]),
                Some(Category::Sound) => ("\u{E995}", [255, 45, 85]),
                Some(Category::Media) => ("\u{E8D6}", [255, 69, 58]),
                Some(Category::Battery) => ("\u{E83F}", [52, 199, 89]),
                Some(Category::Utilities) => ("\u{E713}", [142, 142, 147]),
            };
            let ic: Rect = (r.0 + 10.0 * dpi, r.1 + 7.0 * dpi, 22.0 * dpi, 22.0 * dpi);
            let _ = self.rrect(&content, ic, 6.0 * dpi, Color { A: 255, R: col[0], G: col[1], B: col[2] });
            self.glyph(&content, gl, (12.0 * dpi) as i32, WHITE, ic.0 + ic.2 / 2.0, ic.1 + ic.3 / 2.0, 1.0);
            self.text(&content, prefs::tr(title), (13.0 * dpi) as i32, 500, WHITE, r.0 + 42.0 * dpi, r.1 + 9.0 * dpi, r.2 - 48.0 * dpi, 1.0);
            cats.push((c, r));
            y += 38.0 * dpi;
        }
        let mut cards = Vec::new();
        let list: Vec<Kind> = controls::ALL
            .iter()
            .copied()
            .filter(|k| cat.map(|c| k.category() == c).unwrap_or(true))
            .filter(|k| query.is_empty() || prefs::tr(k.title()).to_lowercase().contains(&query))
            .collect();
        let (cx0, mut cy) = (side + 36.0 * dpi, 20.0 * dpi - scroll);
        let (cw, ch, cg) = (130.0 * dpi, 112.0 * dpi, 14.0 * dpi);
        let total_y0 = 20.0 * dpi;
        for c in Category::ALL {
            if cat.map(|x| x != c).unwrap_or(false) {
                continue;
            }
            let ks: Vec<Kind> = list.iter().copied().filter(|k| k.category() == c).collect();
            if ks.is_empty() {
                continue;
            }
            if cy + 24.0 * dpi > 0.0 && cy < gh - 60.0 * dpi {
                self.text(&content, prefs::tr(c.title()), (13.0 * dpi) as i32, 600, WHITE, cx0, cy, 3.0 * cw, 0.85);
            }
            cy += 28.0 * dpi;
            for (i, k) in ks.iter().enumerate() {
                let (col, row) = (i % 3, i / 3);
                let r: Rect = (cx0 + col as f32 * (cw + cg), cy + row as f32 * (ch + cg), cw, ch);
                if r.1 + r.3 < 0.0 || r.1 > gh - 60.0 * dpi {
                    continue;
                }
                let present = self.layout.contains(k);
                let Ok(card) = self.comp.CreateContainerVisual() else { continue };
                let _ = content.Children().and_then(|k| k.InsertAtTop(&card));
                conts.push((r, card.clone(), 1.06));
                let content = card;
                let _ = self.rrect(&content, r, 20.0 * dpi, Color { A: if present { 30 } else { 85 }, R: 255, G: 255, B: 255 });
                let disc: Rect = (r.0 + r.2 / 2.0 - 22.0 * dpi, r.1 + 18.0 * dpi, 44.0 * dpi, 44.0 * dpi);
                let _ = self.rrect(&content, disc, 22.0 * dpi, Color { A: if present { 60 } else { 150 }, R: 0, G: 0, B: 0 });
                self.glyph(&content, glyph_of(*k), (20.0 * dpi) as i32, WHITE, disc.0 + 22.0 * dpi, disc.1 + 22.0 * dpi, if present { 0.5 } else { 1.0 });
                self.text(&content, prefs::tr(k.title()), (12.0 * dpi) as i32, 500, WHITE, r.0 + 10.0 * dpi, r.1 + r.3 - 30.0 * dpi, r.2 - 20.0 * dpi, if present { 0.5 } else { 1.0 });
                cards.push((*k, r));
            }
            cy += ks.len().div_ceil(3) as f32 * (ch + cg);
            if r_sep_visible(cy, gh, dpi) {
                let _ = self.rrect(&content, (cx0, cy - cg / 2.0, 3.0 * cw + 2.0 * cg, 1.0), 0.5, Color { A: 40, R: 255, G: 255, B: 255 });
            }
            cy += 14.0 * dpi;
        }
        let content_h = cy + scroll - total_y0;
        if let Some(g) = self.gallery.as_mut() {
            g.content_h = content_h;
        }
        self.text(&content, prefs::tr("Klik kontrol untuk menambahkannya ke Quick Panel."), (12.0 * dpi) as i32, 400, WHITE, side + 36.0 * dpi, gh - 34.0 * dpi, gw - side - 180.0 * dpi, 0.55);
        let done: Rect = (gw - 100.0 * dpi, gh - 46.0 * dpi, 84.0 * dpi, 32.0 * dpi);
        if let Ok(dc) = self.comp.CreateContainerVisual() {
            let _ = content.Children().and_then(|k| k.InsertAtTop(&dc));
            let _ = self.rrect(&dc, done, 16.0 * dpi, prefs::accent_c(255));
            self.text(&dc, prefs::tr("Selesai"), (13.0 * dpi) as i32, 600, WHITE, done.0 + 18.0 * dpi, done.1 + 7.0 * dpi, 70.0 * dpi, 1.0);
            conts.push((done, dc, motion::HOVER_BUTTON));
        }
        if let Some(g) = self.gallery.as_mut() {
            g.cards = cards;
            g.cats = cats;
            g.done = done;
            g.conts = conts;
            g.hover = None;
        }
    }

    fn update_values(&mut self) {
        let Some(p) = self.panel.as_ref() else { return };
        for m in &p.mods {
            if let (Some(t), Some(f)) = (m.track, &m.fill) {
                let frac = if m.placed.kind == Kind::Display { brightness::latest().map(|v| v as f32 / 100.0).unwrap_or(0.5) } else { audio::latest().level };
                if m.vertical {
                    place_slider_v(f, &m.thumb, t, frac, p.dpi);
                } else {
                    place_slider(f, &m.thumb, t, frac, p.dpi);
                }
            }
        }
    }
}

fn slider_value(t: Rect, vertical: bool, x: f32, y: f32, dpi: f32) -> f32 {
    use nacre_core::hud::slider::value_at;
    if vertical {
        let kh = KNOB.0 * dpi;
        value_at(y, t.1 + kh / 2.0, (t.3 - kh).max(1.0), true)
    } else {
        let kw = KNOB.0 * dpi;
        value_at(x, t.0 + kw / 2.0, (t.2 - kw).max(1.0), false)
    }
}

fn place_slider(fill: &SpriteVisual, thumb: &[SpriteVisual], t: Rect, frac: f32, dpi: f32) {
    let th = t.3 - 20.0 * dpi;
    let (kw, kh) = (KNOB.0 * dpi, KNOB.1 * dpi);
    let kx = nacre_core::hud::slider::pos_of(frac, t.0 + kw / 2.0, (t.2 - kw).max(1.0), false);
    let cy = t.1 + t.3 / 2.0;
    let _ = fill.SetSize(Vector2 { X: (kx - t.0).max(th), Y: th });
    for (j, s) in thumb.iter().enumerate() {
        let inset = [-1.0, 0.0, 1.5].get(j).copied().unwrap_or(0.0) * dpi;
        let _ = s.SetOffset(Vector3 { X: kx - kw / 2.0 + inset, Y: cy - kh / 2.0 + inset, Z: 0.0 });
    }
}

fn place_slider_v(fill: &SpriteVisual, thumb: &[SpriteVisual], t: Rect, frac: f32, dpi: f32) {
    let tw = t.2 - 28.0 * dpi;
    let (kw, kh) = (KNOB.1 * dpi, KNOB.0 * dpi);
    let ky = nacre_core::hud::slider::pos_of(frac, t.1 + kh / 2.0, (t.3 - kh).max(1.0), true);
    let cx = t.0 + t.2 / 2.0;
    let _ = fill.SetOffset(Vector3 { X: cx - tw / 2.0, Y: ky, Z: 0.0 });
    let _ = fill.SetSize(Vector2 { X: tw, Y: (t.1 + t.3 - ky).max(tw) });
    for (j, s) in thumb.iter().enumerate() {
        let inset = [-1.0, 0.0, 1.5].get(j).copied().unwrap_or(0.0) * dpi;
        let _ = s.SetOffset(Vector3 { X: cx - kw / 2.0 + inset, Y: ky - kh / 2.0 + inset, Z: 0.0 });
    }
}

const KNOB: (f32, f32) = (42.0, 28.0);
const CARD_R: f32 = 0.34;

const DROP_IDLE: [Color; 3] = [Color { A: 45, R: 0, G: 0, B: 0 }, Color { A: 250, R: 255, G: 255, B: 255 }, Color { A: 0, R: 255, G: 255, B: 255 }];
const DROP_ACTIVE: [Color; 3] = [Color { A: 0, R: 0, G: 0, B: 0 }, Color { A: 125, R: 255, G: 255, B: 255 }, Color { A: 40, R: 255, G: 255, B: 255 }];

fn raindrop(comp: &Compositor, thumb: &[SpriteVisual], active: bool) {
    let (cols, from, to) = if active { (DROP_ACTIVE, 1.0, 1.22) } else { (DROP_IDLE, 1.22, 1.0) };
    for (j, v) in thumb.iter().enumerate() {
        if let Ok(b) = v.Brush() {
            let _ = motion::color(comp, &b, cols[j.min(2)], 150);
        }
        if let Ok(sz) = v.Size() {
            let _ = v.SetCenterPoint(Vector3 { X: sz.X / 2.0, Y: sz.Y / 2.0, Z: 0.0 });
        }
        let _ = motion::vec3(comp, v, "Scale", Vector3 { X: from, Y: from, Z: 1.0 }, Vector3 { X: to, Y: to, Z: 1.0 }, 170);
    }
}

unsafe extern "system" fn ctl_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_RADIO => {
            with(|c| {
                if let Some(r) = &c.radios {
                    while let Ok(s) = r.snapshots.try_recv() {
                        c.wifi = s.wifi;
                        c.bt = s.bluetooth;
                    }
                }
                c.ssid = wifi::ssid();
                if c.panel.is_some() && !c.edit {
                    c.build();
                }
            });
            LRESULT(0)
        }
        WM_MEDIA => {
            with(|c| {
                let mut changed = false;
                if let Some(m) = &c.media {
                    while let Ok(s) = m.snapshots.try_recv() {
                        c.track = Some((s.title, s.artist, s.has_media, s.playing));
                        match s.art {
                            ArtUpdate::New(b) => c.art = Some(b),
                            ArtUpdate::None => c.art = None,
                            ArtUpdate::Unchanged => {}
                        }
                        changed = true;
                    }
                }
                if changed && c.panel.is_some() && !c.edit {
                    c.build();
                }
            });
            LRESULT(0)
        }
        WM_VALUES => {
            with(|c| c.update_values());
            LRESULT(0)
        }
        WM_TIMER if wp.0 == T_CLOSE => {
            with(|c| c.finish_close());
            LRESULT(0)
        }
        WM_TIMER if wp.0 == T_REMOVE => {
            with(|c| c.finish_remove());
            LRESULT(0)
        }
        WM_TIMER if wp.0 == T_INTRO => {
            with(|c| c.start_intro());
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

fn xy(lp: LPARAM) -> (f32, f32) {
    ((lp.0 as i16) as f32, ((lp.0 >> 16) as i16) as f32)
}

unsafe extern "system" fn panel_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        m if m == WM_CLOSE_CC => {
            close();
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let (x, y) = pxy(lp);
            with(|c| {
                c.pick_up = None;
                if c.closing {
                    return;
                }
                let Some(p) = c.panel.as_ref() else { return };
                if hit(p.edit_btn, x, y) {
                    if c.edit { c.leave_edit() } else { c.enter_edit() }
                    return;
                }
                if let Some((_, cam)) = p.chip.filter(|(r, _)| hit(*r, x, y)) {
                    c.close();
                    launch::open(if cam { "ms-settings:privacy-webcam" } else { "ms-settings:privacy-microphone" });
                    return;
                }
                let edit = c.edit;
                let Some(i) = p.mods.iter().position(|m| hit(m.rect, x, y) || (edit && (hit(m.badge, x, y) || hit(m.grip, x, y)))) else { return };
                let m = &p.mods[i];
                if edit {
                    if hit(m.badge, x, y) {
                        let k = m.placed.kind;
                        c.remove(k);
                    } else if hit(m.grip, x, y) && c.pending_remove.is_none() {
                        c.start_resize(i);
                        SetCapture(hwnd);
                    } else if c.pending_remove.is_none() {
                        c.start_drag(i, x, y);
                        SetCapture(hwnd);
                    }
                } else if m.track.map(|t| hit(t, x, y)).unwrap_or(false) {
                    if let Some(p) = c.panel.as_mut() {
                        p.slider = Some(i);
                        raindrop(&c.comp, &p.mods[i].thumb, true);
                    }
                    SetCapture(hwnd);
                    c.set_slider(i, x, y, true);
                } else {
                    c.click(i, x, y);
                }
            });
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let (x, y) = pxy(lp);
            with(|c| {
                let slider = c.panel.as_ref().and_then(|p| p.slider);
                if let Some(i) = slider {
                    c.set_slider(i, x, y, true);
                    return;
                }
                if c.panel.as_ref().is_some_and(|p| p.resize.is_some()) {
                    c.resize_move(x, y);
                    return;
                }
                if c.panel.as_ref().is_some_and(|p| p.drag.is_some()) {
                    c.drag_move(x, y);
                    return;
                }
                let mut t = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
                let _ = TrackMouseEvent(&mut t);
                let hit = c.panel.as_ref().filter(|p| p.drag.is_none() && !c.edit && !c.closing).and_then(|p| p.mods.iter().position(|m| hit(m.rect, x, y)));
                c.set_hover(hit);
            });
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            with(|c| c.set_hover(None));
            LRESULT(0)
        }
        m if m == MENU_PICK => {
            let id = wp.0;
            with(|c| c.pick_size(id));
            LRESULT(0)
        }
        m if m == MENU_CLOSE => {
            with(|c| {
                if let Some(mn) = &c.menu {
                    mn.hide();
                }
            });
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            let (x, y) = pxy(lp);
            with(|c| {
                if c.closing {
                    return;
                }
                let Some(p) = c.panel.as_ref() else { return };
                let Some(i) = p.mods.iter().position(|m| hit(m.rect, x, y)) else { return };
                if c.edit {
                    c.open_size_menu(i, x, y);
                } else if matches!(p.mods[i].placed.kind, Kind::Wifi | Kind::Bluetooth) {
                    c.open_picker(i);
                }
            });
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let (x, y) = pxy(lp);
            with(|c| {
                let (slider, drag) = match c.panel.as_mut() {
                    Some(p) => (p.slider.take(), p.drag),
                    None => (None, None),
                };
                if let (Some(i), Some(p)) = (slider, c.panel.as_ref()) {
                    if let Some(m) = p.mods.get(i) {
                        raindrop(&c.comp, &m.thumb, false);
                    }
                }
                if slider.is_some() || drag.is_some() {
                    let _ = ReleaseCapture();
                }
                if drag.is_some() {
                    let _ = (x, y);
                    c.end_drag();
                }
                if c.panel.as_ref().is_some_and(|p| p.resize.is_some()) {
                    let _ = ReleaseCapture();
                    c.end_resize();
                }
                if let Some(i) = c.pick_up.take() {
                    if !c.closing && c.panel.as_ref().and_then(|p| p.mods.get(i)).is_some_and(|m| hit(m.rect, x, y)) {
                        c.open_picker(i);
                    }
                }
            });
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe extern "system" fn gallery_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEMOVE => {
            let (x, y) = xy(lp);
            with(|c| {
                let Some(g) = c.gallery.as_mut() else { return };
                if !g.tracking {
                    let mut t = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
                    g.tracking = TrackMouseEvent(&mut t).is_ok();
                }
                let i = g.conts.iter().position(|(r, _, _)| hit(*r, x, y));
                c.set_gallery_hover(i);
            });
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            with(|c| {
                if let Some(g) = c.gallery.as_mut() {
                    g.tracking = false;
                }
                c.set_gallery_hover(None);
            });
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let (x, y) = xy(lp);
            with(|c| {
                let Some(g) = c.gallery.as_ref() else { return };
                if hit(g.done, x, y) {
                    c.leave_edit();
                    return;
                }
                if let Some(cat) = g.cats.iter().find(|(_, r)| hit(*r, x, y)).map(|(c, _)| *c) {
                    if let Some(g) = c.gallery.as_mut() {
                        g.cat = cat;
                        g.scroll = 0.0;
                    }
                    c.refresh_gallery();
                    return;
                }
                if let Some(k) = g.cards.iter().find(|(_, r)| hit(*r, x, y)).map(|(k, _)| *k) {
                    c.add(k);
                }
            });
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = ((wp.0 >> 16) as i16) as f32;
            with(|c| {
                if let Some(g) = c.gallery.as_mut() {
                    let max = (g.content_h - (g.popup.rect.3 as f32 - 80.0 * g.dpi)).max(0.0);
                    g.scroll = (g.scroll - delta / 120.0 * 60.0 * g.dpi).clamp(0.0, max);
                }
                c.refresh_gallery();
            });
            LRESULT(0)
        }
        WM_CHAR => {
            let ch = char::from_u32(wp.0 as u32).unwrap_or('\0');
            if !ch.is_control() {
                with(|c| {
                    if let Some(g) = c.gallery.as_mut() {
                        g.query.push(ch);
                        g.scroll = 0.0;
                    }
                    c.refresh_gallery();
                });
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if wp.0 as u16 == VK_BACK.0 {
                with(|c| {
                    if let Some(g) = c.gallery.as_mut() {
                        g.query.pop();
                    }
                    c.refresh_gallery();
                });
            } else if wp.0 as u16 == VK_ESCAPE.0 {
                with(|c| c.leave_edit());
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
