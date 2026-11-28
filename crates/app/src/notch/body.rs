use super::{ears, spring};
use nacre_core::settings::Material;
use nacre_platform::log;
use nacre_ui::gfx::Gfx;
use nacre_ui::glass::{GlassPanel, GlassParams, Tier};
use windows::UI::Color;
use windows::UI::Composition::{AnimationIterationBehavior, CompositionBrush, CompositionColorGradientStop, CompositionDrawingSurface, CompositionObject, CompositionRoundedRectangleGeometry, Compositor, ContainerVisual, InsetClip, SpriteVisual};
use windows_core::Interface;
use windows_numerics::{Vector2, Vector3};

pub const ONYX: (u8, u8, u8) = (10, 10, 12);

pub fn glass_params(radius: f32) -> GlassParams {
    let look = crate::prefs::look(crate::prefs::Role::Notch);
    let mut g = crate::prefs::apply(GlassParams::dark((10.0, 10.0), radius), &look);
    if is_black() {
        g.tint = Color { A: 255, R: ONYX.0, G: ONYX.1, B: ONYX.2 };
        g.solid = Color { A: 245, R: ONYX.0, G: ONYX.1, B: ONYX.2 };
    } else {
        g.tint.A = g.tint.A.max(96);
    }
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
    pub ear: f32,
    pub top_over: f32,
    pub island: f32,
    _depth: ContainerVisual,
    depth_geo: CompositionRoundedRectangleGeometry,
    sheen: SpriteVisual,
    aurora: ContainerVisual,
    waves: Vec<(ContainerVisual, SpriteVisual, CompositionColorGradientStop, CompositionColorGradientStop)>,
    wave_key: (i32, i32),
    aurora_on: bool,
}

const WAVES: [(f32, f32, u32); 3] = [(0.30, 0.78, 7_200), (0.62, 0.70, 9_400), (0.86, 0.62, 11_800)];

fn clear(c: [u8; 3], a: u8) -> Color {
    Color { A: a, R: c[0], G: c[1], B: c[2] }
}

impl Body {
    pub fn new(comp: &Compositor, gfx: &Gfx, root: &ContainerVisual, win: (i32, i32), dpi: f32, tier: Tier) -> windows_core::Result<Body> {
        let stage = comp.CreateContainerVisual()?;
        root.Children()?.InsertAtTop(&stage)?;
        let panel = GlassPanel::new(comp, &stage, glass_params(15.0 * dpi))?;
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
        let depth = comp.CreateContainerVisual()?;
        stage.Children()?.InsertAtTop(&depth)?;
        let depth_geo = comp.CreateRoundedRectangleGeometry()?;
        let gclip = comp.CreateGeometricClipWithGeometry(&depth_geo)?;
        depth.SetClip(&gclip)?;
        let sheen = comp.CreateSpriteVisual()?;
        let sb = comp.CreateLinearGradientBrush()?;
        sb.SetStartPoint(Vector2 { X: 0.0, Y: 0.0 })?;
        sb.SetEndPoint(Vector2 { X: 0.0, Y: 1.0 })?;
        sb.ColorStops()?.Append(&comp.CreateColorGradientStopWithOffsetAndColor(0.0, Color { A: 0, R: 255, G: 255, B: 255 })?)?;
        sb.ColorStops()?.Append(&comp.CreateColorGradientStopWithOffsetAndColor(0.55, Color { A: 0, R: 255, G: 255, B: 255 })?)?;
        sb.ColorStops()?.Append(&comp.CreateColorGradientStopWithOffsetAndColor(1.0, Color { A: 16, R: 255, G: 255, B: 255 })?)?;
        sheen.SetBrush(&sb)?;
        depth.Children()?.InsertAtTop(&sheen)?;
        let aurora = comp.CreateContainerVisual()?;
        aurora.SetOpacity(0.0)?;
        depth.Children()?.InsertAtTop(&aurora)?;
        let mut waves = Vec::new();
        for _ in WAVES {
            let holder = comp.CreateContainerVisual()?;
            let sprite = comp.CreateSpriteVisual()?;
            let brush = comp.CreateRadialGradientBrush()?;
            let s0 = comp.CreateColorGradientStopWithOffsetAndColor(0.0, clear([120, 160, 255], 190))?;
            let s1 = comp.CreateColorGradientStopWithOffsetAndColor(1.0, clear([120, 160, 255], 0))?;
            brush.ColorStops()?.Append(&s0)?;
            brush.ColorStops()?.Append(&s1)?;
            sprite.SetBrush(&brush)?;
            holder.Children()?.InsertAtTop(&sprite)?;
            aurora.Children()?.InsertAtTop(&holder)?;
            waves.push((holder, sprite, s0, s1));
        }
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
            island: 0.0,
            _depth: depth,
            depth_geo,
            sheen,
            aurora,
            waves,
            wave_key: (0, 0),
            aurora_on: false,
        };
        body.paint_ears(tier);
        Ok(body)
    }

    pub fn resize(&self, win: (i32, i32)) {
        for c in [&self.content, &self.ghost] {
            let _ = c.SetSize(Vector2 { X: win.0 as f32, Y: win.1 as f32 });
        }
    }

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

    pub fn set_geometry(&mut self, win: (i32, i32), x0: f32, w: f32, h: f32, radius: f32, springy: Option<(f32, u32)>) {
        let island = self.island > 0.0;
        let (e, over) = (self.ear, if island { 0.0 } else { self.top_over });
        self.layout_depth(x0, w, h, radius, over, springy);
        for v in &self.ears {
            let _ = v.SetOpacity(if island { 0.0 } else { 1.0 });
        }
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

    fn layout_depth(&mut self, x0: f32, w: f32, h: f32, radius: f32, over: f32, springy: Option<(f32, u32)>) {
        let g = &self.depth_geo;
        match springy {
            None => {
                let _ = g.SetOffset(Vector2 { X: x0, Y: -over });
                let _ = g.SetSize(Vector2 { X: w, Y: h + over });
                let _ = g.SetCornerRadius(Vector2 { X: radius, Y: radius });
            }
            Some((ratio, period)) => {
                let o: CompositionObject = g.cast().unwrap();
                spring(&self.comp, &o, "Offset.X", x0, ratio, period);
                spring(&self.comp, &o, "Offset.Y", -over, ratio, period);
                spring(&self.comp, &o, "Size.X", w, ratio, period);
                spring(&self.comp, &o, "Size.Y", h + over, ratio, period);
                spring(&self.comp, &o, "CornerRadius.X", radius, ratio, period);
                spring(&self.comp, &o, "CornerRadius.Y", radius, ratio, period);
            }
        }
        let _ = self.sheen.SetOffset(Vector3 { X: x0, Y: -over, Z: 0.0 });
        let _ = self.sheen.SetSize(Vector2 { X: w, Y: h + over });
        let key = ((w / 8.0).round() as i32, (h / 8.0).round() as i32);
        if key == self.wave_key {
            for (i, (holder, _, _, _)) in self.waves.iter().enumerate() {
                let (fx, _, _) = WAVES[i];
                let (bw, bh) = (w * 0.8, (h * 0.8).max(22.0));
                let _ = holder.SetOffset(Vector3 { X: x0 + w * fx - bw / 2.0, Y: h - bh * 0.5, Z: 0.0 });
            }
            return;
        }
        self.wave_key = key;
        for (i, (holder, sprite, _, _)) in self.waves.iter().enumerate() {
            let (fx, _, ms) = WAVES[i];
            let (bw, bh) = (w * 0.8, (h * 0.8).max(22.0));
            let _ = holder.SetOffset(Vector3 { X: x0 + w * fx - bw / 2.0, Y: h - bh * 0.5, Z: 0.0 });
            let _ = holder.SetSize(Vector2 { X: bw, Y: bh });
            let _ = holder.SetCenterPoint(Vector3 { X: bw / 2.0, Y: bh / 2.0, Z: 0.0 });
            let _ = sprite.SetSize(Vector2 { X: bw, Y: bh });
            let drift = w * 0.16;
            let go = || -> windows_core::Result<()> {
                let a = self.comp.CreateScalarKeyFrameAnimation()?;
                let ease = crate::motion::ease(&self.comp)?;
                let dir = if i % 2 == 0 { 1.0 } else { -1.0 };
                a.InsertKeyFrame(0.0, 0.0)?;
                a.InsertKeyFrameWithEasingFunction(0.5, dir * drift, &ease)?;
                a.InsertKeyFrameWithEasingFunction(1.0, 0.0, &ease)?;
                a.SetDuration(windows::Foundation::TimeSpan { Duration: ms as i64 * 10_000 })?;
                a.SetIterationBehavior(AnimationIterationBehavior::Forever)?;
                sprite.cast::<CompositionObject>()?.StartAnimation(&windows_core::HSTRING::from("Offset.X"), &a)
            };
            let _ = go();
        }
    }

    pub fn set_aurora(&mut self, rgb: Option<[u8; 3]>) {
        let on = rgb.is_some();
        if let Some(base) = rgb {
            let pal = nacre_core::notch::tint::aurora(base);
            for (i, (_, _, s0, s1)) in self.waves.iter().enumerate() {
                let alpha = [150u8, 120, 105][i];
                let _ = crate::motion::color(&self.comp, s0, clear(pal[i], alpha), 700);
                let _ = s1.SetColor(clear(pal[i], 0));
            }
        }
        if on != self.aurora_on {
            self.aurora_on = on;
            let _ = crate::motion::scalar(&self.comp, &self.aurora, "Opacity", if on { 0.0 } else { 1.0 }, if on { 1.0 } else { 0.0 }, if on { 600 } else { 450 });
            let _ = self.aurora.SetOpacity(if on { 1.0 } else { 0.0 });
        }
    }

    pub fn react(&self, levels: [f32; 3]) {
        if !self.aurora_on {
            return;
        }
        for (i, (holder, _, _, _)) in self.waves.iter().enumerate() {
            let (_, base, _) = WAVES[i];
            let v = levels[i].clamp(0.0, 1.0);
            let _ = holder.SetScale(Vector3 { X: 1.0 + 0.1 * v, Y: base + 0.9 * v, Z: 1.0 });
            let _ = holder.SetOpacity(0.5 + 0.5 * v);
        }
    }

    pub fn stage_y(&self, y: f32, ratio: f32, period: u32) {
        let o: CompositionObject = self.stage.cast().unwrap();
        spring(&self.comp, &o, "Offset.Y", y + self.island, ratio, period);
    }

    pub fn set_island(&mut self, gap: f32, shown: bool) {
        if (gap - self.island).abs() < 0.5 {
            return;
        }
        self.island = gap;
        if shown {
            if let Ok(o) = self.stage.cast::<CompositionObject>() {
                let _ = o.StopAnimation(&windows_core::HSTRING::from("Offset.Y"));
            }
            let _ = self.stage.SetOffset(Vector3 { X: 0.0, Y: gap, Z: 0.0 });
        }
    }
}
