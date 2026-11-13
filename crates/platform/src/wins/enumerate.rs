//! Daftar window yang pantas muncul di dock (SPEC Â§4.5, versi M2a).
//!
//! Belum ditangani: PWA browser (AUMID web-app), relaunch-command untuk app yang UI-nya di proses pembantu, prompt UAC `consent.exe`.

use nacre_core::dock::items::Win;
use std::ffi::c_void;
use windows::core::BOOL;
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetWindow, GetWindowLongW, GetWindowTextW, GetWindowThreadProcessId,
    IsWindowVisible, GWL_EXSTYLE, GW_OWNER, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
};

/// Proses sistem/latar yang punya window tetapi bukan app bagi pengguna.
const HIDDEN_EXES: &[&str] = &[
    "conhost.exe",
    "shellexperiencehost.exe",
    "searchhost.exe",
    "textinputhost.exe",
    "startmenuexperiencehost.exe",
    "systemsettingsbroker.exe",
];

fn text_of(hwnd: HWND) -> String {
    let mut buf = [0u16; 512];
    let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn class_of(hwnd: HWND) -> String {
    let mut buf = [0u16; 128];
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

pub fn exe_path_of_pid(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, windows::core::PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(h);
        ok.ok()?;
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}

/// Jalur exe proses yang punya jendela tingkat atas dan bernama berkas `name` (mis. "Spotify.exe"); dipakai bila sesi
/// media hanya memberi nama exe sebagai id app.
pub fn exe_path_of_running(name: &str) -> Option<String> {
    struct Find {
        name: String,
        found: Option<String>,
        seen: Vec<u32>,
    }
    unsafe extern "system" fn cb(hwnd: HWND, lp: LPARAM) -> BOOL {
        let f = &mut *(lp.0 as *mut Find);
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 || f.seen.contains(&pid) {
            return BOOL(1);
        }
        f.seen.push(pid);
        if let Some(path) = exe_path_of_pid(pid) {
            let file = path.rsplit('\\').next().unwrap_or("").to_ascii_lowercase();
            if file == f.name {
                f.found = Some(path);
                return BOOL(0);
            }
        }
        BOOL(1)
    }
    let mut f = Find { name: name.to_ascii_lowercase(), found: None, seen: Vec::new() };
    unsafe {
        let _ = EnumWindows(Some(cb), LPARAM(&mut f as *mut Find as isize));
    }
    f.found
}

fn is_cloaked(hwnd: HWND) -> bool {
    let mut cloaked = 0u32;
    unsafe {
        DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, &mut cloaked as *mut u32 as *mut c_void, 4).is_ok() && cloaked != 0
    }
}

struct Ctx {
    own_pid: u32,
    out: Vec<Win>,
}

unsafe extern "system" fn each(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let ctx = &mut *(lparam.0 as *mut Ctx);
    if !IsWindowVisible(hwnd).as_bool() || is_cloaked(hwnd) {
        return BOOL(1);
    }
    let title = text_of(hwnd);
    if title.is_empty() || title == "Program Manager" {
        return BOOL(1);
    }
    let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
    let app_window = ex & WS_EX_APPWINDOW.0 != 0;
    if ex & WS_EX_TOOLWINDOW.0 != 0 && !app_window {
        return BOOL(1);
    }
    // Dialog milik window lain tampil di bawah window induknya, seperti di taskbar.
    if !app_window {
        if let Ok(owner) = GetWindow(hwnd, GW_OWNER) {
            if !owner.0.is_null() && IsWindowVisible(owner).as_bool() {
                return BOOL(1);
            }
        }
    }
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid == 0 || pid == ctx.own_pid {
        return BOOL(1);
    }
    let Some(path) = exe_path_of_pid(pid) else { return BOOL(1) };
    let lower = path.to_lowercase();
    let file = lower.rsplit('\\').next().unwrap_or("");
    if HIDDEN_EXES.contains(&file) {
        return BOOL(1);
    }
    let class = class_of(hwnd);
    if file == "explorer.exe" && class != "CabinetWClass" && class != "ExploreWClass" {
        return BOOL(1);
    }
    let mut path = path;
    if file == "applicationframehost.exe" {
        if class != "ApplicationFrameWindow" {
            return BOOL(1);
        }
        // Tiap app UWP punya frame sendiri. AUMID memberi identitas, jalur peluncuran, dan ikon yang benar;
        // tanpa AUMID, pisahkan kuncinya dengan judul (ikon host generik).
        path = match super::identity::aumid(hwnd.0 as isize) {
            Some(id) => super::identity::shell_path(&id),
            None => format!("{path}?{title}"),
        };
    }
    let name = super::names::friendly(&path);
    ctx.out.push(Win { hwnd: hwnd.0 as isize, path, title, name });
    BOOL(1)
}

pub fn list(own_pid: u32) -> Vec<Win> {
    let mut ctx = Ctx { own_pid, out: Vec::new() };
    unsafe {
        let _ = EnumWindows(Some(each), LPARAM(&mut ctx as *mut Ctx as isize));
    }
    ctx.out
}


