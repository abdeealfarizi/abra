use std::collections::HashMap;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{Mutex, MutexGuard};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    EVENT_OBJECT_CREATE, EVENT_OBJECT_DESTROY, EVENT_OBJECT_HIDE, EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_SHOW, EVENT_OBJECT_UNCLOAKED,
    EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_MINIMIZEEND, EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MOVESIZEEND,
    GetForegroundWindow, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
};

static FOCUS: Mutex<Option<HashMap<isize, i64>>> = Mutex::new(None);
static CALLBACKS: Mutex<Vec<fn()>> = Mutex::new(Vec::new());
static FX_CALLBACKS: Mutex<Vec<fn(u32, isize)>> = Mutex::new(Vec::new());
static HOOKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static EXTERNAL_FG: AtomicIsize = AtomicIsize::new(0);

pub fn external_foreground() -> isize {
    EXTERNAL_FG.load(Ordering::Relaxed)
}

fn focus_map() -> MutexGuard<'static, Option<HashMap<isize, i64>>> {
    FOCUS.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn recent(hwnd: isize) -> i64 {
    focus_map().as_ref().and_then(|m| m.get(&hwnd).copied()).unwrap_or(0)
}

pub fn last_focused_except(exclude: isize, alive: impl Fn(isize) -> bool) -> Option<isize> {
    let map = focus_map();
    map.as_ref()?
        .iter()
        .filter(|(h, _)| **h != exclude && alive(**h))
        .max_by_key(|(_, t)| **t)
        .map(|(h, _)| *h)
}

pub fn prune(alive: impl Fn(isize) -> bool) {
    if let Some(m) = focus_map().as_mut() {
        m.retain(|h, _| alive(*h));
    }
}

unsafe extern "system" fn callback(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _thread: u32,
    _time: u32,
) {
    if hwnd.0.is_null() || id_object != 0 || id_child != 0 {
        return;
    }
    if matches!(
        event,
        EVENT_SYSTEM_FOREGROUND | EVENT_SYSTEM_MINIMIZESTART | EVENT_SYSTEM_MINIMIZEEND | EVENT_OBJECT_DESTROY | EVENT_SYSTEM_MOVESIZEEND | EVENT_OBJECT_SHOW | EVENT_OBJECT_UNCLOAKED
    ) || (event == EVENT_OBJECT_LOCATIONCHANGE && (hwnd == GetForegroundWindow() || hwnd.0 as isize == EXTERNAL_FG.load(Ordering::Relaxed)))
    {
        let fx: Vec<fn(u32, isize)> = FX_CALLBACKS.lock().unwrap_or_else(|e| e.into_inner()).clone();
        for f in fx {
            f(event, hwnd.0 as isize);
        }
    }
    if event == EVENT_OBJECT_UNCLOAKED {
        return;
    }
    if event == EVENT_OBJECT_LOCATIONCHANGE && hwnd != GetForegroundWindow() && hwnd.0 as isize != EXTERNAL_FG.load(Ordering::Relaxed) {
        return;
    }
    if event == EVENT_SYSTEM_FOREGROUND {
        focus_map().get_or_insert_with(HashMap::new).insert(hwnd.0 as isize, now_ms());
        EXTERNAL_FG.store(hwnd.0 as isize, Ordering::Relaxed);
    }
    let list: Vec<fn()> = CALLBACKS.lock().unwrap_or_else(|e| e.into_inner()).clone();
    for f in list {
        f();
    }
}

/// Synchronize foreground tracking and notify bars immediately after ABRA switches focus itself.
pub fn bump() {
    unsafe {
        let fg = GetForegroundWindow();
        if !fg.0.is_null() {
            let mut pid = 0u32;
            windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(fg, Some(&mut pid));
            if pid != std::process::id() {
                let h = fg.0 as isize;
                focus_map().get_or_insert_with(HashMap::new).insert(h, now_ms());
                EXTERNAL_FG.store(h, Ordering::Relaxed);
                let fx: Vec<fn(u32, isize)> = FX_CALLBACKS.lock().unwrap_or_else(|e| e.into_inner()).clone();
                for f in fx {
                    f(EVENT_SYSTEM_FOREGROUND, h);
                }
            }
        }
    }
    let list: Vec<fn()> = CALLBACKS.lock().unwrap_or_else(|e| e.into_inner()).clone();
    for f in list {
        f();
    }
}

pub fn install_fx(cb: fn(u32, isize)) {
    FX_CALLBACKS.lock().unwrap_or_else(|e| e.into_inner()).push(cb);
}

pub const EV_FOREGROUND: u32 = EVENT_SYSTEM_FOREGROUND;
pub const EV_MINIMIZE_START: u32 = EVENT_SYSTEM_MINIMIZESTART;
pub const EV_MINIMIZE_END: u32 = EVENT_SYSTEM_MINIMIZEEND;
pub const EV_DESTROY: u32 = EVENT_OBJECT_DESTROY;
pub const EV_MOVESIZE_END: u32 = EVENT_SYSTEM_MOVESIZEEND;
pub const EV_LOCATION: u32 = EVENT_OBJECT_LOCATIONCHANGE;
pub const EV_SHOW: u32 = EVENT_OBJECT_SHOW;
pub const EV_UNCLOAKED: u32 = EVENT_OBJECT_UNCLOAKED;

pub fn install(on_change: fn()) {
    CALLBACKS.lock().unwrap_or_else(|e| e.into_inner()).push(on_change);
    unsafe {
        let fg = GetForegroundWindow();
        let mut pid = 0u32;
        windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(fg, Some(&mut pid));
        if !fg.0.is_null() && pid != std::process::id() && EXTERNAL_FG.load(Ordering::Relaxed) == 0 {
            EXTERNAL_FG.store(fg.0 as isize, Ordering::Relaxed);
        }
    }
    if HOOKED.swap(true, Ordering::Relaxed) {
        return;
    }
    for ev in [
        EVENT_OBJECT_CREATE,
        EVENT_OBJECT_DESTROY,
        EVENT_OBJECT_SHOW,
        EVENT_OBJECT_HIDE,
        EVENT_OBJECT_LOCATIONCHANGE,
        EVENT_SYSTEM_MOVESIZEEND,
        EVENT_SYSTEM_FOREGROUND,
        EVENT_SYSTEM_MINIMIZESTART,
        EVENT_SYSTEM_MINIMIZEEND,
        EVENT_OBJECT_UNCLOAKED,
    ] {
        unsafe {
            let _ = SetWinEventHook(ev, ev, None, Some(callback), 0, 0, WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS);
        }
    }
}
