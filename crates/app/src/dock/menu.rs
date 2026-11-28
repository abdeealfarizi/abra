use crate::popup::Popup;
use nacre_platform::text::{measure, render, Style};
use nacre_ui::gfx::Gfx;
use nacre_ui::glass::{GlassParams, Tier};
use std::cell::RefCell;
use std::collections::HashMap;
use windows::core::w;
use windows::UI::Color;
use windows::UI::Composition::{CompositionStretch, Compositor, SpriteVisual};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};
use windows::Win32::UI::WindowsAndMessaging::*;
const WM_MOUSELEAVE: u32 = 0x02A3;
use windows_core::Result;
use windows_numerics::{Vector2, Vector3};

pub const MENU_PICK: u32 = WM_APP + 12;
pub const MENU_CLOSE: u32 = WM_APP + 13;

#[derive(Clone)]
pub struct Item {
    pub id: usize,
    pub label: String,
    pub enabled: bool,
    pub separator: bool,
    pub accel: String,
    pub sym: Option<nacre_core::symbols::Sym>,
    pub toggle: Option<bool>,
    pub heading: bool,
    pub title: bool,
    pub lead: Option<(nacre_core::symbols::Sym, bool)>,
    pub trail: Vec<nacre_core::symbols::Sym>,
    pub keep_open: bool,
    pub children: Vec<Item>,
}

impl Item {
    pub fn action(id: usize, label: &str, enabled: bool) -> Item {
        Item { id, label: label.to_string(), enabled, separator: false, accel: String::new(), sym: None, toggle: None, heading: false, title: false, lead: None, trail: Vec::new(), keep_open: false, children: Vec::new() }
    }
    pub fn separator() -> Item {
        Item { separator: true, ..Item::action(0, "", false) }
    }
    pub fn heading(label: &str) -> Item {
        Item { heading: true, ..Item::action(0, label, false) }
    }
    pub fn switch(id: usize, label: &str, on: bool) -> Item {
        Item { title: true, toggle: Some(on), keep_open: true, ..Item::action(id, label, true) }
    }
    pub fn with_lead(mut self, s: nacre_core::symbols::Sym, on: bool) -> Item {
        self.lead = Some((s, on));
        self
    }
    pub fn with_trail(mut self, s: Vec<nacre_core::symbols::Sym>) -> Item {
        self.trail = s;
        self
    }
    pub fn with_children(mut self, children: Vec<Item>) -> Item {
        if !children.is_empty() && self.trail.is_empty() {
            self.trail.push(nacre_core::symbols::Sym::ChevronRight);
        }
        self.children = children;
        self
    }
    pub fn keep_open(mut self) -> Item {
        self.keep_open = true;
        self
    }
    pub fn with_sym(mut self, s: Option<nacre_core::symbols::Sym>) -> Item {
        self.sym = s;
        self
    }
    pub fn with_accel(mut self, a: &str) -> Item {
        self.accel = a.to_string();
        self
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Card {
    pub radius: f32,
    pub pad: f32,
    pub min_w: f32,
}

const T_HIDE: usize = 77;

struct Live {
    target: HWND,
    parent_hwnd: isize,
    comp: Compositor,
    gfx: Gfx,
    dpi: f32,
    tier: Tier,
    px: i32,
    py: i32,
    rows: Vec<(f32, f32, Item)>,
    size: (f32, f32),
    hover: Option<usize>,
    open_sub: Option<usize>,
    highlight: SpriteVisual,
    boxes: Vec<windows::UI::Composition::ContainerVisual>,
    pad: f32,
    hl_max: Option<f32>,
    owner_hides: bool,
}

thread_local! {
    static LIVE: RefCell<HashMap<isize, Live>> = RefCell::new(HashMap::new());
    static SUBS: RefCell<HashMap<isize, Box<Menu>>> = RefCell::new(HashMap::new());
}

fn close_sub(parent_key: isize) {
    nacre_platform::input::set_extra_rect(None);
    SUBS.with(|s| {
        if let Some(sub) = s.borrow_mut().get_mut(&parent_key) {
            sub.hide_sub();
        }
    });
}

fn open_sub(parent_key: isize, items: Vec<Item>, px: i32, py: i32, pw: i32, row_y: i32, pad: i32, comp: &Compositor, gfx: &Gfx, target: HWND, dpi: f32, tier: Tier) {
    SUBS.with(|s| {
        let mut map = s.borrow_mut();
        if !map.contains_key(&parent_key) {
            if let Ok(m) = Menu::new_tier(comp, target, dpi, tier) {
                map.insert(parent_key, Box::new(m));
            }
        }
        if let Some(sub) = map.get_mut(&parent_key) {
            let _ = sub.show_sub(comp, gfx, items, px, py, pw, row_y, pad, parent_key);
        }
    });
}

fn tint(src: &[u8], rgb: [u8; 3]) -> Vec<u8> {
    let mut out = src.to_vec();
    for px in out.chunks_exact_mut(4) {
        let a = px[3] as u32;
        px[0] = (rgb[2] as u32 * a / 255) as u8;
        px[1] = (rgb[1] as u32 * a / 255) as u8;
        px[2] = (rgb[0] as u32 * a / 255) as u8;
    }
    out
}

fn row_at(l: &Live, x: f32, y: f32) -> Option<usize> {
    if x < 0.0 || y < 0.0 || x >= l.size.0 || y >= l.size.1 {
        return None;
    }
    l.rows.iter().position(|(ry, rh, it)| y >= *ry && y < ry + rh && !it.separator && it.enabled)
}

fn set_hover(live: &mut Live, hit: Option<usize>) {
    let comp = live.highlight.Compositor().ok();
    let row_center = |i: usize| Vector3 { X: live.size.0 / 2.0, Y: live.rows[i].0 + live.rows[i].1 / 2.0, Z: 0.0 };
    if let (Some(comp), Some(old)) = (&comp, live.hover) {
        if let Some(b) = live.boxes.get(old) {
            let _ = crate::motion::hover_pop(comp, b, row_center(old), 1.0);
        }
    }
    if let (Some(comp), Some(new)) = (&comp, hit) {
        let k = crate::motion::fit_pop(crate::motion::HOVER_ROW, live.size.0 / 2.0 - live.pad, live.pad);
        if let Some(b) = live.boxes.get(new) {
            let _ = crate::motion::hover_pop(comp, b, row_center(new), k);
        }
        let _ = live.highlight.SetScale(Vector3 { X: 1.0, Y: 1.0, Z: 1.0 });
        let hc = Vector3 { X: live.size.0 / 2.0 - live.pad, Y: live.rows[new].1 / 2.0, Z: 0.0 };
        let _ = crate::motion::hover_pop(comp, &live.highlight, hc, k);
    }
    live.hover = hit;
    match hit {
        Some(i) => {
            let (ry, rh, _) = &live.rows[i];
            let _ = live.highlight.SetOffset(Vector3 { X: live.pad, Y: *ry, Z: 0.0 });
            let _ = live.highlight.SetSize(Vector2 { X: live.size.0 - 2.0 * live.pad, Y: *rh });
            if let Ok(comp) = live.highlight.Compositor() {
                if let (Ok(g), Ok(clip)) = (comp.CreateRoundedRectangleGeometry(), comp.CreateGeometricClip()) {
                    let r = match live.hl_max {
                        Some(m) => (*rh / 2.0).min(m),
                        None => (*rh / 3.0).clamp(6.0, 9.0),
                    };
                    let _ = g.SetSize(Vector2 { X: live.size.0 - 2.0 * live.pad, Y: *rh });
                    let _ = g.SetCornerRadius(Vector2 { X: r, Y: r });
                    let _ = clip.SetGeometry(&g);
                    let _ = live.highlight.SetClip(&clip);
                }
            }
            let _ = live.highlight.SetOpacity(1.0);
        }
        None => {
            let _ = live.highlight.SetOpacity(0.0);
        }
    }
}

fn rounded(comp: &Compositor, r: (f32, f32, f32, f32), radius: f32, c: Color) -> Result<SpriteVisual> {
    let v = comp.CreateSpriteVisual()?;
    v.SetBrush(&comp.CreateColorBrushWithColor(c)?)?;
    v.SetSize(Vector2 { X: r.2, Y: r.3 })?;
    v.SetOffset(Vector3 { X: r.0, Y: r.1, Z: 0.0 })?;
    let g = comp.CreateRoundedRectangleGeometry()?;
    g.SetSize(Vector2 { X: r.2, Y: r.3 })?;
    g.SetCornerRadius(Vector2 { X: radius, Y: radius })?;
    let clip = comp.CreateGeometricClip()?;
    clip.SetGeometry(&g)?;
    v.SetClip(&clip)?;
    Ok(v)
}

fn xy(lp: LPARAM) -> (f32, f32) {
    ((lp.0 as i16) as f32, ((lp.0 >> 16) as i16) as f32)
}

unsafe extern "system" fn proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    let key = h.0 as isize;
    match m {
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_MOUSEMOVE => {
            let (x, y) = xy(l);
            let mut t = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: h, dwHoverTime: 0 };
            let _ = TrackMouseEvent(&mut t);
            enum SubOp {
                None,
                Close,
                Open { items: Vec<Item>, px: i32, py: i32, pw: i32, row_y: i32, pad: i32, comp: Compositor, gfx: Gfx, target: HWND, dpi: f32, tier: Tier },
            }
            let op = LIVE.with(|c| {
                let mut map = c.borrow_mut();
                let Some(live) = map.get_mut(&key) else { return SubOp::None };
                let hit = row_at(live, x, y).filter(|i| live.rows[*i].2.toggle.is_none());
                if hit != live.hover {
                    set_hover(live, hit);
                }
                if let Some(idx) = hit {
                    if !live.rows[idx].2.children.is_empty() {
                        if live.open_sub != Some(idx) {
                            live.open_sub = Some(idx);
                            return SubOp::Open {
                                items: live.rows[idx].2.children.clone(),
                                px: live.px,
                                py: live.py,
                                pw: live.size.0.ceil() as i32,
                                row_y: live.rows[idx].0.round() as i32,
                                pad: live.pad.round() as i32,
                                comp: live.comp.clone(),
                                gfx: live.gfx.clone(),
                                target: live.target,
                                dpi: live.dpi,
                                tier: live.tier,
                            };
                        }
                    } else if live.open_sub.is_some() {
                        live.open_sub = None;
                        return SubOp::Close;
                    }
                }
                SubOp::None
            });
            match op {
                SubOp::Open { items, px, py, pw, row_y, pad, comp, gfx, target, dpi, tier } => {
                    open_sub(key, items, px, py, pw, row_y, pad, &comp, &gfx, target, dpi, tier);
                }
                SubOp::Close => close_sub(key),
                SubOp::None => {}
            }
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            LIVE.with(|c| {
                if let Some(live) = c.borrow_mut().get_mut(&key) {
                    if live.hover.is_some() && live.open_sub.is_none() {
                        set_hover(live, None);
                    }
                }
            });
            LRESULT(0)
        }
        WM_LBUTTONUP | WM_RBUTTONUP => {
            let (x, y) = xy(l);
            enum Pick {
                None,
                OpenSub { items: Vec<Item>, px: i32, py: i32, pw: i32, row_y: i32, pad: i32, comp: Compositor, gfx: Gfx, target: HWND, dpi: f32, tier: Tier },
                Fire { id: usize, keep: bool, target: HWND, parent: isize },
            }
            let picked = LIVE.with(|c| {
                let mut map = c.borrow_mut();
                let Some(live) = map.get_mut(&key) else { return Pick::None };
                let Some(i) = row_at(live, x, y) else { return Pick::None };
                if !live.rows[i].2.children.is_empty() {
                    live.open_sub = Some(i);
                    return Pick::OpenSub {
                        items: live.rows[i].2.children.clone(),
                        px: live.px,
                        py: live.py,
                        pw: live.size.0.ceil() as i32,
                        row_y: live.rows[i].0.round() as i32,
                        pad: live.pad.round() as i32,
                        comp: live.comp.clone(),
                        gfx: live.gfx.clone(),
                        target: live.target,
                        dpi: live.dpi,
                        tier: live.tier,
                    };
                }
                Pick::Fire {
                    id: live.rows[i].2.id,
                    keep: live.rows[i].2.keep_open || live.owner_hides,
                    target: live.target,
                    parent: live.parent_hwnd,
                }
            });
            match picked {
                Pick::OpenSub { items, px, py, pw, row_y, pad, comp, gfx, target, dpi, tier } => {
                    open_sub(key, items, px, py, pw, row_y, pad, &comp, &gfx, target, dpi, tier);
                }
                Pick::Fire { id, keep, target, parent } => {
                    if !keep {
                        close_sub(key);
                        nacre_platform::input::end_dismiss();
                        let _ = ShowWindow(h, SW_HIDE);
                        LIVE.with(|c| {
                            c.borrow_mut().remove(&key);
                        });
                        if parent != 0 {
                            close_sub(parent);
                            let _ = ShowWindow(HWND(parent as *mut _), SW_HIDE);
                            LIVE.with(|c| {
                                c.borrow_mut().remove(&parent);
                            });
                        }
                    }
                    let _ = PostMessageW(Some(target), MENU_PICK, WPARAM(id), LPARAM(0));
                }
                Pick::None => {}
            }
            LRESULT(0)
        }
        WM_TIMER if w.0 == T_HIDE => {
            let _ = KillTimer(Some(h), T_HIDE);
            let _ = ShowWindow(h, SW_HIDE);
            LRESULT(0)
        }
        _ => DefWindowProcW(h, m, w, l),
    }
}

pub struct Menu {
    target: HWND,
    popup: Popup,
    content: windows::UI::Composition::ContainerVisual,
    highlight: SpriteVisual,
    dpi: f32,
    tier: Tier,
    card: Option<Card>,
    closing: bool,
    pop_at: Vector3,
}

impl Menu {
    pub fn new(comp: &Compositor, dock_hwnd: HWND, dpi: f32, tier: Tier) -> Result<Menu> {
        Self::new_tier(comp, dock_hwnd, dpi, tier)
    }

    fn new_tier(comp: &Compositor, dock_hwnd: HWND, dpi: f32, tier: Tier) -> Result<Menu> {
        let mut popup = Popup::new(comp, w!("NacreMenu"), Some(proc), WINDOW_EX_STYLE(0), crate::prefs::glass((10.0, 10.0), 12.0 * dpi, crate::prefs::Role::Dark))?;
        let _ = popup.panel.set_tier(tier);
        let highlight = comp.CreateSpriteVisual()?;
        highlight.SetBrush(&comp.CreateColorBrushWithColor(crate::prefs::accent_c(235))?)?;
        highlight.SetOpacity(0.0)?;
        let clip = comp.CreateRoundedRectangleGeometry()?;
        clip.SetCornerRadius(Vector2 { X: 7.0 * dpi, Y: 7.0 * dpi })?;
        let _ = &mut popup;
        let content = comp.CreateContainerVisual()?;
        popup.content.Children()?.InsertAtTop(&highlight)?;
        popup.content.Children()?.InsertAtTop(&content)?;
        Ok(Menu { target: dock_hwnd, popup, content, highlight, dpi, tier, card: None, closing: false, pop_at: Vector3 { X: 0.0, Y: 0.0, Z: 0.0 } })
    }

    pub fn set_card(&mut self, card: Option<Card>) {
        self.card = card;
    }

    pub fn show_card(&mut self, comp: &Compositor, gfx: &Gfx, items: Vec<Item>, x: i32, top: i32, right_aligned: bool) -> Result<()> {
        self.show_impl(comp, gfx, items, x, top, false, right_aligned, None)
    }

    pub fn hide_animated(&mut self, comp: &Compositor) {
        if !self.popup.is_visible() || self.closing {
            return;
        }
        let key = self.popup.hwnd.0 as isize;
        close_sub(key);
        nacre_platform::input::end_dismiss();
        LIVE.with(|c| {
            c.borrow_mut().remove(&key);
        });
        if crate::motion::pop_out(comp, self.popup.root(), self.pop_at, 0.55, crate::motion::CC_CLOSE_MS).is_ok() {
            self.closing = true;
            unsafe {
                SetTimer(Some(self.popup.hwnd), T_HIDE, crate::motion::CC_CLOSE_MS + 20, None);
            }
        } else {
            self.popup.hide();
        }
    }

    pub fn apply_look(&mut self, p: GlassParams) {
        let _ = self.popup.panel.set_params(p);
    }

    pub fn set_tier(&mut self, t: Tier) {
        self.tier = t;
        let _ = self.popup.panel.set_tier(t);
    }

    pub fn show(&mut self, comp: &Compositor, gfx: &Gfx, items: Vec<Item>, x: i32, y: i32) -> Result<()> {
        self.show_impl(comp, gfx, items, x, y, true, false, None)
    }

    pub fn show_below(&mut self, comp: &Compositor, gfx: &Gfx, items: Vec<Item>, x: i32, top: i32) -> Result<()> {
        self.show_impl(comp, gfx, items, x, top, false, false, None)
    }

    #[allow(clippy::too_many_arguments)]
    fn show_sub(&mut self, comp: &Compositor, gfx: &Gfx, items: Vec<Item>, p_px: i32, p_py: i32, p_w: i32, row_y: i32, pad: i32, parent_hwnd: isize) -> Result<()> {
        self.show_impl(comp, gfx, items, p_px + p_w - (4.0 * self.dpi) as i32, p_py + row_y - pad, false, false, Some((parent_hwnd, p_px)))
    }

    #[allow(clippy::too_many_arguments)]
    fn show_impl(&mut self, comp: &Compositor, gfx: &Gfx, items: Vec<Item>, x: i32, y: i32, above: bool, right_aligned: bool, sub_of: Option<(isize, i32)>) -> Result<()> {
        let key = self.popup.hwnd.0 as isize;
        if sub_of.is_none() {
            close_sub(key);
        }
        let d = self.dpi;
        if self.closing {
            self.closing = false;
            unsafe {
                let _ = KillTimer(Some(self.popup.hwnd), T_HIDE);
            }
            self.popup.hide();
        }
        let refresh = self.popup.is_visible();
        let card = self.card;
        let radius = card.map_or(13.0, |c| c.radius) * d;
        let look = crate::prefs::look(crate::prefs::Role::Themed);
        let base = GlassParams::dark((10.0, 10.0), radius);
        let _ = self.popup.panel.set_params(crate::prefs::apply(base, &look));
        let ink: [u8; 3] = if look.dark { [255, 255, 255] } else { [24, 24, 28] };
        let pad = card.map_or(6.0, |c| c.pad) * d;
        let (row_h, sep_h, text_px) = (24.0 * d, 11.0 * d, (14.0 * d).round() as i32);
        let style = Style { px: text_px, weight: 400 };
        let head_px = (12.0 * d).round() as i32;
        let style_of = |it: &Item| Style { px: if it.heading { head_px } else { text_px }, weight: if it.title { 700 } else if it.heading { 600 } else { 400 } };
        let has_sym = items.iter().any(|i| i.sym.is_some());
        let sym_w = if has_sym { 24.0 * d } else { 0.0 };
        let lead_d = 26.0 * d;
        let sw = (32.0 * d, 19.0 * d);
        let trail_px = 14.0 * d;
        let label_x = |it: &Item| if it.lead.is_some() { pad + 8.0 * d + lead_d + 10.0 * d } else { pad + 12.0 * d + sym_w };
        let rich = items.iter().any(|i| i.lead.is_some() || i.toggle.is_some());
        let widest = items
            .iter()
            .filter(|i| !i.separator)
            .map(|i| {
                let mut r = label_x(i) + measure(&i.label, &style_of(i)).0 as f32;
                if !i.accel.is_empty() {
                    r += measure(&i.accel, &style).0 as f32 + 28.0 * d;
                }
                if i.toggle.is_some() {
                    r += sw.0 + 20.0 * d;
                }
                if !i.trail.is_empty() {
                    r += i.trail.len() as f32 * (trail_px + 6.0 * d) + 14.0 * d;
                }
                r
            })
            .fold(80.0f32, f32::max);
        let w = (widest + pad + 12.0 * d).max(card.map_or(if rich { 270.0 } else { 170.0 }, |c| c.min_w) * d);
        let mut rows = Vec::new();
        let mut cy = pad;
        for it in items {
            let h = if it.separator {
                sep_h
            } else if it.title {
                34.0 * d
            } else if it.lead.is_some() {
                lead_d + 8.0 * d
            } else if it.heading {
                26.0 * d
            } else {
                row_h
            };
            rows.push((cy, h, it));
            cy += h;
        }
        let h = cy + pad;

        if let Ok(kids) = self.content.Children() {
            let _ = kids.RemoveAll();
        }
        let tex = |label: &str, st: &Style| -> Result<Option<(SpriteVisual, f32, f32)>> {
            let (tw, th) = measure(label, st);
            let tw = tw + 2;
            let Some(bmp) = render(label, st, tw, th) else { return Ok(None) };
            let surface = gfx.surface_from_bgra(bmp.w, bmp.h, &tint(&bmp.bgra, ink))?;
            let brush = comp.CreateSurfaceBrushWithSurface(&surface)?;
            brush.SetStretch(CompositionStretch::None)?;
            let s = comp.CreateSpriteVisual()?;
            s.SetBrush(&brush)?;
            s.SetSize(Vector2 { X: tw as f32, Y: th as f32 })?;
            Ok(Some((s, tw as f32, th as f32)))
        };
        let symbol = |sym: nacre_core::symbols::Sym, n: u32, rgb: [u8; 3]| -> Result<SpriteVisual> {
            let data = nacre_core::symbols::render(sym, n, rgb);
            let surface = gfx.surface_from_bgra(n, n, &data)?;
            let brush = comp.CreateSurfaceBrushWithSurface(&surface)?;
            brush.SetStretch(CompositionStretch::None)?;
            let s = comp.CreateSpriteVisual()?;
            s.SetBrush(&brush)?;
            s.SetSize(Vector2 { X: n as f32, Y: n as f32 })?;
            Ok(s)
        };
        let gray = Color { A: if look.dark { 46 } else { 30 }, R: ink[0], G: ink[1], B: ink[2] };
        let mut boxes: Vec<(windows::UI::Composition::ContainerVisual, f32)> = Vec::new();
        for (ry, rh, it) in &rows {
            let rb = comp.CreateContainerVisual()?;
            self.content.Children()?.InsertAtTop(&rb)?;
            boxes.push((rb.clone(), ry + rh / 2.0));
            if it.separator {
                let line = comp.CreateSpriteVisual()?;
                line.SetSize(Vector2 { X: w - 2.0 * pad - 8.0 * d, Y: 1.0 })?;
                line.SetOffset(Vector3 { X: pad + 4.0 * d, Y: ry + rh / 2.0, Z: 0.0 })?;
                line.SetBrush(&comp.CreateColorBrushWithColor(Color { A: if look.dark { 36 } else { 46 }, R: ink[0], G: ink[1], B: ink[2] })?)?;
                rb.Children()?.InsertAtTop(&line)?;
                continue;
            }
            if let Some((s, _, th)) = tex(&it.label, &style_of(it))? {
                let dy = if it.heading { 3.0 * d } else { 0.0 };
                s.SetOffset(Vector3 { X: label_x(it).round(), Y: (ry + (rh - th) / 2.0 + dy).round(), Z: 0.0 })?;
                s.SetOpacity(if it.heading { 0.5 } else if it.enabled { 1.0 } else { 0.38 })?;
                rb.Children()?.InsertAtTop(&s)?;
            }
            if let Some(sym) = it.sym {
                let n = (15.0 * d).round() as u32;
                let s = symbol(sym, n, ink)?;
                s.SetOffset(Vector3 { X: (pad + 12.0 * d).round(), Y: (ry + (rh - n as f32) / 2.0).round(), Z: 0.0 })?;
                s.SetOpacity(if it.enabled { 0.9 } else { 0.38 })?;
                rb.Children()?.InsertAtTop(&s)?;
            }
            if let Some((sym, on)) = it.lead {
                let (cx, cy) = (pad + 8.0 * d, ry + (rh - lead_d) / 2.0);
                let disc = rounded(comp, (cx, cy, lead_d, lead_d), lead_d / 2.0, if on { crate::prefs::accent_c(255) } else { gray })?;
                rb.Children()?.InsertAtTop(&disc)?;
                let n = (15.0 * d).round() as u32;
                let s = symbol(sym, n, if on { [255, 255, 255] } else { ink })?;
                s.SetOffset(Vector3 { X: (cx + (lead_d - n as f32) / 2.0).round(), Y: (cy + (lead_d - n as f32) / 2.0).round(), Z: 0.0 })?;
                s.SetOpacity(if on { 1.0 } else { 0.85 })?;
                rb.Children()?.InsertAtTop(&s)?;
            }
            if let Some(on) = it.toggle {
                let (tx, ty) = (w - pad - 10.0 * d - sw.0, ry + (rh - sw.1) / 2.0);
                let off = Color { A: if look.dark { 70 } else { 50 }, ..gray };
                let track = rounded(comp, (tx, ty, sw.0, sw.1), sw.1 / 2.0, if on { crate::prefs::accent_c(255) } else { off })?;
                rb.Children()?.InsertAtTop(&track)?;
                let k = sw.1 - 4.0 * d;
                let kx = if on { tx + sw.0 - 2.0 * d - k } else { tx + 2.0 * d };
                let knob = rounded(comp, (kx, ty + 2.0 * d, k, k), k / 2.0, Color { A: 255, R: 255, G: 255, B: 255 })?;
                rb.Children()?.InsertAtTop(&knob)?;
            }
            let mut rx = w - pad - 12.0 * d;
            for sym in it.trail.iter().rev() {
                let n = trail_px.round() as u32;
                let s = symbol(*sym, n, ink)?;
                rx -= n as f32;
                s.SetOffset(Vector3 { X: rx.round(), Y: (ry + (rh - n as f32) / 2.0).round(), Z: 0.0 })?;
                s.SetOpacity(0.7)?;
                rb.Children()?.InsertAtTop(&s)?;
                rx -= 6.0 * d;
            }
            if !it.accel.is_empty() {
                if let Some((s, aw, ah)) = tex(&it.accel, &style)? {
                    s.SetOffset(Vector3 { X: (w - pad - 12.0 * d - aw).round(), Y: (ry + (rh - ah) / 2.0).round(), Z: 0.0 })?;
                    s.SetOpacity(if it.enabled { 0.55 } else { 0.3 })?;
                    rb.Children()?.InsertAtTop(&s)?;
                }
            }
        }
        let (wi, hi) = (w.ceil() as i32, h.ceil() as i32);
        let anchor = x;
        let mon = nacre_platform::monitors::at(anchor, if above { y - 1 } else { y }).area;
        let x = match sub_of {
            Some((_, p_px)) if x + wi > mon.right() - 4 => p_px + (4.0 * d) as i32 - wi,
            _ if right_aligned => x - wi,
            _ => x,
        };
        let px = mon.clamp_x(x, wi, 4);
        let py = if above { mon.clamp_y(y - hi, hi, 4) } else { y.clamp(mon.y, (mon.bottom() - hi - 4).max(mon.y)) };
        self.popup.set_rect(px, py, wi, hi)?;
        self.popup.panel.set_radius(radius)?;
        self.highlight.SetOpacity(0.0)?;
        self.highlight.SetBrush(&comp.CreateColorBrushWithColor(crate::prefs::accent_c(235))?)?;
        LIVE.with(|c| {
            c.borrow_mut().insert(
                key,
                Live {
                    target: self.target,
                    parent_hwnd: sub_of.map_or(0, |s| s.0),
                    comp: comp.clone(),
                    gfx: gfx.clone(),
                    dpi: d,
                    tier: self.tier,
                    px,
                    py,
                    rows,
                    size: (w, h),
                    hover: None,
                    open_sub: None,
                    highlight: self.highlight.clone(),
                    boxes: boxes.iter().map(|b| b.0.clone()).collect(),
                    pad,
                    hl_max: card.map(|_| (radius - pad).max(6.0 * d)),
                    owner_hides: card.is_some(),
                },
            );
        });
        if refresh {
            let mut pt = windows::Win32::Foundation::POINT::default();
            unsafe {
                let _ = GetCursorPos(&mut pt);
            }
            let (cx, cy) = ((pt.x - px) as f32, (pt.y - py) as f32);
            LIVE.with(|c| {
                if let Some(live) = c.borrow_mut().get_mut(&key) {
                    let hit = row_at(live, cx, cy).filter(|i| live.rows[*i].2.toggle.is_none());
                    set_hover(live, hit);
                }
            });
        } else if card.is_some() {
            let ax = if right_aligned { wi as f32 - 24.0 * d } else { ((anchor - px) as f32 + 12.0 * d).clamp(0.0, wi as f32) };
            self.pop_at = Vector3 { X: ax, Y: 0.0, Z: 0.0 };
            self.popup.show();
            self.raise();
            let _ = crate::motion::pop_in(comp, self.popup.root(), self.pop_at, 0.45, 1.025, 40, crate::motion::CC_OPEN_MS);
            for (i, (rb, cy)) in boxes.iter().enumerate() {
                let _ = crate::motion::pop_in(comp, rb, Vector3 { X: w / 2.0, Y: *cy, Z: 0.0 }, 0.7, 1.04, 80 + i as u32 * crate::motion::CC_STAGGER_MS, 360);
            }
        } else {
            self.popup.show();
            self.raise();
            self.popup.fade_in(comp, !above);
        }
        if sub_of.is_some() {
            nacre_platform::input::set_extra_rect(Some((px, py, px + wi, py + hi)));
        } else {
            nacre_platform::input::begin_dismiss((px, py, px + wi, py + hi), self.target.0 as isize, MENU_CLOSE);
        }
        Ok(())
    }

    fn hide_sub(&self) {
        let key = self.popup.hwnd.0 as isize;
        close_sub(key);
        LIVE.with(|c| {
            c.borrow_mut().remove(&key);
        });
        unsafe {
            let _ = KillTimer(Some(self.popup.hwnd), T_HIDE);
        }
        self.popup.hide();
    }

    pub fn raise(&self) {
        unsafe {
            let _ = SetWindowPos(self.popup.hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }

    pub fn hide(&self) {
        let key = self.popup.hwnd.0 as isize;
        close_sub(key);
        nacre_platform::input::end_dismiss();
        LIVE.with(|c| {
            c.borrow_mut().remove(&key);
        });
        unsafe {
            let _ = KillTimer(Some(self.popup.hwnd), T_HIDE);
        }
        self.popup.hide();
    }

    pub fn is_open(&self) -> bool {
        self.popup.is_visible() && !self.closing
    }
}
