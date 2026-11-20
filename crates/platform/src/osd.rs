use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::time::Duration;
use windows::core::{w, BOOL, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
use windows::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowExW, GetClassNameW, GetWindowRect, IsIconic, IsWindow, ShowWindow,
    EVENT_OBJECT_CREATE, EVENT_OBJECT_SHOW, SW_MINIMIZE, WINEVENT_OUTOFCONTEXT,
    WINEVENT_SKIPOWNPROCESS,
};

static CREATE_HOOK: AtomicIsize = AtomicIsize::new(0);
static SHOW_HOOK: AtomicIsize = AtomicIsize::new(0);
static OSD_HOST: AtomicIsize = AtomicIsize::new(0);
static POLLER_STARTED: AtomicBool = AtomicBool::new(false);

fn class_is(hwnd: HWND, expected: PCWSTR) -> bool {
    let mut class = [0u16; 128];
    let len = unsafe { GetClassNameW(hwnd, &mut class) };
    let actual = &class[..len.max(0) as usize];
    let expected = unsafe { expected.as_wide() };
    actual == expected.strip_suffix(&[0]).unwrap_or(expected)
}

fn has_child(parent: HWND, class: PCWSTR) -> Option<HWND> {
    unsafe { FindWindowExW(Some(parent), None, class, PCWSTR::null()).ok() }
}

fn looks_like_modern_osd(hwnd: HWND) -> bool {
    let mut window = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut window) }.is_err() {
        return false;
    }
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFO::default();
    info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return false;
    }
    let width = window.right - window.left;
    let height = window.bottom - window.top;
    let screen_width = info.rcMonitor.right - info.rcMonitor.left;
    let screen_height = info.rcMonitor.bottom - info.rcMonitor.top;
    width > 0 && height > 0 && width * 3 <= screen_width && height * 5 <= screen_height
}

fn is_osd_host(hwnd: HWND) -> bool {
    if hwnd.0.is_null() {
        return false;
    }
    if class_is(hwnd, w!("NativeHWNDHost")) {
        return has_child(hwnd, w!("DirectUIHWND")).is_some();
    }
    if !class_is(hwnd, w!("XamlExplorerHostIslandWindow")) {
        return false;
    }
    let Some(bridge) = has_child(hwnd, w!("Windows.UI.Composition.DesktopWindowContentBridge")) else {
        return looks_like_modern_osd(hwnd);
    };
    has_child(bridge, w!("DesktopWindowXamlSource")).is_some() || looks_like_modern_osd(hwnd)
}

fn minimize(hwnd: HWND) {
    if unsafe { IsWindow(Some(hwnd)).as_bool() } && !unsafe { IsIconic(hwnd).as_bool() } {
        unsafe { let _ = ShowWindow(hwnd, SW_MINIMIZE); }
    }
}

unsafe extern "system" fn on_window_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    object_id: i32,
    child_id: i32,
    _thread_id: u32,
    _event_time: u32,
) {
    if (event == EVENT_OBJECT_CREATE || event == EVENT_OBJECT_SHOW)
        && object_id == 0
        && child_id == 0
        && is_osd_host(hwnd)
    {
        OSD_HOST.store(hwnd.0 as isize, Ordering::Relaxed);
        minimize(hwnd);
    }
}

unsafe extern "system" fn find_and_minimize(hwnd: HWND, _: LPARAM) -> BOOL {
    if is_osd_host(hwnd) {
        OSD_HOST.store(hwnd.0 as isize, Ordering::Relaxed);
        minimize(hwnd);
    }
    BOOL(1)
}

fn start_poller() {
    if POLLER_STARTED.swap(true, Ordering::Relaxed) {
        return;
    }
    let _ = std::thread::Builder::new().name("abra-osd-guard".into()).spawn(|| loop {
        let raw = OSD_HOST.load(Ordering::Relaxed);
        if raw != 0 {
            let hwnd = HWND(raw as *mut _);
            if unsafe { IsWindow(Some(hwnd)).as_bool() } {
                minimize(hwnd);
            } else {
                OSD_HOST.compare_exchange(raw, 0, Ordering::Relaxed, Ordering::Relaxed).ok();
            }
        }
        std::thread::sleep(Duration::from_millis(250));
    });
}

pub fn init() {
    unsafe {
        let flags = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
        if CREATE_HOOK.load(Ordering::Relaxed) == 0 {
            let hook = SetWinEventHook(EVENT_OBJECT_CREATE, EVENT_OBJECT_CREATE, None, Some(on_window_event), 0, 0, flags);
            if !hook.0.is_null() {
                CREATE_HOOK.store(hook.0 as isize, Ordering::Relaxed);
            }
        }
        if SHOW_HOOK.load(Ordering::Relaxed) == 0 {
            let hook = SetWinEventHook(EVENT_OBJECT_SHOW, EVENT_OBJECT_SHOW, None, Some(on_window_event), 0, 0, flags);
            if !hook.0.is_null() {
                SHOW_HOOK.store(hook.0 as isize, Ordering::Relaxed);
            }
        }
        let _ = EnumWindows(Some(find_and_minimize), LPARAM(0));
    }
    start_poller();
}

pub fn hide_native() {
    unsafe { let _ = EnumWindows(Some(find_and_minimize), LPARAM(0)); }
}
