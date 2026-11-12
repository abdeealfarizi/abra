//! Membuka aplikasi dan menu Start.

use windows::core::{w, HSTRING};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_ESCAPE,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GetForegroundWindow, GetWindowThreadProcessId, PostMessageW, SC_TASKLIST, SW_SHOWNORMAL,
    WM_SYSCOMMAND,
};

/// Jalankan `path` lewat shell (exe, shortcut, URI). Bagian setelah `?` (kunci internal host UWP) dibuang.
pub fn open(path: &str) {
    let path = path.split('?').next().unwrap_or(path);
    unsafe {
        ShellExecuteW(None, w!("open"), &HSTRING::from(path), None, None, SW_SHOWNORMAL);
    }
}

/// Menu Start dihosting `StartMenuExperienceHost` (lama) atau `SearchHost` (Windows 11 24H2+).
pub fn is_start_open() -> bool {
    let mut pid = 0u32;
    unsafe {
        let fg = GetForegroundWindow();
        if fg.0.is_null() {
            return false;
        }
        GetWindowThreadProcessId(fg, Some(&mut pid));
    }
    let Some(path) = crate::wins::enumerate::exe_path_of_pid(pid) else { return false };
    let file = path.rsplit('\\').next().unwrap_or("").to_lowercase();
    file == "startmenuexperiencehost.exe" || file == "searchhost.exe"
}

fn tap(vk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY) {
    let key = |flags| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 } },
    };
    let seq = [key(Default::default()), key(KEYEVENTF_KEYUP)];
    unsafe {
        SendInput(&seq, std::mem::size_of::<INPUT>() as i32);
    }
}

/// Buka/tutup Start. Dibuka dengan meminta Explorer langsung (andal walau ada app elevated di depan);
/// ditutup dengan Escape. `was_open_at_press`: Start menutup dirinya saat tombol *ditekan*, jadi saat klik
/// tiba ia sudah tertutup; dalam kasus itu jangan dibuka lagi.
pub fn toggle_start(was_open_at_press: bool) {
    if is_start_open() {
        tap(VK_ESCAPE);
    } else if !was_open_at_press {
        unsafe {
            if let Ok(tray) = FindWindowW(w!("Shell_TrayWnd"), None) {
                let _ = PostMessageW(Some(tray), WM_SYSCOMMAND, WPARAM(SC_TASKLIST as usize), LPARAM(0));
            }
        }
    }
}

/// Jalankan `file` dengan `params` tanpa jendela konsol (mis. `shutdown.exe /r /t 0`).
pub fn run(file: &str, params: &str) {
    let file = system_tool(file);
    unsafe {
        ShellExecuteW(None, w!("open"), &HSTRING::from(file.as_str()), &HSTRING::from(params), None, windows::Win32::UI::WindowsAndMessaging::SW_HIDE);
    }
}

/// Nama biner polos (mis. `rundll32.exe`) dijadikan jalur absolut di System32 agar tidak bisa dibajak lewat PATH
/// atau folder kerja. Bila berkasnya tidak ada di sana, nama aslinya dipakai.
fn system_tool(file: &str) -> String {
    if file.contains(['\\', '/', ':']) {
        return file.to_string();
    }
    if let Some(root) = std::env::var_os("SystemRoot") {
        let p = std::path::Path::new(&root).join("System32").join(file);
        if p.is_file() {
            return p.to_string_lossy().to_string();
        }
    }
    file.to_string()
}

#[cfg(test)]
mod tests {
    use super::system_tool;

    #[test]
    fn bare_tools_resolve_to_system32() {
        let p = system_tool("rundll32.exe");
        assert!(p.to_lowercase().ends_with("system32\\rundll32.exe"), "{p}");
        assert_eq!(system_tool("C:\\x\\y.exe"), "C:\\x\\y.exe");
        assert_eq!(system_tool("tidak-ada-xyz.exe"), "tidak-ada-xyz.exe");
    }
}
