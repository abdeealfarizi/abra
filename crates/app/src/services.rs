//! Layanan sistem bersama (volume, kecerahan). Event-driven: pekerja platform mengabari lewat pesan window
//! ke HUD dan island yang terdaftar; nilai terbaru dibaca dari `audio::latest()` / `brightness::latest()`.

use nacre_platform::{audio, brightness};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::OnceLock;
use std::sync::mpsc::Sender;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

pub const WM_VOLUME: u32 = WM_APP + 20;
pub const WM_BRIGHT: u32 = WM_APP + 21;
pub const WM_RADIO: u32 = WM_APP + 22;

static HUD: AtomicIsize = AtomicIsize::new(0);
static ISLAND: AtomicIsize = AtomicIsize::new(0);
static AUDIO: OnceLock<Sender<audio::Cmd>> = OnceLock::new();
static BRIGHT: OnceLock<Sender<brightness::Cmd>> = OnceLock::new();

pub fn register_hud(h: HWND) {
    HUD.store(h.0 as isize, Ordering::Relaxed);
}

pub fn register_island(h: HWND) {
    ISLAND.store(h.0 as isize, Ordering::Relaxed);
}

fn post_all(msg: u32) {
    for h in [HUD.load(Ordering::Relaxed), ISLAND.load(Ordering::Relaxed)] {
        if h != 0 {
            unsafe {
                let _ = PostMessageW(Some(HWND(h as *mut _)), msg, WPARAM(0), LPARAM(0));
            }
        }
    }
}

/// Memulai pekerja volume dan kecerahan (sekali).
pub fn init() {
    if AUDIO.get().is_some() {
        return;
    }
    let a = audio::spawn(|| post_all(WM_VOLUME));
    let _ = AUDIO.set(a.commands);
    let b = brightness::spawn(|| post_all(WM_BRIGHT));
    let _ = BRIGHT.set(b.commands);
}

pub fn set_volume(v: f32) {
    if let Some(tx) = AUDIO.get() {
        let _ = tx.send(audio::Cmd::SetLevel(v));
    }
}

pub fn toggle_mute() {
    if let Some(tx) = AUDIO.get() {
        let _ = tx.send(audio::Cmd::ToggleMute);
    }
}

pub fn set_brightness(frac: f32) {
    if let Some(tx) = BRIGHT.get() {
        let _ = tx.send(brightness::Cmd::Set((frac.clamp(0.0, 1.0) * 100.0).round() as u8));
    }
}

// ---- posisi notch (dibaca menu bar untuk menyisakan celah) ----
use std::sync::atomic::AtomicI32;
static NOTCH_L: AtomicI32 = AtomicI32::new(0);
static NOTCH_R: AtomicI32 = AtomicI32::new(0);
static NOTCH_H: AtomicI32 = AtomicI32::new(0);

/// Island mengumumkan pil notch (px layar) saat diam; `(0,0,0)` bila notch mati.
pub fn set_notch(l: i32, r: i32, h: i32) {
    NOTCH_L.store(l, Ordering::Relaxed);
    NOTCH_R.store(r, Ordering::Relaxed);
    NOTCH_H.store(h, Ordering::Relaxed);
    let bar = BAR.load(Ordering::Relaxed);
    if bar != 0 {
        unsafe {
            let _ = PostMessageW(Some(HWND(bar as *mut _)), WM_APP + 71, WPARAM(0), LPARAM(0));
        }
    }
}

/// `(kiri, kanan, tinggi)` pil notch atau `None` bila notch mati.
pub fn notch() -> Option<(i32, i32, i32)> {
    let (l, r, h) = (NOTCH_L.load(Ordering::Relaxed), NOTCH_R.load(Ordering::Relaxed), NOTCH_H.load(Ordering::Relaxed));
    (r > l && h > 0).then_some((l, r, h))
}

static BAR: AtomicIsize = AtomicIsize::new(0);
pub fn register_bar(h: HWND) {
    BAR.store(h.0 as isize, Ordering::Relaxed);
}

/// Kirim pesan ke island (mis. buka panel Kontrol dari menu bar).
pub fn post_island(msg: u32) {
    let h = ISLAND.load(Ordering::Relaxed);
    if h != 0 {
        unsafe {
            let _ = PostMessageW(Some(HWND(h as *mut _)), msg, WPARAM(0), LPARAM(0));
        }
    }
}
