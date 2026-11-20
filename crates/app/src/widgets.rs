use crate::dock::menu::{Item as MItem, Menu, MENU_CLOSE, MENU_PICK};
use crate::popup::Popup;
use crate::{motion, paths, prefs};
use nacre_core::island::calendar;
use nacre_core::widgets::{self, Category, Kind, Placed};
use nacre_platform::text::{measure, render, render_face, Style, ICON_FACE};
use nacre_platform::{launch, log, sys, time};
use nacre_ui::gfx::Gfx;
use nacre_ui::glass::{GlassParams, Tier};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use windows::core::w;
use windows::Foundation::TimeSpan;
use windows::UI::Color;
use windows::UI::Composition::{CompositionDrawingSurface, CompositionObject, CompositionStretch, Compositor, ContainerVisual, SpriteVisual};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT, VK_BACK, VK_ESCAPE};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows_core::{Interface, HSTRING};
use windows_numerics::{Vector2, Vector3};

const WM_MOUSELEAVE: u32 = 0x02A3;
const T_TICK: usize = 1;
const T_REMOVE: usize = 2;
const T_GCLOSE: usize = 3;
const T_SNAP: usize = 4;
const SNAP_MS: f32 = 220.0;
const CMD_EDIT: usize = 1;
const CMD_REMOVE: usize = 2;
const MARGIN: f32 = 26.0;
const RADIUS: f32 = 32.0;
const PREVIEW: f32 = 0.6;
const RED: [u8; 3] = [255, 59, 48];
const ORANGE: [u8; 3] = [255, 149, 0];
const GREEN: [u8; 3] = [52, 199, 89];
const BLUE: [u8; 3] = [10, 132, 255];

static FLOAT: AtomicBool = AtomicBool::new(false);

type Rect = (f32, f32, f32, f32);

struct Hand {
    v: SpriteVisual,
    which: u8,
    offset: Option<i32>,
}

struct Widget {
    kind: Kind,
    popup: Popup,
    pos: (i32, i32),
    size: (f32, f32),
    hands: Vec<Hand>,
    sig: String,
    hover: bool,
    tracking: bool,
}

struct Drag {
    idx: usize,
    grab: (i32, i32),
    start: (i32, i32),
    moved: bool,
}

struct Gallery {
    popup: Popup,
    cat: Option<Category>,
    query: String,
    cards: Vec<(Kind, Rect)>,
    cats: Vec<(Option<Category>, Rect)>,
    done: Rect,
    conts: Vec<(Rect, ContainerVisual, f32)>,
    hover: Option<usize>,
    tracking: bool,
    press: Option<(Kind, (i32, i32))>,
    closing: bool,
}

struct Mgr {
    ctl: HWND,
    comp: Compositor,
    gfx: Option<Gfx>,
    dpi: f32,
    list: Vec<Widget>,
    cache: HashMap<String, (CompositionDrawingSurface, i32, i32)>,
    edit: bool,
    gallery: Option<Gallery>,
    drag: Option<Drag>,
    menu: Option<Menu>,
    menu_for: Option<usize>,
    cpu_prev: Option<(u64, u64)>,
    cpu: u8,
    ram: u8,
    ticks: u32,
    pending_remove: Option<HWND>,
    slot: Option<Popup>,
    slot_at: Option<(i32, i32, i32, i32)>,
    snap: Option<(HWND, (i32, i32), (i32, i32), std::time::Instant)>,
}

thread_local! {
    static MGR: RefCell<Option<Mgr>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut Mgr) -> R) -> Option<R> {
    MGR.with(|c| c.try_borrow_mut().ok().and_then(|mut b| b.as_mut().map(f)))
}

fn list_file() -> std::path::PathBuf {
    paths::config_dir().join("widgets.txt")
}

fn col(rgb: [u8; 3], a: u8) -> Color {
    Color { A: a, R: rgb[0], G: rgb[1], B: rgb[2] }
}

fn recolor(b: &nacre_platform::icons::Bitmap, rgb: [u8; 3]) -> Vec<u8> {
    let mut out = b.bgra.clone();
    for px in out.chunks_exact_mut(4) {
        let a = px[3] as u32;
        px[0] = (rgb[2] as u32 * a / 255) as u8;
        px[1] = (rgb[1] as u32 * a / 255) as u8;
        px[2] = (rgb[0] as u32 * a / 255) as u8;
    }
    out
}

fn cursor() -> (i32, i32) {
    let mut p = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut p);
    }
    (p.x, p.y)
}

fn hit(r: Rect, x: f32, y: f32) -> bool {
    x >= r.0 && x < r.0 + r.2 && y >= r.1 && y < r.1 + r.3
}

fn lang() -> nacre_core::i18n::Lang {
    prefs::get().lang
}

fn month_label(m: u32) -> String {
    nacre_core::i18n::month_name(lang(), m, calendar::month_name(m)).to_string()
}

fn weekday_label(wd: u32) -> String {
    const ID: [&str; 7] = ["Minggu", "Senin", "Selasa", "Rabu", "Kamis", "Jumat", "Sabtu"];
    const EN: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
    const ES: [&str; 7] = ["Domingo", "Lunes", "Martes", "Miércoles", "Jueves", "Viernes", "Sábado"];
    match lang() {
        nacre_core::i18n::Lang::Id => ID[(wd % 7) as usize],
        nacre_core::i18n::Lang::En => EN[(wd % 7) as usize],
        nacre_core::i18n::Lang::Es => ES[(wd % 7) as usize],
    }
    .to_string()
}

fn hhmm(h: u32, m: u32) -> String {
    if prefs::get().clock_24h {
        let separator = if lang() == nacre_core::i18n::Lang::Id { '.' } else { ':' };
        format!("{h:02}{separator}{m:02}")
    } else {
        format!("{}:{m:02}", if h % 12 == 0 { 12 } else { h % 12 })
    }
}

fn utc_minutes() -> i32 {
    let n = time::local_now();
    (n.hour * 60 + n.minute) as i32 - sys::utc_offset_min()
}

fn area(dpi: f32) -> widgets::Rect {
    let (sw, sh) = sys::screen_size();
    let top = (44.0 * dpi) as i32;
    let side = (20.0 * dpi) as i32;
    (side, top, sw - 2 * side, sh - top - (120.0 * dpi) as i32)
}

fn style() -> u8 {
    prefs::get().widget_style
}

fn palette(kind: Kind) -> (Option<Color>, [u8; 3]) {
    if style() == 1 {
        return (None, [255, 255, 255]);
    }
    let dark = prefs::look(prefs::Role::Themed).dark;
    if dark || kind.dark_card() {
        (Some(Color { A: 240, R: 28, G: 28, B: 30 }), [255, 255, 255])
    } else if kind.frosted_card() {
        (Some(Color { A: 205, R: 246, G: 246, B: 248 }), [22, 22, 24])
    } else {
        (Some(Color { A: 248, R: 255, G: 255, B: 255 }), [22, 22, 24])
    }
}

pub fn init(comp: &Compositor) -> windows_core::Result<()> {
    unsafe {
        let hi: HINSTANCE = GetModuleHandleW(None)?.into();
        let class = w!("AbraWidgetsCtl");
        RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(ctl_proc), hInstance: hi, lpszClassName: class, ..Default::default() });
        let ctl = CreateWindowExW(WINDOW_EX_STYLE(0), class, w!("abra-widgets"), WS_POPUP, 0, 0, 0, 0, Some(HWND_MESSAGE), None, Some(hi), None)?;
        let dpi = windows::Win32::UI::HiDpi::GetDpiForSystem().max(96) as f32 / 96.0;
        MGR.with(|c| {
            *c.borrow_mut() = Some(Mgr {
                ctl,
                comp: comp.clone(),
                gfx: Gfx::shared(comp).ok(),
                dpi,
                list: Vec::new(),
                cache: HashMap::new(),
                edit: false,
                gallery: None,
                drag: None,
                menu: None,
                menu_for: None,
                cpu_prev: sys::cpu_times(),
                cpu: 0,
                ram: sys::memory_load().unwrap_or(0),
                ticks: 0,
                pending_remove: None,
                slot: None,
                slot_at: None,
                snap: None,
            })
        });
        prefs::listen(ctl);
        SetTimer(Some(ctl), T_TICK, 1000, None);
    }
    with(|m| m.load());
    Ok(())
}

pub fn open_gallery() {
    with(|m| m.open_gallery());
}

impl Mgr {
    fn pitch(&self) -> i32 {
        ((widgets::CELL + widgets::GAP) * self.dpi).round() as i32
    }

    fn grid(&self) -> ((i32, i32), i32, (i32, i32)) {
        let ar = area(self.dpi);
        let step = self.pitch();
        let gap = (widgets::GAP * self.dpi).round() as i32;
        ((ar.0, ar.1), step, (((ar.2 + gap) / step).max(1), ((ar.3 + gap) / step).max(1)))
    }

    fn cell_of(&self, pos: (i32, i32)) -> (i32, i32) {
        let (o, step, _) = self.grid();
        (((pos.0 - o.0) as f32 / step as f32).round() as i32, ((pos.1 - o.1) as f32 / step as f32).round() as i32)
    }

    fn cell_px(&self, cell: (i32, i32)) -> (i32, i32) {
        let (o, step, _) = self.grid();
        (o.0 + cell.0 * step, o.1 + cell.1 * step)
    }

    fn cells_taken(&self, skip: Option<usize>) -> Vec<widgets::Rect> {
        self.list
            .iter()
            .enumerate()
            .filter(|(i, _)| Some(*i) != skip)
            .map(|(_, w)| {
                let c = self.cell_of(w.pos);
                let (cw, ch) = w.kind.cells();
                (c.0, c.1, cw as i32, ch as i32)
            })
            .collect()
    }

    fn slot_kind(&self, skip: Option<usize>, kind: Kind, pos: (i32, i32)) -> (i32, i32) {
        let (_, _, grid) = self.grid();
        let (cw, ch) = kind.cells();
        let cell = widgets::nearest_cell(&self.cells_taken(skip), (cw as i32, ch as i32), self.cell_of(pos), grid).unwrap_or_else(|| self.cell_of(pos));
        self.cell_px(cell)
    }

    fn slot_for(&self, skip: Option<usize>, _size: (i32, i32), pos: (i32, i32)) -> (i32, i32) {
        let kind = skip.and_then(|i| self.list.get(i)).map(|w| w.kind).unwrap_or(Kind::Clock);
        self.slot_kind(skip, kind, pos)
    }

    fn load(&mut self) {
        let placed = widgets::from_text(&std::fs::read_to_string(list_file()).unwrap_or_default());
        let mut moved = false;
        for p in placed {
            let at = self.slot_kind(None, p.kind, (p.x, p.y));
            moved |= at != (p.x, p.y);
            self.spawn(p.kind, at, false);
        }
        if moved {
            self.save();
        }
        log::line("widgets", &format!("{} widget dimuat", self.list.len()));
    }

    fn save(&self) {
        let list: Vec<Placed> = self.list.iter().map(|w| Placed { kind: w.kind, x: w.pos.0, y: w.pos.1 }).collect();
        let _ = std::fs::create_dir_all(paths::config_dir());
        let _ = std::fs::write(list_file(), widgets::to_text(&list));
    }

    fn rebuild_all(&mut self) {
        let placed: Vec<(Kind, (i32, i32))> = self.list.iter().map(|w| (w.kind, w.pos)).collect();
        for w in self.list.drain(..) {
            unsafe {
                let _ = DestroyWindow(w.popup.hwnd);
            }
        }
        self.cache.clear();
        for (k, p) in placed {
            self.spawn(k, p, false);
        }
    }

    fn spawn(&mut self, kind: Kind, pos: (i32, i32), pop: bool) -> Option<usize> {
        let d = self.dpi;
        let (w, h) = widgets::size_of(kind, d);
        let m = MARGIN * d;
        let base = GlassParams::dark((w, h), RADIUS * d);
        let glass = if style() == 1 { prefs::glass((w, h), RADIUS * d, prefs::Role::Dock) } else { base };
        let mut popup = Popup::new(&self.comp, w!("AbraWidget"), Some(widget_proc), WINDOW_EX_STYLE(0), glass).ok()?;
        let _ = popup.set_rect(pos.0 - m as i32, pos.1 - m as i32, (w + 2.0 * m) as i32, (h + 2.0 * m) as i32);
        if style() == 1 {
            let c = sys::conditions();
            let _ = popup.panel.set_tier(if !c.transparency || c.battery_saver { Tier::Solid } else { Tier::Full });
            let _ = popup.panel.set_rect(m, m, w, h);
            let _ = popup.panel.set_radius(RADIUS * d);
        } else {
            let _ = popup.panel.set_rect(0.0, 0.0, 0.0, 0.0);
        }
        let _ = popup.content.SetOffset(Vector3 { X: m, Y: m, Z: 0.0 });
        unsafe {
            let _ = SetWindowPos(popup.hwnd, Some(HWND_BOTTOM), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
        popup.show();
        self.list.push(Widget { kind, popup, pos, size: (w, h), hands: vec![], sig: String::new(), hover: false, tracking: false });
        let i = self.list.len() - 1;
        self.paint(i);
        if pop {
            let wd = &self.list[i];
            let _ = motion::pop_in(&self.comp, wd.popup.root(), self.center(i), 0.4, 1.06, 0, 380);
        }
        Some(i)
    }

    fn center(&self, i: usize) -> Vector3 {
        let w = &self.list[i];
        let m = MARGIN * self.dpi;
        Vector3 { X: m + w.size.0 / 2.0, Y: m + w.size.1 / 2.0, Z: 0.0 }
    }

    fn sig_for(&self, kind: Kind) -> String {
        let n = time::local_now();
        match kind {
            Kind::Clock => format!("{}", n.day),
            Kind::WorldClock => format!("{}{}", n.day, n.hour),
            Kind::Calendar | Kind::Month => format!("{}-{}-{}", n.year, n.month, n.day),
            Kind::Digital => format!("{}{}{}", n.day, n.hour, n.minute),
            Kind::Battery => format!("{:?}", sys::battery()),
            Kind::System => format!("{}-{}", self.cpu, self.ram),
        }
    }

    fn paint(&mut self, i: usize) {
        let kind = self.list[i].kind;
        let (w, h) = self.list[i].size;
        let content = self.list[i].popup.content.clone();
        if let Ok(k) = content.Children() {
            let _ = k.RemoveAll();
        }
        let hands = self.draw(&content, kind, w, h, false);
        if self.edit {
            self.badge(&content);
        }
        let sig = self.sig_for(kind);
        let wd = &mut self.list[i];
        wd.hands = hands;
        wd.sig = sig;
        self.turn_hands(i, false);
    }

    fn badge(&self, c: &ContainerVisual) {
        let d = self.dpi;
        let s = 22.0 * d;
        let _ = self.rr(c, (-7.0 * d, -7.0 * d, s, s), s / 2.0, Color { A: 240, R: 72, G: 72, B: 76 });
        let _ = self.rr(c, (-7.0 * d + 6.0 * d, -7.0 * d + s / 2.0 - 1.0 * d, s - 12.0 * d, 2.0 * d), d, Color { A: 255, R: 255, G: 255, B: 255 });
    }

    fn badge_hit(&self, i: usize, x: f32, y: f32) -> bool {
        let d = self.dpi;
        let m = MARGIN * d;
        self.edit && hit((m - 9.0 * d, m - 9.0 * d, 26.0 * d, 26.0 * d), x, y) && i < self.list.len()
    }

    fn text_surf(&mut self, s: &str, px: i32, weight: i32, rgb: [u8; 3]) -> Option<(CompositionDrawingSurface, i32, i32)> {
        let key = format!("{px}|{weight}|{rgb:?}|{s}");
        if let Some(v) = self.cache.get(&key) {
            return Some(v.clone());
        }
        let st = Style { px, weight };
        let (tw, th) = measure(s, &st);
        let bmp = render(s, &st, tw + 2, th)?;
        let surf = self.gfx.as_ref()?.surface_from_bgra(bmp.w, bmp.h, &recolor(&bmp, rgb)).ok()?;
        if self.cache.len() > 500 {
            self.cache.clear();
        }
        let v = (surf, bmp.w as i32, bmp.h as i32);
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
        let surf = self.gfx.as_ref()?.surface_from_bgra(bmp.w, bmp.h, &recolor(&bmp, rgb)).ok()?;
        let v = (surf, bmp.w as i32, bmp.h as i32);
        self.cache.insert(key, v.clone());
        Some(v)
    }

    fn put(&self, p: &ContainerVisual, s: &CompositionDrawingSurface, x: f32, y: f32, w: f32, h: f32, op: f32) -> Option<SpriteVisual> {
        let go = || -> windows_core::Result<SpriteVisual> {
            let b = self.comp.CreateSurfaceBrushWithSurface(s)?;
            b.SetStretch(CompositionStretch::None)?;
            let v = self.comp.CreateSpriteVisual()?;
            v.SetBrush(&b)?;
            v.SetSize(Vector2 { X: w, Y: h })?;
            v.SetOffset(Vector3 { X: x.round(), Y: y.round(), Z: 0.0 })?;
            v.SetOpacity(op)?;
            p.Children()?.InsertAtTop(&v)?;
            Ok(v)
        };
        go().ok()
    }

    #[allow(clippy::too_many_arguments)]
    fn text(&mut self, p: &ContainerVisual, s: &str, px: f32, weight: i32, rgb: [u8; 3], x: f32, y: f32, anchor: u8, op: f32) -> f32 {
        let Some((surf, w, h)) = self.text_surf(s, px.round() as i32, weight, rgb) else { return 0.0 };
        let x = match anchor {
            1 => x - w as f32 / 2.0,
            2 => x - w as f32,
            _ => x,
        };
        self.put(p, &surf, x, y, w as f32, h as f32, op);
        w as f32
    }

    fn glyph(&mut self, p: &ContainerVisual, g: &str, px: f32, rgb: [u8; 3], cx: f32, cy: f32, op: f32) {
        let Some((surf, w, h)) = self.glyph_surf(g, px.round() as i32, rgb) else { return };
        self.put(p, &surf, cx - w as f32 / 2.0, cy - h as f32 / 2.0, w as f32, h as f32, op);
    }

    fn rr(&self, p: &ContainerVisual, r: Rect, radius: f32, c: Color) -> Option<SpriteVisual> {
        let go = || -> windows_core::Result<SpriteVisual> {
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
            p.Children()?.InsertAtTop(&v)?;
            Ok(v)
        };
        go().ok()
    }

    fn shadow(&self, p: &ContainerVisual, w: f32, h: f32, radius: f32, op: f32) {
        let d = self.dpi;
        let s = nacre_core::notch::shade::rounded(radius, 16.0 * d, 5.0 * d);
        let Some(gfx) = self.gfx.as_ref() else { return };
        let go = || -> windows_core::Result<()> {
            let surf = gfx.surface_from_bgra(s.w, s.h, &s.bgra)?;
            let src = self.comp.CreateSurfaceBrushWithSurface(&surf)?;
            src.SetStretch(CompositionStretch::Fill)?;
            let ng = self.comp.CreateNineGridBrush()?;
            ng.SetSource(&src)?;
            let (l, t, r, b) = s.insets;
            ng.SetInsetsWithValues(l, t, r, b)?;
            let v = self.comp.CreateSpriteVisual()?;
            v.SetBrush(&ng)?;
            let mg = s.margin as f32;
            v.SetOffset(Vector3 { X: -mg, Y: -mg, Z: 0.0 })?;
            v.SetSize(Vector2 { X: w + 2.0 * mg, Y: h + 2.0 * mg + s.dy as f32 })?;
            v.SetOpacity(op)?;
            p.Children()?.InsertAtTop(&v)?;
            Ok(())
        };
        let _ = go();
    }

    fn draw(&mut self, p: &ContainerVisual, kind: Kind, w: f32, h: f32, preview: bool) -> Vec<Hand> {
        let d = self.dpi;
        let radius = RADIUS * d;
        let (card, fg) = palette(kind);
        match card {
            Some(c) => {
                self.shadow(p, w, h, radius, if preview { 0.22 } else { 0.34 });
                let _ = self.rr(p, (0.0, 0.0, w, h), radius, c);
            }
            None if preview => {
                let _ = self.rr(p, (0.0, 0.0, w, h), radius, Color { A: 70, R: 255, G: 255, B: 255 });
            }
            None => {}
        }
        let mut hands = Vec::new();
        let n = time::local_now();
        match kind {
            Kind::Clock => {
                let r = (w.min(h) - 24.0 * d) / 2.0;
                self.face(p, w / 2.0, h / 2.0, r, None, true, &mut hands);
            }
            Kind::WorldClock => {
                let pad = 12.0 * d;
                let r = ((w - 2.0 * pad - 3.0 * 10.0 * d) / 8.0).min((h - 54.0 * d) / 2.0);
                let slot = (w - 2.0 * pad) / 4.0;
                let utc = utc_minutes();
                let local = sys::utc_offset_min();
                for (k, city) in widgets::CITIES.iter().enumerate() {
                    let cx = pad + slot * (k as f32 + 0.5);
                    let cy = 14.0 * d + r;
                    self.face(p, cx, cy, r, Some(city.offset_min), false, &mut hands);
                    let (_, _, day, diff) = widgets::city_time(utc, local, city.offset_min);
                    let when = match day {
                        0 => prefs::tr("Hari ini"),
                        1 => prefs::tr("Besok"),
                        _ => prefs::tr("Kemarin"),
                    };
                    self.text(p, city.name, 11.5 * d, 600, fg, cx, cy + r + 7.0 * d, 1, 1.0);
                    let rel = if diff == 0 { when.to_string() } else { format!("{when} {diff:+}{}", prefs::tr("JAM")) };
                    self.text(p, &rel, 9.5 * d, 500, fg, cx, cy + r + 23.0 * d, 1, 0.6);
                }
            }
            Kind::Calendar => {
                let wd = calendar::weekday(n.year, n.month, n.day);
                self.text(p, &weekday_label(wd).to_uppercase(), 12.5 * d, 700, RED, 16.0 * d, 14.0 * d, 0, 1.0);
                self.text(p, &n.day.to_string(), 52.0 * d, 300, fg, 13.0 * d, 30.0 * d, 0, 1.0);
                self.text(p, &format!("{} {}", month_label(n.month), n.year), 12.5 * d, 500, fg, 16.0 * d, h - 32.0 * d, 0, 0.6);
            }
            Kind::Month => {
                let monday_first = lang() != nacre_core::i18n::Lang::Id;
                let g = calendar::grid(n.year, n.month, monday_first);
                self.text(p, &month_label(n.month).to_uppercase(), 11.0 * d, 700, RED, 14.0 * d, 11.0 * d, 0, 1.0);
                let labels = calendar::weekday_labels(monday_first);
                let (gx, gy) = (10.0 * d, 30.0 * d);
                let cw = (w - 2.0 * gx) / 7.0;
                let rows = g.rows.max(5) as f32;
                let ch = (h - gy - 8.0 * d) / (rows + 1.0);
                for (k, l) in labels.iter().enumerate() {
                    let s: String = l.chars().take(1).collect();
                    self.text(p, &s, 9.0 * d, 600, fg, gx + cw * (k as f32 + 0.5), gy, 1, 0.5);
                }
                for day in 1..=g.days {
                    let idx = g.first_col + day - 1;
                    let (cx, cy) = (gx + cw * ((idx % 7) as f32 + 0.5), gy + ch * ((idx / 7) as f32 + 1.0));
                    let today = day == n.day;
                    if today {
                        let s = ch.min(cw) * 0.92;
                        let _ = self.rr(p, (cx - s / 2.0, cy - 1.5 * d, s, s), s / 2.0, col(RED, 255));
                    }
                    self.text(p, &day.to_string(), 9.5 * d, if today { 700 } else { 500 }, if today { [255, 255, 255] } else { fg }, cx, cy, 1, 1.0);
                }
            }
            Kind::Digital => {
                let wd = calendar::weekday(n.year, n.month, n.day);
                self.text(p, &hhmm(n.hour, n.minute), 46.0 * d, 600, fg, w / 2.0, h / 2.0 - 40.0 * d, 1, 1.0);
                self.text(p, &weekday_label(wd), 13.0 * d, 600, ORANGE, w / 2.0, h / 2.0 + 18.0 * d, 1, 1.0);
                self.text(p, &format!("{} {}", n.day, month_label(n.month)), 12.5 * d, 500, fg, w / 2.0, h / 2.0 + 36.0 * d, 1, 0.6);
            }
            Kind::Battery => match sys::battery() {
                Some((pct, charging)) => {
                    let n_ring = (58.0 * d).round() as u32;
                    let ring_c = if pct <= 20 && !charging { RED } else { GREEN };
                    let track = if fg[0] > 128 { [255, 255, 255, 46] } else { [0, 0, 0, 30] };
                    let data = widgets::ring(n_ring, 5.5 * d, pct as f32 / 100.0, ring_c, track);
                    if let Some(Ok(s)) = self.gfx.as_ref().map(|g| g.surface_from_bgra(n_ring, n_ring, &data)) {
                        self.put(p, &s, 16.0 * d, 16.0 * d, n_ring as f32, n_ring as f32, 1.0);
                    }
                    let c = 16.0 * d + n_ring as f32 / 2.0;
                    self.glyph(p, if charging { "\u{E83E}" } else { "\u{E7F8}" }, 17.0 * d, fg, c, c, 0.9);
                    self.text(p, &format!("{pct}%"), 34.0 * d, 600, fg, 16.0 * d, h - 52.0 * d, 0, 1.0);
                }
                None => {
                    self.glyph(p, "\u{E83F}", 26.0 * d, fg, w / 2.0, h / 2.0 - 14.0 * d, 0.8);
                    self.text(p, prefs::tr("Tidak ada baterai"), 12.5 * d, 500, fg, w / 2.0, h / 2.0 + 14.0 * d, 1, 0.65);
                }
            },
            Kind::System => {
                self.text(p, prefs::tr("Sistem"), 13.0 * d, 700, fg, 16.0 * d, 14.0 * d, 0, 1.0);
                let rows = [("CPU", self.cpu, BLUE), ("RAM", self.ram, GREEN)];
                for (k, (label, v, c)) in rows.iter().enumerate() {
                    let y = 46.0 * d + k as f32 * 52.0 * d;
                    self.text(p, label, 12.0 * d, 600, fg, 16.0 * d, y, 0, 0.7);
                    self.text(p, &format!("{v}%"), 15.0 * d, 700, fg, w - 16.0 * d, y - 2.0 * d, 2, 1.0);
                    let bar = (16.0 * d, y + 22.0 * d, w - 32.0 * d, 8.0 * d);
                    let track = if fg[0] > 128 { Color { A: 46, R: 255, G: 255, B: 255 } } else { Color { A: 26, R: 0, G: 0, B: 0 } };
                    let _ = self.rr(p, bar, 4.0 * d, track);
                    let fw = (bar.2 * *v as f32 / 100.0).max(8.0 * d);
                    let _ = self.rr(p, (bar.0, bar.1, fw, bar.3), 4.0 * d, col(*c, 255));
                }
            }
        }
        hands
    }

    fn face(&mut self, p: &ContainerVisual, cx: f32, cy: f32, r: f32, offset: Option<i32>, numbers: bool, hands: &mut Vec<Hand>) {
        let ink = [24, 24, 26];
        let _ = self.rr(p, (cx - r, cy - r, 2.0 * r, 2.0 * r), r, Color { A: 255, R: 252, G: 252, B: 253 });
        if numbers {
            for i in 1..=12u32 {
                let a = (i as f32 * 30.0).to_radians();
                let rr = r * 0.78;
                let px = (r * 0.2).max(8.0);
                self.text(p, &i.to_string(), px, 500, ink, cx + a.sin() * rr, cy - a.cos() * rr - px * 0.62, 1, 1.0);
            }
        } else {
            for i in 0..12u32 {
                let a = (i as f32 * 30.0).to_radians();
                let (len, th) = if i % 3 == 0 { (r * 0.16, (r * 0.05).max(1.5)) } else { (r * 0.09, (r * 0.03).max(1.0)) };
                if let Some(t) = self.rr(p, (cx - th / 2.0, cy - r + r * 0.08, th, len), th / 2.0, col(ink, 200)) {
                    let _ = t.SetCenterPoint(Vector3 { X: th / 2.0, Y: r - r * 0.08, Z: 0.0 });
                    let _ = t.SetRotationAngleInDegrees(a.to_degrees());
                }
            }
        }
        let specs: [(u8, f32, f32, f32, [u8; 3]); 3] = [(0, r * 0.07, r * 0.5, 0.0, ink), (1, r * 0.05, r * 0.78, 0.0, ink), (2, r * 0.022, r * 0.86, r * 0.18, ORANGE)];
        for (which, th, len, tail, c) in specs {
            let th = th.max(1.2);
            if let Some(v) = self.rr(p, (cx - th / 2.0, cy - len, th, len + tail), th / 2.0, col(c, 255)) {
                let _ = v.SetCenterPoint(Vector3 { X: th / 2.0, Y: len, Z: 0.0 });
                hands.push(Hand { v, which, offset });
            }
        }
        let dot = (r * 0.12).max(4.0);
        let _ = self.rr(p, (cx - dot / 2.0, cy - dot / 2.0, dot, dot), dot / 2.0, col(ORANGE, 255));
    }

    fn turn_hands(&mut self, i: usize, animate: bool) {
        let n = time::local_now();
        let utc = utc_minutes();
        let local = sys::utc_offset_min();
        let comp = self.comp.clone();
        let Some(wd) = self.list.get(i) else { return };
        for h in &wd.hands {
            let (hh, mm) = match h.offset {
                None => (n.hour, n.minute),
                Some(o) => {
                    let (a, b, _, _) = widgets::city_time(utc, local, o);
                    (a, b)
                }
            };
            let (ha, ma, sa) = widgets::hands(hh, mm, n.second);
            let to = match h.which {
                0 => ha,
                1 => ma,
                _ => sa,
            };
            if h.which == 2 && animate {
                let _ = sweep(&comp, &h.v, to, to + 6.0);
            } else {
                let _ = h.v.SetRotationAngleInDegrees(to);
            }
        }
    }

    fn tick(&mut self) {
        self.ticks = self.ticks.wrapping_add(1);
        if self.ticks % 2 == 0 {
            if let (Some(prev), Some(now)) = (self.cpu_prev, sys::cpu_times()) {
                self.cpu = widgets::cpu_percent(prev, now);
                self.cpu_prev = Some(now);
            }
            self.ram = sys::memory_load().unwrap_or(self.ram);
        }
        for i in 0..self.list.len() {
            let kind = self.list[i].kind;
            if self.sig_for(kind) != self.list[i].sig && self.drag.as_ref().map(|d| d.idx) != Some(i) {
                self.paint(i);
            }
            self.turn_hands(i, true);
        }
    }

    fn set_hover(&mut self, i: usize, on: bool) {
        if i >= self.list.len() || self.list[i].hover == on {
            return;
        }
        self.list[i].hover = on;
        let c = self.center(i);
        let _ = motion::hover_pop(&self.comp, self.list[i].popup.root(), c, if on { 1.03 } else { 1.0 });
    }

    fn move_to(&self, i: usize, pos: (i32, i32), topmost: bool) {
        let m = (MARGIN * self.dpi) as i32;
        let w = &self.list[i];
        unsafe {
            let after = if topmost { HWND_TOPMOST } else { HWND_BOTTOM };
            let _ = SetWindowPos(w.popup.hwnd, Some(after), pos.0 - m, pos.1 - m, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }

    fn lift(&mut self, i: usize) {
        FLOAT.store(true, Ordering::Relaxed);
        let c = self.center(i);
        let _ = motion::hover_pop(&self.comp, self.list[i].popup.root(), c, 1.06);
        let pos = self.list[i].pos;
        self.move_to(i, pos, true);
    }

    fn drag_to(&mut self, i: usize, pos: (i32, i32)) {
        if i >= self.list.len() {
            return;
        }
        self.list[i].pos = pos;
        self.move_to(i, pos, true);
        let size = (self.list[i].size.0 as i32, self.list[i].size.1 as i32);
        let spot = self.slot_for(Some(i), size, pos);
        self.show_slot((spot.0, spot.1, size.0, size.1));
        unsafe {
            let _ = SetWindowPos(self.list[i].popup.hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }

    fn show_slot(&mut self, r: (i32, i32, i32, i32)) {
        if self.slot_at == Some(r) {
            return;
        }
        let d = self.dpi;
        let m = (MARGIN * d) as i32;
        if self.slot.is_none() {
            let base = GlassParams::dark((10.0, 10.0), 0.0);
            self.slot = Popup::new(&self.comp, w!("AbraWidgetSlot"), Some(slot_proc), WS_EX_TRANSPARENT, base).ok();
        }
        let resized = self.slot_at.map(|s| (s.2, s.3)) != Some((r.2, r.3));
        let comp = self.comp.clone();
        let Some(slot) = self.slot.as_mut() else { return };
        let _ = slot.set_rect(r.0 - m, r.1 - m, r.2 + 2 * m, r.3 + 2 * m);
        let _ = slot.panel.set_rect(0.0, 0.0, 0.0, 0.0);
        let _ = slot.content.SetOffset(Vector3 { X: m as f32, Y: m as f32, Z: 0.0 });
        let content = slot.content.clone();
        let root = slot.root().clone();
        let hwnd = slot.hwnd;
        if resized {
            if let Ok(k) = content.Children() {
                let _ = k.RemoveAll();
            }
            let (w, h) = (r.2 as f32, r.3 as f32);
            let _ = self.rr(&content, (0.0, 0.0, w, h), RADIUS * d, Color { A: 70, R: 255, G: 255, B: 255 });
            let t = 2.0 * d;
            let _ = self.rr(&content, (t, t, w - 2.0 * t, h - 2.0 * t), RADIUS * d - t, Color { A: 40, R: 120, G: 120, B: 128 });
        }
        let center = Vector3 { X: m as f32 + r.2 as f32 / 2.0, Y: m as f32 + r.3 as f32 / 2.0, Z: 0.0 };
        if self.slot_at.is_none() {
            let _ = motion::pop_in(&comp, &root, center, 0.85, 1.03, 0, 260);
        } else {
            let _ = root.SetCenterPoint(center);
            let _ = root.SetScale(Vector3 { X: 0.94, Y: 0.94, Z: 1.0 });
            let _ = motion::hover_pop(&comp, &root, center, 1.0);
        }
        unsafe {
            let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW);
        }
        self.slot_at = Some(r);
    }

    fn hide_slot(&mut self) {
        self.slot_at = None;
        if let Some(s) = &self.slot {
            s.hide();
        }
    }

    fn drop_at(&mut self, i: usize) {
        let w = &self.list[i];
        let size = (w.size.0 as i32, w.size.1 as i32);
        let from = w.pos;
        let spot = self.slot_for(Some(i), size, from);
        self.hide_slot();
        self.list[i].pos = spot;
        FLOAT.store(false, Ordering::Relaxed);
        unsafe {
            let _ = SetWindowPos(self.list[i].popup.hwnd, Some(HWND_NOTOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
        let c = self.center(i);
        let root = self.list[i].popup.root().clone();
        let _ = motion::hover_pop(&self.comp, &root, c, 1.0);
        self.finish_snap();
        self.snap = Some((self.list[i].popup.hwnd, from, spot, std::time::Instant::now()));
        unsafe {
            SetTimer(Some(self.ctl), T_SNAP, 15, None);
        }
        self.save();
    }

    fn step_snap(&mut self) {
        let Some((h, from, to, t0)) = self.snap else {
            unsafe {
                let _ = KillTimer(Some(self.ctl), T_SNAP);
            }
            return;
        };
        let t = (t0.elapsed().as_secs_f32() * 1000.0 / SNAP_MS).min(1.0);
        let (c1, c3) = (1.1f32, 2.1f32);
        let e = 1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2);
        let m = (MARGIN * self.dpi) as i32;
        let x = from.0 as f32 + (to.0 - from.0) as f32 * e;
        let y = from.1 as f32 + (to.1 - from.1) as f32 * e;
        unsafe {
            let _ = SetWindowPos(h, None, x.round() as i32 - m, y.round() as i32 - m, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
        }
        if t >= 1.0 {
            self.finish_snap();
        }
    }

    fn finish_snap(&mut self) {
        unsafe {
            let _ = KillTimer(Some(self.ctl), T_SNAP);
        }
        if let Some((h, _, to, _)) = self.snap.take() {
            let m = (MARGIN * self.dpi) as i32;
            unsafe {
                let _ = SetWindowPos(h, Some(HWND_BOTTOM), to.0 - m, to.1 - m, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
            }
        }
    }

    fn add(&mut self, kind: Kind, at: Option<(i32, i32)>) -> Option<usize> {
        let d = self.dpi;
        let (w, h) = widgets::size_of(kind, d);
        let _ = (w, h, d);
        let pos = match at {
            Some(p) => p,
            None => {
                let (_, _, grid) = self.grid();
                let (cw, ch) = kind.cells();
                let cell = widgets::first_cell(&self.cells_taken(None), (cw as i32, ch as i32), grid).unwrap_or((0, 0));
                self.cell_px(cell)
            }
        };
        let i = self.spawn(kind, pos, at.is_none())?;
        if at.is_none() {
            self.save();
        }
        Some(i)
    }

    fn remove(&mut self, i: usize) {
        if i >= self.list.len() {
            return;
        }
        if self.pending_remove.is_some() {
            self.finish_remove();
        }
        let c = self.center(i);
        let _ = motion::pop_out(&self.comp, self.list[i].popup.root(), c, 0.4, 180);
        self.pending_remove = Some(self.list[i].popup.hwnd);
        unsafe {
            SetTimer(Some(self.ctl), T_REMOVE, 190, None);
        }
    }

    fn finish_remove(&mut self) {
        unsafe {
            let _ = KillTimer(Some(self.ctl), T_REMOVE);
        }
        let Some(h) = self.pending_remove.take() else { return };
        if let Some(i) = self.list.iter().position(|w| w.popup.hwnd == h) {
            let w = self.list.remove(i);
            unsafe {
                let _ = DestroyWindow(w.popup.hwnd);
            }
            self.save();
        }
    }

    fn index_of(&self, h: HWND) -> Option<usize> {
        self.list.iter().position(|w| w.popup.hwnd == h)
    }

    fn open_app(kind: Kind) {
        match kind {
            Kind::Clock | Kind::WorldClock | Kind::Digital => launch::open("ms-clock:"),
            Kind::Calendar | Kind::Month => launch::open("ms-actioncenter:"),
            Kind::Battery => launch::open("ms-settings:batterysaver"),
            Kind::System => launch::show("taskmgr.exe", ""),
        }
    }

    fn show_menu(&mut self, i: usize) {
        let (x, y) = cursor();
        let d = self.dpi;
        if self.menu.is_none() {
            self.menu = Menu::new(&self.comp, self.ctl, d, sys::screen_size(), Tier::Full).ok();
        }
        let items = vec![MItem::action(CMD_EDIT, prefs::tr("Edit Widget\u{2026}"), true), MItem::action(CMD_REMOVE, prefs::tr("Hapus Widget"), true)];
        self.menu_for = Some(i);
        let comp = self.comp.clone();
        if let (Some(m), Some(g)) = (self.menu.as_mut(), self.gfx.as_ref()) {
            let _ = m.show(&comp, g, items, x, y);
        }
    }

    fn set_edit(&mut self, on: bool) {
        if self.edit == on {
            return;
        }
        self.edit = on;
        for i in 0..self.list.len() {
            self.paint(i);
            let c = self.center(i);
            let _ = motion::pop_in(&self.comp, self.list[i].popup.root(), c, 0.9, 1.04, i as u32 * 18, 320);
        }
    }

    fn open_gallery(&mut self) {
        if self.gallery.is_some() {
            return;
        }
        let d = self.dpi;
        let (gw, gh) = (760.0 * d, 500.0 * d);
        let mut glass = prefs::glass((gw, gh), 26.0 * d, prefs::Role::Themed);
        glass.blur_sigma = glass.blur_sigma.max(22.0);
        let Ok(mut popup) = Popup::new_with(&self.comp, w!("AbraWidgetGallery"), Some(gallery_proc), WINDOW_EX_STYLE(0), glass, true) else { return };
        let (sw, sh) = sys::screen_size();
        let _ = popup.set_rect(((sw as f32 - gw) / 2.0) as i32, (sh as f32 - gh - 110.0 * d).max(40.0 * d) as i32, gw as i32, gh as i32);
        let _ = popup.panel.set_radius(26.0 * d);
        self.gallery = Some(Gallery { popup, cat: None, query: String::new(), cards: vec![], cats: vec![], done: (0.0, 0.0, 0.0, 0.0), conts: vec![], hover: None, tracking: false, press: None, closing: false });
        self.refresh_gallery();
        self.set_edit(true);
        if let Some(g) = &self.gallery {
            g.popup.show_active();
            let _ = motion::pop_in(&self.comp, g.popup.root(), Vector3 { X: gw / 2.0, Y: gh, Z: 0.0 }, 0.82, 1.02, 0, 340);
        }
    }

    fn close_gallery(&mut self) {
        let Some(g) = self.gallery.as_mut() else { return };
        if g.closing {
            return;
        }
        g.closing = true;
        let (gw, gh) = (g.popup.rect.2 as f32, g.popup.rect.3 as f32);
        let _ = motion::pop_out(&self.comp, g.popup.root(), Vector3 { X: gw / 2.0, Y: gh, Z: 0.0 }, 0.86, 180);
        unsafe {
            SetTimer(Some(self.ctl), T_GCLOSE, 190, None);
        }
        self.set_edit(false);
    }

    fn finish_close_gallery(&mut self) {
        unsafe {
            let _ = KillTimer(Some(self.ctl), T_GCLOSE);
        }
        if let Some(g) = self.gallery.take() {
            unsafe {
                let _ = DestroyWindow(g.popup.hwnd);
            }
            self.cache.clear();
            nacre_platform::sys::trim_later(5000);
        }
    }

    fn refresh_gallery(&mut self) {
        let Some(g) = self.gallery.as_ref() else { return };
        let d = self.dpi;
        let (cat, raw_query) = (g.cat, g.query.clone());
        let query = raw_query.to_lowercase();
        let content = g.popup.content.clone();
        let (gw, gh) = (g.popup.rect.2 as f32, g.popup.rect.3 as f32);
        if let Ok(k) = content.Children() {
            let _ = k.RemoveAll();
        }
        let look = prefs::look(prefs::Role::Themed);
        let fg = look.fg;
        let side = 200.0 * d;
        let mut conts: Vec<(Rect, ContainerVisual, f32)> = Vec::new();
        let _ = self.rr(&content, (side, 0.0, 1.0, gh), 0.0, col(fg, 26));
        let sb: Rect = (14.0 * d, 16.0 * d, side - 28.0 * d, 30.0 * d);
        let _ = self.rr(&content, sb, 9.0 * d, col(fg, 22));
        self.glyph(&content, "\u{E721}", 11.0 * d, fg, sb.0 + 14.0 * d, sb.1 + sb.3 / 2.0, 0.6);
        let q = if query.is_empty() { prefs::tr("Cari Widget").to_string() } else { format!("{raw_query}|") };
        self.text(&content, &q, 13.0 * d, 400, fg, sb.0 + 28.0 * d, sb.1 + 7.0 * d, 0, if query.is_empty() { 0.5 } else { 1.0 });
        let mut cats = Vec::new();
        let mut y = 60.0 * d;
        let mut entries: Vec<(Option<Category>, &'static str, &'static str, [u8; 3])> = vec![(None, "Semua Widget", "\u{E71D}", [99, 99, 108])];
        for c in Category::ALL {
            let (gl, cc) = match c {
                Category::Clock => ("\u{E121}", ORANGE),
                Category::Calendar => ("\u{E787}", RED),
                Category::Battery => ("\u{E83F}", GREEN),
                Category::System => ("\u{E950}", BLUE),
            };
            entries.push((Some(c), c.title(), gl, cc));
        }
        for (c, title, gl, cc) in entries {
            let r: Rect = (10.0 * d, y, side - 20.0 * d, 34.0 * d);
            let Ok(row) = self.comp.CreateContainerVisual() else { continue };
            let _ = content.Children().and_then(|k| k.InsertAtTop(&row));
            conts.push((r, row.clone(), motion::HOVER_ROW));
            if c == cat {
                let _ = self.rr(&row, r, 9.0 * d, crate::prefs::accent_c(220));
            }
            let ic: Rect = (r.0 + 8.0 * d, r.1 + 6.0 * d, 22.0 * d, 22.0 * d);
            let _ = self.rr(&row, ic, 6.0 * d, col(cc, 255));
            self.glyph(&row, gl, 12.0 * d, [255, 255, 255], ic.0 + ic.2 / 2.0, ic.1 + ic.3 / 2.0, 1.0);
            self.text(&row, prefs::tr(title), 13.0 * d, if c == cat { 600 } else { 500 }, if c == cat { [255, 255, 255] } else { fg }, r.0 + 40.0 * d, r.1 + 8.0 * d, 0, 1.0);
            cats.push((c, r));
            y += 38.0 * d;
        }
        let x0 = side + 24.0 * d;
        let header = cat.map(|c| c.title()).unwrap_or("Semua Widget");
        self.text(&content, prefs::tr(header), 15.0 * d, 700, fg, x0, 16.0 * d, 0, 1.0);
        let list: Vec<Kind> = Kind::ALL
            .iter()
            .copied()
            .filter(|k| cat.map(|c| k.category() == c).unwrap_or(true))
            .filter(|k| query.is_empty() || prefs::tr(k.title()).to_lowercase().contains(&query) || prefs::tr(k.category().title()).to_lowercase().contains(&query))
            .collect();
        let (mut cx, mut cy) = (x0, 52.0 * d);
        let row_h = widgets::size_of(Kind::Clock, d).1 * PREVIEW + 62.0 * d;
        let right = gw - 20.0 * d;
        let mut cards = Vec::new();
        for k in list {
            let (w, h) = widgets::size_of(k, d);
            let (pw, ph) = (w * PREVIEW, h * PREVIEW);
            let cell_w = pw.max(130.0 * d);
            if cx + cell_w > right && cx > x0 {
                cx = x0;
                cy += row_h;
            }
            if cy + ph > gh - 60.0 * d {
                break;
            }
            let r: Rect = (cx + (cell_w - pw) / 2.0, cy, pw, ph);
            let Ok(card) = self.comp.CreateContainerVisual() else { continue };
            let _ = content.Children().and_then(|kk| kk.InsertAtTop(&card));
            let Ok(inner) = self.comp.CreateContainerVisual() else { continue };
            let _ = card.Children().and_then(|kk| kk.InsertAtTop(&inner));
            let _ = inner.SetOffset(Vector3 { X: r.0, Y: r.1, Z: 0.0 });
            let _ = inner.SetScale(Vector3 { X: PREVIEW, Y: PREVIEW, Z: 1.0 });
            let mut hands = self.draw(&inner, k, w, h, true);
            let n = time::local_now();
            for hd in hands.drain(..) {
                let (a, b, c) = widgets::hands(n.hour, n.minute, n.second);
                let _ = hd.v.SetRotationAngleInDegrees(match hd.which {
                    0 => a,
                    1 => b,
                    _ => c,
                });
            }
            self.text(&card, prefs::tr(k.title()), 12.0 * d, 600, fg, cx + cell_w / 2.0, cy + ph + 8.0 * d, 1, 1.0);
            self.text(&card, prefs::tr(k.desc()), 10.5 * d, 400, fg, cx + cell_w / 2.0, cy + ph + 26.0 * d, 1, 0.6);
            conts.push((r, card, 1.06));
            cards.push((k, r));
            cx += cell_w + 22.0 * d;
        }
        let _ = self.rr(&content, (side, gh - 52.0 * d, gw - side, 1.0), 0.0, col(fg, 26));
        self.text(&content, prefs::tr("Seret widget ke desktop, atau klik untuk menambahkannya."), 12.0 * d, 400, fg, x0, gh - 34.0 * d, 0, 0.6);
        let done: Rect = (gw - 104.0 * d, gh - 42.0 * d, 86.0 * d, 30.0 * d);
        if let Ok(dc) = self.comp.CreateContainerVisual() {
            let _ = content.Children().and_then(|k| k.InsertAtTop(&dc));
            let _ = self.rr(&dc, done, 15.0 * d, crate::prefs::accent_c(255));
            self.text(&dc, prefs::tr("Selesai"), 13.0 * d, 600, [255, 255, 255], done.0 + done.2 / 2.0, done.1 + 6.0 * d, 1, 1.0);
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

    fn gallery_hover(&mut self, i: Option<usize>) {
        let comp = self.comp.clone();
        let Some(g) = self.gallery.as_mut() else { return };
        if g.hover == i {
            return;
        }
        for (k, on) in [(g.hover, false), (i, true)] {
            if let Some((r, c, to)) = k.and_then(|k| g.conts.get(k)) {
                let _ = motion::hover_pop(&comp, c, Vector3 { X: r.0 + r.2 / 2.0, Y: r.1 + r.3 / 2.0, Z: 0.0 }, if on { *to } else { 1.0 });
            }
        }
        g.hover = i;
    }
}

fn sweep(comp: &Compositor, v: &SpriteVisual, from: f32, to: f32) -> windows_core::Result<()> {
    let a = comp.CreateScalarKeyFrameAnimation()?;
    a.InsertKeyFrame(0.0, from)?;
    a.InsertKeyFrameWithEasingFunction(1.0, to, &comp.CreateLinearEasingFunction()?)?;
    a.SetDuration(TimeSpan { Duration: 1000 * 10_000 })?;
    v.cast::<CompositionObject>()?.StartAnimation(&HSTRING::from("RotationAngleInDegrees"), &a)
}

fn xy(lp: LPARAM) -> (f32, f32) {
    ((lp.0 as i16) as f32, ((lp.0 >> 16) as i16) as f32)
}

unsafe extern "system" fn ctl_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_TIMER => {
            match wp.0 {
                T_TICK => {
                    with(|m| m.tick());
                }
                T_REMOVE => {
                    with(|m| m.finish_remove());
                }
                T_GCLOSE => {
                    with(|m| m.finish_close_gallery());
                }
                T_SNAP => {
                    with(|m| m.step_snap());
                }
                _ => {}
            }
            LRESULT(0)
        }
        prefs::WM_PREFS => {
            with(|m| {
                m.rebuild_all();
                if m.gallery.is_some() {
                    m.refresh_gallery();
                }
            });
            LRESULT(0)
        }
        m if m == MENU_PICK => {
            let id = wp.0;
            with(|mg| {
                if let Some(mn) = &mg.menu {
                    mn.hide();
                }
                let Some(i) = mg.menu_for.take() else { return };
                match id {
                    CMD_EDIT => mg.open_gallery(),
                    CMD_REMOVE => mg.remove(i),
                    _ => {}
                }
            });
            LRESULT(0)
        }
        m if m == MENU_CLOSE => {
            with(|mg| {
                if let Some(mn) = &mg.menu {
                    mn.hide();
                }
            });
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe extern "system" fn widget_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_WINDOWPOSCHANGING => {
            let p = lp.0 as *mut WINDOWPOS;
            if !p.is_null() && !FLOAT.load(Ordering::Relaxed) && (*p).flags.0 & SWP_NOZORDER.0 == 0 {
                (*p).hwndInsertAfter = HWND_BOTTOM;
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_LBUTTONDOWN => {
            let (x, y) = xy(lp);
            with(|m| {
                m.finish_snap();
                let Some(i) = m.index_of(hwnd) else { return };
                if m.badge_hit(i, x, y) {
                    m.remove(i);
                    return;
                }
                let c = cursor();
                let pos = m.list[i].pos;
                m.drag = Some(Drag { idx: i, grab: (c.0 - pos.0, c.1 - pos.1), start: c, moved: false });
                SetCapture(hwnd);
            });
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            with(|m| {
                let Some(i) = m.index_of(hwnd) else { return };
                if let Some(dg) = m.drag.as_ref().filter(|dg| dg.idx == i) {
                    let c = cursor();
                    let (grab, start, moved) = (dg.grab, dg.start, dg.moved);
                    if !moved && ((c.0 - start.0).abs() > 4 || (c.1 - start.1).abs() > 4) {
                        if let Some(dg) = m.drag.as_mut() {
                            dg.moved = true;
                        }
                        m.lift(i);
                    }
                    if m.drag.as_ref().is_some_and(|dg| dg.moved) {
                        m.drag_to(i, (c.0 - grab.0, c.1 - grab.1));
                    }
                    return;
                }
                if !m.list[i].tracking {
                    let mut t = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
                    m.list[i].tracking = TrackMouseEvent(&mut t).is_ok();
                }
                m.set_hover(i, true);
            });
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            with(|m| {
                if let Some(i) = m.index_of(hwnd) {
                    m.list[i].tracking = false;
                    m.set_hover(i, false);
                }
            });
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let _ = ReleaseCapture();
            with(|m| {
                let Some(dg) = m.drag.take() else { return };
                if dg.moved {
                    m.drop_at(dg.idx);
                } else if !m.edit {
                    if let Some(w) = m.list.get(dg.idx) {
                        Mgr::open_app(w.kind);
                    }
                }
            });
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            with(|m| {
                if let Some(i) = m.index_of(hwnd) {
                    m.show_menu(i);
                }
            });
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe extern "system" fn slot_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe extern "system" fn gallery_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_LBUTTONDOWN => {
            let (x, y) = xy(lp);
            with(|m| {
                let Some(g) = m.gallery.as_ref() else { return };
                if hit(g.done, x, y) {
                    m.close_gallery();
                    return;
                }
                if let Some(c) = g.cats.iter().find(|(_, r)| hit(*r, x, y)).map(|(c, _)| *c) {
                    if let Some(g) = m.gallery.as_mut() {
                        g.cat = c;
                    }
                    m.refresh_gallery();
                    return;
                }
                if let Some(k) = g.cards.iter().find(|(_, r)| hit(*r, x, y)).map(|(k, _)| *k) {
                    if let Some(g) = m.gallery.as_mut() {
                        g.press = Some((k, cursor()));
                    }
                    SetCapture(hwnd);
                }
            });
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let (x, y) = xy(lp);
            with(|m| {
                let c = cursor();
                if let Some(dg) = m.drag.as_ref() {
                    let (i, grab) = (dg.idx, dg.grab);
                    m.drag_to(i, (c.0 - grab.0, c.1 - grab.1));
                    return;
                }
                let press = m.gallery.as_ref().and_then(|g| g.press);
                if let Some((k, start)) = press {
                    if (c.0 - start.0).abs() > 6 || (c.1 - start.1).abs() > 6 {
                        let (w, h) = widgets::size_of(k, m.dpi);
                        let grab = ((w / 2.0) as i32, (h / 2.0) as i32);
                        FLOAT.store(true, Ordering::Relaxed);
                        if let Some(i) = m.add(k, Some((c.0 - grab.0, c.1 - grab.1))) {
                            m.lift(i);
                            m.drag = Some(Drag { idx: i, grab, start, moved: true });
                        }
                        if let Some(g) = m.gallery.as_mut() {
                            g.press = None;
                        }
                    }
                    return;
                }
                let Some(g) = m.gallery.as_mut() else { return };
                if !g.tracking {
                    let mut t = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
                    g.tracking = TrackMouseEvent(&mut t).is_ok();
                }
                let i = g.conts.iter().position(|(r, _, _)| hit(*r, x, y));
                m.gallery_hover(i);
            });
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            with(|m| {
                if let Some(g) = m.gallery.as_mut() {
                    g.tracking = false;
                }
                m.gallery_hover(None);
            });
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let _ = ReleaseCapture();
            with(|m| {
                if let Some(dg) = m.drag.take() {
                    m.drop_at(dg.idx);
                    return;
                }
                let press = m.gallery.as_mut().and_then(|g| g.press.take());
                if let Some((k, _)) = press {
                    m.add(k, None);
                }
            });
            LRESULT(0)
        }
        WM_CHAR => {
            let ch = char::from_u32(wp.0 as u32).unwrap_or('\0');
            if !ch.is_control() {
                with(|m| {
                    if let Some(g) = m.gallery.as_mut() {
                        g.query.push(ch);
                    }
                    m.refresh_gallery();
                });
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if wp.0 as u16 == VK_BACK.0 {
                with(|m| {
                    if let Some(g) = m.gallery.as_mut() {
                        g.query.pop();
                    }
                    m.refresh_gallery();
                });
            } else if wp.0 as u16 == VK_ESCAPE.0 {
                with(|m| m.close_gallery());
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            with(|m| m.close_gallery());
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
