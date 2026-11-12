//! Pembungkus tipis thumbnail DWM (hidup, di GPU, tanpa menyalin bitmap ke proses kita).
//! Thumbnail digambar DWM di atas semua visual Composition dan selalu persegi (hasil Spike C).

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::{
    DwmQueryThumbnailSourceSize, DwmRegisterThumbnail, DwmUnregisterThumbnail, DwmUpdateThumbnailProperties,
    DWM_THUMBNAIL_PROPERTIES, DWM_TNP_OPACITY, DWM_TNP_RECTDESTINATION, DWM_TNP_VISIBLE,
};

/// Daftarkan `src` ke dalam window `dest`. Mengembalikan pegangan thumbnail.
pub fn register(dest: isize, src: isize) -> Option<isize> {
    unsafe { DwmRegisterThumbnail(HWND(dest as *mut _), HWND(src as *mut _)).ok() }
}

/// Ukuran asli window sumber (untuk menjaga rasio).
pub fn source_size(handle: isize) -> Option<(i32, i32)> {
    unsafe { DwmQueryThumbnailSourceSize(handle).ok().map(|s| (s.cx, s.cy)) }
}

/// `rect` = (kiri, atas, kanan, bawah) dalam koordinat klien window tujuan.
pub fn place(handle: isize, rect: (i32, i32, i32, i32), opacity: u8) {
    let props = DWM_THUMBNAIL_PROPERTIES {
        dwFlags: DWM_TNP_RECTDESTINATION | DWM_TNP_VISIBLE | DWM_TNP_OPACITY,
        rcDestination: RECT { left: rect.0, top: rect.1, right: rect.2, bottom: rect.3 },
        fVisible: true.into(),
        opacity,
        ..Default::default()
    };
    unsafe {
        let _ = DwmUpdateThumbnailProperties(handle, &props);
    }
}

pub fn unregister(handle: isize) {
    unsafe {
        let _ = DwmUnregisterThumbnail(handle);
    }
}
