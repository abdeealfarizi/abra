use super::effect_graph::{backdrop_blur_brush, backdrop_refract_brush};
use windows::UI::Color;
use windows::UI::Composition::{
    CompositionBrush, CompositionClip, CompositionColorBrush, CompositionRoundedRectangleGeometry, ContainerVisual, Compositor, ShapeVisual,
    SpriteVisual,
};
use windows_core::{Interface, Result};
use windows_numerics::{Vector2, Vector3};

/// Kekuatan tepi kanan-bawah relatif terhadap kiri-atas: kilau tepi Tahoe terlihat di dua sisi.
const RIM_BOTTOM: f32 = 0.8;

/// Tingkat material, dari paling mahal ke paling murah. Perpindahan hanya menukar kuas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
    /// Blur penuh + saturasi + sorotan.
    Full,
    /// Blur kecil (lebih murah di GPU lemah).
    Light,
    /// Tanpa blur: kuas translusen statis. Untuk transparansi dimatikan, Battery Saver, sesi jarak jauh.
    Solid,
}

#[derive(Clone, Copy)]
pub struct GlassParams {
    pub size: (f32, f32),
    pub radius: f32,
    pub blur_sigma: f32,
    pub saturation: f32,
    pub tint: Color,
    pub rim: Color,
    /// Dasar untuk Tier::Solid (tidak ada blur yang menutupi isi di belakang).
    pub solid: Color,
    /// 0..1: kekuatan sorotan spekular (0 = permukaan rata, mis. notch hitam utuh).
    pub highlight: f32,
}

impl GlassParams {
    pub fn light(size: (f32, f32), radius: f32) -> Self {
        GlassParams {
            size,
            radius,
            blur_sigma: 24.0,
            saturation: 1.6,
            tint: Color { A: 46, R: 255, G: 255, B: 255 },
            rim: Color { A: 140, R: 255, G: 255, B: 255 },
            solid: Color { A: 215, R: 226, G: 228, B: 236 },
            highlight: 1.0,
        }
    }

    pub fn dark(size: (f32, f32), radius: f32) -> Self {
        GlassParams {
            tint: Color { A: 150, R: 20, G: 20, B: 24 },
            rim: Color { A: 70, R: 255, G: 255, B: 255 },
            solid: Color { A: 235, R: 28, G: 28, B: 32 },
            ..Self::light(size, radius)
        }
    }
}

/// Panel kaca yang ukuran dan posisinya bisa diubah setiap saat.
/// Lapisan: blur backdrop → tint → sorotan spekular (kiri-atas) → tepi 1 px bergradien,
/// semuanya di-clip rounded-rect beranti-alias.
pub struct GlassPanel {
    comp: Compositor,
    p: GlassParams,
    tier: Tier,
    glass: SpriteVisual,
    tint: SpriteVisual,
    tint_brush: CompositionColorBrush,
    rim_stops: windows::UI::Composition::CompositionColorGradientStopCollection,
    /// Peta distorsi + geser maksimum (px) bila refraksi tepi aktif.
    refr: Option<(CompositionBrush, f32)>,
    refr_size: (u32, u32),
    spec: SpriteVisual,
    rim: ShapeVisual,
    clip_geo: CompositionRoundedRectangleGeometry,
    rim_geo: CompositionRoundedRectangleGeometry,
}

impl GlassPanel {
    pub fn new(comp: &Compositor, parent: &ContainerVisual, p: GlassParams) -> Result<GlassPanel> {
        let clip_geo = comp.CreateRoundedRectangleGeometry()?;
        clip_geo.SetCornerRadius(Vector2 { X: p.radius, Y: p.radius })?;
        let clip = comp.CreateGeometricClip()?;
        clip.SetGeometry(&clip_geo)?;
        let clip: CompositionClip = clip.cast()?;

        let glass = comp.CreateSpriteVisual()?;
        glass.SetClip(&clip)?;

        let tint = comp.CreateSpriteVisual()?;
        let tint_brush = comp.CreateColorBrushWithColor(p.tint)?;
        tint.SetBrush(&tint_brush)?;
        tint.SetClip(&clip)?;

        // Sorotan: putih lembut dari sudut kiri-atas yang memudar diagonal, meniru cahaya yang jatuh di kaca.
        let spec = comp.CreateSpriteVisual()?;
        let sg = comp.CreateLinearGradientBrush()?;
        sg.SetStartPoint(Vector2 { X: 0.0, Y: 0.0 })?;
        sg.SetEndPoint(Vector2 { X: 0.0, Y: 1.0 })?;
        let stops = sg.ColorStops()?;
        stops.Append(&comp.CreateColorGradientStopWithOffsetAndColor(0.0, Color { A: 70, R: 255, G: 255, B: 255 })?)?;
        stops.Append(&comp.CreateColorGradientStopWithOffsetAndColor(0.34, Color { A: 0, R: 255, G: 255, B: 255 })?)?;
        spec.SetBrush(&sg)?;
        spec.SetClip(&clip)?;

        // Tepi: terang di atas, redup di bawah.
        let rim_geo = comp.CreateRoundedRectangleGeometry()?;
        rim_geo.SetOffset(Vector2 { X: 0.5, Y: 0.5 })?;
        rim_geo.SetCornerRadius(Vector2 { X: (p.radius - 0.5).max(0.0), Y: (p.radius - 0.5).max(0.0) })?;
        let rim_shape = comp.CreateSpriteShapeWithGeometry(&rim_geo)?;
        let rg = comp.CreateLinearGradientBrush()?;
        rg.SetStartPoint(Vector2 { X: 0.0, Y: 0.0 })?;
        rg.SetEndPoint(Vector2 { X: 1.0, Y: 1.0 })?;
        let rs = rg.ColorStops()?;
        let rim_stops = rs.clone();
        let top = p.rim;
        let mid = Color { A: (p.rim.A as f32 * 0.14) as u8, ..p.rim };
        let bottom = Color { A: (p.rim.A as f32 * RIM_BOTTOM) as u8, ..p.rim };
        rs.Append(&comp.CreateColorGradientStopWithOffsetAndColor(0.0, top)?)?;
        rs.Append(&comp.CreateColorGradientStopWithOffsetAndColor(0.5, mid)?)?;
        rs.Append(&comp.CreateColorGradientStopWithOffsetAndColor(1.0, bottom)?)?;
        rim_shape.SetStrokeBrush(&rg)?;
        rim_shape.SetStrokeThickness(1.5)?;
        let rim = comp.CreateShapeVisual()?;
        rim.Shapes()?.Append(&rim_shape)?;

        let kids = parent.Children()?;
        kids.InsertAtTop(&glass)?;
        kids.InsertAtTop(&tint)?;
        kids.InsertAtTop(&spec)?;
        kids.InsertAtTop(&rim)?;

        let mut panel = GlassPanel { comp: comp.clone(), p, tier: Tier::Solid, glass, tint, tint_brush, rim_stops, refr: None, refr_size: (0, 0), spec, rim, clip_geo, rim_geo };
        panel.set_tier(Tier::Full)?;
        panel.set_rect(0.0, 0.0, p.size.0, p.size.1)?;
        Ok(panel)
    }

    pub fn tier(&self) -> Tier {
        self.tier
    }

    /// Tukar material. Murah: hanya mengganti kuas dan opasitas lapisan.
    pub fn set_tier(&mut self, tier: Tier) -> Result<()> {
        if tier == self.tier && self.glass.Brush().is_ok() {
            return Ok(());
        }
        self.apply_tier(tier)
    }

    /// Ganti parameter material (blur, saturasi, tint, tepi, dasar solid) saat jalan, mis. dari Pengaturan.
    pub fn set_params(&mut self, p: GlassParams) -> Result<()> {
        self.p.blur_sigma = p.blur_sigma;
        self.p.saturation = p.saturation;
        self.p.tint = p.tint;
        self.p.rim = p.rim;
        self.p.solid = p.solid;
        self.p.highlight = p.highlight;
        self.tint_brush.SetColor(p.tint)?;
        self.rim_stops.GetAt(0)?.SetColor(p.rim)?;
        self.rim_stops.GetAt(1)?.SetColor(Color { A: (p.rim.A as f32 * 0.14) as u8, ..p.rim })?;
        self.rim_stops.GetAt(2)?.SetColor(Color { A: (p.rim.A as f32 * RIM_BOTTOM) as u8, ..p.rim })?;
        let t = self.tier;
        self.apply_tier(t)
    }

    /// Aktifkan refraksi tepi untuk panel berukuran `w`×`h` px (melewati bila ukuran hampir sama dengan peta saat ini).
    /// `band`: tebal pita (px), `strength`: geser maksimum (px). `None` mematikannya.
    pub fn set_refraction(&mut self, gfx: &crate::gfx::Gfx, size: Option<(u32, u32, f32, f32, f32)>) -> Result<()> {
        let Some((w, h, radius, band, strength)) = size else {
            if self.refr.take().is_some() {
                self.refr_size = (0, 0);
                let t = self.tier;
                self.apply_tier(t)?;
            }
            return Ok(());
        };
        let (w, h) = (w.clamp(16, 4096), h.clamp(16, 1024));
        if self.refr.is_some() && (w as i32 - self.refr_size.0 as i32).abs() < 3 && (h as i32 - self.refr_size.1 as i32).abs() < 3 {
            return Ok(());
        }
        let data = super::refract::displacement_map(w, h, radius, band);
        let surface = gfx.surface_from_bgra(w, h, &data)?;
        let brush = self.comp.CreateSurfaceBrushWithSurface(&surface)?;
        brush.SetStretch(windows::UI::Composition::CompositionStretch::Fill)?;
        // Tanpa interpolasi kasar: peta halus agar pita tidak bergerigi.
        let _ = brush.SetBitmapInterpolationMode(windows::UI::Composition::CompositionBitmapInterpolationMode::Linear);
        self.refr = Some((brush.cast()?, strength));
        self.refr_size = (w, h);
        let t = self.tier;
        self.apply_tier(t)
    }

    fn apply_tier(&mut self, tier: Tier) -> Result<()> {
        let effect = match tier {
            Tier::Full => Some(match &self.refr {
                Some((map, k)) => backdrop_refract_brush(&self.comp, self.p.blur_sigma, self.p.saturation, map, *k),
                None => backdrop_blur_brush(&self.comp, self.p.blur_sigma, self.p.saturation),
            }),
            Tier::Light => Some(backdrop_blur_brush(&self.comp, self.p.blur_sigma * 0.4, self.p.saturation * 0.8)),
            Tier::Solid => None,
        };
        // Bila efek blur gagal dibuat, jangan menjatuhkan seluruh dock/notch: pakai kuas solid.
        let (brush, tier): (CompositionBrush, Tier) = match effect {
            Some(Ok(b)) => (b.cast()?, tier),
            _ => (self.comp.CreateColorBrushWithColor(self.p.solid)?.cast()?, Tier::Solid),
        };
        self.glass.SetBrush(&brush)?;
        // Tanpa blur tint sudah menyatu dengan dasar; jangan menumpuk dua kali.
        self.tint.SetOpacity(if tier == Tier::Solid { 0.0 } else { 1.0 })?;
        self.spec.SetOpacity(self.p.highlight * if tier == Tier::Solid { 0.5 } else { 1.0 })?;
        self.tier = tier;
        Ok(())
    }

    /// Pindahkan/ubah ukuran/radius dengan spring di compositor (nol CPU di proses kita selama animasi).
    pub fn animate_rect(&self, x: f32, y: f32, w: f32, h: f32, radius: f32, ratio: f32, period_ms: u32) -> Result<()> {
        use windows::Foundation::{PropertyValue, TimeSpan};
        use windows::UI::Composition::CompositionObject;
        use windows_core::HSTRING;
        let comp = &self.comp;
        let go = |obj: &CompositionObject, prop: &str, v: f32| -> Result<()> {
            let a = comp.CreateSpringScalarAnimation()?;
            a.SetDampingRatio(ratio)?;
            a.SetPeriod(TimeSpan { Duration: period_ms as i64 * 10_000 })?;
            a.SetFinalValue(&PropertyValue::CreateSingle(v)?.cast::<windows::Foundation::IReference<f32>>()?)?;
            obj.StartAnimation(&HSTRING::from(prop), &a)
        };
        for v in [&self.glass, &self.tint, &self.spec] {
            let o: CompositionObject = v.cast()?;
            go(&o, "Size.X", w)?;
            go(&o, "Size.Y", h)?;
            go(&o, "Offset.X", x)?;
            go(&o, "Offset.Y", y)?;
        }
        let o: CompositionObject = self.rim.cast()?;
        go(&o, "Size.X", w)?;
        go(&o, "Size.Y", h)?;
        go(&o, "Offset.X", x)?;
        go(&o, "Offset.Y", y)?;
        let g: CompositionObject = self.clip_geo.cast()?;
        go(&g, "Size.X", w)?;
        go(&g, "Size.Y", h)?;
        go(&g, "CornerRadius.X", radius)?;
        go(&g, "CornerRadius.Y", radius)?;
        let r: CompositionObject = self.rim_geo.cast()?;
        go(&r, "Size.X", w - 1.0)?;
        go(&r, "Size.Y", h - 1.0)?;
        go(&r, "CornerRadius.X", (radius - 0.5).max(0.0))?;
        go(&r, "CornerRadius.Y", (radius - 0.5).max(0.0))?;
        Ok(())
    }

    /// Ikat posisi-x dan lebar panel ke ekspresi compositor (mis. mengikuti lebar dock yang berubah saat
    /// magnifikasi) sehingga tidak perlu pembaruan per-gerakan dari proses kita. `make` membuat animasi ekspresi
    /// lengkap dengan parameter acuannya.
    pub fn bind_x_width(
        &self,
        make: &dyn Fn(&str) -> Result<windows::UI::Composition::ExpressionAnimation>,
        x_expr: &str,
        w_expr: &str,
    ) -> Result<()> {
        use windows::UI::Composition::CompositionObject;
        use windows_core::HSTRING;
        let (x, w) = (make(x_expr)?, make(w_expr)?);
        let rim_w = make(&format!("({w_expr}) - 1"))?;
        for v in [&self.glass, &self.tint, &self.spec] {
            let o: CompositionObject = v.cast()?;
            o.StartAnimation(&HSTRING::from("Offset.X"), &x)?;
            o.StartAnimation(&HSTRING::from("Size.X"), &w)?;
        }
        let o: CompositionObject = self.rim.cast()?;
        o.StartAnimation(&HSTRING::from("Offset.X"), &x)?;
        o.StartAnimation(&HSTRING::from("Size.X"), &w)?;
        let g: CompositionObject = self.clip_geo.cast()?;
        g.StartAnimation(&HSTRING::from("Size.X"), &w)?;
        let r: CompositionObject = self.rim_geo.cast()?;
        r.StartAnimation(&HSTRING::from("Size.X"), &rim_w)?;
        Ok(())
    }

    pub fn set_radius(&self, r: f32) -> Result<()> {
        self.clip_geo.SetCornerRadius(Vector2 { X: r, Y: r })?;
        self.rim_geo.SetCornerRadius(Vector2 { X: (r - 0.5).max(0.0), Y: (r - 0.5).max(0.0) })?;
        Ok(())
    }

    pub fn set_rect(&self, x: f32, y: f32, w: f32, h: f32) -> Result<()> {
        let size = Vector2 { X: w, Y: h };
        let at = Vector3 { X: x, Y: y, Z: 0.0 };
        for v in [&self.glass, &self.tint, &self.spec] {
            v.SetSize(size)?;
            v.SetOffset(at)?;
        }
        self.rim.SetSize(size)?;
        self.rim.SetOffset(at)?;
        self.clip_geo.SetSize(size)?;
        self.rim_geo.SetSize(Vector2 { X: (w - 1.0).max(0.0), Y: (h - 1.0).max(0.0) })?;
        Ok(())
    }
}

/// Pembungkus ringkas untuk panel tetap.
pub fn add_panel(comp: &Compositor, parent: &ContainerVisual, p: GlassParams) -> Result<()> {
    GlassPanel::new(comp, parent, p)?;
    Ok(())
}


