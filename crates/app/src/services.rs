use nacre_platform::{audio, brightness};
use std::collections::HashMap;
use std::sync::mpsc::Sender;
use std::sync::{LazyLock, Mutex, OnceLock};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

pub const WM_VOLUME: u32 = WM_APP + 20;
pub const WM_BRIGHT: u32 = WM_APP + 21;
pub const WM_RADIO: u32 = WM_APP + 22;

static HUD: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
static ISLANDS: Mutex<Vec<isize>> = Mutex::new(Vec::new());
static BAR_WINDOWS: Mutex<Vec<isize>> = Mutex::new(Vec::new());
static NOTCHES: LazyLock<Mutex<HashMap<u8, (i32, i32, i32)>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
static AUDIO: OnceLock<Sender<audio::Cmd>> = OnceLock::new();
static BRIGHT: OnceLock<Sender<brightness::Cmd>> = OnceLock::new();

pub fn register_hud(h: HWND) { HUD.store(h.0 as isize, std::sync::atomic::Ordering::Relaxed); }

pub fn register_island(h: HWND) {
    let raw = h.0 as isize;
    let mut islands = ISLANDS.lock().unwrap_or_else(|e| e.into_inner());
    if !islands.contains(&raw) { islands.push(raw); }
}

pub fn unregister_island(h: HWND) {
    ISLANDS.lock().unwrap_or_else(|e| e.into_inner()).retain(|id| *id != h.0 as isize);
}

fn post_all(msg: u32) {
    let mut targets = ISLANDS.lock().unwrap_or_else(|e| e.into_inner()).clone();
    targets.push(HUD.load(std::sync::atomic::Ordering::Relaxed));
    for h in targets.into_iter().filter(|h| *h != 0) {
        unsafe { let _ = PostMessageW(Some(HWND(h as *mut _)), msg, WPARAM(0), LPARAM(0)); }
    }
}

pub fn init() {
    if AUDIO.get().is_some() { return; }
    let a = audio::spawn(|| post_all(WM_VOLUME));
    let _ = AUDIO.set(a.commands);
    let b = brightness::spawn(|| post_all(WM_BRIGHT));
    let _ = BRIGHT.set(b.commands);
}

pub fn set_volume(v: f32) {
    if let Some(tx) = AUDIO.get() { let _ = tx.send(audio::Cmd::SetLevel(v)); }
}

pub fn toggle_mute() {
    if let Some(tx) = AUDIO.get() { let _ = tx.send(audio::Cmd::ToggleMute); }
}

pub fn set_brightness(frac: f32) {
    if let Some(tx) = BRIGHT.get() { let _ = tx.send(brightness::Cmd::Set((frac.clamp(0.0, 1.0) * 100.0).round() as u8)); }
}

pub fn set_notch(monitor: u8, l: i32, r: i32, h: i32) {
    let mut notches = NOTCHES.lock().unwrap_or_else(|e| e.into_inner());
    if r > l && h > 0 { notches.insert(monitor, (l, r, h)); } else { notches.remove(&monitor); }
    drop(notches);
    for raw in BAR_WINDOWS.lock().unwrap_or_else(|e| e.into_inner()).iter().copied() {
        unsafe { let _ = PostMessageW(Some(HWND(raw as *mut _)), WM_APP + 71, WPARAM(0), LPARAM(0)); }
    }
}

pub fn notch_for(monitor: u8) -> Option<(i32, i32, i32)> {
    NOTCHES.lock().unwrap_or_else(|e| e.into_inner()).get(&monitor).copied()
}

pub fn register_bar(h: HWND) {
    let raw = h.0 as isize;
    let mut bars = BAR_WINDOWS.lock().unwrap_or_else(|e| e.into_inner());
    if !bars.contains(&raw) { bars.push(raw); }
}

pub fn unregister_bar(h: HWND) {
    BAR_WINDOWS.lock().unwrap_or_else(|e| e.into_inner()).retain(|id| *id != h.0 as isize);
}

pub fn post_island(msg: u32) {
    for raw in ISLANDS.lock().unwrap_or_else(|e| e.into_inner()).iter().copied() {
        unsafe { let _ = PostMessageW(Some(HWND(raw as *mut _)), msg, WPARAM(0), LPARAM(0)); }
    }
}
