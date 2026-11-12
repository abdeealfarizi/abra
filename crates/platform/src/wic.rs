//! Dekode gambar (JPEG/PNG/…) ke BGRA ter-premultiply lewat WIC. COM harus sudah diinisialisasi di thread ini.

use crate::icons::Bitmap;
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICImagingFactory, WICBitmapDitherTypeNone,
    WICBitmapInterpolationModeFant, WICBitmapPaletteTypeCustom, WICDecodeMetadataCacheOnDemand,
};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::UI::Shell::SHCreateMemStream;

/// Dekode `bytes` lalu skala ke `px`×`px` (peregangan; artwork media umumnya persegi).
pub fn decode_square(bytes: &[u8], px: u32) -> Option<Bitmap> {
    unsafe {
        let factory: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok()?;
        let stream = SHCreateMemStream(Some(bytes))?;
        let decoder = factory
            .CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand)
            .ok()?;
        let frame = decoder.GetFrame(0).ok()?;
        let scaler = factory.CreateBitmapScaler().ok()?;
        scaler.Initialize(&frame, px, px, WICBitmapInterpolationModeFant).ok()?;
        let conv = factory.CreateFormatConverter().ok()?;
        conv.Initialize(&scaler, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom)
            .ok()?;
        let mut buf = vec![0u8; (px * px * 4) as usize];
        conv.CopyPixels(std::ptr::null(), px * 4, &mut buf).ok()?;
        Some(Bitmap { w: px, h: px, bgra: buf })
    }
}

/// Dekode berkas gambar (PNG/JPG/ICO/BMP/…) ke kanvas persegi `px`×`px`: diskalakan muat dengan menjaga
/// rasio, dipusatkan, sisanya transparan. Ikon kustom dock memakai ini.
pub fn decode_file_fit(path: &str, px: u32) -> Option<Bitmap> {
    unsafe {
        let factory: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok()?;
        let decoder = factory
            .CreateDecoderFromFilename(&windows::core::HSTRING::from(path), None, windows::Win32::Foundation::GENERIC_READ, WICDecodeMetadataCacheOnDemand)
            .ok()?;
        let frame = decoder.GetFrame(0).ok()?;
        let (mut w, mut h) = (0u32, 0u32);
        frame.GetSize(&mut w, &mut h).ok()?;
        if w == 0 || h == 0 {
            return None;
        }
        let k = (px as f32 / w as f32).min(px as f32 / h as f32);
        let (sw, sh) = (((w as f32 * k).round() as u32).max(1), ((h as f32 * k).round() as u32).max(1));
        let scaler = factory.CreateBitmapScaler().ok()?;
        scaler.Initialize(&frame, sw, sh, WICBitmapInterpolationModeFant).ok()?;
        let conv = factory.CreateFormatConverter().ok()?;
        conv.Initialize(&scaler, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom)
            .ok()?;
        let mut small = vec![0u8; (sw * sh * 4) as usize];
        conv.CopyPixels(std::ptr::null(), sw * 4, &mut small).ok()?;
        let mut out = vec![0u8; (px * px * 4) as usize];
        let (ox, oy) = ((px - sw) / 2, (px - sh) / 2);
        for y in 0..sh {
            let d = (((oy + y) * px + ox) * 4) as usize;
            let s = (y * sw * 4) as usize;
            out[d..d + (sw * 4) as usize].copy_from_slice(&small[s..s + (sw * 4) as usize]);
        }
        Some(Bitmap { w: px, h: px, bgra: out })
    }
}
