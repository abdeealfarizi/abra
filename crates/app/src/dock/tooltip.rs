use crate::popup::Popup;
use nacre_platform::text::{measure, render, Style};
use nacre_ui::gfx::Gfx;
use nacre_ui::glass::GlassParams;
use windows::core::w;
use windows::UI::Color;
use std::collections::HashMap;
use windows::UI::Composition::{CompositionDrawingSurface, CompositionStretch, Compositor, SpriteVisual};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows_core::Result;
use windows_numerics::{Vector2, Vector3};

unsafe extern "system" fn proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    DefWindowProcW(h, m, w, l)
}

pub struct Tooltip {
    popup: Popup,
    text: SpriteVisual,
    dpi: f32,
    screen_origin: (i32, i32),
    screen_w: i32,
    cache: HashMap<String, (CompositionDrawingSurface, i32, i32)>,
    shown: Option<String>,
    tail: SpriteVisual,
    tail_color: Color,
    tail_surface: Option<(CompositionDrawingSurface, i32)>,
}

fn tail_bitmap(t: i32, c: Color) -> Vec<u8> {
    let (w, h) = (2 * t, t);
    let mut out = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        let half = t as f32 - (y as f32 + 0.5);
        for x in 0..w {
            let d = (x as f32 + 0.5 - t as f32).abs();
            let cov = (half - d + 0.5).clamp(0.0, 1.0);
            let a = cov * c.A as f32 / 255.0;
            let i = ((y * w + x) * 4) as usize;
            out[i] = (c.B as f32 * a) as u8;
            out[i + 1] = (c.G as f32 * a) as u8;
            out[i + 2] = (c.R as f32 * a) as u8;
            out[i + 3] = (a * 255.0) as u8;
        }
    }
    out
}

impl Tooltip {
    pub fn new(comp: &Compositor, dpi: f32, screen_origin: (i32, i32), screen_w: i32) -> Result<Tooltip> {
        let glass = crate::prefs::glass((10.0, 10.0), 10.0, crate::prefs::Role::Dark);
        let popup = Popup::new(comp, w!("NacreTooltip"), Some(proc), WS_EX_TRANSPARENT, glass)?;
        let text = comp.CreateSpriteVisual()?;
        popup.content.Children()?.InsertAtTop(&text)?;
        let tail = comp.CreateSpriteVisual()?;
        popup.content.Children()?.InsertAtTop(&tail)?;
        Ok(Tooltip { popup, text, dpi, screen_origin, screen_w, cache: HashMap::new(), shown: None, tail, tail_color: Color { A: 200, R: 30, G: 32, B: 38 }, tail_surface: None })
    }

    pub fn show(&mut self, comp: &Compositor, gfx: &Gfx, label: &str, cx: i32, bottom: i32) -> Result<()> {
        let style = Style { px: (15.0 * self.dpi).round() as i32, weight: 500 };
        let (px, py) = ((14.0 * self.dpi) as i32, (5.0 * self.dpi) as i32);
        if !self.cache.contains_key(label) {
            let (tw, th) = measure(label, &style);
            let tw = (tw + 2).min((320.0 * self.dpi) as i32);
            let Some(bmp) = render(label, &style, tw, th) else { return Ok(()) };
            let surface = gfx.surface_from_bgra(bmp.w, bmp.h, &bmp.bgra)?;
            if self.cache.len() >= 48 {
                self.cache.clear();
            }
            self.cache.insert(label.to_string(), (surface, tw, th));
        }
        let (surface, tw, th) = self.cache.get(label).cloned().unwrap();
        let (w, h) = (tw + 2 * px, th + 2 * py);
        if self.shown.as_deref() != Some(label) {
            let brush = comp.CreateSurfaceBrushWithSurface(&surface)?;
            brush.SetStretch(CompositionStretch::None)?;
            self.text.SetBrush(&brush)?;
            self.text.SetSize(Vector2 { X: tw as f32, Y: th as f32 })?;
            self.text.SetOffset(Vector3 { X: px as f32, Y: py as f32, Z: 0.0 })?;
            self.shown = Some(label.to_string());
        }
        let left = self.screen_origin.0;
        let x = (cx - w / 2).clamp(left + 4, left + (self.screen_w - w - 4).max(4));
        let t = ((5.0 * self.dpi).round() as i32).max(3);
        self.popup.set_rect(x, bottom - h - t, w, h + t)?;
        self.popup.panel.set_rect(0.0, 0.0, w as f32, h as f32)?;
        self.popup.panel.set_radius(h as f32 / 2.0)?;
        if self.tail_surface.as_ref().map(|(_, tt)| *tt) != Some(t) {
            let bmp = tail_bitmap(t, self.tail_color);
            let s = gfx.surface_from_bgra((2 * t) as u32, t as u32, &bmp)?;
            let brush = comp.CreateSurfaceBrushWithSurface(&s)?;
            brush.SetStretch(CompositionStretch::None)?;
            self.tail.SetBrush(&brush)?;
            self.tail.SetSize(Vector2 { X: (2 * t) as f32, Y: t as f32 })?;
            self.tail_surface = Some((s, t));
        }
        let tx = (cx - x - t).clamp(h / 2, (w - h / 2 - 2 * t).max(h / 2));
        self.tail.SetOffset(Vector3 { X: tx as f32, Y: h as f32 - 1.0, Z: 0.0 })?;
        self.popup.show();
        Ok(())
    }

    pub fn apply_look(&mut self, p: GlassParams) {
        let _ = self.popup.panel.set_params(p);
        let c = if p.tint.A > 0 { p.solid } else { self.tail_color };
        self.tail_color = Color { A: 205, R: c.R, G: c.G, B: c.B };
        self.tail_surface = None;
    }

    pub fn set_tier(&mut self, tier: nacre_ui::glass::Tier) {
        let _ = self.popup.panel.set_tier(tier);
    }

    pub fn hide(&self) {
        if self.popup.is_visible() {
            self.popup.hide();
        }
    }
}
