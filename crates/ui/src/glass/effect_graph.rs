//! Grafik efek Direct2D untuk Composition tanpa Win2D (terbukti di Spike A).
//! Kita mengimplementasikan `IGraphicsEffect` + `IGraphicsEffectD2D1Interop` sendiri.

use std::cell::RefCell;
use windows::Foundation::{IPropertyValue, PropertyValue};
use windows::Graphics::Effects::{IGraphicsEffect, IGraphicsEffectSource, IGraphicsEffectSource_Impl, IGraphicsEffect_Impl};
use windows::Win32::Foundation::{E_INVALIDARG, E_NOTIMPL};
use windows::Win32::Graphics::Direct2D::{CLSID_D2D1DisplacementMap, CLSID_D2D1GaussianBlur, CLSID_D2D1Saturation};
use windows::Win32::System::WinRT::Graphics::Direct2D::{
    IGraphicsEffectD2D1Interop, IGraphicsEffectD2D1Interop_Impl, GRAPHICS_EFFECT_PROPERTY_MAPPING,
};
use windows::UI::Composition::{
    CompositionEffectBrush, CompositionEffectSourceParameter, Compositor,
};
use windows_core::{implement, Interface, Result, GUID, HSTRING, PCWSTR};

#[implement(IGraphicsEffect, IGraphicsEffectSource, IGraphicsEffectD2D1Interop)]
struct D2dEffect {
    name: RefCell<HSTRING>,
    id: GUID,
    props: Vec<IPropertyValue>,
    sources: Vec<IGraphicsEffectSource>,
}

impl D2dEffect {
    fn make(id: GUID, props: Vec<IPropertyValue>, sources: Vec<IGraphicsEffectSource>) -> IGraphicsEffect {
        D2dEffect { name: RefCell::new(HSTRING::new()), id, props, sources }.into()
    }
}

impl IGraphicsEffectSource_Impl for D2dEffect_Impl {}

impl IGraphicsEffect_Impl for D2dEffect_Impl {
    fn Name(&self) -> Result<HSTRING> {
        Ok(self.name.borrow().clone())
    }
    fn SetName(&self, name: &HSTRING) -> Result<()> {
        *self.name.borrow_mut() = name.clone();
        Ok(())
    }
}

impl IGraphicsEffectD2D1Interop_Impl for D2dEffect_Impl {
    fn GetEffectId(&self) -> Result<GUID> {
        Ok(self.id)
    }
    fn GetNamedPropertyMapping(&self, _: &PCWSTR, _: *mut u32, _: *mut GRAPHICS_EFFECT_PROPERTY_MAPPING) -> Result<()> {
        Err(E_NOTIMPL.into())
    }
    fn GetPropertyCount(&self) -> Result<u32> {
        Ok(self.props.len() as u32)
    }
    fn GetProperty(&self, i: u32) -> Result<IPropertyValue> {
        self.props.get(i as usize).cloned().ok_or(E_INVALIDARG.into())
    }
    fn GetSource(&self, i: u32) -> Result<IGraphicsEffectSource> {
        self.sources.get(i as usize).cloned().ok_or(E_INVALIDARG.into())
    }
    fn GetSourceCount(&self) -> Result<u32> {
        Ok(self.sources.len() as u32)
    }
}

fn prop_f32(v: f32) -> Result<IPropertyValue> {
    PropertyValue::CreateSingle(v)?.cast()
}
fn prop_u32(v: u32) -> Result<IPropertyValue> {
    PropertyValue::CreateUInt32(v)?.cast()
}

/// Kuas "Saturation(GaussianBlur(backdrop))". `blur_sigma` dalam piksel (±24 untuk kaca penuh).
pub fn backdrop_blur_brush(comp: &Compositor, blur_sigma: f32, saturation: f32) -> Result<CompositionEffectBrush> {
    // Batas efek Direct2D: nilai di luar rentang membuat pembuatan efek gagal (0x80070057).
    let blur_sigma = blur_sigma.clamp(0.5, 100.0);
    let saturation = saturation.clamp(0.0, 2.0);
    const SRC: &str = "backdrop";
    let source: IGraphicsEffectSource = CompositionEffectSourceParameter::Create(&HSTRING::from(SRC))?.cast()?;
    // properti Gaussian: sigma, optimasi (0 = cepat), mode tepi (1 = keras)
    let blur = D2dEffect::make(CLSID_D2D1GaussianBlur, vec![prop_f32(blur_sigma)?, prop_u32(0)?, prop_u32(1)?], vec![source]);
    let sat = D2dEffect::make(CLSID_D2D1Saturation, vec![prop_f32(saturation)?], vec![blur.cast()?]);
    let brush = comp.CreateEffectFactory(&sat)?.CreateBrush()?;
    brush.SetSourceParameter(&HSTRING::from(SRC), &comp.CreateBackdropBrush()?)?;
    Ok(brush)
}

/// Seperti `backdrop_blur_brush` tetapi dengan refraksi tepi: backdrop → blur → displacement map → saturasi.
/// `map` adalah kuas permukaan peta distorsi (diregangkan memenuhi visual), `strength_px` geser maksimum.
pub fn backdrop_refract_brush(
    comp: &Compositor,
    blur_sigma: f32,
    saturation: f32,
    map: &windows::UI::Composition::CompositionBrush,
    strength_px: f32,
) -> Result<CompositionEffectBrush> {
    let blur_sigma = blur_sigma.clamp(0.5, 100.0);
    let saturation = saturation.clamp(0.0, 2.0);
    let src: IGraphicsEffectSource = CompositionEffectSourceParameter::Create(&HSTRING::from("backdrop"))?.cast()?;
    let map_src: IGraphicsEffectSource = CompositionEffectSourceParameter::Create(&HSTRING::from("map"))?.cast()?;
    let blur = D2dEffect::make(CLSID_D2D1GaussianBlur, vec![prop_f32(blur_sigma)?, prop_u32(0)?, prop_u32(1)?], vec![src]);
    // properti: Scale, XChannelSelect (R=0), YChannelSelect (G=1); offset = (kanal - 0,5) * Scale
    let disp = D2dEffect::make(
        CLSID_D2D1DisplacementMap,
        vec![prop_f32(strength_px.clamp(0.0, 60.0) * 2.0)?, prop_u32(0)?, prop_u32(1)?],
        vec![blur.cast()?, map_src],
    );
    let sat = D2dEffect::make(CLSID_D2D1Saturation, vec![prop_f32(saturation)?], vec![disp.cast()?]);
    let brush = comp.CreateEffectFactory(&sat)?.CreateBrush()?;
    brush.SetSourceParameter(&HSTRING::from("backdrop"), &comp.CreateBackdropBrush()?)?;
    brush.SetSourceParameter(&HSTRING::from("map"), map)?;
    Ok(brush)
}
