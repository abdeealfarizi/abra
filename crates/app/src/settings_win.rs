use crate::popup::Popup;
use crate::prefs::{self, Role, WM_PREFS};
use nacre_core::hud::slider;
use nacre_core::i18n::Lang;
use nacre_core::settings::{Behavior, Material, Theme};
use nacre_platform::icons::Bitmap;
use nacre_platform::text::{measure, render, render_face, Style, ICON_FACE};
use nacre_platform::{log, sys};
use nacre_ui::gfx::Gfx;
use std::cell::RefCell;
use std::collections::HashMap;
use windows::core::w;
use windows::UI::Color;
use windows::UI::Composition::{CompositionDrawingSurface, CompositionStretch, Compositor, ContainerVisual};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, SetFocus, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};
use windows::Win32::UI::WindowsAndMessaging::*;
const WM_MOUSELEAVE: u32 = 0x02A3;
use windows_core::Interface;
use windows_numerics::{Vector2, Vector3};

const WM_CLOSE_SELF: u32 = WM_APP + 50;
const W: f32 = 780.0;
const H: f32 = 560.0;
const SIDEBAR_W: f32 = 214.0;
const CONTENT_X: f32 = SIDEBAR_W + 24.0;
const CONTENT_W: f32 = W - SIDEBAR_W - 48.0;
const VIEW_TOP: f32 = 56.0;
const SIDE_INSET: f32 = 8.0;
const SIDE_RADIUS: f32 = 18.0;

fn side_glass(mut g: nacre_ui::glass::GlassParams) -> nacre_ui::glass::GlassParams {
    g.blur_sigma = g.blur_sigma.max(16.0);
    g
}

fn round_sdf(px: f32, py: f32, r: (f32, f32, f32, f32), rad: f32) -> f32 {
    let (cx, cy) = (r.0 + r.2 / 2.0, r.1 + r.3 / 2.0);
    let rad = rad.min(r.2 / 2.0).min(r.3 / 2.0);
    let qx = (px - cx).abs() - (r.2 / 2.0 - rad);
    let qy = (py - cy).abs() - (r.3 / 2.0 - rad);
    (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - rad
}

fn frame_pixels(w: i32, h: i32, radius: f32, hole: (f32, f32, f32, f32), hole_radius: f32, dark: bool) -> Vec<u8> {
    let base: [f32; 3] = if dark { [16.0, 16.0, 18.0] } else { [255.0, 255.0, 255.0] };
    let (edge, edge_a): ([f32; 3], f32) = if dark { ([255.0, 255.0, 255.0], 0.16) } else { ([0.0, 0.0, 0.0], 0.12) };
    let outer = (0.0, 0.0, w as f32, h as f32);
    let blur = hole_radius * 1.1;
    let shade_dy = hole_radius * 0.12;
    let (tone, peak, inner): ([f32; 3], f32, f32) = if dark { ([255.0, 255.0, 255.0], 0.0, 0.07) } else { ([0.0, 0.0, 0.0], 0.10, 0.05) };
    let mut out = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let d = round_sdf(px, py, outer, radius);
            let cov = (0.5 - d).clamp(0.0, 1.0);
            if cov <= 0.0 {
                continue;
            }
            let a = cov;
            let sd = round_sdf(px, py - shade_dy, hole, hole_radius);
            let shade = if sd <= 0.0 { inner } else { peak * (1.0 - (sd / blur).clamp(0.0, 1.0)).powi(3) };
            let ring = (cov - (0.5 - (d + 1.0)).clamp(0.0, 1.0)).clamp(0.0, 1.0) * edge_a;
            let i = ((y * w + x) * 4) as usize;
            for c in 0..3 {
                let shaded = base[c] * (1.0 - shade) + tone[c] * shade;
                let v = shaded * (1.0 - ring) + edge[c] * ring;
                out[i + 2 - c] = (v * a).round() as u8;
            }
            out[i + 3] = (a * 255.0).round() as u8;
        }
    }
    out
}

#[derive(Clone, Copy, Debug)]
enum Action {
    EditText(u8),
    NotchIdle(u8),
    LyricsCompact,
    NotchDownloads,
    NotchSend,
    LyricsOnline,
    LyricsDir,
    WinColors,
    WinPower,
    Donate,
    UpdateCheck,
    UpdateAct,
    UpdateNotes,
    PillReminders,
    PillDevices,
    NotchGlow,
    TodoHotkey,
    Close,
    Takeover,
    Dock(Behavior),
    Notch(Behavior),
    Rail(Behavior),
    Monitor(u8, u8),
    Theme(Theme),
    Lang(Lang),
    Tab(u8),
    Autostart,
    Anim,
    Refraction,
    DockOn,
    MenuBarOn,
    Clock24,
    BarStyle(u8),
    HotCycle(usize),
    StageOn,
    SwitcherOn,
    StageFlag(u8),
    StageRight(bool),
    StageIcons(u8),
    HideIcons,
    DockPlates,
    IconPack,
    DownloadsStack,
    RecentApps,
    AboveFull,
    ClipHistory,
    MacKeys,
    Accent(u8),
    IconVariant(u8),
    MinEffect(u8),
    Back,
    Forward,
    Search,
    IconStyle(u8),
    IconTint(u8),
    NotchOn,
    HoverOpen,
    WinShadow,
    Material(bool, Material),
    Slider(u8, f32, f32),
    MenuLogoPick,
    MenuLogoReset,
    UiScale(u8),
    NotchShape(bool),
    WidgetStyle(u8),
    WidgetGallery,
    BatteryPct(u8),
}

struct Region {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    a: Action,
    scroll: bool,
}

enum RowDef {
    Toggle(&'static str, bool, Action),
    Seg(&'static str, Vec<(&'static str, Action)>, usize),
    Slider(&'static str, u8, u8),
    Pick(&'static str, String, Action),
    Swatches(&'static str, usize),
    Thumbs(&'static str, Vec<(&'static str, Thumb, Action)>, usize),
    Radios(Vec<(&'static str, Action)>, usize),
    Text(&'static str, String, String, u8),
}

#[derive(Clone, Copy)]
enum Thumb {
    Appear(u8),
    Icon(nacre_core::settings::IconStyle),
}

const ACCENT_NAMES: [&str; 9] = ["Multiwarna", "Biru", "Ungu", "Merah Muda", "Merah", "Oranye", "Kuning", "Hijau", "Grafit"];

fn row_label(r: &RowDef) -> &'static str {
    match r {
        RowDef::Toggle(l, ..) | RowDef::Seg(l, ..) | RowDef::Slider(l, ..) | RowDef::Pick(l, ..) | RowDef::Swatches(l, ..) | RowDef::Thumbs(l, ..) | RowDef::Text(l, ..) => l,
        RowDef::Radios(..) => "",
    }
}

const DONATE_URL: &str = "https://saweria.co/kreativadev";

fn close_button(d: f32) -> (f32, f32, f32) {
    (W * d - 44.0 * d, 13.0 * d, 28.0 * d)
}

fn user_name() -> String {
    std::env::var("USERNAME").ok().filter(|u| !u.is_empty()).unwrap_or_else(|| "ABRA".to_string())
}

struct Win {
    popup: Popup,
    gfx: Gfx,
    comp: Compositor,
    dpi: f32,
    size: (f32, f32),
    regions: Vec<Region>,
    cache: HashMap<String, (CompositionDrawingSurface, i32, i32)>,
    dragging: Option<(u8, f32, f32)>,
    side: Vec<(u8, (f32, f32, f32, f32), ContainerVisual)>,
    side_hover: Option<u8>,
    side_scroll: f32,
    side_view: (f32, f32),
    side_bar: Option<((f32, f32, f32, f32), f32)>,
    side_drag: Option<f32>,
    press: Option<(f32, f32)>,
    tab: u8,
    scroll: f32,
    content_h: f32,
    history: Vec<u8>,
    hpos: usize,
    search: String,
    search_focus: bool,
    thumbs: HashMap<String, CompositionDrawingSurface>,
    edit: Option<u8>,
    edit_buf: String,
    frame: Option<windows::UI::Composition::SpriteVisual>,
    frame_key: (i32, i32, bool),
}

thread_local! {
    static WIN: RefCell<Option<Box<Win>>> = const { RefCell::new(None) };
}

pub fn open(comp: &Compositor) {
    let existing = WIN.with(|w| w.borrow().as_ref().map(|b| b.popup.hwnd));
    if let Some(h) = existing {
        unsafe {
            let _ = SetWindowPos(h, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            let _ = SetForegroundWindow(h);
        }
        return;
    }
    match create(comp) {
        Ok(()) => log::line("settings", "dibuka"),
        Err(e) => log::line("settings", &format!("gagal: {e}")),
    }
}

fn create(comp: &Compositor) -> windows_core::Result<()> {
    let glass = side_glass(prefs::glass((10.0, 10.0), SIDE_RADIUS, Role::Themed));
    let mut popup = Popup::new(comp, w!("AbraSettings"), Some(proc), WINDOW_EX_STYLE(0), glass)?;
    let dpi = unsafe { GetDpiForWindow(popup.hwnd) }.max(96) as f32 / 96.0;
    let gfx = Gfx::shared(comp)?;
    let tier = {
        let c = sys::conditions();
        if !c.transparency || c.battery_saver || c.remote_session { nacre_ui::glass::Tier::Solid } else { nacre_ui::glass::Tier::Full }
    };
    let _ = popup.panel.set_tier(tier);
    let _ = popup.panel.set_radius(24.0 * dpi);
    let mut win = Box::new(Win { popup, gfx, comp: comp.clone(), dpi, size: (0.0, 0.0), regions: vec![], cache: HashMap::new(), dragging: None, side: vec![], side_hover: None, side_scroll: 0.0, side_view: (0.0, 0.0), side_bar: None, side_drag: None, press: None, tab: 0, scroll: 0.0, content_h: 0.0, history: vec![0], hpos: 0, search: String::new(), search_focus: false, thumbs: HashMap::new(), edit: None, edit_buf: String::new(), frame: None, frame_key: (0, 0, false) });
    win.render();
    let (sw, sh) = sys::screen_size();
    let (w, h) = (win.size.0 as i32, win.size.1 as i32);
    let top = ((sh - h) / 2).clamp(12, (sh - h - 12).max(12));
    log::line("settings", &format!("layar={sw}x{sh} jendela={w}x{h} atas={top}"));
    win.popup.set_rect((sw - w) / 2, top, w, h)?;
    win.frame();
    win.popup.show();
    unsafe {
        let _ = SetForegroundWindow(win.popup.hwnd);
    }
    prefs::listen(win.popup.hwnd);
    WIN.with(|s| *s.borrow_mut() = Some(win));
    Ok(())
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

fn col(rgb: [u8; 3], a: u8) -> Color {
    Color { A: a, R: rgb[0], G: rgb[1], B: rgb[2] }
}

impl Win {
    fn surface(&mut self, s: &str, px: i32, weight: i32, rgb: [u8; 3]) -> Option<(CompositionDrawingSurface, i32, i32)> {
        let key = format!("{px}|{weight}|{rgb:?}|{s}");
        if let Some(v) = self.cache.get(&key) {
            return Some(v.clone());
        }
        if self.cache.len() > 160 {
            self.cache.clear();
        }
        let style = Style { px, weight };
        let (tw, th) = measure(s, &style);
        let tw = tw + 2;
        let bmp = render(s, &style, tw, th)?;
        let data = recolor(&bmp, rgb);
        let surf = self.gfx.surface_from_bgra(bmp.w, bmp.h, &data).ok()?;
        let v = (surf, tw, th);
        self.cache.insert(key, v.clone());
        Some(v)
    }

    #[allow(clippy::too_many_arguments)]
    fn text(&mut self, parent: &ContainerVisual, s: &str, x: f32, y: f32, size: f32, weight: i32, rgb: [u8; 3], alpha: f32, anchor: u8) -> f32 {
        let px = (size * self.dpi).round() as i32;
        let Some((surf, w, h)) = self.surface(s, px, weight, rgb) else { return 0.0 };
        let left = match anchor {
            1 => x - w as f32 / 2.0,
            2 => x - w as f32,
            _ => x,
        };
        let make = || -> windows_core::Result<()> {
            let b = self.comp.CreateSurfaceBrushWithSurface(&surf)?;
            b.SetStretch(CompositionStretch::None)?;
            let v = self.comp.CreateSpriteVisual()?;
            v.SetBrush(&b)?;
            v.SetSize(Vector2 { X: w as f32, Y: h as f32 })?;
            v.SetOffset(Vector3 { X: left, Y: y, Z: 0.0 })?;
            v.SetOpacity(alpha)?;
            parent.Children()?.InsertAtTop(&v)
        };
        let _ = make();
        w as f32
    }

    fn rect(&self, parent: &ContainerVisual, x: f32, y: f32, w: f32, h: f32, r: f32, c: Color) {
        let make = || -> windows_core::Result<()> {
            let v = self.comp.CreateSpriteVisual()?;
            v.SetSize(Vector2 { X: w, Y: h })?;
            v.SetOffset(Vector3 { X: x, Y: y, Z: 0.0 })?;
            v.SetBrush(&self.comp.CreateColorBrushWithColor(c)?)?;
            if r > 0.0 {
                let g = self.comp.CreateRoundedRectangleGeometry()?;
                g.SetSize(Vector2 { X: w, Y: h })?;
                g.SetCornerRadius(Vector2 { X: r, Y: r })?;
                let clip = self.comp.CreateGeometricClip()?;
                clip.SetGeometry(&g)?;
                v.SetClip(&clip)?;
            }
            parent.Children()?.InsertAtTop(&v)
        };
        let _ = make();
    }

    fn glass_knob(&self, parent: &ContainerVisual, x: f32, y: f32, w: f32, h: f32) {
        let d = self.dpi;
        self.rect(parent, x - 1.0 * d, y + 1.0 * d, w + 2.0 * d, h + 2.0 * d, h / 2.0 + d, Color { A: 46, R: 0, G: 0, B: 0 });
        let make = || -> windows_core::Result<()> {
            let geo = self.comp.CreateRoundedRectangleGeometry()?;
            geo.SetOffset(Vector2 { X: 0.5, Y: 0.5 })?;
            geo.SetSize(Vector2 { X: (w - 1.0).max(0.0), Y: (h - 1.0).max(0.0) })?;
            geo.SetCornerRadius(Vector2 { X: h / 2.0, Y: h / 2.0 })?;
            let shape = self.comp.CreateSpriteShapeWithGeometry(&geo)?;
            shape.SetFillBrush(&self.comp.CreateColorBrushWithColor(Color { A: 92, R: 255, G: 255, B: 255 })?)?;
            shape.SetStrokeBrush(&self.comp.CreateColorBrushWithColor(Color { A: 190, R: 255, G: 255, B: 255 })?)?;
            shape.SetStrokeThickness(1.0)?;
            let v = self.comp.CreateShapeVisual()?;
            v.SetSize(Vector2 { X: w, Y: h })?;
            v.SetOffset(Vector3 { X: x, Y: y, Z: 0.0 })?;
            v.Shapes()?.Append(&shape)?;
            parent.Children()?.InsertAtTop(&v)
        };
        let _ = make();
    }

    fn glyph_icon(&mut self, parent: &ContainerVisual, g: &str, cx: f32, cy: f32, size: f32, rgb: [u8; 3]) {
        let px = (size * self.dpi).round() as i32;
        let b = (size * 1.8 * self.dpi).round() as i32;
        let key = format!("ico|{px}|{rgb:?}|{g}");
        let hit = self.cache.get(&key).cloned();
        let (surf, w, h) = match hit {
            Some(v) => v,
            None => {
                let Some(bmp) = render_face(g, &Style { px, weight: 400 }, ICON_FACE, b, b) else { return };
                let Some((cd, cw, ch)) = nacre_core::symbols::crop_ink(&recolor(&bmp, rgb), bmp.w, bmp.h) else { return };
                let Ok(sf) = self.gfx.surface_from_bgra(cw, ch, &cd) else { return };
                let v = (sf, cw as i32, ch as i32);
                self.cache.insert(key, v.clone());
                v
            }
        };
        let make = || -> windows_core::Result<()> {
            let br = self.comp.CreateSurfaceBrushWithSurface(&surf)?;
            br.SetStretch(CompositionStretch::None)?;
            let v = self.comp.CreateSpriteVisual()?;
            v.SetBrush(&br)?;
            v.SetSize(Vector2 { X: w as f32, Y: h as f32 })?;
            v.SetOffset(Vector3 { X: cx - w as f32 / 2.0, Y: cy - h as f32 / 2.0, Z: 0.0 })?;
            parent.Children()?.InsertAtTop(&v)
        };
        let _ = make();
    }

    fn sprite(&self, parent: &ContainerVisual, s: &CompositionDrawingSurface, x: f32, y: f32, w: f32, h: f32) {
        let make = || -> windows_core::Result<()> {
            let b = self.comp.CreateSurfaceBrushWithSurface(s)?;
            b.SetStretch(CompositionStretch::Fill)?;
            let v = self.comp.CreateSpriteVisual()?;
            v.SetBrush(&b)?;
            v.SetSize(Vector2 { X: w, Y: h })?;
            v.SetOffset(Vector3 { X: x, Y: y, Z: 0.0 })?;
            parent.Children()?.InsertAtTop(&v)
        };
        let _ = make();
    }

    fn thumb_surface(&mut self, key: String, w: u32, h: u32, make: impl FnOnce() -> Vec<u8>) -> Option<CompositionDrawingSurface> {
        if let Some(s) = self.thumbs.get(&key) {
            return Some(s.clone());
        }
        if self.thumbs.len() > 64 {
            self.thumbs.clear();
        }
        let s = self.gfx.surface_from_bgra(w, h, &make()).ok()?;
        self.thumbs.insert(key, s.clone());
        Some(s)
    }

    fn go(&mut self, t: u8, push: bool) {
        if push && t != self.tab {
            self.history.truncate(self.hpos + 1);
            self.history.push(t);
            self.hpos = self.history.len() - 1;
        }
        self.tab = t;
        self.scroll = 0.0;
        self.render();
    }

    fn page_matches(&self, idx: u8, name: &str) -> bool {
        let q = self.search.trim().to_lowercase();
        if q.is_empty() || name.to_lowercase().contains(&q) {
            return true;
        }
        self.page(idx).iter().any(|(h, rows)| h.is_some_and(|h| h.to_lowercase().contains(&q)) || rows.iter().any(|r| row_label(r).to_lowercase().contains(&q)))
    }

    fn region(&mut self, x: f32, y: f32, w: f32, h: f32, a: Action) {
        self.regions.push(Region { x, y, w, h, a, scroll: true });
    }

    fn hover_side(&mut self, hit: Option<u8>) {
        if hit == self.side_hover {
            return;
        }
        for (page, on) in [(self.side_hover, false), (hit, true)] {
            let Some((_, r, ic)) = self.side.iter().find(|e| Some(e.0) == page) else { continue };
            let to = if on { crate::motion::HOVER_ROW } else { 1.0 };
            let _ = crate::motion::hover_pop(&self.comp, ic, Vector3 { X: r.0 + r.2 / 2.0, Y: r.1 + r.3 / 2.0, Z: 0.0 }, to);
        }
        self.side_hover = hit;
    }

    fn region_fixed(&mut self, x: f32, y: f32, w: f32, h: f32, a: Action) {
        self.regions.push(Region { x, y, w, h, a, scroll: false });
    }

    fn spec(id: u8) -> (f32, f32, &'static str) {
        match id {
            4 => (32.0, 72.0, " dp"),
            5 => (0.0, 24.0, " dp"),
            6 => (100.0, 250.0, "%"),
            7 => (0.0, 60.0, " dp"),
            8 => (70.0, 260.0, " dp"),
            9 => (0.0, 60.0, " dp"),
            10 => (2.0, 10.0, ""),
            11 => (50.0, 200.0, "%"),
            12 => (80.0, 130.0, "%"),
            13 => (0.0, 40.0, " dp"),
            _ => (0.0, 100.0, "%"),
        }
    }

    fn commit_edit(&mut self) {
        let Some(f) = self.edit.take() else { return };
        let t = nacre_core::settings::ShortText::new(&self.edit_buf);
        prefs::update(|s| {
            if f == 0 {
                s.greet_name = t;
            } else {
                s.greet_text = t;
            }
        });
        self.render();
    }

    fn row_height(r: &RowDef) -> f32 {
        match r {
            RowDef::Slider(..) => 50.0,
            RowDef::Thumbs(..) => 96.0,
            RowDef::Radios(..) => 34.0,
            RowDef::Swatches(..) => 58.0,
            _ => 44.0,
        }
    }

    fn card(&mut self, c: &ContainerVisual, y0: f32, rows: &[RowDef], look: &nacre_core::settings::Look) -> f32 {
        let d = self.dpi;
        let (x0, cw) = (CONTENT_X * d, CONTENT_W * d);
        let total: f32 = rows.iter().map(|r| Self::row_height(r) * d).sum();
        self.rect(c, x0, y0, cw, total, 12.0 * d, col(look.fg, if look.dark { 22 } else { 16 }));
        let mut y = y0;
        let accent = prefs::accent();
        for (i, r) in rows.iter().enumerate() {
            let rh = Self::row_height(r) * d;
            if i > 0 && !matches!(r, RowDef::Radios(..)) {
                self.rect(c, x0 + 16.0 * d, y, cw - 32.0 * d, 1.0, 0.0, col(look.fg, 28));
            }
            let (lx, rx) = (x0 + 16.0 * d, x0 + cw - 16.0 * d);
            match r {
                RowDef::Toggle(label, on, act) => {
                    self.text(c, label, lx, y + (rh - 17.0 * d) / 2.0, 13.5, 400, look.fg, 0.95, 0);
                    let (tw, th) = (46.0 * d, 22.0 * d);
                    let (tx, ty) = (rx - tw, y + (rh - th) / 2.0);
                    self.rect(c, tx, ty, tw, th, th / 2.0, if *on { col(accent, 255) } else { col(look.fg, 60) });
                    let knob = th - 4.0 * d;
                    let pressed = self.press.map_or(false, |(px, py)| px >= x0 && px < x0 + cw && py + self.scroll >= y && py + self.scroll < y + rh && py > VIEW_TOP * d);
                    let kw = if pressed { knob * 1.8 } else { knob * 1.55 };
                    let kx = if *on { tx + tw - kw - 2.0 * d } else { tx + 2.0 * d };
                    if pressed {
                        self.glass_knob(c, kx, ty + 2.0 * d, kw, knob);
                    } else {
                        self.rect(c, kx, ty + 2.0 * d, kw, knob, knob / 2.0, Color { A: 255, R: 255, G: 255, B: 255 });
                    }
                    self.region(x0, y, cw, rh, *act);
                }
                RowDef::Seg(label, opts, sel) => {
                    self.text(c, label, lx, y + (rh - 17.0 * d) / 2.0, 13.5, 400, look.fg, 0.95, 0);
                    let texts: Vec<f32> = opts.iter().map(|(t, _)| measure(t, &Style { px: (12.5 * d).round() as i32, weight: 500 }).0 as f32).collect();
                    let avail = (rx - lx) - measure(label, &Style { px: (13.5 * d).round() as i32, weight: 400 }).0 as f32 - 28.0 * d;
                    let need = |pad: f32, min: f32| texts.iter().map(|w| (w + pad).max(min)).sum::<f32>() + 6.0 * d;
                    let (pad, min) = if need(32.0 * d, 66.0 * d) <= avail { (32.0 * d, 66.0 * d) } else { (18.0 * d, 48.0 * d) };
                    let widths: Vec<f32> = texts.iter().map(|w| (w + pad).max(min)).collect();
                    let total_w: f32 = widths.iter().sum::<f32>() + 6.0 * d;
                    let (bx, bh) = (rx - total_w, 26.0 * d);
                    let by = y + (rh - bh) / 2.0;
                    self.rect(c, bx, by, total_w, bh, bh / 2.0, col(look.fg, 30));
                    let inverse = if look.dark { [10u8, 10, 12] } else { [255u8, 255, 255] };
                    let mut sx = bx + 3.0 * d;
                    for (i, (t, act)) in opts.iter().enumerate() {
                        let on = i == *sel;
                        if on {
                            self.rect(c, sx, by + 3.0 * d, widths[i], bh - 6.0 * d, (bh - 6.0 * d) / 2.0, col(look.fg, 235));
                        }
                        let rgb = if on { inverse } else { look.fg };
                        self.text(c, t, sx + widths[i] / 2.0, by + (bh - 16.0 * d) / 2.0 - 1.0, 12.5, if on { 600 } else { 500 }, rgb, if on { 1.0 } else { 0.85 }, 1);
                        self.region(sx, y, widths[i], rh, *act);
                        sx += widths[i];
                    }
                }
                RowDef::Slider(label, value, id) => {
                    let (min, max, unit) = Self::spec(*id);
                    let frac = ((*value as f32 - min) / (max - min)).clamp(0.0, 1.0);
                    self.text(c, label, lx, y + (rh - 17.0 * d) / 2.0, 13.5, 400, look.fg, 0.95, 0);
                    self.text(c, &format!("{value}{unit}"), rx, y + (rh - 17.0 * d) / 2.0, 12.5, 500, look.fg, 0.6, 2);
                    let (tw, th) = (190.0 * d, 5.0 * d);
                    let tx = rx - 56.0 * d - tw;
                    let ty = y + rh / 2.0 - th / 2.0;
                    self.rect(c, tx, ty, tw, th, th / 2.0, col(look.fg, 50));
                    self.rect(c, tx, ty, (tw * frac).max(th), th, th / 2.0, col(accent, 255));
                    let knob = 18.0 * d;
                    if self.dragging.map(|(i, _, _)| i) == Some(*id) {
                        let (kw, kh) = (knob * 1.9, knob * 1.25);
                        let cx = (tx + tw * frac).clamp(tx + kw / 2.0 - knob / 4.0, tx + tw - kw / 2.0 + knob / 4.0);
                        self.glass_knob(c, cx - kw / 2.0, ty + th / 2.0 - kh / 2.0, kw, kh);
                    } else {
                        let kx = (tx + tw * frac - knob / 2.0).clamp(tx - knob / 4.0, tx + tw - knob * 0.75);
                        self.rect(c, kx - 1.0 * d, ty + th / 2.0 - knob / 2.0, knob + 2.0 * d, knob + 2.0 * d, knob / 2.0 + d, Color { A: 60, R: 0, G: 0, B: 0 });
                        self.rect(c, kx, ty + th / 2.0 - knob / 2.0 - 1.0 * d, knob, knob, knob / 2.0, Color { A: 255, R: 255, G: 255, B: 255 });
                    }
                    self.region(tx - 8.0 * d, y + 4.0 * d, tw + 16.0 * d, rh - 8.0 * d, Action::Slider(*id, tx, tw));
                }
                RowDef::Swatches(label, sel) => {
                    self.text(c, label, lx, y + 13.0 * d, 13.5, 400, look.fg, 0.95, 0);
                    let (dia, gap) = (19.0 * d, 10.0 * d);
                    let n = nacre_core::settings::ACCENTS.len() + 1;
                    let total_w = n as f32 * (dia + gap) - gap;
                    let mut sx = rx - total_w;
                    let cy = y + 12.0 * d;
                    for i in 0..n {
                        if i == 0 {
                            let key = format!("multi|{}", dia.round());
                            let px = dia.round() as u32;
                            if let Some(s) = self.thumb_surface(key, px, px, || nacre_core::preview::multicolor(px)) {
                                self.sprite(c, &s, sx, cy, dia, dia);
                            }
                        } else {
                            self.rect(c, sx, cy, dia, dia, dia / 2.0, col(nacre_core::settings::ACCENTS[i - 1], 255));
                        }
                        if i == *sel {
                            self.rect(c, sx + dia * 0.32, cy + dia * 0.32, dia * 0.36, dia * 0.36, dia * 0.18, Color { A: 255, R: 255, G: 255, B: 255 });
                            self.text(c, prefs::tr(ACCENT_NAMES[i]), sx + dia / 2.0, cy + dia + 5.0 * d, 10.5, 500, look.fg, 0.7, 1);
                        }
                        let v = if i == 0 { nacre_core::settings::ACCENT_MULTI } else { (i - 1) as u8 };
                        self.region(sx - 2.0 * d, y, dia + 4.0 * d, rh, Action::Accent(v));
                        sx += dia + gap;
                    }
                }
                RowDef::Thumbs(label, opts, sel) => {
                    self.text(c, label, lx, y + 14.0 * d, 13.5, 400, look.fg, 0.95, 0);
                    let icon_kind = matches!(opts.first().map(|o| o.1), Some(Thumb::Icon(_)));
                    let (tw, th) = if icon_kind { (48.0 * d, 48.0 * d) } else { (70.0 * d, 46.0 * d) };
                    let gap = 16.0 * d;
                    let total = opts.len() as f32 * tw + (opts.len() as f32 - 1.0) * gap;
                    let mut x = rx - total;
                    let ty = y + 12.0 * d;
                    let dark_icons = prefs::icon_dark();
                    let s0 = prefs::get();
                    for (i, (name, kind, act)) in opts.iter().enumerate() {
                        let on = i == *sel;
                        let r = if icon_kind { tw * 0.22 } else { th * 0.14 };
                        if on {
                            self.rect(c, x - 3.0 * d, ty - 3.0 * d, tw + 6.0 * d, th + 6.0 * d, r + 3.0 * d, col(accent, 255));
                            self.rect(c, x - 1.0 * d, ty - 1.0 * d, tw + 2.0 * d, th + 2.0 * d, r + 1.0 * d, col(if look.dark { [30, 30, 34] } else { [246, 246, 248] }, 255));
                        }
                        let (pw, ph) = (tw.round() as u32, th.round() as u32);
                        let surf = match *kind {
                            Thumb::Appear(k) => self.thumb_surface(format!("appear|{k}|{pw}x{ph}"), pw, ph, || nacre_core::preview::appearance(k, pw, ph)),
                            Thumb::Icon(st) => {
                                let tint = s0.icon_tint;
                                self.thumb_surface(format!("icon|{st:?}|{tint}|{dark_icons}|{pw}"), pw, pw, || nacre_core::preview::icon_tile(st, tint, dark_icons, pw))
                            }
                        };
                        if let Some(sf) = surf {
                            self.sprite(c, &sf, x, ty, tw, th);
                        }
                        self.text(c, name, x + tw / 2.0, ty + th + 6.0 * d, 11.5, if on { 600 } else { 400 }, look.fg, if on { 1.0 } else { 0.7 }, 1);
                        self.region(x - 4.0 * d, y, tw + 8.0 * d, rh, *act);
                        x += tw + gap;
                    }
                }
                RowDef::Radios(opts, sel) => {
                    let rpx = (12.5 * d).round() as i32;
                    let dia = 14.0 * d;
                    let widths: Vec<f32> = opts.iter().map(|(t, _)| dia + 6.0 * d + measure(t, &Style { px: rpx, weight: 400 }).0 as f32 + 16.0 * d).collect();
                    let mut x = rx - widths.iter().sum::<f32>() + 16.0 * d;
                    let cy = y + rh / 2.0 - 4.0 * d;
                    for (i, (t, act)) in opts.iter().enumerate() {
                        let on = i == *sel;
                        if on {
                            self.rect(c, x, cy - dia / 2.0, dia, dia, dia / 2.0, col(accent, 255));
                            self.rect(c, x + dia * 0.32, cy - dia * 0.18, dia * 0.36, dia * 0.36, dia * 0.18, Color { A: 255, R: 255, G: 255, B: 255 });
                        } else {
                            self.rect(c, x, cy - dia / 2.0, dia, dia, dia / 2.0, col(look.fg, 70));
                            self.rect(c, x + 1.2 * d, cy - dia / 2.0 + 1.2 * d, dia - 2.4 * d, dia - 2.4 * d, dia / 2.0 - 1.2 * d, col(if look.dark { [44, 44, 48] } else { [244, 244, 246] }, 255));
                        }
                        self.text(c, t, x + dia + 6.0 * d, cy - 8.0 * d, 12.5, 400, look.fg, 0.9, 0);
                        self.region(x - 4.0 * d, y, widths[i], rh, *act);
                        x += widths[i];
                    }
                }
                RowDef::Text(label, value, hint, field) => {
                    self.text(c, label, lx, y + (rh - 17.0 * d) / 2.0, 13.5, 400, look.fg, 0.95, 0);
                    let editing = self.edit == Some(*field);
                    let (bw, bh) = (240.0 * d, 26.0 * d);
                    let (bx, by) = (rx - bw, y + (rh - bh) / 2.0);
                    self.rect(c, bx, by, bw, bh, bh / 2.0, col(look.fg, if editing { 40 } else { 26 }));
                    if editing {
                        self.rect(c, bx, by, bw, bh, bh / 2.0, col(accent, 40));
                    }
                    let (shown, op) = if editing {
                        (format!("{}|", self.edit_buf), 1.0)
                    } else if value.is_empty() {
                        (hint.clone(), 0.45)
                    } else {
                        (value.clone(), 0.95)
                    };
                    self.text(c, &shown, bx + 12.0 * d, by + (bh - 16.0 * d) / 2.0 - 1.0, 12.5, 500, look.fg, op, 0);
                    self.region(bx, y, bw, rh, Action::EditText(*field));
                }
                RowDef::Pick(label, value, act) => {
                    self.text(c, label, lx, y + (rh - 17.0 * d) / 2.0, 13.5, 400, look.fg, 0.95, 0);
                    let vw = (measure(value, &Style { px: (12.5 * d).round() as i32, weight: 500 }).0 as f32 + 44.0 * d).max(96.0 * d);
                    let (bx, bh) = (rx - vw, 26.0 * d);
                    let by = y + (rh - bh) / 2.0;
                    self.rect(c, bx, by, vw, bh, bh / 2.0, col(look.fg, 30));
                    self.text(c, value, bx + 14.0 * d, by + (bh - 16.0 * d) / 2.0 - 1.0, 12.5, 500, look.fg, 1.0, 0);
                    self.text(c, "\u{2303}", bx + vw - 14.0 * d, by + (bh - 16.0 * d) / 2.0, 10.0, 600, look.fg, 0.6, 1);
                    self.region(x0, y, cw, rh, *act);
                }
            }
            y += rh;
        }
        total
    }

    fn page(&self, page: u8) -> Vec<(Option<&'static str>, Vec<RowDef>)> {
        let s = prefs::get();
        let behaviors = |target: u8| -> Vec<(&'static str, Action)> {
            [(prefs::tr("Tetap"), Behavior::Fixed), (prefs::tr("Menimpa"), Behavior::Overlay), (prefs::tr("Pintar"), Behavior::Smart)]
                .into_iter()
                .map(|(t, b)| {
                    let action = match target {
                        0 => Action::Dock(b),
                        1 => Action::Notch(b),
                        _ => Action::Rail(b),
                    };
                    (t, action)
                })
                .collect()
        };
        let beh_idx = |b: Behavior| match b {
            Behavior::Fixed => 0,
            Behavior::Overlay => 1,
            Behavior::Smart => 2,
        };
        let monitor_choices = |component: u8, current: u8| {
            let count = crate::display::monitors().len().clamp(1, 8);
            let mut options: Vec<(&'static str, Action)> = (0..count).map(|i| {
                let label = match i {
                    0 => prefs::tr("Layar Utama"), 1 => prefs::tr("Layar 2"), 2 => prefs::tr("Layar 3"),
                    3 => prefs::tr("Layar 4"), 4 => prefs::tr("Layar 5"), 5 => prefs::tr("Layar 6"),
                    6 => prefs::tr("Layar 7"), _ => prefs::tr("Layar 8"),
                };
                (label, Action::Monitor(component, i as u8))
            }).collect();
            let selected = if current == u8::MAX { count } else { (current as usize).min(count - 1) };
            options.push((prefs::tr("Semua layar"), Action::Monitor(component, u8::MAX)));
            (options, selected)
        };
        match page {
            0 => vec![
                (
                    None,
                    vec![RowDef::Seg(
                        prefs::tr("Bahasa"),
                        vec![("Indonesia", Action::Lang(Lang::Id)), ("English", Action::Lang(Lang::En)), ("Español", Action::Lang(Lang::Es))],
                        match s.lang { Lang::Id => 0, Lang::En => 1, Lang::Es => 2 },
                    )],
                ),
                (
                    Some(prefs::tr("Sistem")),
                    vec![
                        RowDef::Toggle(prefs::tr("Jalankan saat login Windows"), s.autostart, Action::Autostart),
                        RowDef::Toggle(prefs::tr("Sembunyikan taskbar Windows"), s.takeover, Action::Takeover),
                        RowDef::Toggle(prefs::tr("Sembunyikan ikon desktop saat ABRA berjalan"), s.hide_desktop_icons, Action::HideIcons),
                        RowDef::Toggle(prefs::tr("Selalu di atas jendela layar penuh"), s.above_fullscreen, Action::AboveFull),
                        RowDef::Toggle(prefs::tr("Pintasan cepat (Alt+` ganti jendela app)"), s.mac_shortcuts, Action::MacKeys),
                    ],
                ),
                (
                    Some(prefs::tr("Jendela")),
                    vec![
                        RowDef::Toggle(prefs::tr("Animasi jendela (minimize, tutup, buka)"), s.anim, Action::Anim),
                        RowDef::Toggle(prefs::tr("Bayangan halus di bawah jendela aktif"), s.win_shadow, Action::WinShadow),
                    ],
                ),
                (
                    Some(prefs::tr("Tentang")),
                    {
                        use crate::updater::Phase;
                        let ver: &'static str = match s.lang {
                            Lang::Id => concat!("Versi ", env!("CARGO_PKG_VERSION"), " \u{00B7} Early Access"),
                            Lang::En => concat!("Version ", env!("CARGO_PKG_VERSION"), " \u{00B7} Early Access"),
                            Lang::Es => concat!("Versión ", env!("CARGO_PKG_VERSION"), " \u{00B7} Acceso anticipado"),
                        };
                        let button = match crate::updater::phase() {
                            Phase::Idle => prefs::tr("Periksa pembaruan").to_string(),
                            Phase::Checking => prefs::tr("Memeriksa\u{2026}").to_string(),
                            Phase::Latest => prefs::tr("Sudah terbaru").to_string(),
                            Phase::Available(r) if crate::updater::installed() => format!("{} {}", prefs::tr("Perbarui ke"), r.version),
                            Phase::Available(r) => format!("{} {}", prefs::tr("Unduh"), r.version),
                            Phase::Downloading(v) => format!("{} {v}\u{2026}", prefs::tr("Mengunduh")),
                            Phase::Failed(e) => format!("{} \u{00B7} {e}", prefs::tr("Coba lagi")),
                        };
                        vec![
                            RowDef::Pick(ver, button, Action::UpdateAct),
                            RowDef::Toggle(prefs::tr("Periksa pembaruan otomatis"), s.update_check, Action::UpdateCheck),
                            RowDef::Pick(prefs::tr("Catatan rilis"), prefs::tr("Buka").to_string(), Action::UpdateNotes),
                            RowDef::Pick(prefs::tr("Dibuat oleh kreativa.dev"), prefs::tr("Belikan aku kopi").to_string(), Action::Donate),
                        ]
                    },
                ),
            ],
            1 => vec![
                (
                    None,
                    vec![
                        RowDef::Toggle(prefs::tr("Aktifkan Deck"), s.dock_enabled, Action::DockOn),
                        RowDef::Seg(prefs::tr("Perilaku"), behaviors(0), beh_idx(s.dock)),
                        { let (o, i) = monitor_choices(0, s.dock_monitor); RowDef::Seg(prefs::tr("Layar"), o, i) },
                        RowDef::Seg(prefs::tr("Efek minimize"), vec![(prefs::tr("Funnel"), Action::MinEffect(0)), (prefs::tr("Skala"), Action::MinEffect(1))], s.min_effect as usize),
                        RowDef::Toggle(prefs::tr("Ikon seragam (squircle)"), s.dock_plates, Action::DockPlates),
                        RowDef::Toggle(prefs::tr("Tumpukan Unduhan"), s.downloads_stack, Action::DownloadsStack),
                        RowDef::Toggle(prefs::tr("App terakhir dipakai"), s.recent_apps, Action::RecentApps),
                    ],
                ),
                (
                    Some(prefs::tr("Ukuran")),
                    vec![
                        RowDef::Slider(prefs::tr("Ukuran ikon"), s.dock_size, 4),
                        RowDef::Slider(prefs::tr("Jarak antar ikon"), s.dock_gap, 5),
                        RowDef::Slider(prefs::tr("Zoom saat hover"), s.dock_zoom, 6),
                        RowDef::Slider(prefs::tr("Jarak dari tepi bawah"), s.dock_margin, 7),
                        RowDef::Slider(prefs::tr("Kebulatan sudut"), s.dock_round, 2),
                    ],
                ),
            ],
            2 => vec![(
                None,
                vec![
                    RowDef::Toggle(prefs::tr("Aktifkan Visor"), s.notch_enabled, Action::NotchOn),
                    RowDef::Seg(prefs::tr("Bentuk"), vec![(prefs::tr("Visor"), Action::NotchShape(false)), (prefs::tr("Capsule"), Action::NotchShape(true))], if s.notch_island { 1 } else { 0 }),
                    RowDef::Seg(prefs::tr("Perilaku"), behaviors(1), beh_idx(s.notch)),
                    RowDef::Slider(prefs::tr("Ukuran"), s.notch_scale, 12),
                    { let (o, i) = monitor_choices(1, s.notch_monitor); RowDef::Seg(prefs::tr("Layar"), o, i) },
                    RowDef::Slider(prefs::tr("Kebulatan sudut"), s.notch_round, 3),
                ]
                .into_iter()
                .chain(s.notch_island.then(|| RowDef::Slider(prefs::tr("Jarak dari tepi atas"), s.island_gap, 13)))
                .chain(std::iter::once(RowDef::Toggle(prefs::tr("Buka otomatis saat kursor di Visor"), s.notch_hover_open, Action::HoverOpen)))
                .collect(),
            ), (
                Some(prefs::tr("Saat Visor diam")),
                {
                    let name = if s.greet_name.is_empty() { user_name() } else { s.greet_name.as_str().to_string() };
                    let hour = nacre_platform::time::local_now().hour;
                    let auto = nacre_core::notch::glance::greeting_text("", &name, hour, s.lang);
                    vec![
                        RowDef::Seg(
                            prefs::tr("Tampilkan"),
                            vec![(prefs::tr("Sapaan & info"), Action::NotchIdle(0)), (prefs::tr("Musik & lirik"), Action::NotchIdle(2)), (prefs::tr("Kosong"), Action::NotchIdle(1))],
                            match s.notch_idle {
                                0 => 0,
                                2 => 1,
                                _ => 2,
                            },
                        ),
                        RowDef::Text(prefs::tr("Nama panggilan"), s.greet_name.as_str().to_string(), user_name(), 0),
                        RowDef::Text(prefs::tr("Teks sapaan"), s.greet_text.as_str().to_string(), auto, 1),
                    ]
                },
            ), (
                Some(prefs::tr("Lirik")),
                vec![
                    RowDef::Toggle(prefs::tr("Tampilkan lirik di Visor"), s.lyrics_compact, Action::LyricsCompact),
                    RowDef::Toggle(prefs::tr("Cari lirik online (LRCLIB)"), s.lyrics_online, Action::LyricsOnline),
                    RowDef::Pick(prefs::tr("Folder lirik (.lrc)"), prefs::tr("Buka").to_string(), Action::LyricsDir),
                ],
            ), (
                Some(prefs::tr("Aktivitas")),
                vec![
                    RowDef::Toggle(prefs::tr("Tampilkan unduhan (Store, Steam, peramban)"), s.notch_downloads, Action::NotchDownloads),
                    RowDef::Toggle(prefs::tr("Kirim ke HP lewat QR (Wi-Fi yang sama)"), s.notch_send, Action::NotchSend),
                ],
            ), (
                Some(prefs::tr("Pill & Konteks")),
                vec![
                    RowDef::Toggle(prefs::tr("Pengingat tugas"), s.pill_reminders, Action::PillReminders),
                    RowDef::Toggle(prefs::tr("Shortcut Ctrl+Alt+N untuk menambah tugas"), s.todo_hotkey, Action::TodoHotkey),
                    RowDef::Toggle(prefs::tr("Perangkat Bluetooth & flashdisk tersambung"), s.pill_devices, Action::PillDevices),
                    RowDef::Toggle(prefs::tr("Aurora warna album saat musik main"), s.notch_glow, Action::NotchGlow),
                ],
            )],
            3 => {
                let cur = if s.menubar_material {
                    3
                } else if !s.menubar_glass {
                    0
                } else {
                    match s.menubar_style {
                        nacre_core::settings::BarStyle::Light => 1,
                        nacre_core::settings::BarStyle::Dark => 2,
                    }
                };
                vec![(
                    None,
                    vec![
                        RowDef::Toggle(prefs::tr("Aktifkan Rail"), s.menubar, Action::MenuBarOn),
                        { let (o, i) = monitor_choices(2, s.menubar_monitor); RowDef::Seg(prefs::tr("Layar"), o, i) },
                        RowDef::Seg(
                            prefs::tr("Perilaku"),
                            [(prefs::tr("Tetap"), Behavior::Fixed), (prefs::tr("Menimpa"), Behavior::Overlay), (prefs::tr("Pintar"), Behavior::Smart)]
                                .into_iter()
                                .map(|(t, b)| (t, Action::Rail(b)))
                                .collect(),
                            beh_idx(s.rail),
                        ),
                        RowDef::Seg(
                            prefs::tr("Latar Rail"),
                            vec![(prefs::tr("Transparan"), Action::BarStyle(0)), (prefs::tr("Terang"), Action::BarStyle(1)), (prefs::tr("Gelap"), Action::BarStyle(2)), (prefs::tr("Material"), Action::BarStyle(3))],
                            cur,
                        ),
                        RowDef::Toggle(prefs::tr("Jam 24 jam"), s.clock_24h, Action::Clock24),
                        RowDef::Seg(
                            prefs::tr("Persentase baterai"),
                            vec![(prefs::tr("Di dalam"), Action::BatteryPct(1)), (prefs::tr("Di luar"), Action::BatteryPct(2)), (prefs::tr("Sembunyi"), Action::BatteryPct(0))],
                            match s.battery_pct {
                                1 => 0,
                                2 => 1,
                                _ => 2,
                            },
                        ),
                    ],
                ), (
                    Some(prefs::tr("Logo Rail")),
                    {
                        let custom = crate::menulogo::path().is_some();
                        let mut rows = vec![RowDef::Pick(
                            prefs::tr("Logo Rail"),
                            prefs::tr(if custom { "Gambar sendiri" } else { "Lingkaran bawaan" }).to_string(),
                            Action::MenuLogoPick,
                        )];
                        if custom {
                            rows.push(RowDef::Pick(prefs::tr("Logo kustom"), prefs::tr("Kembalikan bawaan").to_string(), Action::MenuLogoReset));
                        }
                        rows
                    },
                )]
            }
            4 => {
                use nacre_core::settings::IconStyle;
                let theme_idx = match s.theme {
                    Theme::Auto => 0,
                    Theme::Light => 1,
                    Theme::Dark => 2,
                };
                let mat_idx = |m: Material| Material::ALL.iter().position(|x| *x == m).unwrap_or(0);
                let mat_opts = |dock: bool| vec![(prefs::tr("Blur"), Action::Material(dock, Material::Blur)), (prefs::tr("Crystal"), Action::Material(dock, Material::Glass)), (prefs::tr("Hitam buram"), Action::Material(dock, Material::Black))];
                let ist = match s.icon_style {
                    IconStyle::Default => 0,
                    IconStyle::Dark => 1,
                    IconStyle::Clear => 2,
                    IconStyle::Tinted => 3,
                };
                let accent_idx = if s.accent == nacre_core::settings::ACCENT_MULTI { 0 } else { s.accent as usize + 1 };
                let mut theme = vec![
                    RowDef::Swatches(prefs::tr("Warna"), accent_idx),
                    RowDef::Thumbs(
                        prefs::tr("Gaya ikon & widget"),
                        vec![
                            (prefs::tr("Bawaan"), Thumb::Icon(IconStyle::Default), Action::IconStyle(0)),
                            (prefs::tr("Gelap"), Thumb::Icon(IconStyle::Dark), Action::IconStyle(1)),
                            (prefs::tr("Bening"), Thumb::Icon(IconStyle::Clear), Action::IconStyle(2)),
                            (prefs::tr("Berwarna"), Thumb::Icon(IconStyle::Tinted), Action::IconStyle(3)),
                        ],
                        ist,
                    ),
                ];
                if s.icon_style != IconStyle::Default {
                    let v = match s.icon_variant {
                        1 => 0,
                        2 => 1,
                        _ => 2,
                    };
                    theme.push(RowDef::Radios(vec![(prefs::tr("Terang"), Action::IconVariant(1)), (prefs::tr("Gelap"), Action::IconVariant(2)), (prefs::tr("Otomatis"), Action::IconVariant(0))], v));
                }
                if s.icon_style == IconStyle::Tinted {
                    theme.push(RowDef::Seg(
                        prefs::tr("Warna tint"),
                        vec![(prefs::tr("Biru"), Action::IconTint(0)), (prefs::tr("Ungu"), Action::IconTint(1)), (prefs::tr("Merah Muda"), Action::IconTint(2)), (prefs::tr("Oranye"), Action::IconTint(3)), (prefs::tr("Hijau"), Action::IconTint(4))],
                        s.icon_tint as usize,
                    ));
                }
                theme.push(RowDef::Toggle(prefs::tr("Paket ikon (folder iconpack)"), s.icon_pack, Action::IconPack));
                let scale_idx = match s.ui_scale {
                    0..=112 => 0,
                    113..=137 => 1,
                    _ => 2,
                };
                let mut glass_rows = vec![
                    RowDef::Seg(prefs::tr("Material"), mat_opts(true), mat_idx(s.dock_material)),
                    RowDef::Seg(prefs::tr("Material Visor"), mat_opts(false), mat_idx(s.notch_material)),
                    RowDef::Slider(prefs::tr("Tingkat blur"), s.blur, 0),
                    RowDef::Slider(prefs::tr("Transparansi"), s.transparency, 1),
                    RowDef::Toggle(prefs::tr("Refraksi tepi kaca (Deck)"), s.refraction, Action::Refraction),
                ];
                {
                    let c = sys::conditions();
                    if !c.transparency {
                        glass_rows.insert(0, RowDef::Pick(prefs::tr("Efek transparansi Windows mati, Deck tampil solid"), prefs::tr("Nyalakan").to_string(), Action::WinColors));
                    } else if c.battery_saver {
                        glass_rows.insert(0, RowDef::Pick(prefs::tr("Penghemat baterai aktif, Deck tampil solid"), prefs::tr("Buka").to_string(), Action::WinPower));
                    }
                }
                vec![
                    (
                        None,
                        vec![RowDef::Seg(
                            prefs::tr("Skala Deck, Visor, Rail & Quick Panel"),
                            vec![("100%", Action::UiScale(100)), ("125%", Action::UiScale(125)), ("150%", Action::UiScale(150))],
                            scale_idx,
                        )],
                    ),
                    (
                        None,
                        vec![RowDef::Thumbs(
                            prefs::tr("Tampilan"),
                            vec![
                                (prefs::tr("Otomatis"), Thumb::Appear(0), Action::Theme(Theme::Auto)),
                                (prefs::tr("Terang"), Thumb::Appear(1), Action::Theme(Theme::Light)),
                                (prefs::tr("Gelap"), Thumb::Appear(2), Action::Theme(Theme::Dark)),
                            ],
                            theme_idx,
                        )],
                    ),
                    (Some(prefs::tr("Tema")), theme),
                    (Some(prefs::tr("Kaca")), glass_rows),
                ]
            }
            5 => vec![
                (
                    None,
                    vec![
                        RowDef::Toggle(prefs::tr("Aktifkan Scene"), s.stage, Action::StageOn),
                        RowDef::Toggle(prefs::tr("Ganti Alt+Tab dengan pengalih ABRA"), s.switcher, Action::SwitcherOn),
                        RowDef::Toggle(prefs::tr("Klik wallpaper menyimpan semua jendela"), s.stage_desktop_click, Action::StageFlag(1)),
                        RowDef::Toggle(prefs::tr("Judul jendela saat hover"), s.stage_titles, Action::StageFlag(3)),
                        RowDef::Toggle(prefs::tr("Jeda media app di latar"), s.stage_media, Action::StageFlag(2)),
                    ],
                ),
                (
                    Some(prefs::tr("Daftar jendela")),
                    vec![
                        RowDef::Seg(prefs::tr("Posisi daftar jendela"), vec![(prefs::tr("Kiri"), Action::StageRight(false)), (prefs::tr("Kanan"), Action::StageRight(true))], if s.stage_right { 1 } else { 0 }),
                        RowDef::Seg(
                            prefs::tr("Ikon desktop"),
                            vec![(prefs::tr("Sembunyi"), Action::StageIcons(0)), (prefs::tr("Otomatis"), Action::StageIcons(1)), (prefs::tr("Tampil"), Action::StageIcons(2))],
                            s.stage_icons as usize,
                        ),
                        RowDef::Slider(prefs::tr("Ukuran kartu"), s.stage_size, 8),
                        RowDef::Slider(prefs::tr("Jarak antar kartu"), s.stage_gap, 9),
                        RowDef::Slider(prefs::tr("Jumlah kartu"), s.stage_count, 10),
                        RowDef::Slider(prefs::tr("Kecepatan animasi"), s.stage_speed, 11),
                    ],
                ),
            ],
            6 => {
                let names = ["Kiri atas", "Kanan atas", "Kiri bawah", "Kanan bawah"];
                vec![(None, names.iter().enumerate().map(|(i, nm)| RowDef::Pick(prefs::tr(nm), prefs::tr(hot_label(s.hot[i])).to_string(), Action::HotCycle(i))).collect())]
            }
            8 => vec![(
                None,
                vec![
                    RowDef::Seg(prefs::tr("Material widget"), vec![(prefs::tr("Original"), Action::WidgetStyle(0)), (prefs::tr("Crystal"), Action::WidgetStyle(1))], s.widget_style.min(1) as usize),
                    RowDef::Pick(prefs::tr("Widget desktop"), prefs::tr("Tambah widget").to_string(), Action::WidgetGallery),
                ],
            )],
            _ => vec![(None, vec![RowDef::Toggle(prefs::tr("Riwayat Papan Klip di Quick Search"), s.clipboard_history, Action::ClipHistory)])],
        }
    }

    fn render(&mut self) {
        let content = self.popup.content.clone();
        if let Ok(k) = content.Children() {
            let _ = k.RemoveAll();
        }
        self.regions.clear();
        let d = self.dpi;
        let look = prefs::look(Role::Themed);
        let c = &content;

        for (i, rgb) in [[255u8, 95, 87], [254, 188, 46], [40, 200, 64]].iter().enumerate() {
            let (cx, cy) = ((26.0 + i as f32 * 20.0) * d, 27.0 * d);
            self.rect(c, cx - 6.0 * d, cy - 6.0 * d, 12.0 * d, 12.0 * d, 6.0 * d, col(*rgb, 255));
        }
        self.region_fixed(18.0 * d, 17.0 * d, 20.0 * d, 20.0 * d, Action::Close);
        let (bx, by, bs) = close_button(d);
        self.rect(c, bx, by, bs, bs, bs / 2.0, col(look.fg, 22));
        self.glyph_icon(c, "\u{E711}", bx + bs / 2.0, by + bs / 2.0, 11.0, look.fg);
        self.region_fixed(bx, by, bs, bs, Action::Close);

        let (fx, fy, fw, fh) = (16.0 * d, 48.0 * d, (SIDEBAR_W - 32.0) * d, 30.0 * d);
        self.rect(c, fx, fy, fw, fh, 9.0 * d, col(look.fg, if self.search_focus { 36 } else { 22 }));
        self.glyph_icon(c, "\u{E721}", fx + 15.0 * d, fy + fh / 2.0, 11.0, look.fg);
        let (shown, op) = if self.search.is_empty() && !self.search_focus {
            (prefs::tr("Cari").to_string(), 0.45)
        } else {
            (format!("{}{}", self.search, if self.search_focus { "|" } else { "" }), 0.95)
        };
        self.text(c, &shown, fx + 29.0 * d, fy + (fh - 17.0 * d) / 2.0, 13.0, 400, look.fg, op, 0);
        self.region_fixed(fx, fy, fw, fh, Action::Search);

        let (px0, py0, av) = (20.0 * d, 90.0 * d, 38.0 * d);
        self.rect(c, px0, py0, av, av, av / 2.0, crate::prefs::accent_c(255));
        let user = user_name();
        let initial: String = user.chars().take(1).collect::<String>().to_uppercase();
        self.text(c, &initial, px0 + av / 2.0, py0 + (av - 20.0 * d) / 2.0, 15.0, 600, [255, 255, 255], 1.0, 1);
        self.text(c, &user, px0 + av + 10.0 * d, py0 + 2.0 * d, 13.5, 600, look.fg, 0.95, 0);
        self.text(c, prefs::tr("Akun Windows"), px0 + av + 10.0 * d, py0 + 20.0 * d, 11.0, 400, look.fg, 0.6, 0);

        let pages: [(&'static str, &'static str, [u8; 3], u8); 9] = [
            (prefs::tr("Umum"), "\u{E713}", [142, 142, 147], 0),
            (prefs::tr("Deck"), "\u{E7C4}", [10, 132, 255], 1),
            (prefs::tr("Visor"), "\u{E7F4}", [58, 58, 60], 2),
            (prefs::tr("Rail"), "\u{E700}", [90, 200, 250], 3),
            (prefs::tr("Tampilan"), "\u{E790}", [88, 86, 214], 4),
            (prefs::tr("Scene"), "\u{E8A0}", [48, 209, 88], 5),
            (prefs::tr("Screen Corners"), "\u{E73F}", [175, 82, 222], 6),
            (prefs::tr("Quick Search"), "\u{E721}", [255, 149, 10], 7),
            (prefs::tr("Widget"), "\u{E737}", [0, 122, 255], 8),
        ];
        let mut sy = 140.0 * d;
        self.side.clear();
        let (view_top, view_bot) = (134.0 * d, (H - 96.0) * d);
        let side_body = match (self.comp.CreateContainerVisual(), self.comp.CreateContainerVisual()) {
            (Ok(v), Ok(b)) => {
                let _ = v.SetOffset(Vector3 { X: 0.0, Y: view_top, Z: 0.0 });
                let _ = v.SetSize(Vector2 { X: SIDEBAR_W * d, Y: view_bot - view_top });
                if let Ok(clip) = self.comp.CreateInsetClip() {
                    let _ = v.SetClip(&clip);
                }
                let _ = b.SetOffset(Vector3 { X: 0.0, Y: -view_top - self.side_scroll, Z: 0.0 });
                let _ = c.Children().and_then(|k| k.InsertAtTop(&v));
                let _ = v.Children().and_then(|k| k.InsertAtTop(&b));
                b
            }
            _ => c.clone(),
        };
        let parent = &side_body;
        for (name, glyph, color, idx) in pages {
            if !self.page_matches(idx, name) {
                continue;
            }
            let sel = idx == self.tab;
            let (rx, rw, rh) = (16.0 * d, (SIDEBAR_W - 32.0) * d, 36.0 * d);
            let Ok(ic) = self.comp.CreateContainerVisual() else { continue };
            let _ = parent.Children().and_then(|k| k.InsertAtTop(&ic));
            if self.side_hover == Some(idx) {
                let _ = ic.SetCenterPoint(Vector3 { X: rx + rw / 2.0, Y: sy + rh / 2.0, Z: 0.0 });
                let _ = ic.SetScale(Vector3 { X: crate::motion::HOVER_ROW, Y: crate::motion::HOVER_ROW, Z: 1.0 });
            }
            self.side.push((idx, (rx, sy, rw, rh), ic.clone()));
            let c = &ic;
            if sel {
                self.rect(c, rx, sy, rw, rh, 10.0 * d, crate::prefs::accent_c(235));
            }
            self.rect(c, rx + 8.0 * d, sy + 6.0 * d, 24.0 * d, 24.0 * d, 7.0 * d, col(color, 255));
            self.glyph_icon(c, glyph, rx + 20.0 * d, sy + 18.0 * d, 13.0, [255, 255, 255]);
            self.text(c, name, rx + 42.0 * d, sy + (rh - 17.0 * d) / 2.0, 13.5, if sel { 600 } else { 400 }, if sel { [255, 255, 255] } else { look.fg }, 1.0, 0);
            let vis_top = (sy - self.side_scroll).max(view_top);
            let vis_bot = (sy - self.side_scroll + rh).min(view_bot);
            if vis_bot > vis_top {
                self.region_fixed(rx, vis_top, rw, vis_bot - vis_top, Action::Tab(idx));
            }
            sy += 40.0 * d;
        }
        let side_max = (sy - 4.0 * d - view_bot).max(0.0);
        self.side_view = (view_top, side_max);
        if self.side_scroll > side_max {
            self.side_scroll = side_max;
        }
        self.side_bar = None;
        if side_max > 0.5 {
            let (track_top, track_h) = (view_top + 4.0 * d, view_bot - view_top - 8.0 * d);
            let content = view_bot - view_top + side_max;
            let th = (track_h * (view_bot - view_top) / content).max(28.0 * d);
            let ty = track_top + (track_h - th) * (self.side_scroll / side_max);
            let bw = if self.side_drag.is_some() { 7.0 * d } else { 5.0 * d };
            let bx = (SIDEBAR_W - SIDE_INSET - 5.0) * d - bw;
            self.rect(c, bx, ty, bw, th, bw / 2.0, col(look.fg, if self.side_drag.is_some() { 110 } else { 70 }));
            self.side_bar = Some(((bx - 6.0 * d, track_top, bw + 12.0 * d, track_h), th));
        }
        let (kx, kw) = (20.0 * d, (SIDEBAR_W - 40.0) * d);
        let ky = (H - 84.0) * d;
        self.text(c, prefs::tr("Dibuat oleh kreativa.dev"), kx + kw / 2.0, ky, 11.0, 500, look.fg, 0.55, 1);
        let (kby, kbh) = (ky + 22.0 * d, 32.0 * d);
        self.rect(c, kx, kby, kw, kbh, kbh / 2.0, col([255, 159, 10], 255));
        self.text(c, prefs::tr("Belikan aku kopi"), kx + kw / 2.0, kby + (kbh - 17.0 * d) / 2.0, 13.0, 600, [255, 255, 255], 1.0, 1);
        self.region_fixed(kx, kby, kw, kbh, Action::Donate);

        let (bx, by, bw, bh) = (CONTENT_X * d, 11.0 * d, 64.0 * d, 28.0 * d);
        self.rect(c, bx, by, bw, bh, 14.0 * d, col(look.fg, 24));
        let (can_back, can_fwd) = (self.hpos > 0, self.hpos + 1 < self.history.len());
        let dim = |on: bool| if on { look.fg } else if look.dark { [110, 110, 116] } else { [170, 170, 176] };
        self.glyph_icon(c, "\u{E76B}", bx + bw * 0.27, by + bh / 2.0, 11.0, dim(can_back));
        self.glyph_icon(c, "\u{E76C}", bx + bw * 0.73, by + bh / 2.0, 11.0, dim(can_fwd));
        self.rect(c, bx + bw / 2.0, by + 7.0 * d, 1.0, bh - 14.0 * d, 0.0, col(look.fg, 34));
        self.region_fixed(bx, by, bw / 2.0, bh, Action::Back);
        self.region_fixed(bx + bw / 2.0, by, bw / 2.0, bh, Action::Forward);
        let title = pages.iter().find(|p| p.3 == self.tab).map(|p| p.0).unwrap_or("");
        self.text(c, title, bx + bw + 12.0 * d, by + (bh - 19.0 * d) / 2.0, 15.0, 700, look.fg, 1.0, 0);

        let view = match (self.comp.CreateContainerVisual(), self.comp.CreateContainerVisual()) {
            (Ok(v), Ok(b)) => Some((v, b)),
            _ => None,
        };
        let Some((viewport, body)) = view else { return };
        let (vt, vh) = (VIEW_TOP * d, (H - VIEW_TOP - 10.0) * d);
        let _ = viewport.SetOffset(Vector3 { X: 0.0, Y: vt, Z: 0.0 });
        let _ = viewport.SetSize(Vector2 { X: W * d, Y: vh });
        if let Ok(clip) = self.comp.CreateInsetClip() {
            let _ = clip.SetLeftInset(0.0);
            let _ = clip.SetRightInset(0.0);
            let _ = clip.SetTopInset(0.0);
            let _ = clip.SetBottomInset(0.0);
            let _ = viewport.SetClip(&clip);
        }
        let _ = body.SetOffset(Vector3 { X: 0.0, Y: -self.scroll, Z: 0.0 });
        let _ = c.Children().and_then(|k| k.InsertAtTop(&viewport));
        let _ = viewport.Children().and_then(|k| k.InsertAtTop(&body));

        let mut y = VIEW_TOP * d - vt;
        y += 4.0 * d;
        let groups = self.page(self.tab);
        for (header, rows) in groups {
            if let Some(h) = header {
                self.text_abs(&body, h, CONTENT_X * d + 8.0 * d, y + 2.0 * d, 12.0, 600, look.fg, 0.55);
                y += 24.0 * d;
            }
            let used = self.card_in(&body, y, &rows, &look, vt);
            y += used + 20.0 * d;
        }
        self.content_h = y + 6.0 * d;
        let max_scroll = (self.content_h - vh).max(0.0);
        if self.scroll > max_scroll {
            self.scroll = max_scroll;
            let _ = body.SetOffset(Vector3 { X: 0.0, Y: -self.scroll, Z: 0.0 });
        }
        self.size = (W * d, H * d);
        let (x, yy, ww, hh) = self.popup.rect;
        let (nw, nh) = ((W * d) as i32, (H * d) as i32);
        if (nw, nh) != (ww, hh) {
            let _ = self.popup.set_rect(x, yy, nw, nh);
        }
        self.frame();
    }

    fn frame(&mut self) {
        let d = self.dpi;
        let (w, h) = ((W * d) as i32, (H * d) as i32);
        let (hx, hy, hw, hh) = (SIDE_INSET * d, SIDE_INSET * d, (SIDEBAR_W - 2.0 * SIDE_INSET) * d, (H - 2.0 * SIDE_INSET) * d);
        let _ = self.popup.panel.set_rect(hx, hy, hw, hh);
        let _ = self.popup.panel.set_radius(SIDE_RADIUS * d);
        let dark = prefs::look(Role::Themed).dark;
        if self.frame.is_some() && self.frame_key == (w, h, dark) {
            return;
        }
        let data = frame_pixels(w, h, 24.0 * d, (hx, hy, hw, hh), SIDE_RADIUS * d, dark);
        let go = || -> windows_core::Result<windows::UI::Composition::SpriteVisual> {
            let surf = self.gfx.surface_from_bgra(w as u32, h as u32, &data)?;
            let br = self.comp.CreateSurfaceBrushWithSurface(&surf)?;
            br.SetStretch(CompositionStretch::Fill)?;
            let v = match &self.frame {
                Some(v) => v.clone(),
                None => {
                    let v = self.comp.CreateSpriteVisual()?;
                    self.popup.root().Children()?.InsertAtBottom(&v)?;
                    v
                }
            };
            v.SetBrush(&br)?;
            v.SetSize(Vector2 { X: w as f32, Y: h as f32 })?;
            Ok(v)
        };
        match go() {
            Ok(v) => {
                self.frame = Some(v);
                self.frame_key = (w, h, dark);
            }
            Err(e) => log::line("settings", &format!("bingkai gagal: {e}")),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn text_abs(&mut self, parent: &ContainerVisual, s: &str, x: f32, y: f32, size: f32, weight: i32, rgb: [u8; 3], alpha: f32) {
        self.text(parent, s, x, y, size, weight, rgb, alpha, 0);
    }

    fn card_in(&mut self, body: &ContainerVisual, y: f32, rows: &[RowDef], look: &nacre_core::settings::Look, vt: f32) -> f32 {
        let before = self.regions.len();
        let used = self.card(body, y, rows, look);
        for r in self.regions[before..].iter_mut() {
            r.y += vt;
        }
        used
    }

    fn hit(&self, x: f32, y: f32) -> Option<Action> {
        let d = self.dpi;
        let (vt, vb) = (VIEW_TOP * d, (H - 10.0) * d);
        self.regions
            .iter()
            .rev()
            .find(|r| {
                let yy = if r.scroll {
                    if y < vt || y > vb {
                        return false;
                    }
                    y + self.scroll
                } else {
                    y
                };
                x >= r.x && x < r.x + r.w && yy >= r.y && yy < r.y + r.h
            })
            .map(|r| r.a)
    }

    fn slide(&self, id: u8, x: f32, sx: f32, sw: f32, save: bool) {
        let (min, max, _) = Self::spec(id);
        let v = (min + slider::value_at(x, sx, sw, false) * (max - min)).round() as u8;
        prefs::update_with(save, |s| {
            match id {
                0 => s.blur = v,
                1 => s.transparency = v,
                2 => s.dock_round = v,
                3 => s.notch_round = v,
                4 => s.dock_size = v,
                5 => s.dock_gap = v,
                6 => s.dock_zoom = v,
                8 => s.stage_size = v,
                9 => s.stage_gap = v,
                10 => s.stage_count = v,
                11 => s.stage_speed = v,
                12 => s.notch_scale = v,
                13 => s.island_gap = v,
                _ => s.dock_margin = v,
            }
        });
    }
}

fn hot_label(a: nacre_core::settings::HotAction) -> &'static str {
    use nacre_core::settings::HotAction::*;
    match a {
        None => "Tidak ada",
        Mission => "Overview",
        Launchpad => "Apps",
        Desktop => "Tampilkan Desktop",
        Stage => "Scene",
        Notifications => "Pusat Notifikasi",
        Lock => "Kunci Layar",
    }
}

fn xy(lp: LPARAM) -> (f32, f32) {
    ((lp.0 as i16) as f32, ((lp.0 >> 16) as i16) as f32)
}

fn with_win<R>(f: impl FnOnce(&mut Win) -> R) -> Option<R> {
    WIN.with(|s| s.try_borrow_mut().ok().and_then(|mut b| b.as_mut().map(|w| f(w))))
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_NCHITTEST => {
            let mut r = windows::Win32::Foundation::RECT::default();
            let _ = GetWindowRect(hwnd, &mut r);
            let (px, py) = ((lp.0 as i16) as i32, ((lp.0 >> 16) as i16) as i32);
            let drag = with_win(|w| {
                let d = w.dpi;
                let (bx, by, bs) = close_button(d);
                let (lx, ly) = ((px - r.left) as f32, (py - r.top) as f32);
                if lx >= bx && lx < bx + bs && ly >= by && ly < by + bs {
                    return false;
                }
                py - r.top < (VIEW_TOP * d) as i32 && px - r.left > ((CONTENT_X + 70.0) * d) as i32
            })
            .unwrap_or(false);
            LRESULT(if drag { HTCAPTION } else { HTCLIENT } as isize)
        }
        WM_MOUSEWHEEL => {
            let delta = ((wp.0 >> 16) as i16) as f32;
            with_win(|w| {
                let d = w.dpi;
                let vh = (H - VIEW_TOP - 10.0) * d;
                let mut p = windows::Win32::Foundation::POINT::default();
                let _ = GetCursorPos(&mut p);
                if ((p.x - w.popup.rect.0) as f32) < SIDEBAR_W * d {
                    let max = w.side_view.1;
                    w.side_scroll = (w.side_scroll - delta / 120.0 * 40.0 * d).clamp(0.0, max);
                    w.render();
                    return;
                }
                let max = (w.content_h - vh).max(0.0);
                w.scroll = (w.scroll - delta / 120.0 * 48.0 * d).clamp(0.0, max);
                w.render();
            });
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let (x, y) = xy(lp);
            if GetForegroundWindow() != hwnd {
                let _ = SetForegroundWindow(hwnd);
            }
            let grabbed = with_win(|w| {
                let Some((r, th)) = w.side_bar else { return false };
                if !(x >= r.0 && x < r.0 + r.2 && y >= r.1 && y < r.1 + r.3) {
                    return false;
                }
                let max = w.side_view.1;
                let ty = r.1 + (r.3 - th) * (w.side_scroll / max.max(0.001));
                let grab = if y >= ty && y < ty + th { y - ty } else { th / 2.0 };
                w.side_drag = Some(grab);
                w.side_scroll = (((y - grab - r.1) / (r.3 - th).max(1.0)) * max).clamp(0.0, max);
                w.render();
                true
            })
            .unwrap_or(false);
            if grabbed {
                SetCapture(hwnd);
                return LRESULT(0);
            }
            let started = with_win(|w| match w.hit(x, y) {
                Some(Action::Slider(id, sx, sw)) => {
                    w.dragging = Some((id, sx, sw));
                    w.slide(id, x, sx, sw, false);
                    w.render();
                    true
                }
                Some(_) => {
                    w.press = Some((x, y));
                    w.render();
                    false
                }
                None => false,
            })
            .unwrap_or(false);
            if started {
                let _ = SetCapture(hwnd);
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let (x, y) = xy(lp);
            with_win(|w| {
                if let (Some(grab), Some((r, th))) = (w.side_drag, w.side_bar) {
                    let max = w.side_view.1;
                    w.side_scroll = (((y - grab - r.1) / (r.3 - th).max(1.0)) * max).clamp(0.0, max);
                    w.render();
                    return;
                }
                if let Some((id, sx, sw)) = w.dragging {
                    w.slide(id, x, sx, sw, false);
                    return;
                }
                let mut t = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
                let _ = TrackMouseEvent(&mut t);
                let (top, s) = (w.side_view.0, w.side_scroll);
                let hit = if y < top || y > (H - 96.0) * w.dpi { None } else { w.side.iter().find(|(_, r, _)| x >= r.0 && x < r.0 + r.2 && y + s >= r.1 && y + s < r.1 + r.3).map(|e| e.0) };
                w.hover_side(hit);
            });
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            with_win(|w| w.hover_side(None));
            LRESULT(0)
        }
        WM_CHAR => {
            let ch = wp.0 as u32;
            with_win(|w| {
                if w.edit.is_some() {
                    if ch >= 32 && ch != 127 && w.edit_buf.chars().count() < 60 {
                        if let Some(c) = char::from_u32(ch) {
                            w.edit_buf.push(c);
                            w.render();
                        }
                    }
                    return;
                }
                if w.search_focus && ch >= 32 && ch != 127 {
                    if let Some(c) = char::from_u32(ch) {
                        w.search.push(c);
                        w.render();
                    }
                }
            });
            LRESULT(0)
        }
        WM_KEYDOWN => {
            let vk = wp.0 as u16;
            with_win(|w| {
                if w.edit.is_some() {
                    match vk {
                        0x08 => {
                            w.edit_buf.pop();
                            w.render();
                        }
                        0x1B => {
                            w.edit = None;
                            w.render();
                        }
                        0x0D => w.commit_edit(),
                        _ => {}
                    }
                    return;
                }
                if !w.search_focus {
                    if vk == 0x1B {
                        let _ = PostMessageW(Some(hwnd), WM_CLOSE_SELF, WPARAM(0), LPARAM(0));
                    }
                    return;
                }
                match vk {
                    0x08 => {
                        w.search.pop();
                    }
                    0x1B => {
                        w.search.clear();
                        w.search_focus = false;
                    }
                    0x0D => {
                        if let Some(t) = (0u8..8).find(|i| w.page_matches(*i, "")) {
                            w.go(t, true);
                            return;
                        }
                    }
                    _ => return,
                }
                w.render();
            });
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let (x, y) = xy(lp);
            let _ = ReleaseCapture();
            if with_win(|w| w.side_drag.take().is_some()).unwrap_or(false) {
                with_win(|w| w.render());
                return LRESULT(0);
            }
            let act = with_win(|w| {
                let was_pressed = w.press.take().is_some();
                if let Some((id, sx, sw)) = w.dragging.take() {
                    w.slide(id, x, sx, sw, true);
                    w.render();
                    return None;
                }
                if was_pressed {
                    w.render();
                }
                w.hit(x, y)
            })
            .flatten();
            if !matches!(act, Some(Action::Search)) {
                with_win(|w| {
                    if w.search_focus {
                        w.search_focus = false;
                        w.render();
                    }
                });
            }
            if !matches!(act, Some(Action::EditText(_))) {
                with_win(|w| {
                    if w.edit.is_some() {
                        w.commit_edit();
                    }
                });
            }
            match act {
                Some(Action::EditText(f)) => {
                    with_win(|w| {
                        if w.edit != Some(f) {
                            if w.edit.is_some() {
                                w.commit_edit();
                            }
                            let s = prefs::get();
                            w.edit_buf = if f == 0 { s.greet_name.as_str().to_string() } else { s.greet_text.as_str().to_string() };
                            w.edit = Some(f);
                            w.render();
                        }
                    });
                    let _ = SetForegroundWindow(hwnd);
                    let _ = SetFocus(Some(hwnd));
                }
                Some(Action::NotchIdle(v)) => prefs::update(|s| s.notch_idle = v),
                Some(Action::LyricsCompact) => prefs::update(|s| s.lyrics_compact = !s.lyrics_compact),
                Some(Action::NotchDownloads) => prefs::update(|s| s.notch_downloads = !s.notch_downloads),
                Some(Action::NotchSend) => prefs::update(|s| s.notch_send = !s.notch_send),
                Some(Action::LyricsOnline) => prefs::update(|s| s.lyrics_online = !s.lyrics_online),
                Some(Action::Donate) => nacre_platform::launch::open(DONATE_URL),
                Some(Action::UpdateCheck) => prefs::update(|s| s.update_check = !s.update_check),
                Some(Action::UpdateNotes) => crate::updater::open_release_notes(),
                Some(Action::UpdateAct) => match crate::updater::phase() {
                    crate::updater::Phase::Available(_) => crate::updater::install(),
                    crate::updater::Phase::Downloading(_) => {}
                    _ => crate::updater::check_now(),
                },
                Some(Action::PillReminders) => prefs::update(|s| s.pill_reminders = !s.pill_reminders),
                Some(Action::PillDevices) => prefs::update(|s| s.pill_devices = !s.pill_devices),
                Some(Action::NotchGlow) => prefs::update(|s| s.notch_glow = !s.notch_glow),
                Some(Action::TodoHotkey) => prefs::update(|s| s.todo_hotkey = !s.todo_hotkey),
                Some(Action::MenuLogoPick) => {
                    let _ = SetWindowPos(hwnd, Some(HWND_NOTOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
                    let picked = nacre_platform::pick::open_file(prefs::tr("Pilih gambar"), &[(prefs::tr("Gambar logo"), "*.png;*.jpg;*.jpeg;*.bmp;*.ico;*.gif;*.webp;*.tif;*.tiff")]);
                    let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
                    if let Some(p) = picked {
                        if crate::menulogo::set(&p) {
                            prefs::broadcast();
                        }
                    }
                }
                Some(Action::UiScale(v)) => prefs::update(|s| s.ui_scale = v),
                Some(Action::WidgetStyle(v)) => prefs::update(|s| s.widget_style = v),
                Some(Action::BatteryPct(v)) => prefs::update(|s| s.battery_pct = v),
                Some(Action::WidgetGallery) => crate::widgets::open_gallery(),
                Some(Action::NotchShape(b)) => prefs::update(|s| s.notch_island = b),
                Some(Action::MenuLogoReset) => {
                    crate::menulogo::reset();
                    prefs::broadcast();
                }
                Some(Action::LyricsDir) => {
                    let dir = crate::paths::config_dir().join("lyrics");
                    let _ = std::fs::create_dir_all(&dir);
                    nacre_platform::launch::open(&dir.to_string_lossy());
                }
                Some(Action::WinColors) => nacre_platform::launch::open("ms-settings:personalization-colors"),
                Some(Action::WinPower) => nacre_platform::launch::open("ms-settings:batterysaver"),
                Some(Action::Search) => {
                    with_win(|w| {
                        w.search_focus = true;
                        w.render();
                    });
                    let _ = SetForegroundWindow(hwnd);
                    let _ = SetFocus(Some(hwnd));
                }
                Some(Action::Back) => {
                    with_win(|w| {
                        if w.hpos > 0 {
                            w.hpos -= 1;
                            let t = w.history[w.hpos];
                            w.go(t, false);
                        }
                    });
                }
                Some(Action::Forward) => {
                    with_win(|w| {
                        if w.hpos + 1 < w.history.len() {
                            w.hpos += 1;
                            let t = w.history[w.hpos];
                            w.go(t, false);
                        }
                    });
                }
                Some(Action::IconVariant(v)) => prefs::update(|s| s.icon_variant = v),
                Some(Action::MinEffect(v)) => prefs::update(|s| s.min_effect = v),
                Some(Action::Close) => {
                    let _ = PostMessageW(Some(hwnd), WM_CLOSE_SELF, WPARAM(0), LPARAM(0));
                }
                Some(Action::Takeover) => prefs::update(|s| s.takeover = !s.takeover),
                Some(Action::Dock(b)) => prefs::update(|s| s.dock = b),
                Some(Action::Notch(b)) => prefs::update(|s| s.notch = b),
                Some(Action::Rail(b)) => prefs::update(|s| s.rail = b),
                Some(Action::Monitor(c, target)) => prefs::update(|s| match c { 0 => s.dock_monitor = target, 1 => s.notch_monitor = target, _ => s.menubar_monitor = target }),
                Some(Action::Theme(t)) => prefs::update(|s| s.theme = t),
                Some(Action::Lang(l)) => prefs::update(|s| s.lang = l),
                Some(Action::WinShadow) => prefs::update(|s| s.win_shadow = !s.win_shadow),
                Some(Action::HoverOpen) => prefs::update(|s| s.notch_hover_open = !s.notch_hover_open),
                Some(Action::MenuBarOn) => prefs::update(|s| s.menubar = !s.menubar),
                Some(Action::HotCycle(i)) => prefs::update(|s| s.hot[i] = s.hot[i].next()),
                Some(Action::StageOn) => prefs::update(|s| s.stage = !s.stage),
                Some(Action::SwitcherOn) => prefs::update(|s| s.switcher = !s.switcher),
                Some(Action::StageFlag(k)) => prefs::update(|s| match k {
                    1 => s.stage_desktop_click = !s.stage_desktop_click,
                    2 => s.stage_media = !s.stage_media,
                    _ => s.stage_titles = !s.stage_titles,
                }),
                Some(Action::StageRight(r)) => prefs::update(|s| s.stage_right = r),
                Some(Action::StageIcons(m)) => prefs::update(|s| s.stage_icons = m),
                Some(Action::MacKeys) => prefs::update(|s| s.mac_shortcuts = !s.mac_shortcuts),
                Some(Action::Accent(i)) => prefs::update(|s| s.accent = i),
                Some(Action::IconStyle(m)) => prefs::update(|s| {
                    s.icon_style = match m {
                        0 => nacre_core::settings::IconStyle::Default,
                        1 => nacre_core::settings::IconStyle::Dark,
                        2 => nacre_core::settings::IconStyle::Clear,
                        _ => nacre_core::settings::IconStyle::Tinted,
                    }
                }),
                Some(Action::IconTint(i)) => prefs::update(|s| s.icon_tint = i),
                Some(Action::ClipHistory) => prefs::update(|s| s.clipboard_history = !s.clipboard_history),
                Some(Action::AboveFull) => prefs::update(|s| s.above_fullscreen = !s.above_fullscreen),
                Some(Action::DockPlates) => prefs::update(|s| s.dock_plates = !s.dock_plates),
                Some(Action::IconPack) => prefs::update(|s| s.icon_pack = !s.icon_pack),
                Some(Action::DownloadsStack) => prefs::update(|s| s.downloads_stack = !s.downloads_stack),
                Some(Action::RecentApps) => prefs::update(|s| s.recent_apps = !s.recent_apps),
                Some(Action::HideIcons) => prefs::update(|s| s.hide_desktop_icons = !s.hide_desktop_icons),
                Some(Action::BarStyle(m)) => prefs::update(|s| match m {
                    0 => {
                        s.menubar_glass = false;
                        s.menubar_material = false;
                    }
                    3 => {
                        s.menubar_glass = false;
                        s.menubar_material = true;
                    }
                    1 => {
                        s.menubar_material = false;
                        s.menubar_glass = true;
                        s.menubar_style = nacre_core::settings::BarStyle::Light;
                    }
                    _ => {
                        s.menubar_material = false;
                        s.menubar_glass = true;
                        s.menubar_style = nacre_core::settings::BarStyle::Dark;
                    }
                }),
                Some(Action::Clock24) => prefs::update(|s| s.clock_24h = !s.clock_24h),
                Some(Action::DockOn) => prefs::update(|s| s.dock_enabled = !s.dock_enabled),
                Some(Action::NotchOn) => prefs::update(|s| s.notch_enabled = !s.notch_enabled),
                Some(Action::Refraction) => prefs::update(|s| s.refraction = !s.refraction),
                Some(Action::Anim) => prefs::update(|s| s.anim = !s.anim),
                Some(Action::Autostart) => prefs::update(|s| s.autostart = !s.autostart),
                Some(Action::Material(true, m)) => prefs::update(|s| s.dock_material = m),
                Some(Action::Material(false, m)) => prefs::update(|s| s.notch_material = m),
                Some(Action::Tab(t)) => {
                    with_win(|w| w.go(t, true));
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_PREFS => {
            with_win(|w| w.render());
            let (themed, _) = (prefs::look(Role::Themed), ());
            with_win(|w| {
                let base = nacre_ui::glass::GlassParams::dark((10.0, 10.0), 24.0 * w.dpi);
                let _ = w.popup.panel.set_params(side_glass(prefs::apply(base, &themed)));
            });
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = PostMessageW(Some(hwnd), WM_CLOSE_SELF, WPARAM(0), LPARAM(0));
            LRESULT(0)
        }
        WM_CLOSE_SELF => {
            prefs::unlisten(hwnd);
            let win = WIN.with(|s| s.borrow_mut().take());
            drop(win);
            let _ = DestroyWindow(hwnd);
            nacre_platform::sys::trim_later(5000);
            log::line("settings", "ditutup");
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
