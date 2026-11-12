//! Pintasan ala Mac yang berlaku di semua app: `Alt+\`` berpindah antar jendela app yang sama (Cmd+\`), `Alt+Shift+\``
//! mundur, `Ctrl+Alt+H` menyembunyikan (meminimalkan) semua jendela app aktif, `Ctrl+Alt+M` meminimalkan jendela aktif,
//! dan `Ctrl+Alt+Q` menutup semua jendela app aktif dengan sopan (WM_CLOSE, app tetap boleh bertanya "simpan?").
//! Hanya memakai pesan jendela standar; tidak menyuntik apa pun ke app lain. Bisa dimatikan di Pengaturan.

use crate::prefs;
use nacre_core::dock::ring::{next_in_ring, prev_in_ring};
use nacre_platform::log;
use nacre_platform::wins::{enumerate, focus, tracker};
use windows::core::w;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT};
use windows::Win32::UI::WindowsAndMessaging::*;

const VK_BACKTICK: u32 = 0xC0; // VK_OEM_3
const ID_NEXT: i32 = 0xAB20;
const ID_PREV: i32 = 0xAB21;
const ID_HIDE: i32 = 0xAB22;
const ID_MIN: i32 = 0xAB23;
const ID_QUIT: i32 = 0xAB24;

static CTL: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
static REGISTERED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn exe_of(wins: &[nacre_core::dock::items::Win], hwnd: isize) -> Option<String> {
    wins.iter().find(|w| w.hwnd == hwnd).map(|w| w.path.to_lowercase()).filter(|p| !p.is_empty())
}

/// Pindah ke jendela berikut/sebelumnya milik app yang sedang aktif.
pub fn cycle(back: bool) {
    let fg = focus::foreground();
    let wins = enumerate::list(std::process::id());
    let Some(cur) = exe_of(&wins, fg) else { return };
    let mut group: Vec<isize> = wins.iter().filter(|w| w.path.to_lowercase() == cur).map(|w| w.hwnd).collect();
    if group.len() < 2 {
        return;
    }
    // paling baru difokus lebih dulu (untuk kasus jendela depan bukan bagian grup)
    group.sort_by_key(|h| std::cmp::Reverse(tracker::recent(*h)));
    let target = if back { prev_in_ring(&group, fg) } else { next_in_ring(&group, fg) };
    if let Some(t) = target {
        focus::force_foreground(t);
    }
}

/// Minimalkan semua jendela app aktif.
pub fn hide_foreground_app() {
    let fg = focus::foreground();
    let wins = enumerate::list(std::process::id());
    let Some(cur) = exe_of(&wins, fg) else { return };
    for w in wins.iter().filter(|w| w.path.to_lowercase() == cur && !focus::is_minimized(w.hwnd)) {
        unsafe {
            let _ = ShowWindow(HWND(w.hwnd as *mut _), SW_MINIMIZE);
        }
    }
}

pub fn minimize_foreground() {
    let fg = focus::foreground();
    if fg != 0 && !crate::topmost::is_own_window(fg) {
        unsafe {
            let _ = ShowWindow(HWND(fg as *mut _), SW_MINIMIZE);
        }
    }
}

/// Tutup semua jendela app aktif dengan WM_CLOSE (sopan). Shell Windows tidak pernah ditutup.
pub fn quit_foreground_app() {
    let fg = focus::foreground();
    let wins = enumerate::list(std::process::id());
    let Some(cur) = exe_of(&wins, fg) else { return };
    let name = cur.rsplit(['\\', '/']).next().unwrap_or("");
    if matches!(name, "explorer.exe" | "dwm.exe" | "winlogon.exe" | "csrss.exe" | "searchhost.exe" | "shellexperiencehost.exe") && !wins.iter().any(|w| w.hwnd == fg && w.path.to_lowercase().ends_with("explorer.exe") && wins.len() > 1) {
        // Explorer: hanya jendela aktif yang ditutup, bukan seluruh shell
        unsafe {
            let _ = PostMessageW(Some(HWND(fg as *mut _)), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
        return;
    }
    for w in wins.iter().filter(|w| w.path.to_lowercase() == cur) {
        unsafe {
            let _ = PostMessageW(Some(HWND(w.hwnd as *mut _)), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
}

fn register(hwnd: HWND) {
    unsafe {
        let want = prefs::get().mac_shortcuts;
        let have = REGISTERED.load(std::sync::atomic::Ordering::Relaxed);
        if want && !have {
            let mut ok = 0;
            for (id, mods, vk) in [
                (ID_NEXT, MOD_ALT, VK_BACKTICK),
                (ID_PREV, MOD_ALT | MOD_SHIFT, VK_BACKTICK),
                (ID_HIDE, MOD_CONTROL | MOD_ALT, 0x48),
                (ID_MIN, MOD_CONTROL | MOD_ALT, 0x4D),
                (ID_QUIT, MOD_CONTROL | MOD_ALT, 0x51),
            ] {
                if RegisterHotKey(Some(hwnd), id, mods | MOD_NOREPEAT, vk).is_ok() {
                    ok += 1;
                } else {
                    log::line("macshortcuts", &format!("pintasan {id:#x} sudah dipakai program lain"));
                }
            }
            REGISTERED.store(ok > 0, std::sync::atomic::Ordering::Relaxed);
        } else if !want && have {
            for id in [ID_NEXT, ID_PREV, ID_HIDE, ID_MIN, ID_QUIT] {
                let _ = UnregisterHotKey(Some(hwnd), id);
            }
            REGISTERED.store(false, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

pub fn init() -> windows_core::Result<()> {
    unsafe {
        let hi: HINSTANCE = GetModuleHandleW(None)?.into();
        let class = w!("AbraMacKeys");
        RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(proc), hInstance: hi, lpszClassName: class, ..Default::default() });
        let hwnd = CreateWindowExW(WINDOW_EX_STYLE(0), class, w!("abra-mackeys"), WS_POPUP, 0, 0, 0, 0, Some(HWND_MESSAGE), None, Some(hi), None)?;
        CTL.store(hwnd.0 as isize, std::sync::atomic::Ordering::Relaxed);
        prefs::listen(hwnd);
        register(hwnd);
    }
    Ok(())
}

pub fn shutdown() {
    let h = CTL.load(std::sync::atomic::Ordering::Relaxed);
    if h != 0 && REGISTERED.load(std::sync::atomic::Ordering::Relaxed) {
        unsafe {
            for id in [ID_NEXT, ID_PREV, ID_HIDE, ID_MIN, ID_QUIT] {
                let _ = UnregisterHotKey(Some(HWND(h as *mut _)), id);
            }
        }
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_HOTKEY => {
            match wp.0 as i32 {
                ID_NEXT => cycle(false),
                ID_PREV => cycle(true),
                ID_HIDE => hide_foreground_app(),
                ID_MIN => minimize_foreground(),
                ID_QUIT => quit_foreground_app(),
                _ => {}
            }
            LRESULT(0)
        }
        m if m == prefs::WM_PREFS => {
            register(hwnd);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
