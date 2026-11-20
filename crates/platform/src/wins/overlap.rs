use std::ffi::c_void;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetWindowLongW, GetWindowRect, GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible,
    IsZoomed, GWL_EXSTYLE, GWL_STYLE, WS_CAPTION, WS_EX_TOOLWINDOW,
};

const EDGE_SLACK: i32 = 2;

pub fn covers(r: (i32, i32, i32, i32), screen: (i32, i32)) -> bool {
    r.0 <= EDGE_SLACK && r.1 <= EDGE_SLACK && r.2 >= screen.0 - EDGE_SLACK && r.3 >= screen.1 - EDGE_SLACK
}

pub fn exceeds(r: (i32, i32, i32, i32), work: (i32, i32, i32, i32)) -> bool {
    r.0 < work.0 - EDGE_SLACK || r.1 < work.1 - EDGE_SLACK || r.2 > work.2 + EDGE_SLACK || r.3 > work.3 + EDGE_SLACK
}

pub fn is_fullscreen(frame: (i32, i32, i32, i32), screen: (i32, i32), work: (i32, i32, i32, i32), zoomed: bool, caption: bool) -> bool {
    covers(frame, screen) && (!zoomed || !caption || exceeds(frame, work))
}

fn work_area(h: HWND) -> Option<(i32, i32, i32, i32)> {
    unsafe {
        let m = MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        GetMonitorInfoW(m, &mut mi).as_bool().then(|| (mi.rcWork.left, mi.rcWork.top, mi.rcWork.right, mi.rcWork.bottom))
    }
}

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
        if cx < screen.left || cy < screen.top || cx >= screen.right || cy >= screen.bottom {
            return none;
        }
        let frame = (r.left - screen.left, r.top - screen.top, r.right - screen.left, r.bottom - screen.top);
        let zoomed = IsZoomed(h).as_bool();
        let caption = GetWindowLongW(h, GWL_STYLE) as u32 & WS_CAPTION.0 == WS_CAPTION.0;
        let work = work_area(h).unwrap_or((screen.left, screen.top, screen.right, screen.bottom));
        let work = (work.0 - screen.left, work.1 - screen.top, work.2 - screen.left, work.3 - screen.top);
        let fullscreen = is_fullscreen(frame, (screen.right - screen.left, screen.bottom - screen.top), work, zoomed, caption) || exclusive_fullscreen();
        if fullscreen || zoomed {
            return Overlap { overlapped: true, fullscreen };
        }
        let hit = r.bottom > dock.1 && r.top < dock.3 && r.right > dock.0 && r.left < dock.2;
        Overlap { overlapped: hit, fullscreen: false }
    }
}

fn exclusive_fullscreen() -> bool {
    use windows::Win32::UI::Shell::{SHQueryUserNotificationState, QUNS_RUNNING_D3D_FULL_SCREEN};
    unsafe { SHQueryUserNotificationState().is_ok_and(|s| s == QUNS_RUNNING_D3D_FULL_SCREEN) }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: (i32, i32) = (1920, 1080);
    const FULL_WORK: (i32, i32, i32, i32) = (0, 0, 1920, 1080);
    const RESERVED_WORK: (i32, i32, i32, i32) = (0, 30, 1920, 1000);

    #[test]
    fn browser_fullscreen_one_pixel_short_counts() {
        assert!(is_fullscreen((0, 0, 1920, 1079), SCREEN, FULL_WORK, false, false));
    }

    #[test]
    fn fullscreen_from_maximized_without_title_bar_counts() {
        assert!(is_fullscreen((0, 0, 1920, 1080), SCREEN, FULL_WORK, true, false));
    }

    #[test]
    fn maximized_window_beyond_reserved_work_area_counts() {
        assert!(is_fullscreen((0, 0, 1920, 1080), SCREEN, RESERVED_WORK, true, true));
    }

    #[test]
    fn plain_maximized_window_is_not_fullscreen() {
        assert!(!is_fullscreen((0, 0, 1920, 1080), SCREEN, FULL_WORK, true, true));
        assert!(!is_fullscreen((0, 30, 1920, 1000), SCREEN, RESERVED_WORK, true, true));
    }

    #[test]
    fn small_window_is_not_fullscreen() {
        assert!(!is_fullscreen((100, 100, 900, 700), SCREEN, FULL_WORK, false, true));
    }
}
