use crate::hookthread::{HookThread, Kind};
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use std::sync::Mutex;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, PostMessageW, MSLLHOOKSTRUCT, WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_RBUTTONDOWN, WM_XBUTTONDOWN,
};

static HOOK: Mutex<Option<HookThread>> = Mutex::new(None);
static TARGET: AtomicIsize = AtomicIsize::new(0);
static MSG: AtomicU32 = AtomicU32::new(0);
static RECT: [std::sync::atomic::AtomicI32; 4] = [const { std::sync::atomic::AtomicI32::new(0) }; 4];
static EXTRA: [std::sync::atomic::AtomicI32; 4] = [const { std::sync::atomic::AtomicI32::new(0) }; 4];

fn in_box(b: &[std::sync::atomic::AtomicI32; 4], x: i32, y: i32) -> bool {
    let (l, t, r, bt) = (
        b[0].load(Ordering::Relaxed),
        b[1].load(Ordering::Relaxed),
        b[2].load(Ordering::Relaxed),
        b[3].load(Ordering::Relaxed),
    );
    r > l && bt > t && x >= l && x < r && y >= t && y < bt
}

unsafe extern "system" fn proc(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code >= 0 {
        let m = wp.0 as u32;
        if matches!(m, WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN) {
            let pt = (*(lp.0 as *const MSLLHOOKSTRUCT)).pt;
            let inside = in_box(&RECT, pt.x, pt.y) || in_box(&EXTRA, pt.x, pt.y);
            if !inside {
                let packed = ((pt.y as i16 as u16 as u32) << 16) | (pt.x as i16 as u16 as u32);
                let _ = PostMessageW(Some(HWND(TARGET.load(Ordering::Relaxed) as *mut _)), MSG.load(Ordering::Relaxed), WPARAM(0), LPARAM(packed as isize));
                return LRESULT(1);
            }
        }
    }
    CallNextHookEx(None, code, wp, lp)
}

pub fn set_extra_rect(rect: Option<(i32, i32, i32, i32)>) {
    let (l, t, r, b) = rect.unwrap_or((0, 0, 0, 0));
    EXTRA[0].store(l, Ordering::Relaxed);
    EXTRA[1].store(t, Ordering::Relaxed);
    EXTRA[2].store(r, Ordering::Relaxed);
    EXTRA[3].store(b, Ordering::Relaxed);
}

pub fn begin_dismiss(rect: (i32, i32, i32, i32), target: isize, msg: u32) {
    end_dismiss();
    RECT[0].store(rect.0, Ordering::Relaxed);
    RECT[1].store(rect.1, Ordering::Relaxed);
    RECT[2].store(rect.2, Ordering::Relaxed);
    RECT[3].store(rect.3, Ordering::Relaxed);
    TARGET.store(target, Ordering::Relaxed);
    MSG.store(msg, Ordering::Relaxed);
    let h = HookThread::start(Kind::Mouse, Some(proc));
    *HOOK.lock().unwrap_or_else(|e| e.into_inner()) = h;
}

pub fn end_dismiss() {
    set_extra_rect(None);
    let h = HOOK.lock().unwrap_or_else(|e| e.into_inner()).take();
    drop(h);
}
