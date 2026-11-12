//! Visual satu slot dock: simpul yang diskalakan (ikon), titik "berjalan", dan animasi spring-nya.

use windows::Foundation::TimeSpan;
use windows::UI::Color;
use windows::UI::Composition::{
    CompositionClip, CompositionDrawingSurface, CompositionStretch, ContainerVisual, Compositor,
    SpringScalarNaturalMotionAnimation, SpriteVisual,
};
use windows_core::{Interface, Result, HSTRING};
use windows_numerics::{Vector2, Vector3};

fn ms(v: u32) -> TimeSpan {
    TimeSpan { Duration: v as i64 * 10_000 }
}

pub fn spring(comp: &Compositor, ratio: f32, period_ms: u32) -> Result<SpringScalarNaturalMotionAnimation> {
    let a = comp.CreateSpringScalarAnimation()?;
    a.SetDampingRatio(ratio)?;
    a.SetPeriod(ms(period_ms))?;
    Ok(a)
}

fn f32_ref(v: f32) -> Result<windows::Foundation::IReference<f32>> {
    windows::Foundation::PropertyValue::CreateSingle(v)?.cast()
}

pub fn rounded_clip(comp: &Compositor, w: f32, h: f32, r: f32) -> Result<CompositionClip> {
    let g = comp.CreateRoundedRectangleGeometry()?;
    g.SetSize(Vector2 { X: w, Y: h })?;
    g.SetCornerRadius(Vector2 { X: r, Y: r })?;
    let c = comp.CreateGeometricClip()?;
    c.SetGeometry(&g)?;
    c.cast()
}

pub enum Kind {
    Start,
    App { path: String },
    Separator,
}

pub struct View {
    pub kind: Kind,
    pub node: ContainerVisual,
    pub icon: Option<SpriteVisual>,
    pub dot: Option<SpriteVisual>,
    base_w: f32,
    icon_px: f32,
    ax: SpringScalarNaturalMotionAnimation,
    ay: SpringScalarNaturalMotionAnimation,
    ox: SpringScalarNaturalMotionAnimation,
    dox: SpringScalarNaturalMotionAnimation,
    bounce: SpringScalarNaturalMotionAnimation,
}

impl View {
    fn base(comp: &Compositor, content: &ContainerVisual, kind: Kind, base_w: f32, icon_px: f32) -> Result<View> {
        let node = comp.CreateContainerVisual()?;
        node.SetSize(Vector2 { X: base_w, Y: icon_px })?;
        // Skala dari tengah-bawah: ikon membesar ke atas, sejajar dasar dock seperti di macOS.
        node.SetCenterPoint(Vector3 { X: base_w / 2.0, Y: icon_px, Z: 0.0 })?;
        content.Children()?.InsertAtTop(&node)?;
        Ok(View {
            kind,
            node,
            icon: None,
            dot: None,
            base_w,
            icon_px,
            ax: spring(comp, 0.72, 80)?,
            ay: spring(comp, 0.72, 80)?,
            ox: spring(comp, 0.75, 80)?,
            dox: spring(comp, 0.75, 80)?,
            bounce: spring(comp, 0.45, 90)?,
        })
    }

    /// Ikon aplikasi. Selagi ikon belum termuat, tampil kotak abu-abu netral.
    pub fn app(comp: &Compositor, content: &ContainerVisual, path: String, icon_px: f32, running: bool, dpi: f32) -> Result<View> {
        let mut v = View::base(comp, content, Kind::App { path }, icon_px, icon_px)?;
        let sprite = comp.CreateSpriteVisual()?;
        sprite.SetSize(Vector2 { X: icon_px, Y: icon_px })?;
        sprite.SetBrush(&comp.CreateColorBrushWithColor(Color { A: 70, R: 200, G: 200, B: 205 })?)?;
        sprite.SetClip(&rounded_clip(comp, icon_px, icon_px, icon_px * 0.23)?)?;
        v.node.Children()?.InsertAtTop(&sprite)?;
        v.icon = Some(sprite);
        if running {
            let d = (4.0 * dpi).round();
            let dot = comp.CreateSpriteVisual()?;
            dot.SetSize(Vector2 { X: d, Y: d })?;
            dot.SetBrush(&comp.CreateColorBrushWithColor(if crate::prefs::look(crate::prefs::Role::Dock).dark { Color { A: 225, R: 255, G: 255, B: 255 } } else { Color { A: 215, R: 40, G: 40, B: 48 } })?)?;
            dot.SetClip(&rounded_clip(comp, d, d, d / 2.0)?)?;
            content.Children()?.InsertAtTop(&dot)?;
            v.dot = Some(dot);
        }
        Ok(v)
    }

    pub fn start(comp: &Compositor, content: &ContainerVisual, icon_px: f32) -> Result<View> {
        let v = View::base(comp, content, Kind::Start, icon_px, icon_px)?;
        // badan ikon sama besar dengan ikon lain (86% kanvas) bila ikon seragam aktif
        let full = icon_px;
        let icon_px = if crate::prefs::get().dock_plates { (full * nacre_core::iconplate::BODY).round() } else { full };
        let inset = (full - icon_px) / 2.0;
        let tile = comp.CreateSpriteVisual()?;
        tile.SetOffset(Vector3 { X: inset, Y: inset, Z: 0.0 })?;
        tile.SetSize(Vector2 { X: icon_px, Y: icon_px })?;
        let grad = comp.CreateLinearGradientBrush()?;
        grad.SetStartPoint(Vector2 { X: 0.0, Y: 0.0 })?;
        grad.SetEndPoint(Vector2 { X: 1.0, Y: 1.0 })?;
        let stops = grad.ColorStops()?;
        stops.Append(&comp.CreateColorGradientStopWithOffsetAndColor(0.0, Color { A: 255, R: 120, G: 170, B: 255 })?)?;
        stops.Append(&comp.CreateColorGradientStopWithOffsetAndColor(1.0, Color { A: 255, R: 150, G: 90, B: 240 })?)?;
        tile.SetBrush(&grad)?;
        tile.SetClip(&rounded_clip(comp, icon_px, icon_px, icon_px * 0.23)?)?;
        v.node.Children()?.InsertAtTop(&tile)?;
        // tanda sendiri: empat kelopak bulat (bukan logo siapa pun)
        let petal = icon_px * 0.17;
        let c = inset + icon_px / 2.0;
        let off = icon_px * 0.15;
        for (dx, dy) in [(-off, -off), (off, -off), (-off, off), (off, off)] {
            let p = comp.CreateSpriteVisual()?;
            p.SetSize(Vector2 { X: petal * 2.0, Y: petal * 2.0 })?;
            p.SetOffset(Vector3 { X: c + dx - petal, Y: c + dy - petal, Z: 0.0 })?;
            p.SetBrush(&comp.CreateColorBrushWithColor(Color { A: 230, R: 255, G: 255, B: 255 })?)?;
            p.SetClip(&rounded_clip(comp, petal * 2.0, petal * 2.0, petal)?)?;
            v.node.Children()?.InsertAtTop(&p)?;
        }
        Ok(v)
    }

    pub fn separator(comp: &Compositor, content: &ContainerVisual, width: f32, icon_px: f32) -> Result<View> {
        let v = View::base(comp, content, Kind::Separator, width, icon_px)?;
        let line = comp.CreateSpriteVisual()?;
        let (lw, lh) = (1.0f32.max((width / 10.0).round()), icon_px * 0.7);
        line.SetSize(Vector2 { X: lw, Y: lh })?;
        line.SetOffset(Vector3 { X: (width - lw) / 2.0, Y: (icon_px - lh) / 2.0, Z: 0.0 })?;
        line.SetBrush(&comp.CreateColorBrushWithColor(Color { A: 90, R: 255, G: 255, B: 255 })?)?;
        v.node.Children()?.InsertAtTop(&line)?;
        Ok(v)
    }

    /// Pasang ikon yang sudah dimuat menggantikan kotak sementara.
    pub fn set_surface(&mut self, comp: &Compositor, surface: &CompositionDrawingSurface) -> Result<()> {
        if let Some(sprite) = &self.icon {
            let brush = comp.CreateSurfaceBrushWithSurface(surface)?;
            brush.SetStretch(CompositionStretch::Uniform)?;
            let _ = brush.SetBitmapInterpolationMode(windows::UI::Composition::CompositionBitmapInterpolationMode::Linear);
            sprite.SetBrush(&brush)?;
            // Ikon asli sudah punya bentuknya sendiri (perisai, lingkaran, dll.): jangan dipotong klip kotak penampung.
            let _ = sprite.SetClip(None::<&windows::UI::Composition::CompositionClip>);
        }
        Ok(())
    }

    /// Tempatkan slot. `x`,`w`,`scale` dari `layout`; `y` = atas ikon saat diam; `dot_y` = atas titik.
    pub fn place(&self, x: f32, w: f32, scale: f32, y: f32, dot_y: f32, animate: bool) -> Result<()> {
        let left = x + w / 2.0 - self.base_w / 2.0;
        let dot_x = x + w / 2.0;
        if animate {
            self.ox.SetFinalValue(&f32_ref(left)?)?;
            self.node.StartAnimation(&HSTRING::from("Offset.X"), &self.ox)?;
            self.ax.SetFinalValue(&f32_ref(scale)?)?;
            self.node.StartAnimation(&HSTRING::from("Scale.X"), &self.ax)?;
            self.ay.SetFinalValue(&f32_ref(scale)?)?;
            self.node.StartAnimation(&HSTRING::from("Scale.Y"), &self.ay)?;
        } else {
            self.stop_animations();
            self.node.SetOpacity(1.0)?;
            self.node.SetOffset(Vector3 { X: left, Y: y, Z: 0.0 })?;
            self.node.SetScale(Vector3 { X: scale, Y: scale, Z: 1.0 })?;
        }
        if let Some(dot) = &self.dot {
            let half = dot.Size()?.X / 2.0;
            if animate {
                self.dox.SetFinalValue(&f32_ref(dot_x - half)?)?;
                dot.StartAnimation(&HSTRING::from("Offset.X"), &self.dox)?;
            } else {
                dot.SetOffset(Vector3 { X: dot_x - half, Y: dot_y, Z: 0.0 })?;
            }
        }
        Ok(())
    }

    fn stop_animations(&self) {
        for p in ["Offset.X", "Offset.Y", "Scale.X", "Scale.Y"] {
            let _ = self.node.StopAnimation(&HSTRING::from(p));
        }
        if let Some(dot) = &self.dot {
            let _ = dot.StopAnimation(&HSTRING::from("Offset.X"));
        }
    }

    /// Ikon diangkat dan mengikuti kursor selama drag-reorder (animasi dihentikan agar tidak melawan).
    pub fn drag_to(&self, left: f32, y: f32) -> Result<()> {
        self.stop_animations();
        self.node.SetOffset(Vector3 { X: left, Y: y, Z: 0.0 })?;
        self.node.SetScale(Vector3 { X: 1.1, Y: 1.1, Z: 1.0 })?;
        self.node.SetOpacity(0.85)?;
        Ok(())
    }

    /// Satu pantulan vertikal saat app diluncurkan.
    pub fn bounce(&self, rest_y: f32) -> Result<()> {
        self.bounce.SetInitialValue(&f32_ref(rest_y - self.icon_px * 0.45)?)?;
        self.bounce.SetFinalValue(&f32_ref(rest_y)?)?;
        self.node.StartAnimation(&HSTRING::from("Offset.Y"), &self.bounce)?;
        Ok(())
    }
}


