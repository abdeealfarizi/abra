use windows::core::{s, w, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND};
use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PropVariantToStringAlloc};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow};

pub struct WebApp {
    pub id: String,
    pub cmd: String,
    pub icon: String,
    pub name: String,
}

fn read_prop(store: &IPropertyStore, pid: u32) -> Option<String> {
    let mut key = PKEY_AppUserModel_ID;
    key.pid = pid;
    unsafe {
        let mut pv = store.GetValue(&key).ok()?;
        let s = PropVariantToStringAlloc(&pv).ok().and_then(|p| {
            let text = p.to_string().ok();
            CoTaskMemFree(Some(p.0 as *const _));
            text
        });
        let _ = PropVariantClear(&mut pv);
        s.map(|t| t.trim().to_string()).filter(|t| !t.is_empty())
    }
}

pub fn aumid(hwnd: isize) -> Option<String> {
    unsafe {
        let store: IPropertyStore = SHGetPropertyStoreForWindow(HWND(hwnd as *mut _)).ok()?;
        read_prop(&store, PKEY_AppUserModel_ID.pid)
    }
}

pub fn process_aumid(pid: u32) -> Option<String> {
    type FnGetAumid = unsafe extern "system" fn(windows::Win32::Foundation::HANDLE, *mut u32, PWSTR) -> i32;
    unsafe {
        let k32 = GetModuleHandleW(w!("kernel32.dll")).ok()?;
        let proc = GetProcAddress(k32, s!("GetApplicationUserModelId"))?;
        let get_aumid: FnGetAumid = std::mem::transmute(proc);
        let hp = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut len = 0u32;
        let rc = get_aumid(hp, &mut len, PWSTR::null());
        if rc != 122 || len == 0 {
            let _ = CloseHandle(hp);
            return None;
        }
        let mut buf = vec![0u16; len as usize];
        let ok = get_aumid(hp, &mut len, PWSTR(buf.as_mut_ptr())) == 0;
        let _ = CloseHandle(hp);
        if !ok {
            return None;
        }
        PCWSTR(buf.as_ptr()).to_string().ok().filter(|s| !s.is_empty())
    }
}

pub fn webapp_info(hwnd: isize) -> Option<WebApp> {
    unsafe {
        let store: IPropertyStore = SHGetPropertyStoreForWindow(HWND(hwnd as *mut _)).ok()?;
        let id = read_prop(&store, PKEY_AppUserModel_ID.pid)?;
        let cmd = read_prop(&store, 2).unwrap_or_default();
        let icon = read_prop(&store, 3).unwrap_or_default();
        let name = read_prop(&store, 4).unwrap_or_default();
        if cmd.is_empty() && icon.is_empty() && !id.to_ascii_lowercase().contains("taskbar") {
            return None;
        }
        Some(WebApp { id, cmd, icon, name })
    }
}

pub fn shell_path(aumid: &str) -> String {
    format!("shell:AppsFolder\\{aumid}")
}

pub fn webapp_path(w: &WebApp, fallback_cmd: &str, fallback_title: &str) -> String {
    let clean = |s: &str| s.replace(['|', '\t', '\r', '\n'], " ").trim().to_string();
    let id = clean(&w.id);
    let cmd = clean(if w.cmd.is_empty() { fallback_cmd } else { &w.cmd });
    let icon = clean(&w.icon);
    let raw_name = if w.name.is_empty() { fallback_title } else { &w.name };
    let name = clean(raw_name.split(" — ").next().unwrap_or(raw_name));
    format!("webapp:{id}|{cmd}|{icon}|{name}")
}

pub fn parse_webapp(path: &str) -> Option<WebApp> {
    let rest = path.strip_prefix("webapp:")?;
    let mut parts = rest.splitn(4, '|');
    let id = parts.next()?.to_string();
    let cmd = parts.next().unwrap_or("").to_string();
    let icon = parts.next().unwrap_or("").to_string();
    let name = parts.next().unwrap_or("").to_string();
    Some(WebApp { id, cmd, icon, name })
}
