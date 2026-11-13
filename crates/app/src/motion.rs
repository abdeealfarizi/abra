//! Konstanta dan pembantu animasi bersama (durasi dan kurva yang disamakan dengan rekaman referensi macOS Tahoe).

use windows::Foundation::TimeSpan;
use windows::UI::Composition::{AnimationDelayBehavior, CompositionEasingFunction, CompositionObject, Compositor, ContainerVisual};
use windows_core::{Interface, Result, HSTRING};
use windows_numerics::{Vector2, Vector3};

/// Kartu strip ⇄ jendela panggung.
pub const STAGE_SWAP_MS: u32 = 280;
/// Strip masuk/keluar dari tepi kiri.
pub const STRIP_SLIDE_MS: u32 = 240;
/// Hover kartu strip.
pub const CARD_HOVER_MS: u32 = 160;
/// Sorotan pengalih app berpindah.
pub const SWITCHER_MOVE_MS: u32 = 90;
/// Panel Control Center muncul.
pub const CC_OPEN_MS: u32 = 420;
/// Panel Control Center menyusut ke ikon.
pub const CC_CLOSE_MS: u32 = 170;
/// Jeda antar modul saat panel terbuka (efek kaca "mencair" bertahap).
pub const CC_STAGGER_MS: u32 = 22;
/// Mission Control menyebar.
pub const MISSION_MS: u32 = 260;

/// Durasi animasi Stage Manager setelah dikalikan pengaturan kecepatan (100 = normal, lebih besar = lebih cepat).
pub fn stage_ms(base: u32) -> u32 {
    (base as u64 * 100 / crate::prefs::get().stage_speed.max(10) as u64).max(40) as u32
}

fn dur(ms: u32) -> TimeSpan {
    TimeSpan { Duration: ms as i64 * 10_000 }
}

/// Kurva "ease-out kuat" ala Apple (cepat di awal, mendarat lembut).
pub fn ease(comp: &Compositor) -> Result<CompositionEasingFunction> {
    comp.CreateCubicBezierEasingFunction(Vector2 { X: 0.2, Y: 0.0 }, Vector2 { X: 0.0, Y: 1.0 })?.cast()
}

/// Muncul ala "cairan kaca": membesar dari `from` melewati `over` (pantulan kecil) ke 1, sambil memudar masuk.
/// `center` = titik tumpu skala (mis. ikon Control Center); `delay_ms` untuk stagger antar modul.
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

/// Kebalikan `pop_in`: mengecil ke `center` dan memudar (tanpa pantulan).
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

/// Warna kuas berpindah halus dari nilainya sekarang ke `to`.
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
