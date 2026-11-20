use std::sync::Mutex;
use windows::core::w;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::Shell::{SHAppBarMessage, ABE_BOTTOM, ABE_TOP, ABM_NEW, ABM_QUERYPOS, ABM_REMOVE, ABM_SETPOS, APPBARDATA};
use windows::Win32::UI::WindowsAndMessaging::RegisterWindowMessageW;

static BOTTOM: Mutex<Vec<isize>> = Mutex::new(Vec::new());
static TOP: Mutex<Vec<isize>> = Mutex::new(Vec::new());

fn data(h: HWND) -> APPBARDATA {
    APPBARDATA { cbSize: std::mem::size_of::<APPBARDATA>() as u32, hWnd: h, ..Default::default() }
}

fn reserve(slot: &Mutex<Vec<isize>>, hwnd: HWND, top_edge: bool, bounds: RECT, strip_h: i32) -> Option<i32> {
    let mut registered = slot.lock().unwrap_or_else(|e| e.into_inner());
    unsafe {
        let mut abd = data(hwnd);
        abd.uCallbackMessage = RegisterWindowMessageW(w!("NacreAppBar"));
        if !registered.contains(&(hwnd.0 as isize)) {
            if SHAppBarMessage(ABM_NEW, &mut abd) == 0 { return None; }
            registered.push(hwnd.0 as isize);
        }
        if top_edge {
            abd.uEdge = ABE_TOP;
            abd.rc = RECT { left: bounds.left, top: bounds.top, right: bounds.right, bottom: bounds.top + strip_h };
            SHAppBarMessage(ABM_QUERYPOS, &mut abd);
            abd.rc.bottom = abd.rc.top + strip_h;
            SHAppBarMessage(ABM_SETPOS, &mut abd);
            Some(abd.rc.bottom)
        } else {
            abd.uEdge = ABE_BOTTOM;
            abd.rc = RECT { left: bounds.left, top: bounds.bottom - strip_h, right: bounds.right, bottom: bounds.bottom };
            SHAppBarMessage(ABM_QUERYPOS, &mut abd);
            abd.rc.top = abd.rc.bottom - strip_h;
            SHAppBarMessage(ABM_SETPOS, &mut abd);
            Some(abd.rc.top)
        }
    }
}

pub fn reserve_bottom(hwnd: HWND, bounds: RECT, strip_h: i32) -> Option<i32> {
    reserve(&BOTTOM, hwnd, false, bounds, strip_h)
}

pub fn reserve_top(hwnd: HWND, bounds: RECT, strip_h: i32) -> Option<i32> {
    reserve(&TOP, hwnd, true, bounds, strip_h)
}

fn remove(slot: &Mutex<Vec<isize>>, hwnd: Option<HWND>) {
    let mut registered = slot.lock().unwrap_or_else(|e| e.into_inner());
    let removed: Vec<isize> = match hwnd {
        Some(h) => { registered.retain(|id| *id != h.0 as isize); vec![h.0 as isize] }
        None => std::mem::take(&mut *registered),
    };
    for raw in removed {
        unsafe {
            let mut abd = data(HWND(raw as *mut _));
            let _ = SHAppBarMessage(ABM_REMOVE, &mut abd);
        }
    }
}

pub fn release_top(hwnd: Option<HWND>) { remove(&TOP, hwnd); }
pub fn release_bottom(hwnd: Option<HWND>) { remove(&BOTTOM, hwnd); }
pub fn release() { remove(&BOTTOM, None); remove(&TOP, None); }
