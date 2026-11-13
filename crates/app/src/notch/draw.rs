//! Pembantu gambar isi notch: teks, simbol ABRA, dan persegi membulat sebagai sprite Composition. Permukaan teks dan
//! simbol di-cache (isi notch digambar ulang tiap berganti tampilan).

use nacre_core::symbols::{self, Sym};
use nacre_platform::text::{self, Style};
use nacre_ui::gfx::Gfx;
use std::collections::HashMap;
use windows::UI::Color;
use windows::UI::Composition::{CompositionClip, CompositionDrawingSurface, CompositionStretch, Compositor, ContainerVisual, SpriteVisual};
use windows_numerics::{Vector2, Vector3};

pub const WHITE: [u8; 3] = [255, 255, 255];

#[derive(Clone, Copy)]
pub enum Anchor {
    Left,
    Center,
    Right,
}

pub struct Painter {
    pub comp: Compositor,
    pub gfx: Gfx,
    pub dpi: f32,
    cache: HashMap<String, (CompositionDrawingSurface, i32, i32)>,
}

pub fn clip(comp: &Compositor, w: f32, h: f32, r: f32) -> windows_core::Result<CompositionClip> {
    let g = comp.CreateRoundedRectangleGeometry()?;
    g.SetSize(Vector2 { X: w, Y: h })?;
    g.SetCornerRadius(Vector2 { X: r, Y: r })?;
    let c = comp.CreateGeometricClip()?;
    c.SetGeometry(&g)?;
    windows_core::Interface::cast(&c)
}

fn recolor(b: &[u8], rgb: [u8; 3]) -> Vec<u8> {
    let mut out = b.to_vec();
    for px in out.chunks_exact_mut(4) {
        let a = px[3] as u32;
        px[0] = (rgb[2] as u32 * a / 255) as u8;
        px[1] = (rgb[1] as u32 * a / 255) as u8;
        px[2] = (rgb[0] as u32 * a / 255) as u8;
    }
    out
}

impl Painter {
    pub fn new(comp: Compositor, gfx: Gfx, dpi: f32) -> Painter {
        Painter { comp, gfx, dpi, cache: HashMap::new() }
    }

    pub fn reset(&mut self, dpi: f32) {
        self.dpi = dpi;
        self.cache.clear();
    }

    fn trim(&mut self) {
        if self.cache.len() > 160 {
            self.cache.clear();
        }
    }

    pub fn px(&self, dp: f32) -> f32 {
        dp * self.dpi
    }

    /// Lebar teks dalam px (ukuran dalam dp).
    pub fn measure(&self, s: &str, size: f32, weight: i32) -> f32 {
        text::measure(s, &Style { px: (size * self.dpi).round() as i32, weight }).0 as f32
    }

    pub fn place(&self, parent: &ContainerVisual, s: &CompositionDrawingSurface, x: f32, y: f32, w: f32, h: f32, opacity: f32) -> Option<SpriteVisual> {
        let go = || -> windows_core::Result<SpriteVisual> {
            let brush = self.comp.CreateSurfaceBrushWithSurface(s)?;
            brush.SetStretch(CompositionStretch::None)?;
            let v = self.comp.CreateSpriteVisual()?;
            v.SetBrush(&brush)?;
            v.SetSize(Vector2 { X: w, Y: h })?;
            v.SetOffset(Vector3 { X: x.round(), Y: y.round(), Z: 0.0 })?;
            v.SetOpacity(opacity)?;
            parent.Children()?.InsertAtTop(&v)?;
            Ok(v)
        };
        go().ok()
    }

    /// Teks; `x` jangkar menurut `anchor`, `y` tepi atas, ukuran dalam dp, `max_w` px. Mengembalikan lebar terpakai (px).
    #[allow(clippy::too_many_arguments)]
    pub fn text(&mut self, parent: &ContainerVisual, s: &str, x: f32, y: f32, size: f32, weight: i32, rgb: [u8; 3], opacity: f32, anchor: Anchor, max_w: f32) -> f32 {
        self.text_v(parent, s, x, y, size, weight, rgb, opacity, anchor, max_w).0
    }

    /// Seperti `text`, tetapi juga mengembalikan sprite-nya (untuk dianimasikan).
    #[allow(clippy::too_many_arguments)]
    pub fn text_v(&mut self, parent: &ContainerVisual, s: &str, x: f32, y: f32, size: f32, weight: i32, rgb: [u8; 3], opacity: f32, anchor: Anchor, max_w: f32) -> (f32, Option<SpriteVisual>) {
        if s.is_empty() {
            return (0.0, None);
        }
        let px = (size * self.dpi).round() as i32;
        let st = Style { px, weight };
        let (tw, th) = text::measure(s, &st);
        let w = (tw + 2).min(max_w as i32).max(1);
        let key = format!("t|{px}|{weight}|{rgb:?}|{w}|{s}");
        let hit = self.cache.get(&key).cloned();
        let (surf, w, h) = match hit {
            Some(v) => v,
            None => {
                let Some(bmp) = text::render(s, &st, w, th) else { return (0.0, None) };
                let Ok(sf) = self.gfx.surface_from_bgra(bmp.w, bmp.h, &recolor(&bmp.bgra, rgb)) else { return (0.0, None) };
                self.trim();
                let v = (sf, bmp.w as i32, bmp.h as i32);
                self.cache.insert(key, v.clone());
                v
            }
        };
        let left = match anchor {
            Anchor::Left => x,
            Anchor::Center => x - w as f32 / 2.0,
            Anchor::Right => x - w as f32,
        };
        let v = self.place(parent, &surf, left, y, w as f32, h as f32, opacity);
        (w as f32, v)
    }

    /// Teks rata kiri yang diburamkan `blur` px (baris lirik yang tidak sedang dinyanyikan).
    #[allow(clippy::too_many_arguments)]
    pub fn text_soft(&mut self, parent: &ContainerVisual, s: &str, x: f32, y: f32, size: f32, weight: i32, rgb: [u8; 3], opacity: f32, blur: f32) -> Option<SpriteVisual> {
        if s.is_empty() {
            return None;
        }
        let r = blur.round().max(0.0) as usize;
        let px = (size * self.dpi).round() as i32;
        let st = Style { px, weight };
        let (tw, th) = text::measure(s, &st);
        let key = format!("tb|{px}|{weight}|{rgb:?}|{r}|{s}");
        let hit = self.cache.get(&key).cloned();
        let (surf, w, h) = match hit {
            Some(v) => v,
            None => {
                let bmp = text::render(s, &st, tw + 2, th)?;
                let (bw, bh) = (bmp.w as usize, bmp.h as usize);
                let (w, h) = (bw + 2 * r, bh + 2 * r);
                let src = recolor(&bmp.bgra, rgb);
                let mut out = vec![0u8; w * h * 4];
                for yy in 0..bh {
                    let d0 = ((yy + r) * w + r) * 4;
                    out[d0..d0 + bw * 4].copy_from_slice(&src[yy * bw * 4..(yy + 1) * bw * 4]);
                }
                nacre_core::notch::typeset::box_blur(&mut out, w, h, r);
                let sf = self.gfx.surface_from_bgra(w as u32, h as u32, &out).ok()?;
                self.trim();
                let v = (sf, w as i32, h as i32);
                self.cache.insert(key, v.clone());
                v
            }
        };
        self.place(parent, &surf, x - r as f32, y - r as f32, w as f32, h as f32, opacity)
    }

    /// Simbol ABRA berpusat pada tintanya di (`cx`,`cy`); ukuran kotak dalam dp.
    #[allow(clippy::too_many_arguments)]
    pub fn sym(&mut self, parent: &ContainerVisual, s: Sym, cx: f32, cy: f32, size: f32, rgb: [u8; 3], opacity: f32) {
        let n = (size * self.dpi).round().max(8.0) as u32;
        let key = format!("s|{s:?}|{n}|{rgb:?}");
        let hit = self.cache.get(&key).cloned();
        let (surf, w, h) = match hit {
            Some(v) => v,
            None => {
                let data = symbols::render(s, n, rgb);
                let Some((cd, cw, ch)) = symbols::crop_ink(&data, n, n) else { return };
                let Ok(sf) = self.gfx.surface_from_bgra(cw, ch, &cd) else { return };
                self.trim();
                let v = (sf, cw as i32, ch as i32);
                self.cache.insert(key, v.clone());
                v
            }
        };
        self.place(parent, &surf, cx - w as f32 / 2.0, cy - h as f32 / 2.0, w as f32, h as f32, opacity);
    }

    /// Busur pengukur unduhan berpusat di (`cx`,`cy`) px: jari-jari `r` dan tebal `w` dalam dp, `from`..`to` sepanjang
    /// busur (lihat `notch::download::arc_bitmap`). Mengembalikan sprite persegi (untuk diputar pada pusatnya).
    #[allow(clippy::too_many_arguments)]
    pub fn arc(&mut self, parent: &ContainerVisual, cx: f32, cy: f32, r: f32, w: f32, from: f32, to: f32, rgb: [u8; 3], alpha: u8) -> Option<(SpriteVisual, f32)> {
        let (rp, wp) = (r * self.dpi, w * self.dpi);
        let n = ((rp + wp) * 2.0).ceil() as u32 + 2;
        let key = format!("a|{n}|{rp:.1}|{wp:.1}|{from:.3}|{to:.3}|{rgb:?}|{alpha}");
        let hit = self.cache.get(&key).cloned();
        let surf = match hit {
            Some((s, _, _)) => s,
            None => {
                let data = nacre_core::notch::download::arc_bitmap(n, rp, wp, from, to, rgb, alpha);
                let sf = self.gfx.surface_from_bgra(n, n, &data).ok()?;
                self.trim();
                self.cache.insert(key, (sf.clone(), n as i32, n as i32));
                sf
            }
        };
        let half = n as f32 / 2.0;
        self.place(parent, &surf, cx - half, cy - half, n as f32, n as f32, 1.0).map(|v| (v, n as f32))
    }

    pub fn rect(&self, parent: &ContainerVisual, x: f32, y: f32, w: f32, h: f32, radius: f32, c: Color) -> Option<SpriteVisual> {
        let go = || -> windows_core::Result<SpriteVisual> {
            let v = self.comp.CreateSpriteVisual()?;
            v.SetSize(Vector2 { X: w, Y: h })?;
            v.SetOffset(Vector3 { X: x, Y: y, Z: 0.0 })?;
            v.SetBrush(&self.comp.CreateColorBrushWithColor(c)?)?;
            if radius > 0.0 {
                v.SetClip(&clip(&self.comp, w, h, radius)?)?;
            }
            parent.Children()?.InsertAtTop(&v)?;
            Ok(v)
        };
        go().ok()
    }

    /// Gambar bitmap BGRA (mis. sampul) sebagai sprite bersudut membulat.
    #[allow(clippy::too_many_arguments)]
    pub fn image(&self, parent: &ContainerVisual, s: &CompositionDrawingSurface, x: f32, y: f32, w: f32, h: f32, radius: f32) -> Option<SpriteVisual> {
        let go = || -> windows_core::Result<SpriteVisual> {
            let b = self.comp.CreateSurfaceBrushWithSurface(s)?;
            b.SetStretch(CompositionStretch::UniformToFill)?;
            let v = self.comp.CreateSpriteVisual()?;
            v.SetBrush(&b)?;
            v.SetSize(Vector2 { X: w, Y: h })?;
            v.SetOffset(Vector3 { X: x, Y: y, Z: 0.0 })?;
            if radius > 0.0 {
                v.SetClip(&clip(&self.comp, w, h, radius)?)?;
            }
            parent.Children()?.InsertAtTop(&v)?;
            Ok(v)
        };
        go().ok()
    }
}

impl Painter {
    /// Ikon baterai ABRA (sama dengan menu bar), tepi kiri di `x`, tengah vertikal `cy`. Mengembalikan lebar (px).
    pub fn battery(&mut self, parent: &ContainerVisual, x: f32, cy: f32, pct: u8, charging: bool) -> f32 {
        let (bw, bh) = ((24.0 * self.dpi).round() as u32, (11.0 * self.dpi).round() as u32);
        let fill = if charging { [52, 199, 89] } else if pct <= 10 { [255, 69, 58] } else { WHITE };
        let key = format!("b|{bw}|{bh}|{pct}|{fill:?}");
        let hit = self.cache.get(&key).cloned();
        let (surf, w, h) = match hit {
            Some(v) => v,
            None => {
                let data = nacre_core::logo::render_battery(bw, bh, pct, WHITE, fill);
                let Ok(s) = self.gfx.surface_from_bgra(bw, bh, &data) else { return 0.0 };
                self.trim();
                let v = (s, bw as i32, bh as i32);
                self.cache.insert(key, v.clone());
                v
            }
        };
        self.place(parent, &surf, x, cy - h as f32 / 2.0, w as f32, h as f32, 0.95);
        w as f32
    }

    /// Potong teks dengan "…" agar muat di `max` px.
    pub fn fit(&self, s: &str, size: f32, weight: i32, max: f32) -> String {
        if self.measure(s, size, weight) <= max {
            return s.to_string();
        }
        let mut chars: Vec<char> = s.chars().collect();
        while !chars.is_empty() {
            chars.pop();
            let t: String = chars.iter().collect::<String>().trim_end().to_string() + "\u{2026}";
            if self.measure(&t, size, weight) <= max {
                return t;
            }
        }
        String::new()
    }
}

pub fn white(a: u8) -> Color {
    Color { A: a, R: 255, G: 255, B: 255 }
}
