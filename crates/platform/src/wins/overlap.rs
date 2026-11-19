use std::ffi::c_void;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetWindowLongW, GetWindowRect, GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible,
    IsZoomed, GWL_EXSTYLE, GWL_STYLE, WS_CAPTION, WS_EX_TOOLWINDOW,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Overlap {
    pub overlapped: bool,
    pub fullscreen: bool,
}

fn class_of(h: HWND) -> String {
    let mut b = [0u16; 128];
    let n = unsafe { GetClassNameW(h, &mut b) };
    String::from_utf16_lossy(&b[..n.max(0) as usize])
}

fn is_shell_class(c: &str) -> bool {
    matches!(
        c,
        "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "Windows.UI.Core.CoreWindow" | "SimpleWindow"
    )
}

fn frame_rect(h: HWND) -> Option<RECT> {
    let mut r = RECT::default();
    unsafe {
        if DwmGetWindowAttribute(h, DWMWA_EXTENDED_FRAME_BOUNDS, &mut r as *mut RECT as *mut c_void, std::mem::size_of::<RECT>() as u32)
            .is_ok()
        {
            return Some(r);
        }
        GetWindowRect(h, &mut r).ok()?;
    }
    Some(r)
}

pub fn evaluate(fg: isize, own_pid: u32, dock: (i32, i32, i32, i32), screen: (i32, i32)) -> Overlap {
    evaluate_in(fg, own_pid, dock, RECT { left: 0, top: 0, right: screen.0, bottom: screen.1 })
}

pub fn evaluate_in(fg: isize, own_pid: u32, dock: (i32, i32, i32, i32), screen: RECT) -> Overlap {
    let none = Overlap::default();
    let h = HWND(fg as *mut _);
    unsafe {
        if fg == 0 || !IsWindow(Some(h)).as_bool() || !IsWindowVisible(h).as_bool() || IsIconic(h).as_bool() { return none; }
        let mut cloaked = 0u32;
        if DwmGetWindowAttribute(h, DWMWA_CLOAKED, &mut cloaked as *mut u32 as *mut c_void, 4).is_ok() && cloaked != 0 { return none; }
        let mut pid = 0u32;
        GetWindowThreadProcessId(h, Some(&mut pid));
        if pid == own_pid || is_shell_class(&class_of(h)) { return none; }
        if GetWindowLongW(h, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 != 0 { return none; }
        let Some(r) = frame_rect(h) else { return none };
        let (cx, cy) = ((r.left + r.right) / 2, (r.top + r.bottom) / 2);
        if cx < screen.left || cy < screen.top || cx >= screen.right || cy >= screen.bottom { return none; }
        let covers = r.left <= screen.left && r.top <= screen.top && r.right >= screen.right && r.bottom >= screen.bottom;
        let captioned_max = IsZoomed(h).as_bool() && (GetWindowLongW(h, GWL_STYLE) as u32 & WS_CAPTION.0) != 0;
        let fullscreen = covers && !captioned_max;
        if fullscreen || IsZoomed(h).as_bool() { return Overlap { overlapped: true, fullscreen }; }
        let hit = r.bottom > dock.1 && r.top < dock.3 && r.right > dock.0 && r.left < dock.2;
        Overlap { overlapped: hit, fullscreen: false }
    }
}