//! Satu instance saja. Peluncuran kedua membangunkan instance pertama lalu keluar.
use windows::core::w;
use windows::Win32::Foundation::{CloseHandle, GetLastError, HWND, WPARAM, LPARAM, ERROR_ALREADY_EXISTS, HANDLE};
use windows::Win32::System::Threading::{
    CreateEventW, CreateMutexW, OpenEventW, SetEvent, WaitForSingleObject, EVENT_MODIFY_STATE, INFINITE,
};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

pub const WM_WAKE: u32 = 0x8000 + 2; // WM_APP + 2

pub enum Role {
    /// Instance pertama; pegang handle sampai proses berakhir.
    Primary(HANDLE, HANDLE),
    /// Sudah ada instance lain; sudah dibangunkan.
    Secondary,
}

/// Akhiran nama objek: kosong untuk pemakaian normal; untuk uji (`NACRE_CONFIG_DIR`) diturunkan dari folder config
/// supaya instance uji tidak bentrok dengan instance ABRA asli yang sedang dipakai.
fn suffix() -> String {
    match std::env::var("NACRE_CONFIG_DIR") {
        Ok(d) if !d.is_empty() => {
            let mut h = 0xcbf29ce484222325u64;
            for b in d.to_lowercase().bytes() {
                h ^= b as u64;
                h = h.wrapping_mul(0x100000001b3);
            }
            format!("_{h:016x}")
        }
        _ => String::new(),
    }
}

pub fn acquire() -> Role {
    unsafe {
        let sfx = suffix();
        let mutex_name = windows::core::HSTRING::from(format!("Local\\NacreSingleInstance{sfx}"));
        let wake_name = windows::core::HSTRING::from(format!("Local\\NacreWake{sfx}"));
        let mutex = CreateMutexW(None, true, &mutex_name);
        let already = GetLastError() == ERROR_ALREADY_EXISTS;
        if already || mutex.is_err() {
            if let Ok(ev) = OpenEventW(EVENT_MODIFY_STATE, false, &wake_name) {
                let _ = SetEvent(ev);
                let _ = CloseHandle(ev);
            }
            if let Ok(m) = mutex {
                let _ = CloseHandle(m);
            }
            return Role::Secondary;
        }
        let ev = CreateEventW(None, false, false, &wake_name).unwrap_or_default();
        Role::Primary(mutex.unwrap_or_default(), ev)
    }
}

/// Thread kecil yang tidur sampai event dibunyikan; hanya memposting pesan ke window utama.
pub fn listen(event: HANDLE, main_hwnd: HWND) {
    if event.is_invalid() {
        return;
    }
    let (ev, hw) = (event.0 as isize, main_hwnd.0 as isize);
    std::thread::spawn(move || loop {
        let r = unsafe { WaitForSingleObject(HANDLE(ev as *mut _), INFINITE) };
        if r.0 != 0 {
            return;
        }
        unsafe {
            let _ = PostMessageW(Some(HWND(hw as *mut _)), WM_WAKE, WPARAM(0), LPARAM(0));
        }
    });
}
