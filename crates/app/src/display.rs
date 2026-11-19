use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW, MonitorFromWindow,
    MONITOR_DEFAULTTONEAREST,
};

#[derive(Clone)]
pub struct Monitor {
    pub rect: RECT,
    pub primary: bool,
}

impl Monitor {
    pub fn width(&self) -> i32 { self.rect.right - self.rect.left }
    pub fn height(&self) -> i32 { self.rect.bottom - self.rect.top }
}

pub fn monitors() -> Vec<Monitor> {
    unsafe extern "system" fn collect(handle: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
        let found = &mut *(data.0 as *mut Vec<Monitor>);
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        if GetMonitorInfoW(handle, &mut info as *mut _ as *mut _).as_bool() {
            found.push(Monitor {
                rect: info.monitorInfo.rcMonitor,
                primary: info.monitorInfo.dwFlags & 1 != 0,
            });
        }
        BOOL(1)
    }

    let mut found: Vec<Monitor> = Vec::new();
    unsafe { let _ = EnumDisplayMonitors(None, None, Some(collect), LPARAM(&mut found as *mut _ as isize)); }
    found.sort_by_key(|m| (!m.primary, m.rect.left, m.rect.top));
    found
}

pub fn virtual_bounds() -> Option<RECT> {
    let monitors = monitors();
    let mut bounds = monitors.first()?.rect;
    for monitor in monitors.iter().skip(1) {
        bounds.left = bounds.left.min(monitor.rect.left);
        bounds.top = bounds.top.min(monitor.rect.top);
        bounds.right = bounds.right.max(monitor.rect.right);
        bounds.bottom = bounds.bottom.max(monitor.rect.bottom);
    }
    Some(bounds)
}

pub fn monitor_bounds_for_window(hwnd: isize) -> Option<RECT> {
    let monitor = unsafe { MonitorFromWindow(HWND(hwnd as *mut _), MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    if !unsafe { GetMonitorInfoW(monitor, &mut info as *mut _ as *mut _) }.as_bool() {
        return None;
    }
    Some(info.monitorInfo.rcMonitor)
}

pub fn window_is_on(hwnd: isize, bounds: RECT) -> bool {
    let Some(rect) = monitor_bounds_for_window(hwnd) else {
        return false;
    };
    rect.left == bounds.left && rect.top == bounds.top && rect.right == bounds.right && rect.bottom == bounds.bottom
}
