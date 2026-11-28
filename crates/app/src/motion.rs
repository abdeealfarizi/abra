use windows::Foundation::TimeSpan;
use windows::UI::Composition::{AnimationDelayBehavior, CompositionEasingFunction, CompositionObject, Compositor, ContainerVisual};
use windows_core::{Interface, Result, HSTRING};
use windows_numerics::{Vector2, Vector3};

pub const STAGE_SWAP_MS: u32 = 340;
pub const STRIP_SLIDE_MS: u32 = 240;
pub const CARD_HOVER_MS: u32 = 160;
pub const SWITCHER_MOVE_MS: u32 = 90;
pub const CC_OPEN_MS: u32 = 420;
pub const CC_CLOSE_MS: u32 = 170;
pub const CC_STAGGER_MS: u32 = 22;

pub fn stage_ms(base: u32) -> u32 {
    (base as u64 * 100 / crate::prefs::get().stage_speed.max(10) as u64).max(40) as u32
}

fn dur(ms: u32) -> TimeSpan {
    TimeSpan { Duration: ms as i64 * 10_000 }
}

pub fn ease(comp: &Compositor) -> Result<CompositionEasingFunction> {
    comp.CreateCubicBezierEasingFunction(Vector2 { X: 0.2, Y: 0.0 }, Vector2 { X: 0.0, Y: 1.0 })?.cast()
}

pub fn pop_in(comp: &Compositor, v: &ContainerVisual, center: Vector3, from: f32, over: f32, delay_ms: u32, ms: u32) -> Result<()> {
    v.SetCenterPoint(center)?;
    let e = ease(comp)?;
    let bounce = |a: &dyn Fn() -> Result<windows::UI::Composition::KeyFrameAnimation>| -> Result<()> {
        let a = a()?;
        a.SetDuration(dur(ms))?;
        if delay_ms > 0 {
            a.SetDelayTime(dur(delay_ms))?;
            a.SetDelayBehavior(AnimationDelayBehavior::SetInitialValueBeforeDelay)?;
        }
        Ok(())
    };
    let s = comp.CreateVector3KeyFrameAnimation()?;
    s.InsertKeyFrame(0.0, Vector3 { X: from, Y: from, Z: 1.0 })?;
    s.InsertKeyFrameWithEasingFunction(0.62, Vector3 { X: over, Y: over, Z: 1.0 }, &e)?;
    s.InsertKeyFrameWithEasingFunction(1.0, Vector3 { X: 1.0, Y: 1.0, Z: 1.0 }, &e)?;
    bounce(&|| s.cast())?;
    let o = comp.CreateScalarKeyFrameAnimation()?;
    o.InsertKeyFrame(0.0, 0.0)?;
    o.InsertKeyFrameWithEasingFunction(0.45, 1.0, &e)?;
    o.InsertKeyFrame(1.0, 1.0)?;
    bounce(&|| o.cast())?;
    let obj = v.cast::<CompositionObject>()?;
    obj.StartAnimation(&HSTRING::from("Scale"), &s)?;
    obj.StartAnimation(&HSTRING::from("Opacity"), &o)
}

pub fn pop_out(comp: &Compositor, v: &ContainerVisual, center: Vector3, to: f32, ms: u32) -> Result<()> {
    v.SetCenterPoint(center)?;
    let e = ease(comp)?;
    let s = comp.CreateVector3KeyFrameAnimation()?;
    s.InsertKeyFrame(0.0, Vector3 { X: 1.0, Y: 1.0, Z: 1.0 })?;
    s.InsertKeyFrameWithEasingFunction(1.0, Vector3 { X: to, Y: to, Z: 1.0 }, &e)?;
    s.SetDuration(dur(ms))?;
    let o = comp.CreateScalarKeyFrameAnimation()?;
    o.InsertKeyFrame(0.0, 1.0)?;
    o.InsertKeyFrameWithEasingFunction(1.0, 0.0, &e)?;
    o.SetDuration(dur(ms))?;
    let obj = v.cast::<CompositionObject>()?;
    obj.StartAnimation(&HSTRING::from("Scale"), &s)?;
    obj.StartAnimation(&HSTRING::from("Opacity"), &o)
}

pub fn scalar(comp: &Compositor, obj: &impl Interface, prop: &str, from: f32, to: f32, ms: u32) -> Result<()> {
    let e = ease(comp)?;
    let a = comp.CreateScalarKeyFrameAnimation()?;
    a.InsertKeyFrame(0.0, from)?;
    a.InsertKeyFrameWithEasingFunction(1.0, to, &e)?;
    a.SetDuration(dur(ms))?;
    obj.cast::<CompositionObject>()?.StartAnimation(&HSTRING::from(prop), &a)
}

pub fn color(comp: &Compositor, brush: &impl Interface, to: windows::UI::Color, ms: u32) -> Result<()> {
    let a = comp.CreateColorKeyFrameAnimation()?;
    a.InsertKeyFrameWithEasingFunction(1.0, to, &ease(comp)?)?;
    a.SetDuration(dur(ms))?;
    brush.cast::<CompositionObject>()?.StartAnimation(&HSTRING::from("Color"), &a)
}

pub fn vec3(comp: &Compositor, obj: &impl Interface, prop: &str, from: Vector3, to: Vector3, ms: u32) -> Result<()> {
    let e = ease(comp)?;
    let a = comp.CreateVector3KeyFrameAnimation()?;
    a.InsertKeyFrame(0.0, from)?;
    a.InsertKeyFrameWithEasingFunction(1.0, to, &e)?;
    a.SetDuration(dur(ms))?;
    obj.cast::<CompositionObject>()?.StartAnimation(&HSTRING::from(prop), &a)
}

pub const HOVER_POP_MS: u32 = 280;
pub const HOVER_ROW: f32 = 1.03;
pub const HOVER_BUTTON: f32 = 1.08;

pub fn fit_pop(want: f32, half: f32, room: f32) -> f32 {
    if half <= 0.0 {
        return want;
    }
    want.min(1.0 + (room - 1.0).max(0.0) / (1.3 * half))
}

pub fn pop_ease(comp: &Compositor) -> Result<CompositionEasingFunction> {
    comp.CreateCubicBezierEasingFunction(Vector2 { X: 0.3, Y: 2.0 }, Vector2 { X: 0.32, Y: 1.0 })?.cast()
}

pub fn slide_pop(comp: &Compositor, v: &impl Interface, from: Vector3, to: Vector3) -> Result<()> {
    let e: CompositionEasingFunction = comp.CreateCubicBezierEasingFunction(Vector2 { X: 0.3, Y: 1.45 }, Vector2 { X: 0.35, Y: 1.0 })?.cast()?;
    let a = comp.CreateVector3KeyFrameAnimation()?;
    a.InsertKeyFrame(0.0, from)?;
    a.InsertKeyFrameWithEasingFunction(1.0, to, &e)?;
    a.SetDuration(dur(HOVER_POP_MS + 40))?;
    v.cast::<CompositionObject>()?.StartAnimation(&HSTRING::from("Offset"), &a)
}

pub fn hover_pop(comp: &Compositor, v: &impl Interface, center: Vector3, to: f32) -> Result<()> {
    let vis: windows::UI::Composition::Visual = v.cast()?;
    vis.SetCenterPoint(center)?;
    let a = comp.CreateVector3KeyFrameAnimation()?;
    a.InsertKeyFrameWithEasingFunction(1.0, Vector3 { X: to, Y: to, Z: 1.0 }, &pop_ease(comp)?)?;
    a.SetDuration(dur(HOVER_POP_MS))?;
    vis.cast::<CompositionObject>()?.StartAnimation(&HSTRING::from("Scale"), &a)
}

pub const MORPH_MS: u32 = 640;

pub fn droplet(comp: &Compositor, v: &ContainerVisual, center: Vector3, to_island: bool) -> Result<()> {
    v.SetCenterPoint(center)?;
    let e = ease(comp)?;
    let keys: [(f32, f32, f32); 4] = if to_island {
        [(0.18, 0.9, 1.14), (0.42, 1.09, 0.9), (0.68, 0.97, 1.04), (1.0, 1.0, 1.0)]
    } else {
        [(0.2, 1.12, 0.86), (0.45, 0.94, 1.08), (0.72, 1.02, 0.98), (1.0, 1.0, 1.0)]
    };
    let s = comp.CreateVector3KeyFrameAnimation()?;
    s.InsertKeyFrame(0.0, Vector3 { X: 1.0, Y: 1.0, Z: 1.0 })?;
    for (t, x, y) in keys {
        s.InsertKeyFrameWithEasingFunction(t, Vector3 { X: x, Y: y, Z: 1.0 }, &e)?;
    }
    s.SetDuration(dur(MORPH_MS))?;
    v.cast::<CompositionObject>()?.StartAnimation(&HSTRING::from("Scale"), &s)
}

pub fn drip(comp: &Compositor, parent: &ContainerVisual, x: f32, land: f32, size: f32, rgb: [u8; 3], falling: bool) -> Result<()> {
    let geo = comp.CreateEllipseGeometry()?;
    geo.SetRadius(Vector2 { X: size / 2.0, Y: size / 2.0 })?;
    geo.SetCenter(Vector2 { X: size / 2.0, Y: size / 2.0 })?;
    let shape = comp.CreateSpriteShapeWithGeometry(&geo)?;
    shape.SetFillBrush(&comp.CreateColorBrushWithColor(windows::UI::Color { A: 255, R: rgb[0], G: rgb[1], B: rgb[2] })?)?;
    let sv = comp.CreateShapeVisual()?;
    sv.SetSize(Vector2 { X: size, Y: size })?;
    sv.Shapes()?.Append(&shape)?;
    sv.SetCenterPoint(Vector3 { X: size / 2.0, Y: size / 2.0, Z: 0.0 })?;
    parent.Children()?.InsertAtTop(&sv)?;
    let (top, bottom) = (-size, (land - size * 0.4).max(0.0));
    let (from, to) = if falling { (top, bottom) } else { (bottom, top) };
    let e: CompositionEasingFunction = comp.CreateCubicBezierEasingFunction(Vector2 { X: 0.55, Y: 0.0 }, Vector2 { X: 0.9, Y: 0.6 })?.cast()?;
    let ms = MORPH_MS / 2;
    let o = comp.CreateVector3KeyFrameAnimation()?;
    o.InsertKeyFrame(0.0, Vector3 { X: x - size / 2.0, Y: from, Z: 0.0 })?;
    o.InsertKeyFrameWithEasingFunction(1.0, Vector3 { X: x - size / 2.0, Y: to, Z: 0.0 }, &e)?;
    o.SetDuration(dur(ms))?;
    let sc = comp.CreateVector3KeyFrameAnimation()?;
    sc.InsertKeyFrame(0.0, Vector3 { X: 0.7, Y: 1.0, Z: 1.0 })?;
    sc.InsertKeyFrame(0.6, Vector3 { X: 0.85, Y: 1.25, Z: 1.0 })?;
    sc.InsertKeyFrame(1.0, Vector3 { X: 1.3, Y: 0.55, Z: 1.0 })?;
    sc.SetDuration(dur(ms))?;
    let op = comp.CreateScalarKeyFrameAnimation()?;
    op.InsertKeyFrame(0.0, 0.0)?;
    op.InsertKeyFrame(0.2, 1.0)?;
    op.InsertKeyFrame(0.85, 1.0)?;
    op.InsertKeyFrame(1.0, 0.0)?;
    op.SetDuration(dur(ms))?;
    let obj = sv.cast::<CompositionObject>()?;
    obj.StartAnimation(&HSTRING::from("Offset"), &o)?;
    obj.StartAnimation(&HSTRING::from("Scale"), &sc)?;
    obj.StartAnimation(&HSTRING::from("Opacity"), &op)
}
