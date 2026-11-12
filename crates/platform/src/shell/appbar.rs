//! AppBar: mencadangkan strip layar agar window maksimum tidak menutupi dock (bawah) atau notch (atas).
//! Windows melepas AppBar otomatis saat window/proses hilang, tetapi kita tetap melepasnya eksplisit.

use std::sync::atomic::{AtomicIsize, Ordering};
use windows::core::w;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::Shell::{SHAppBarMessage, ABE_BOTTOM, ABE_TOP, ABM_NEW, ABM_QUERYPOS, ABM_REMOVE, ABM_SETPOS, APPBARDATA};
use windows::Win32::UI::WindowsAndMessaging::RegisterWindowMessageW;

static BOTTOM: AtomicIsize = AtomicIsize::new(0);
static TOP: AtomicIsize = AtomicIsize::new(0);

fn data(h: HWND) -> APPBARDATA {
    APPBARDATA { cbSize: std::mem::size_of::<APPBARDATA>() as u32, hWnd: h, ..Default::default() }
}

fn reserve(slot: &AtomicIsize, hwnd: HWND, top_edge: bool, screen_w: i32, screen_h: i32, strip_h: i32) -> Option<i32> {
    unsafe {
        let mut abd = data(hwnd);
        abd.uCallbackMessage = RegisterWindowMessageW(w!("NacreAppBar"));
        if slot.load(Ordering::Relaxed) != hwnd.0 as isize {
            let _ = SHAppBarMessage(ABM_REMOVE, &mut abd);
            if SHAppBarMessage(ABM_NEW, &mut abd) == 0 {
                return None;
            }
            slot.store(hwnd.0 as isize, Ordering::Relaxed);
        }
        if top_edge {
            abd.uEdge = ABE_TOP;
            abd.rc = RECT { left: 0, top: 0, right: screen_w, bottom: strip_h };
            SHAppBarMessage(ABM_QUERYPOS, &mut abd);
            abd.rc.bottom = abd.rc.top + strip_h;
            SHAppBarMessage(ABM_SETPOS, &mut abd);
            Some(abd.rc.bottom)
        } else {
            abd.uEdge = ABE_BOTTOM;
            abd.rc = RECT { left: 0, top: screen_h - strip_h, right: screen_w, bottom: screen_h };
            SHAppBarMessage(ABM_QUERYPOS, &mut abd);
            // Shell boleh mengoreksi sisi lain; tinggi strip tetap milik kita.
            abd.rc.top = abd.rc.bottom - strip_h;
            SHAppBarMessage(ABM_SETPOS, &mut abd);
            Some(abd.rc.top)
        }
    }
}

/// Cadangkan strip setinggi `strip_h` px di dasar monitor `(0,0,w,h)`.
/// Mengembalikan `top` strip yang disetujui shell, atau `None` bila gagal.
pub fn reserve_bottom(hwnd: HWND, screen_w: i32, screen_h: i32, strip_h: i32) -> Option<i32> {
    reserve(&BOTTOM, hwnd, false, screen_w, screen_h, strip_h)
}

/// Cadangkan strip setinggi `strip_h` px di atas monitor (untuk notch tetap). Mengembalikan `bottom` strip.
pub fn reserve_top(hwnd: HWND, screen_w: i32, strip_h: i32) -> Option<i32> {
    reserve(&TOP, hwnd, true, screen_w, 0, strip_h)
}

fn remove(slot: &AtomicIsize) {
    let h = slot.swap(0, Ordering::Relaxed);
    if h != 0 {
        unsafe {
            let mut abd = data(HWND(h as *mut _));
            SHAppBarMessage(ABM_REMOVE, &mut abd);
        }
    }
}

pub fn release_top() {
    remove(&TOP);
}

pub fn release_bottom() {
    remove(&BOTTOM);
}

/// Lepas semua cadangan (jalur keluar).
pub fn release() {
    remove(&BOTTOM);
    remove(&TOP);
}
