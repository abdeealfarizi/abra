//! Window utama tersembunyi: penerima pesan sistem (TaskbarCreated, sesi berakhir),
//! ikon tray, dan pemilik siklus hidup. Bukan permukaan UI.

use crate::{paths, single_instance};
use nacre_platform::shell::taskbar;
use std::sync::atomic::{AtomicU32, Ordering};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::*;

pub const WM_TRAY: u32 = WM_APP + 1;
const ID_TOGGLE_TASKBAR: usize = 1;
pub const ID_QUIT: usize = 2;
const TIMER_EXIT: usize = 1;

static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

pub fn create() -> windows::core::Result<HWND> {
    unsafe {
        let hi = GetModuleHandleW(None)?.into();
        let class = w!("NacreMain");
        RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(proc), hInstance: hi, lpszClassName: class, ..Default::default() });
        TASKBAR_CREATED.store(RegisterWindowMessageW(w!("TaskbarCreated")), Ordering::Relaxed);
        // Window tingkat-atas tak terlihat (bukan message-only) agar menerima pesan broadcast.
        CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class,
            w!("ABRA"),
            WS_POPUP,
            0, 0, 0, 0,
            None, None, Some(hi), None,
        )
    }
}

pub fn set_exit_timer(hwnd: HWND, seconds: u32) {
    unsafe {
        SetTimer(Some(hwnd), TIMER_EXIT, seconds * 1000, None);
    }
}

/// Ikon ABRA (sumber daya id 1 di exe, ditanam build.rs) seukuran ikon kecil sistem; cadangan: ikon aplikasi umum.
fn tray_icon() -> HICON {
    unsafe {
        let hi = GetModuleHandleW(None).map(|h| HINSTANCE(h.0)).ok();
        let (cx, cy) = (GetSystemMetrics(SM_CXSMICON), GetSystemMetrics(SM_CYSMICON));
        match LoadImageW(hi, PCWSTR(1 as *const u16), IMAGE_ICON, cx, cy, LR_SHARED) {
            Ok(h) if !h.is_invalid() => HICON(h.0),
            _ => LoadIconW(None, IDI_APPLICATION).unwrap_or_default(),
        }
    }
}

fn tray_data(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut nid = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: WM_TRAY,
        hIcon: tray_icon(),
        ..Default::default()
    };
    for (i, c) in "ABRA".encode_utf16().enumerate() {
        nid.szTip[i] = c;
    }
    nid
}

pub fn add_tray(hwnd: HWND) {
    let nid = tray_data(hwnd);
    unsafe {
        let _ = Shell_NotifyIconW(NIM_ADD, &nid);
    }
}

fn remove_tray(hwnd: HWND) {
    let nid = tray_data(hwnd);
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
    }
}

/// Apakah pengaturan "mode penuh" boleh mengubah taskbar (mati untuk uji `--keep-taskbar` atau tanpa dock).
pub static TAKEOVER_ALLOWED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Samakan entri Run dengan pengaturan. Hanya menulis bila berbeda (atau jalur exe berubah).
pub fn apply_autostart() {
    use nacre_platform::startup;
    let want = crate::prefs::get().autostart;
    let Ok(exe) = std::env::current_exe() else { return };
    let cur = startup::current();
    let ok = match (want, &cur) {
        (true, Some(c)) if *c == startup::command_for(&exe) => return,
        (false, None) => return,
        _ => startup::set(want, &exe),
    };
    nacre_platform::log::line("startup", &format!("autostart={want} hasil={ok:?}"));
}

pub fn set_taskbar_hidden(on: bool) {
    if on {
        if let Err(e) = taskbar::hide(&paths::marker(), &paths::guard_exe()) {
            nacre_platform::log::line("taskbar", &format!("hide gagal: {e}"));
        }
    } else {
        taskbar::restore();
    }
    crate::dock::request_appbar_resync();
}

fn show_menu(hwnd: HWND) {
    unsafe {
        let Ok(menu) = CreatePopupMenu() else { return };
        let check = if taskbar::is_hidden() { MF_CHECKED } else { MF_UNCHECKED };
        let _ = AppendMenuW(menu, MF_STRING | check, ID_TOGGLE_TASKBAR, w!("Sembunyikan taskbar (uji)"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        let _ = AppendMenuW(menu, MF_STRING, ID_QUIT, w!("Keluar dari ABRA"));
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        // Wajib agar menu tertutup saat klik di luar.
        let _ = SetForegroundWindow(hwnd);
        let _ = TrackPopupMenu(menu, TPM_RIGHTBUTTON | TPM_BOTTOMALIGN, pt.x, pt.y, Some(0), hwnd, None);
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let tc = TASKBAR_CREATED.load(Ordering::Relaxed);
    if tc != 0 && msg == tc {
        // Explorer dibuat ulang: ikon tray hilang dan taskbar baru muncul lagi.
        add_tray(hwnd);
        taskbar::reassert();
        crate::dock::request_appbar_resync();
        nacre_platform::log::line("shell", "TaskbarCreated");
        return LRESULT(0);
    }
    match msg {
        crate::prefs::WM_PREFS => {
            apply_autostart();
            if TAKEOVER_ALLOWED.load(Ordering::Relaxed) {
                let p = crate::prefs::get();
                let want = p.takeover && p.dock_enabled;
                if want != taskbar::is_hidden() {
                    nacre_platform::log::line("taskbar", &format!("pengaturan mode penuh → {want}"));
                    set_taskbar_hidden(want);
                }
            }
            LRESULT(0)
        }
        WM_TRAY => {
            if (lp.0 as u32 & 0xFFFF) == WM_RBUTTONUP || (lp.0 as u32 & 0xFFFF) == WM_CONTEXTMENU {
                show_menu(hwnd);
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            match wp.0 & 0xFFFF {
                ID_TOGGLE_TASKBAR => set_taskbar_hidden(!taskbar::is_hidden()),
                ID_QUIT => {
                    let _ = DestroyWindow(hwnd);
                }
                _ => {}
            }
            LRESULT(0)
        }
        single_instance::WM_WAKE => {
            nacre_platform::log::line("instance", "peluncuran kedua terdeteksi");
            LRESULT(0)
        }
        WM_TIMER if wp.0 == TIMER_EXIT => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_ENDSESSION => {
            if wp.0 != 0 {
                taskbar::restore();
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            crate::fx::restore_all();
            taskbar::restore();
            remove_tray(hwnd);
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}


