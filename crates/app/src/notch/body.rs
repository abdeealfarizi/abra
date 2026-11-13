//! Badan notch: kaca (atau hitam pekat) yang menempel di tepi atas layar dengan dua "telinga" cekung, wadah isi yang
//! dipotong tepat sebatas badan, dan wadah "ghost" untuk isi lama yang sedang memudar.

use super::{ears, spring};
use nacre_core::settings::Material;
use nacre_platform::log;
use nacre_ui::gfx::Gfx;
use nacre_ui::glass::{GlassPanel, GlassParams, Tier};
use windows::UI::Color;
use windows::UI::Composition::{CompositionBrush, CompositionDrawingSurface, CompositionObject, Compositor, ContainerVisual, InsetClip, SpriteVisual};
use windows_core::Interface;
use windows_numerics::{Vector2, Vector3};

/// Warna notch hitam (sedikit di atas hitam murni agar tetap terasa seperti material).
pub const ONYX: (u8, u8, u8) = (10, 10, 12);

/// Parameter kaca notch menurut material pilihan. Teks notch selalu putih, jadi selalu varian gelap.
pub fn glass_params(radius: f32) -> GlassParams {
    let look = crate::prefs::look(crate::prefs::Role::Notch);
    let mut g = crate::prefs::apply(GlassParams::dark((10.0, 10.0), radius), &look);
    if is_black() {
        g.tint = Color { A: 255, R: ONYX.0, G: ONYX.1, B: ONYX.2 };
        g.solid = Color { A: 245, R: ONYX.0, G: ONYX.1, B: ONYX.2 };
    } else {
        // alas tint minimum agar teks putih tetap terbaca di atas wallpaper terang
        g.tint.A = g.tint.A.max(96);
    }
    // badan dan telinga satu kesatuan: tanpa garis tepi atau kilau (garis akan memotong sambungan dengan telinga);
    // kesan timbul datang dari bayangan tipis di bawahnya
    g.rim = Color { A: 0, R: 255, G: 255, B: 255 };
    g.highlight = 0.0;
    g
}

pub fn is_black() -> bool {
    crate::prefs::get().notch_material == Material::Black
}

pub struct Body {
    comp: Compositor,
    pub stage: ContainerVisual,
    pub panel: GlassPanel,
    ears: [SpriteVisual; 2],
    ear_tint: [SpriteVisual; 2],
    ear_mask: [CompositionDrawingSurface; 2],
    pub content: ContainerVisual,
    pub ghost: ContainerVisual,
    clip: InsetClip,
    /// Sisi telinga (px).
    pub ear: f32,
    /// Bagian badan yang disembunyikan di atas layar agar sudut atasnya tidak terlihat (px).
    pub top_over: f32,
}

impl Body {
    pub fn new(comp: &Compositor, gfx: &Gfx, root: &ContainerVisual, win: (i32, i32), dpi: f32, tier: Tier) -> windows_core::Result<Body> {
        let stage = comp.CreateContainerVisual()?;
        root.Children()?.InsertAtTop(&stage)?;
        let mut panel = GlassPanel::new(comp, &stage, glass_params(15.0 * dpi))?;
        panel.set_tier(tier)?;
        let ear = (13.0 * dpi).ceil();
        let make_ear = |left: bool| -> windows_core::Result<(SpriteVisual, SpriteVisual, CompositionDrawingSurface)> {
            let b = ears::bitmap(ear as u32, ONYX, 255, left);
            let s = gfx.surface_from_bgra(b.w, b.h, &b.bgra)?;
            let v = comp.CreateSpriteVisual()?;
            v.SetSize(Vector2 { X: ear, Y: ear })?;
            let tint = comp.CreateSpriteVisual()?;
            tint.SetSize(Vector2 { X: ear, Y: ear })?;
            v.Children()?.InsertAtTop(&tint)?;
            stage.Children()?.InsertAtTop(&v)?;
            Ok((v, tint, s))
        };
        let (l, r) = (make_ear(true)?, make_ear(false)?);
        let content = comp.CreateContainerVisual()?;
        let ghost = comp.CreateContainerVisual()?;
        stage.Children()?.InsertAtTop(&ghost)?;
        stage.Children()?.InsertAtTop(&content)?;
        let clip = comp.CreateInsetClip()?;
        for c in [&content, &ghost] {
            c.SetSize(Vector2 { X: win.0 as f32, Y: win.1 as f32 })?;
            c.SetClip(&clip)?;
        }
        let body = Body {
            comp: comp.clone(),
            stage,
            panel,
            ears: [l.0, r.0],
            ear_tint: [l.1, r.1],
            ear_mask: [l.2, r.2],
            content,
            ghost,
            clip,
            ear,
            top_over: 32.0 * dpi,
        };
        body.paint_ears(tier);
        Ok(body)
    }

    /// Ukuran jendela berubah (resolusi/DPI): wadah isi mengikuti.
    pub fn resize(&self, win: (i32, i32)) {
        for c in [&self.content, &self.ghost] {
            let _ = c.SetSize(Vector2 { X: win.0 as f32, Y: win.1 as f32 });
        }
    }

    /// DPI berubah: buat ulang topeng telinga.
    pub fn remake_ears(&mut self, gfx: &Gfx, dpi: f32, tier: Tier) {
        self.ear = (13.0 * dpi).ceil();
        self.top_over = 32.0 * dpi;
        for (i, left) in [true, false].into_iter().enumerate() {
            let b = ears::bitmap(self.ear as u32, ONYX, 255, left);
            if let Ok(s) = gfx.surface_from_bgra(b.w, b.h, &b.bgra) {
                self.ear_mask[i] = s;
                let _ = self.ears[i].SetSize(Vector2 { X: self.ear, Y: self.ear });
                let _ = self.ear_tint[i].SetSize(Vector2 { X: self.ear, Y: self.ear });
            }
        }
        self.paint_ears(tier);
    }

    /// Telinga memakai material yang sama dengan badan: hitam pekat, atau blur backdrop + tint dipotong bentuk telinga.
    pub fn paint_ears(&self, tier: Tier) {
        let go = || -> windows_core::Result<()> {
            let g = glass_params(15.0);
            for i in 0..2 {
                let mask = self.comp.CreateSurfaceBrushWithSurface(&self.ear_mask[i])?;
                if is_black() {
                    self.ears[i].SetBrush(&mask)?;
                    self.ear_tint[i].SetOpacity(0.0)?;
                    continue;
                }
                let base: CompositionBrush = if tier == Tier::Solid {
                    self.comp.CreateColorBrushWithColor(g.solid)?.cast()?
                } else {
                    nacre_ui::glass::backdrop_blur_brush(&self.comp, g.blur_sigma, g.saturation)?.cast()?
                };
                let mb = self.comp.CreateMaskBrush()?;
                mb.SetSource(&base)?;
                mb.SetMask(&mask)?;
                self.ears[i].SetBrush(&mb)?;
                let tb = self.comp.CreateMaskBrush()?;
                tb.SetSource(&self.comp.CreateColorBrushWithColor(g.tint)?)?;
                tb.SetMask(&self.comp.CreateSurfaceBrushWithSurface(&self.ear_mask[i])?)?;
                self.ear_tint[i].SetBrush(&tb)?;
                self.ear_tint[i].SetOpacity(1.0)?;
            }
            Ok(())
        };
        if let Err(e) = go() {
            log::line("notch", &format!("telinga gagal: {e}"));
        }
    }

    /// Pindahkan badan ke `x0` dengan ukuran (`w`,`h`) dan radius bawah `radius`. `springy` = Some((rasio, periode))
    /// untuk animasi pegas di compositor, None untuk langsung.
    pub fn set_geometry(&self, win: (i32, i32), x0: f32, w: f32, h: f32, radius: f32, springy: Option<(f32, u32)>) {
        let (e, over) = (self.ear, self.top_over);
        let insets = (x0, (win.0 as f32 - (x0 + w)).max(0.0), (win.1 as f32 - h).max(0.0));
        match springy {
            None => {
                let _ = self.panel.set_rect(x0, -over, w, h + over);
                let _ = self.panel.set_radius(radius);
                let _ = self.ears[0].SetOffset(Vector3 { X: x0 - e, Y: 0.0, Z: 0.0 });
                let _ = self.ears[1].SetOffset(Vector3 { X: x0 + w, Y: 0.0, Z: 0.0 });
                let _ = self.clip.SetLeftInset(insets.0);
                let _ = self.clip.SetRightInset(insets.1);
                let _ = self.clip.SetBottomInset(insets.2);
            }
            Some((ratio, period)) => {
                let _ = self.panel.animate_rect(x0, -over, w, h + over, radius, ratio, period);
                let l: CompositionObject = self.ears[0].cast().unwrap();
                let r: CompositionObject = self.ears[1].cast().unwrap();
                spring(&self.comp, &l, "Offset.X", x0 - e, ratio, period);
                spring(&self.comp, &r, "Offset.X", x0 + w, ratio, period);
                let c: CompositionObject = self.clip.cast().unwrap();
                spring(&self.comp, &c, "LeftInset", insets.0, ratio, period);
                spring(&self.comp, &c, "RightInset", insets.1, ratio, period);
                spring(&self.comp, &c, "BottomInset", insets.2, ratio, period);
            }
        }
    }

    /// Geser seluruh notch (menyingkir ke atas saat disembunyikan).
    pub fn stage_y(&self, y: f32, ratio: f32, period: u32) {
        let o: CompositionObject = self.stage.cast().unwrap();
        spring(&self.comp, &o, "Offset.Y", y, ratio, period);
    }
}
