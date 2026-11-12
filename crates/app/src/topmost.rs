//! Menjaga dock, menu bar, notch, dan jendela ABRA lainnya tetap di atas semua jendela (termasuk app layar penuh).
//! Jendela layar penuh/aktif bisa mengambil posisi teratas dalam grup TOPMOST, jadi setiap perubahan foreground
//! kita tegaskan ulang `HWND_TOPMOST` bagi jendela ABRA yang terlihat (urutan relatifnya dipertahankan).

use nacre_platform::wins::tracker;
use std::sync::atomic::{AtomicIsize, Ordering};
use windows::core::w;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows_core::BOOL;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

const WM_RAISE: u32 = WM_APP + 150;
const T_RAISE: usize = 1;
const T_RAISE2: usize = 2;

static CTL: AtomicIsize = AtomicIsize::new(0);

fn noop() {}

fn hook(ev: u32, _hwnd: isize) {
    if ev == tracker::EV_FOREGROUND || ev == tracker::EV_MOVESIZE_END || ev == tracker::EV_MINIMIZE_END {
        let h = CTL.load(Ordering::Relaxed);
        if h != 0 {
            unsafe {
                let _ = PostMessageW(Some(HWND(h as *mut _)), WM_RAISE, WPARAM(0), LPARAM(0));
            }
        }
    }
}

/// Apakah `hwnd` milik proses ABRA sendiri.
pub fn is_own_window(hwnd: isize) -> bool {
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(HWND(hwnd as *mut _), Some(&mut pid));
    }
    pid == std::process::id()
}

pub fn init() -> windows_core::Result<()> {
    unsafe {
        let hi: HINSTANCE = GetModuleHandleW(None)?.into();
        let class = w!("AbraTopmostCtl");
        RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(ctl_proc), hInstance: hi, lpszClassName: class, ..Default::default() });
        let hwnd = CreateWindowExW(WINDOW_EX_STYLE(0), class, w!("abra-topmost"), WS_POPUP, 0, 0, 0, 0, Some(HWND_MESSAGE), None, Some(hi), None)?;
        CTL.store(hwnd.0 as isize, Ordering::Relaxed);
        tracker::install(noop);
        tracker::install_fx(hook);
    }
    Ok(())
}

unsafe extern "system" fn collect(h: HWND, lp: LPARAM) -> BOOL {
    let v = &mut *(lp.0 as *mut Vec<isize>);
    let mut pid = 0u32;
    GetWindowThreadProcessId(h, Some(&mut pid));
    if pid == std::process::id() && IsWindowVisible(h).as_bool() && GetWindowLongW(h, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST.0 != 0 {
        v.push(h.0 as isize);
    }
    BOOL(1)
}

/// Tegaskan TOPMOST bagi semua jendela ABRA yang terlihat, dari paling bawah ke paling atas agar urutan relatifnya sama.
pub fn raise_all() {
    let mut v: Vec<isize> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(collect), LPARAM(&mut v as *mut _ as isize));
        for h in v.iter().rev() {
            let _ = SetWindowPos(HWND(*h as *mut _), Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }
}

/// Naikkan jendela "chrome" (dock, menu bar, notch) di atas jendela ABRA lain, mis. di atas Launchpad layar penuh.
pub fn raise_chrome() {
    unsafe {
        for class in [w!("NacreDock"), w!("AbraMenuBar"), w!("NacreIsland")] {
            if let Ok(h) = FindWindowW(class, None) {
                if IsWindowVisible(h).as_bool() {
                    let _ = SetWindowPos(h, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
                }
            }
        }
    }
}

unsafe extern "system" fn ctl_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_RAISE => {
            SetTimer(Some(hwnd), T_RAISE, 50, None);
            SetTimer(Some(hwnd), T_RAISE2, 600, None);
            LRESULT(0)
        }
        WM_TIMER => {
            let _ = KillTimer(Some(hwnd), wp.0);
            raise_all();
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
