use std::collections::HashMap;
use std::sync::Mutex;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};

static CACHE: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

fn read(path: &str) -> String {
    unsafe {
        let wide = HSTRING::from(path);
        let size = GetFileVersionInfoSizeW(&wide, None);
        if size == 0 {
            return String::new();
        }
        let mut buf = vec![0u8; size as usize];
        if GetFileVersionInfoW(&wide, None, size, buf.as_mut_ptr() as *mut _).is_err() {
            return String::new();
        }
        let mut ptr: *mut std::ffi::c_void = std::ptr::null_mut();
        let mut len = 0u32;
        let lang = if VerQueryValueW(buf.as_ptr() as *const _, &HSTRING::from("\\VarFileInfo\\Translation"), &mut ptr, &mut len).as_bool()
            && len >= 4
        {
            let t = std::slice::from_raw_parts(ptr as *const u16, 2);
            format!("{:04x}{:04x}", t[0], t[1])
        } else {
            "040904b0".to_string()
        };
        let key = HSTRING::from(format!("\\StringFileInfo\\{lang}\\FileDescription"));
        if VerQueryValueW(buf.as_ptr() as *const _, &key, &mut ptr, &mut len).as_bool() && len > 1 {
            let s = std::slice::from_raw_parts(ptr as *const u16, len as usize);
            return PCWSTR(s.as_ptr()).to_string().unwrap_or_default().trim().to_string();
        }
        String::new()
    }
}

const HOSTS: &[&str] = &["svchost.exe", "dllhost.exe", "rundll32.exe", "taskhostw.exe", "sihost.exe", "runtimebroker.exe", "applicationframehost.exe", "consent.exe", "credentialuibroker.exe"];

pub fn is_host(path: &str) -> bool {
    let lower = path.to_lowercase();
    let file = lower.split('?').next().unwrap_or("").rsplit(['\\', '/']).next().unwrap_or("").to_string();
    HOSTS.contains(&file.as_str())
}

fn shell_name(path: &str) -> String {
    use windows::Win32::UI::Shell::{IShellItem, SHCreateItemFromParsingName, SIGDN_NORMALDISPLAY};
    unsafe {
        let Ok(item) = SHCreateItemFromParsingName::<_, _, IShellItem>(&HSTRING::from(path), None) else { return String::new() };
        let Ok(p) = item.GetDisplayName(SIGDN_NORMALDISPLAY) else { return String::new() };
        let out = p.to_string().unwrap_or_default();
        windows::Win32::System::Com::CoTaskMemFree(Some(p.0 as *const _));
        out.trim().to_string()
    }
}

pub fn friendly(path: &str) -> String {
    if let Some(w) = super::identity::parse_webapp(path) {
        return w.name;
    }
    if let Some((_, title)) = path.split_once('?') {
        return title.trim().to_string();
    }
    if path.to_lowercase().starts_with("shell:appsfolder") {
        let mut g = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(v) = g.get_or_insert_with(HashMap::new).get(path) {
            return v.clone();
        }
        drop(g);
        let v = shell_name(path);
        CACHE.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(path.to_string(), v.clone());
        return v;
    }
    if path.to_lowercase().starts_with("shell:") {
        return String::new();
    }
    let mut g = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let map = g.get_or_insert_with(HashMap::new);
    if let Some(v) = map.get(path) {
        return v.clone();
    }
    drop(g);
    let v = read(path);
    CACHE.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(path.to_string(), v.clone());
    v
}
