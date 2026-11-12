//! Memantau perubahan window lewat WinEvent (tanpa polling) dan mencatat kapan tiap window
//! terakhir difokus. Hook butuh message loop di thread pemasang (thread UI).

use std::collections::HashMap;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{Mutex, MutexGuard};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    EVENT_OBJECT_CREATE, EVENT_OBJECT_DESTROY, EVENT_OBJECT_HIDE, EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_SHOW,
    EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_MINIMIZEEND, EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MOVESIZEEND,
    GetForegroundWindow, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
};

static FOCUS: Mutex<Option<HashMap<isize, i64>>> = Mutex::new(None);
static CALLBACKS: Mutex<Vec<fn()>> = Mutex::new(Vec::new());
static FX_CALLBACKS: Mutex<Vec<fn(u32, isize)>> = Mutex::new(Vec::new());
static HOOKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Window foreground terakhir milik proses lain. Hook memakai SKIPOWNPROCESS, jadi window kita sendiri tidak menimpanya.
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

/// Waktu terakhir `hwnd` menjadi foreground (0 = belum pernah teramati).
pub fn recent(hwnd: isize) -> i64 {
    focus_map().as_ref().and_then(|m| m.get(&hwnd).copied()).unwrap_or(0)
}

/// Window luar yang paling baru difokus selain `exclude` (untuk kasus dock sendiri sempat jadi foreground).
pub fn last_focused_except(exclude: isize, alive: impl Fn(isize) -> bool) -> Option<isize> {
    let map = focus_map();
    map.as_ref()?
        .iter()
        .filter(|(h, _)| **h != exclude && alive(**h))
        .max_by_key(|(_, t)| **t)
        .map(|(h, _)| *h)
}

/// Buang catatan window yang sudah tidak ada agar peta tidak membengkak.
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
    // Kontrol anak melaporkan event dengan HWND induk yang valid; itu bukan perubahan window app.
    if hwnd.0.is_null() || id_object != 0 || id_child != 0 {
        return;
    }
    // Penerima efek jendela: hanya event yang relevan, dengan HWND dan jenis event.
    if matches!(
        event,
        EVENT_SYSTEM_FOREGROUND | EVENT_SYSTEM_MINIMIZESTART | EVENT_SYSTEM_MINIMIZEEND | EVENT_OBJECT_DESTROY | EVENT_SYSTEM_MOVESIZEEND
    ) || (event == EVENT_OBJECT_LOCATIONCHANGE && hwnd == GetForegroundWindow()) {
        let fx: Vec<fn(u32, isize)> = FX_CALLBACKS.lock().unwrap_or_else(|e| e.into_inner()).clone();
        for f in fx {
            f(event, hwnd.0 as isize);
        }
    }
    // Gerak window lain terjadi terus-menerus; hanya yang di depan memengaruhi dock.
    if event == EVENT_OBJECT_LOCATIONCHANGE && hwnd != GetForegroundWindow() {
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

/// Penerima efek jendela: dipanggil di thread UI dengan (kode event, hwnd). Pemanggil harus ringan (mis. memposting pesan).
pub fn install_fx(cb: fn(u32, isize)) {
    FX_CALLBACKS.lock().unwrap_or_else(|e| e.into_inner()).push(cb);
}

pub const EV_FOREGROUND: u32 = EVENT_SYSTEM_FOREGROUND;
pub const EV_MINIMIZE_START: u32 = EVENT_SYSTEM_MINIMIZESTART;
pub const EV_MINIMIZE_END: u32 = EVENT_SYSTEM_MINIMIZEEND;
pub const EV_DESTROY: u32 = EVENT_OBJECT_DESTROY;
pub const EV_MOVESIZE_END: u32 = EVENT_SYSTEM_MOVESIZEEND;
pub const EV_LOCATION: u32 = EVENT_OBJECT_LOCATIONCHANGE;

/// `on_change` dipanggil di thread UI untuk tiap event; pemanggil wajib men-debounce (timer).
pub fn install(on_change: fn()) {
    CALLBACKS.lock().unwrap_or_else(|e| e.into_inner()).push(on_change);
    // Awal: window yang sudah di depan saat kita mulai (belum ada event FOREGROUND untuknya).
    unsafe {
        let fg = GetForegroundWindow();
        let mut pid = 0u32;
        windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(fg, Some(&mut pid));
        if !fg.0.is_null() && pid != std::process::id() && EXTERNAL_FG.load(Ordering::Relaxed) == 0 {
            EXTERNAL_FG.store(fg.0 as isize, Ordering::Relaxed);
        }
    }
    // Hook dipasang sekali; pemanggil berikutnya hanya menambah penerima.
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
    ] {
        // Hook tidak pernah dilepas selama proses hidup; Windows melepasnya saat proses berakhir.
        unsafe {
            let _ = SetWinEventHook(ev, ev, None, Some(callback), 0, 0, WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS);
        }
    }
}



