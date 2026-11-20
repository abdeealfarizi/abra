use crate::icons::Bitmap;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DwmSetWindowAttribute, DWMWINDOWATTRIBUTE};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, HGDIOBJ,
};
use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
use windows::Win32::UI::WindowsAndMessaging::{GetWindowRect, IsIconic, IsWindowVisible};

const DWMWA_TRANSITIONS_FORCEDISABLED: i32 = 3;
const DWMWA_EXTENDED_FRAME_BOUNDS: i32 = 9;

pub fn set_transitions_disabled(hwnd: isize, disabled: bool) -> bool {
    let v: i32 = disabled as i32;
    unsafe {
        DwmSetWindowAttribute(HWND(hwnd as *mut _), DWMWINDOWATTRIBUTE(DWMWA_TRANSITIONS_FORCEDISABLED), &v as *const _ as *const _, 4).is_ok()
    }
}

pub struct Shot {
    pub rect: (i32, i32, i32, i32),
    pub bmp: Bitmap,
}

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
        let ok = PrintWindow(h, dc, PRINT_WINDOW_FLAGS(2)).as_bool();
        let src = std::slice::from_raw_parts(bits as *const u8, (ww * wh * 4) as usize);

        let s = shrink.max(1) as i32;
        let (ow, oh) = ((cw / s).max(1), (ch / s).max(1));
        let mut out = vec![0u8; (ow * oh * 4) as usize];
        let radius = (9.0 / s as f32).max(2.0);
        if s == 1 && cx + ow <= ww && cy + oh <= wh {
            for y in 0..oh {
                let from = (((cy + y) * ww + cx) * 4) as usize;
                let to = (y * ow * 4) as usize;
                let (src_row, dst_row) = (&src[from..from + (ow * 4) as usize], &mut out[to..to + (ow * 4) as usize]);
                dst_row.copy_from_slice(src_row);
                for px in dst_row.chunks_exact_mut(4) {
                    px[3] = 255;
                }
            }
            let band = radius.ceil() as i32 + 1;
            for y in 0..oh {
                if y >= band && y < oh - band {
                    continue;
                }
                for x in 0..ow {
                    if x >= band && x < ow - band {
                        continue;
                    }
                    let a = corner_alpha(x as f32 + 0.5, y as f32 + 0.5, ow as f32, oh as f32, radius) as u32;
                    let o = ((y * ow + x) * 4) as usize;
                    out[o] = (out[o] as u32 * a / 255) as u8;
                    out[o + 1] = (out[o + 1] as u32 * a / 255) as u8;
                    out[o + 2] = (out[o + 2] as u32 * a / 255) as u8;
                    out[o + 3] = a as u8;
                }
            }
            SelectObject(dc, old);
            let _ = DeleteObject(HGDIOBJ(dib.0));
            let _ = DeleteDC(dc);
            if !ok {
                return None;
            }
            return Some(Shot { rect: (fx, fy, fw, fh), bmp: Bitmap { w: ow as u32, h: oh as u32, bgra: out } });
        }
        for y in 0..oh {
            for x in 0..ow {
                let (mut b, mut g, mut r) = (0u32, 0u32, 0u32);
                let mut n = 0u32;
                for dy in 0..s {
                    for dx in 0..s {
                        let (sx, sy) = (cx + x * s + dx, cy + y * s + dy);
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

fn corner_alpha(x: f32, y: f32, w: f32, h: f32, r: f32) -> u8 {
    let dx = if x < r { r - x } else if x > w - r { x - (w - r) } else { 0.0 };
    let dy = if y < r { r - y } else if y > h - r { y - (h - r) } else { 0.0 };
    if dx == 0.0 || dy == 0.0 {
        return 255;
    }
    let d = (dx * dx + dy * dy).sqrt();
    ((r - d + 0.5).clamp(0.0, 1.0) * 255.0) as u8
}
