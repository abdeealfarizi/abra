//! Menu konteks kaca kustom. Menggantikan menu Win32 bawaan: tidak ada loop pesan bersarang
//! (timer dan event dock tetap berjalan), dan menutup saat klik di luar lewat penangkapan mouse
//! hook mouse yang hanya dipasang selama menu terbuka (SetCapture tidak andal untuk window NOACTIVATE).

use crate::popup::Popup;
use nacre_platform::text::{measure, render, Style};
use nacre_ui::gfx::Gfx;
use nacre_ui::glass::{GlassParams, Tier};
use std::cell::RefCell;
use std::sync::atomic::{AtomicIsize, Ordering};
use windows::core::w;
use windows::UI::Color;
use windows::UI::Composition::{CompositionStretch, Compositor, SpriteVisual};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows_core::Result;
use windows_numerics::{Vector2, Vector3};

pub const MENU_PICK: u32 = WM_APP + 12;
pub const MENU_CLOSE: u32 = WM_APP + 13;

static TARGET: AtomicIsize = AtomicIsize::new(0);

#[derive(Clone)]
pub struct Item {
    pub id: usize,
    pub label: String,
    pub enabled: bool,
    pub separator: bool,
    /// Teks pintasan di kanan (mis. "Ctrl+S"), redup.
    pub accel: String,
    /// Simbol di kiri label (opsional).
    pub sym: Option<nacre_core::symbols::Sym>,
}

impl Item {
    pub fn action(id: usize, label: &str, enabled: bool) -> Item {
        Item { id, label: label.to_string(), enabled, separator: false, accel: String::new(), sym: None }
    }
    pub fn separator() -> Item {
        Item { id: 0, label: String::new(), enabled: false, separator: true, accel: String::new(), sym: None }
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

/// Keadaan yang dibaca window proc (thread UI yang sama dengan pemiliknya).
struct Live {
    rows: Vec<(f32, f32, Item)>, // (y, tinggi, item)
    size: (f32, f32),
    hover: Option<usize>,
    highlight: SpriteVisual,
    pad: f32,
    row_h: f32,
}

thread_local! {
    static LIVE: RefCell<Option<Live>> = const { RefCell::new(None) };
}

/// Warnai bitmap teks putih ter-premultiply menjadi `rgb`.
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

fn xy(lp: LPARAM) -> (f32, f32) {
    ((lp.0 as i16) as f32, ((lp.0 >> 16) as i16) as f32)
}

unsafe extern "system" fn proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    let target = HWND(TARGET.load(Ordering::Relaxed) as *mut _);
    match m {
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_MOUSEMOVE => {
            let (x, y) = xy(l);
            LIVE.with(|c| {
                if let Some(live) = c.borrow_mut().as_mut() {
                    let hit = row_at(live, x, y);
                    if hit != live.hover {
                        live.hover = hit;
                        match hit {
                            Some(i) => {
                                let (ry, rh, _) = &live.rows[i];
                                let _ = live.highlight.SetOffset(Vector3 { X: live.pad, Y: *ry, Z: 0.0 });
                                let _ = live.highlight.SetSize(Vector2 { X: live.size.0 - 2.0 * live.pad, Y: *rh });
                                // sudut bulat ala Tahoe (radius ±8)
                                if let Ok(comp) = live.highlight.Compositor() {
                                    if let (Ok(g), Ok(clip)) = (comp.CreateRoundedRectangleGeometry(), comp.CreateGeometricClip()) {
                                        let r = (*rh / 3.0).min(8.0 * 1.0).max(6.0);
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
                }
            });
            LRESULT(0)
        }
        // Klik pertama di luar menu menutupnya (dan tidak diteruskan ke aplikasi di bawah).
        WM_LBUTTONUP | WM_RBUTTONUP => {
            let (x, y) = xy(l);
            let picked = LIVE.with(|c| c.borrow().as_ref().and_then(|live| row_at(live, x, y).map(|i| live.rows[i].2.id)));
            if let Some(id) = picked {
                nacre_platform::input::end_dismiss();
                let _ = ShowWindow(h, SW_HIDE);
                let _ = PostMessageW(Some(target), MENU_PICK, WPARAM(id), LPARAM(0));
            }
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
    screen: (i32, i32),
}

impl Menu {
    pub fn new(comp: &Compositor, dock_hwnd: HWND, dpi: f32, screen: (i32, i32), tier: Tier) -> Result<Menu> {
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
        Ok(Menu { target: dock_hwnd, popup, content, highlight, dpi, screen })
    }

    pub fn apply_look(&mut self, p: GlassParams) {
        let _ = self.popup.panel.set_params(p);
    }

    pub fn set_tier(&mut self, t: Tier) {
        let _ = self.popup.panel.set_tier(t);
    }

    /// Tampilkan di `(x, y)` (sudut kiri-bawah menu di dekat kursor; digeser agar tetap di layar).
    pub fn show(&mut self, comp: &Compositor, gfx: &Gfx, items: Vec<Item>, x: i32, y: i32) -> Result<()> {
        self.show_impl(comp, gfx, items, x, y, true)
    }

    /// Seperti `show`, tetapi `top` adalah tepi ATAS menu (dropdown menu bar muncul tepat di bawah bar).
    pub fn show_below(&mut self, comp: &Compositor, gfx: &Gfx, items: Vec<Item>, x: i32, top: i32) -> Result<()> {
        self.show_impl(comp, gfx, items, x, top, false)
    }

    fn show_impl(&mut self, comp: &Compositor, gfx: &Gfx, items: Vec<Item>, x: i32, y: i32, above: bool) -> Result<()> {
        TARGET.store(self.target.0 as isize, Ordering::Relaxed);
        let d = self.dpi;
        // kaca dan warna teks mengikuti tema (Tahoe: menu terang di mode terang)
        let look = crate::prefs::look(crate::prefs::Role::Themed);
        let base = GlassParams::dark((10.0, 10.0), 13.0 * d);
        let _ = self.popup.panel.set_params(crate::prefs::apply(base, &look));
        let ink: [u8; 3] = if look.dark { [255, 255, 255] } else { [24, 24, 28] };
        let (pad, row_h, sep_h, text_px) = (6.0 * d, 24.0 * d, 11.0 * d, (14.0 * d).round() as i32);
        let style = Style { px: text_px, weight: 400 };
        let widest = items.iter().filter(|i| !i.separator).map(|i| measure(&i.label, &style).0 + if i.accel.is_empty() { 0 } else { measure(&i.accel, &style).0 + (28.0 * d) as i32 }).max().unwrap_or(80);
        let has_sym = items.iter().any(|i| i.sym.is_some());
        let sym_w = if has_sym { 24.0 * d } else { 0.0 };
        let w = (widest as f32 + sym_w + 2.0 * (pad + 12.0 * d)).max(170.0 * d);
        let mut rows = Vec::new();
        let mut cy = pad;
        for it in items {
            let h = if it.separator { sep_h } else { row_h };
            rows.push((cy, h, it));
            cy += h;
        }
        let h = cy + pad;

        if let Ok(kids) = self.content.Children() {
            let _ = kids.RemoveAll();
        }
        for (ry, rh, it) in &rows {
            if it.separator {
                let line = comp.CreateSpriteVisual()?;
                line.SetSize(Vector2 { X: w - 2.0 * pad - 8.0 * d, Y: 1.0 })?;
                line.SetOffset(Vector3 { X: pad + 4.0 * d, Y: ry + rh / 2.0, Z: 0.0 })?;
                line.SetBrush(&comp.CreateColorBrushWithColor(Color { A: if look.dark { 36 } else { 46 }, R: ink[0], G: ink[1], B: ink[2] })?)?;
                self.content.Children()?.InsertAtTop(&line)?;
                continue;
            }
            let (tw, th) = measure(&it.label, &style);
            let tw = tw + 2;
            if let Some(bmp) = render(&it.label, &style, tw, th) {
                let surface = gfx.surface_from_bgra(bmp.w, bmp.h, &tint(&bmp.bgra, ink))?;
                let brush = comp.CreateSurfaceBrushWithSurface(&surface)?;
                brush.SetStretch(CompositionStretch::None)?;
                let s = comp.CreateSpriteVisual()?;
                s.SetBrush(&brush)?;
                s.SetSize(Vector2 { X: tw as f32, Y: th as f32 })?;
                s.SetOffset(Vector3 { X: pad + 12.0 * d + sym_w, Y: ry + (rh - th as f32) / 2.0, Z: 0.0 })?;
                s.SetOpacity(if it.enabled { 1.0 } else { 0.38 })?;
                self.content.Children()?.InsertAtTop(&s)?;
            }
            if let Some(sym) = it.sym {
                let n = (15.0 * d).round() as u32;
                let data = nacre_core::symbols::render(sym, n, ink);
                let surface = gfx.surface_from_bgra(n, n, &data)?;
                let brush = comp.CreateSurfaceBrushWithSurface(&surface)?;
                brush.SetStretch(CompositionStretch::None)?;
                let s = comp.CreateSpriteVisual()?;
                s.SetBrush(&brush)?;
                s.SetSize(Vector2 { X: n as f32, Y: n as f32 })?;
                s.SetOffset(Vector3 { X: pad + 12.0 * d, Y: ry + (rh - n as f32) / 2.0, Z: 0.0 })?;
                s.SetOpacity(if it.enabled { 0.9 } else { 0.38 })?;
                self.content.Children()?.InsertAtTop(&s)?;
            }
            if !it.accel.is_empty() {
                let (aw, ah) = measure(&it.accel, &style);
                let aw = aw + 2;
                if let Some(bmp) = render(&it.accel, &style, aw, ah) {
                    let surface = gfx.surface_from_bgra(bmp.w, bmp.h, &tint(&bmp.bgra, ink))?;
                    let brush = comp.CreateSurfaceBrushWithSurface(&surface)?;
                    brush.SetStretch(CompositionStretch::None)?;
                    let s = comp.CreateSpriteVisual()?;
                    s.SetBrush(&brush)?;
                    s.SetSize(Vector2 { X: aw as f32, Y: ah as f32 })?;
                    s.SetOffset(Vector3 { X: w - pad - 12.0 * d - aw as f32, Y: ry + (rh - ah as f32) / 2.0, Z: 0.0 })?;
                    s.SetOpacity(if it.enabled { 0.55 } else { 0.3 })?;
                    self.content.Children()?.InsertAtTop(&s)?;
                }
            }
        }
        let (wi, hi) = (w.ceil() as i32, h.ceil() as i32);
        let px = x.clamp(4, (self.screen.0 - wi - 4).max(4));
        let py = if above { (y - hi).clamp(4, (self.screen.1 - hi - 4).max(4)) } else { y.clamp(0, (self.screen.1 - hi - 4).max(0)) };
        self.popup.set_rect(px, py, wi, hi)?;
        self.popup.panel.set_radius(13.0 * d)?;
        self.highlight.SetOpacity(0.0)?;
        self.highlight.SetBrush(&comp.CreateColorBrushWithColor(crate::prefs::accent_c(235))?)?;
        LIVE.with(|c| {
            *c.borrow_mut() = Some(Live { rows, size: (w, h), hover: None, highlight: self.highlight.clone(), pad, row_h });
        });
        self.popup.show();
        self.popup.fade_in(comp, !above);
        // Klik di luar menu menutupnya: hook mouse dipasang hanya selama menu terbuka.
        nacre_platform::input::begin_dismiss((px, py, px + wi, py + hi), self.target.0 as isize, MENU_CLOSE);
        Ok(())
    }

    pub fn hide(&self) {
        nacre_platform::input::end_dismiss();
        self.popup.hide();
    }

    pub fn is_open(&self) -> bool {
        self.popup.is_visible()
    }
}

