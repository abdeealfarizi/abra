//! Hook mouse global yang HANYA hidup selama popup kustom terbuka (SPEC: tidak ada hook permanen).
//! Klik di luar area popup menutupnya; klik itu ditelan (tidak sampai ke aplikasi di bawah).
//! Hook low-level berjalan di thread pemasang, jadi `begin_dismiss` harus dipanggil dari thread UI.

use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, PostMessageW, SetWindowsHookExW, UnhookWindowsHookEx, HHOOK, MSLLHOOKSTRUCT, WH_MOUSE_LL,
    WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_RBUTTONDOWN, WM_XBUTTONDOWN,
};

static HOOK: AtomicIsize = AtomicIsize::new(0);
static TARGET: AtomicIsize = AtomicIsize::new(0);
static MSG: AtomicU32 = AtomicU32::new(0);
static RECT: [std::sync::atomic::AtomicI32; 4] = [const { std::sync::atomic::AtomicI32::new(0) }; 4];

unsafe extern "system" fn proc(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code >= 0 {
        let m = wp.0 as u32;
        if matches!(m, WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN) {
            let pt = (*(lp.0 as *const MSLLHOOKSTRUCT)).pt;
            let (l, t, r, b) = (
                RECT[0].load(Ordering::Relaxed),
                RECT[1].load(Ordering::Relaxed),
                RECT[2].load(Ordering::Relaxed),
                RECT[3].load(Ordering::Relaxed),
            );
            let inside = pt.x >= l && pt.x < r && pt.y >= t && pt.y < b;
            if !inside {
                let _ = PostMessageW(Some(HWND(TARGET.load(Ordering::Relaxed) as *mut _)), MSG.load(Ordering::Relaxed), WPARAM(0), LPARAM(0));
                return LRESULT(1); // telan klik penutup
            }
        }
    }
    CallNextHookEx(None, code, wp, lp)
}

/// Mulai mengawasi klik di luar `rect` (kiri, atas, kanan, bawah; piksel layar). Saat terjadi, `msg` diposting ke `target`.
pub fn begin_dismiss(rect: (i32, i32, i32, i32), target: isize, msg: u32) {
    end_dismiss();
    RECT[0].store(rect.0, Ordering::Relaxed);
    RECT[1].store(rect.1, Ordering::Relaxed);
    RECT[2].store(rect.2, Ordering::Relaxed);
    RECT[3].store(rect.3, Ordering::Relaxed);
    TARGET.store(target, Ordering::Relaxed);
    MSG.store(msg, Ordering::Relaxed);
    if let Ok(h) = unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(proc), None, 0) } {
        HOOK.store(h.0 as isize, Ordering::Relaxed);
    }
}

pub fn end_dismiss() {
    let h = HOOK.swap(0, Ordering::Relaxed);
    if h != 0 {
        unsafe {
            let _ = UnhookWindowsHookEx(HHOOK(h as *mut _));
        }
    }
}
