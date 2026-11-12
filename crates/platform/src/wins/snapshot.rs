//! Efek jendela: menonaktifkan transisi DWM bawaan satu jendela (agar kita yang menganimasikan) dan
//! menangkap isinya (PrintWindow, termasuk konten GPU) sebagai bitmap BGRA ter-premultiply.

use crate::icons::Bitmap;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DwmSetWindowAttribute, DWMWINDOWATTRIBUTE};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, HGDIOBJ,
};
use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
use windows::Win32::UI::HiDpi::{GetDpiForSystem, GetDpiForWindow};
use windows::Win32::UI::WindowsAndMessaging::{GetWindowRect, IsIconic, IsWindowVisible};

const DWMWA_TRANSITIONS_FORCEDISABLED: i32 = 3;
const DWMWA_EXTENDED_FRAME_BOUNDS: i32 = 9;

/// Matikan/nyalakan lagi transisi DWM (animasi minimize/restore/close bawaan) untuk satu jendela.
/// Berhasil lintas proses (diuji); gagal diam-diam bila jendela lebih tinggi hak aksesnya.
pub fn set_transitions_disabled(hwnd: isize, disabled: bool) -> bool {
    let v: i32 = disabled as i32;
    unsafe {
        DwmSetWindowAttribute(HWND(hwnd as *mut _), DWMWINDOWATTRIBUTE(DWMWA_TRANSITIONS_FORCEDISABLED), &v as *const _ as *const _, 4).is_ok()
    }
}

pub struct Shot {
    /// Persegi bingkai terlihat di layar (tanpa bayangan tak terlihat), piksel layar.
    pub rect: (i32, i32, i32, i32),
    pub bmp: Bitmap,
}

/// Rect bingkai terlihat jendela (px layar), atau rect biasa bila DWM tak menjawab.
pub fn frame_rect(hwnd: isize) -> Option<(i32, i32, i32, i32)> {
    unsafe {
        let h = HWND(hwnd as *mut _);
        let mut r = RECT::default();
        if DwmGetWindowAttribute(h, DWMWINDOWATTRIBUTE(DWMWA_EXTENDED_FRAME_BOUNDS), &mut r as *mut _ as *mut _, std::mem::size_of::<RECT>() as u32).is_err() {
            GetWindowRect(h, &mut r).ok()?;
        }
        Some((r.left, r.top, r.right - r.left, r.bottom - r.top))
    }
}

/// Tangkap isi jendela. `shrink`: pembagi skala (1 = penuh, 2 = setengah) untuk menghemat memori.
/// `None` bila jendela tersembunyi/diminimalkan atau tangkapan gagal.
pub fn capture(hwnd: isize, shrink: u32) -> Option<Shot> {
    unsafe {
        let h = HWND(hwnd as *mut _);
        if !IsWindowVisible(h).as_bool() || IsIconic(h).as_bool() {
            return None;
        }
        let mut wr = RECT::default();
        GetWindowRect(h, &mut wr).ok()?;
        let (ww, wh) = (wr.right - wr.left, wr.bottom - wr.top);
        if ww < 40 || wh < 40 || ww > 8000 || wh > 8000 {
            return None;
        }
        let (fx, fy, fw, fh) = frame_rect(hwnd)?;
        // Bingkai terlihat relatif terhadap window rect; jepit agar aman.
        let (cx, cy) = ((fx - wr.left).clamp(0, ww - 1), (fy - wr.top).clamp(0, wh - 1));
        let (cw, ch) = (fw.min(ww - cx), fh.min(wh - cy));
        if cw < 40 || ch < 40 {
            return None;
        }

        let screen = GetDC(None);
        let dc = CreateCompatibleDC(Some(screen));
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: ww,
                biHeight: -wh,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let dib = CreateDIBSection(Some(screen), &info, DIB_RGB_COLORS, &mut bits, None, 0);
        ReleaseDC(None, screen);
        let Ok(dib) = dib else {
            let _ = DeleteDC(dc);
            return None;
        };
        if bits.is_null() {
            let _ = DeleteObject(HGDIOBJ(dib.0));
            let _ = DeleteDC(dc);
            return None;
        }
        let old = SelectObject(dc, HGDIOBJ(dib.0));
        let ok = PrintWindow(h, dc, PRINT_WINDOW_FLAGS(2)).as_bool(); // PW_RENDERFULLCONTENT
        let src = std::slice::from_raw_parts(bits as *const u8, (ww * wh * 4) as usize);

        // Jendela yang tidak sadar-DPI digambar PrintWindow pada ukuran logisnya (pojok kiri-atas bitmap):
        // petakan koordinat sumber dengan rasio DPI jendela terhadap DPI sistem.
        let wdpi = GetDpiForWindow(h).max(48) as f32;
        let sdpi = GetDpiForSystem().max(48) as f32;
        let ratio = (wdpi / sdpi).clamp(0.25, 1.0);
        let s = shrink.max(1) as i32;
        let (ow, oh) = ((cw / s).max(1), (ch / s).max(1));
        let mut out = vec![0u8; (ow * oh * 4) as usize];
        let radius = (9.0 / s as f32).max(2.0);
        for y in 0..oh {
            for x in 0..ow {
                // rata-rata blok s×s (kotak sederhana)
                let (mut b, mut g, mut r) = (0u32, 0u32, 0u32);
                let mut n = 0u32;
                for dy in 0..s {
                    for dx in 0..s {
                        let (sx, sy) = ((((cx + x * s + dx) as f32) * ratio) as i32, (((cy + y * s + dy) as f32) * ratio) as i32);
                        if sx < ww && sy < wh {
                            let i = ((sy * ww + sx) * 4) as usize;
                            b += src[i] as u32;
                            g += src[i + 1] as u32;
                            r += src[i + 2] as u32;
                            n += 1;
                        }
                    }
                }
                let n = n.max(1);
                let a = corner_alpha(x as f32 + 0.5, y as f32 + 0.5, ow as f32, oh as f32, radius);
                let o = ((y * ow + x) * 4) as usize;
                // ter-premultiply
                out[o] = (b / n * a as u32 / 255) as u8;
                out[o + 1] = (g / n * a as u32 / 255) as u8;
                out[o + 2] = (r / n * a as u32 / 255) as u8;
                out[o + 3] = a;
            }
        }
        SelectObject(dc, old);
        let _ = DeleteObject(HGDIOBJ(dib.0));
        let _ = DeleteDC(dc);
        if !ok {
            return None;
        }
        Some(Shot { rect: (fx, fy, fw, fh), bmp: Bitmap { w: ow as u32, h: oh as u32, bgra: out } })
    }
}

/// Alfa 0..255 untuk sudut membulat (anti-alias 1 px). Di luar sudut = 255.
fn corner_alpha(x: f32, y: f32, w: f32, h: f32, r: f32) -> u8 {
    let dx = if x < r { r - x } else if x > w - r { x - (w - r) } else { 0.0 };
    let dy = if y < r { r - y } else if y > h - r { y - (h - r) } else { 0.0 };
    if dx == 0.0 || dy == 0.0 {
        return 255;
    }
    let d = (dx * dx + dy * dy).sqrt();
    ((r - d + 0.5).clamp(0.0, 1.0) * 255.0) as u8
}
