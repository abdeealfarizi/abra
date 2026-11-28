use nacre_core::display::{self, Area, Monitor, Screen};
use windows::core::BOOL;
use windows::Win32::Foundation::{LPARAM, POINT, RECT};
use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, GetSystemMetrics, MONITORINFOF_PRIMARY, SM_CXSCREEN, SM_CYSCREEN};

fn area(r: RECT) -> Area {
    Area::new(r.left, r.top, r.right - r.left, r.bottom - r.top)
}

fn fake() -> Option<Vec<Monitor>> {
    let spec = match std::env::var_os("NACRE_FAKE_MONITORS_FILE") {
        Some(p) => std::fs::read_to_string(p).ok()?,
        None => std::env::var("NACRE_FAKE_MONITORS").ok()?,
    };
    let list = display::parse_fake(spec.trim());
    (!list.is_empty()).then_some(list)
}

unsafe extern "system" fn collect(h: HMONITOR, _: HDC, _: *mut RECT, lp: LPARAM) -> BOOL {
    let out = &mut *(lp.0 as *mut Vec<Monitor>);
    let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    if GetMonitorInfoW(h, &mut mi).as_bool() {
        let (mut dx, mut dy) = (96u32, 96u32);
        if GetDpiForMonitor(h, MDT_EFFECTIVE_DPI, &mut dx, &mut dy).is_err() {
            dx = 96;
        }
        out.push(Monitor { area: area(mi.rcMonitor), work: area(mi.rcWork), dpi: dx.max(96), primary: mi.dwFlags & MONITORINFOF_PRIMARY != 0 });
    }
    true.into()
}

pub fn list() -> Vec<Monitor> {
    if let Some(f) = fake() {
        return display::ordered(f);
    }
    let mut out: Vec<Monitor> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(collect), LPARAM(&mut out as *mut Vec<Monitor> as isize));
    }
    if out.is_empty() {
        let (w, h) = unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
        out.push(Monitor { area: Area::new(0, 0, w, h), work: Area::new(0, 0, w, h), dpi: 96, primary: true });
    }
    display::ordered(out)
}

pub fn count() -> usize {
    list().len()
}

pub fn primary() -> Monitor {
    let l = list();
    display::primary(&l).unwrap_or(l[0])
}

pub fn target(screen: Screen) -> Monitor {
    if screen == Screen::Active {
        return under_cursor();
    }
    let l = list();
    display::resolve(screen, &l).unwrap_or(l[0])
}

pub fn at(x: i32, y: i32) -> Monitor {
    let l = list();
    display::at_point(&l, x, y).unwrap_or(l[0])
}

pub fn under_cursor() -> Monitor {
    let mut p = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut p);
    }
    at(p.x, p.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_list_is_never_empty_and_has_a_primary() {
        let l = list();
        assert!(!l.is_empty());
        assert!(l.iter().any(|m| m.primary));
        assert!(l.iter().all(|m| m.area.w > 0 && m.area.h > 0 && m.dpi >= 96));
    }
}
