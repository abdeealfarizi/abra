//! Fokus / minimize / tutup window (SPEC Â§4.5, alur fokus 5 langkah).
//!
//! Belum ada (M2b): koreksi posisi restore bila window minimize berada di luar layar.

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, FlashWindow, GetAncestor, GetForegroundWindow, IsIconic, IsWindow, IsWindowVisible,
    PostMessageW, SetForegroundWindow, ShowWindow, GA_ROOTOWNER, SW_MINIMIZE, SW_RESTORE, SW_SHOW, WM_CLOSE,
};

fn h(x: isize) -> HWND {
    HWND(x as *mut _)
}

pub fn is_alive(hwnd: isize) -> bool {
    unsafe { IsWindow(Some(h(hwnd))).as_bool() }
}

/// Judul window (kosong bila tidak ada).
pub fn title(hwnd: isize) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { windows::Win32::UI::WindowsAndMessaging::GetWindowTextW(h(hwnd), &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

pub fn is_minimized(hwnd: isize) -> bool {
    unsafe { IsIconic(h(hwnd)).as_bool() }
}

pub fn foreground() -> isize {
    unsafe { GetForegroundWindow().0 as isize }
}

/// Mengaktifkan `target`. Mengembalikan `true` hanya bila hasilnya terverifikasi.
/// `dock`: HWND dock sendiri; bila ia sempat jadi foreground, window luar terakhirlah yang dianggap di depan.
pub fn activate(target: isize, dock: isize) -> bool {
    unsafe {
        let hwnd = h(target);
        if !IsWindow(Some(hwnd)).as_bool() {
            return false;
        }

        let mut fg = GetForegroundWindow();
        if fg.0 as isize == dock {
            if let Some(last) = super::tracker::last_focused_except(dock, is_alive) {
                fg = h(last);
            }
        }
        // Klik kedua pada window yang sudah di depan = minimize. Dialog/popup milik window itu dihitung sama.
        let is_front = fg == hwnd || GetAncestor(fg, GA_ROOTOWNER) == hwnd;
        if is_front && IsWindowVisible(hwnd).as_bool() && !IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_MINIMIZE);
            return IsIconic(hwnd).as_bool();
        }

        if !IsWindowVisible(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_SHOW);
        }
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        if SetForegroundWindow(hwnd).as_bool() && GetForegroundWindow() == hwnd {
            return true;
        }
        // Windows membatasi pengambilan foreground. Angkat ke atas dan coba sekali lagi;
        // bila tetap gagal, minta perhatian (kedip) dan jangan mengaku sukses.
        let _ = BringWindowToTop(hwnd);
        if GetForegroundWindow() == hwnd || (SetForegroundWindow(hwnd).as_bool() && GetForegroundWindow() == hwnd) {
            return true;
        }
        let _ = FlashWindow(hwnd, true);
        false
    }
}

/// Minta window menutup diri (sama seperti tombol tutup; app boleh bertanya "simpan?").
pub fn request_close(hwnd: isize) {
    unsafe {
        let _ = PostMessageW(Some(h(hwnd)), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
}



/// Bawa `target` ke depan dengan paksa (tanpa perilaku "klik kedua = minimize" milik `activate`): dipakai Stage Manager,
/// yang harus mengaktifkan jendela pilihan walau proses kita bukan foreground. Memakai AttachThreadInput ke thread
/// foreground; bila masih gagal, mengetuk Alt sekali agar Windows melepas kunci foreground. Hasil diverifikasi.
pub fn force_foreground(target: isize) -> bool {
    use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows::Win32::UI::Input::KeyboardAndMouse::{keybd_event, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, VK_MENU};
    use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
    unsafe {
        let hwnd = h(target);
        if !IsWindow(Some(hwnd)).as_bool() {
            return false;
        }
        if !IsWindowVisible(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_SHOW);
        }
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        if GetForegroundWindow() == hwnd {
            return true;
        }
        let fg = GetForegroundWindow();
        let (me, other) = (GetCurrentThreadId(), GetWindowThreadProcessId(fg, None));
        let attached = other != 0 && other != me && AttachThreadInput(me, other, true).as_bool();
        let _ = BringWindowToTop(hwnd);
        let mut ok = SetForegroundWindow(hwnd).as_bool() && GetForegroundWindow() == hwnd;
        if attached {
            let _ = AttachThreadInput(me, other, false);
        }
        if !ok {
            // ketukan Alt: Windows menganggap proses ini baru menerima input sehingga SetForegroundWindow diizinkan
            keybd_event(VK_MENU.0 as u8, 0, KEYBD_EVENT_FLAGS(0), 0);
            keybd_event(VK_MENU.0 as u8, 0, KEYEVENTF_KEYUP, 0);
            let _ = BringWindowToTop(hwnd);
            ok = SetForegroundWindow(hwnd).as_bool() && GetForegroundWindow() == hwnd;
        }
        if !ok {
            let _ = FlashWindow(hwnd, true);
        }
        ok
    }
}
