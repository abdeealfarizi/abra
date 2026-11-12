//! Menyembunyikan/menampilkan ikon desktop (daftar SysListView32 milik shell), dipakai Stage Manager.

use windows::core::w;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{FindWindowExW, FindWindowW, ShowWindow, SW_HIDE, SW_SHOWNA};

fn list_view() -> Option<HWND> {
    unsafe {
        let mut view = FindWindowW(w!("Progman"), None).ok().and_then(|p| FindWindowExW(Some(p), None, w!("SHELLDLL_DefView"), None).ok());
        if view.is_none() {
            // setelah wallpaper dinamis, SHELLDLL_DefView pindah ke salah satu WorkerW
            let mut prev: Option<HWND> = None;
            while let Ok(wk) = FindWindowExW(None, prev, w!("WorkerW"), None) {
                if let Ok(v) = FindWindowExW(Some(wk), None, w!("SHELLDLL_DefView"), None) {
                    view = Some(v);
                    break;
                }
                prev = Some(wk);
            }
        }
        FindWindowExW(Some(view?), None, w!("SysListView32"), None).ok()
    }
}

pub fn set_visible(show: bool) {
    if let Some(h) = list_view() {
        unsafe {
            let _ = ShowWindow(h, if show { SW_SHOWNA } else { SW_HIDE });
        }
    }
}
