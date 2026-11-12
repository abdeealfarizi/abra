//! Nama ramah aplikasi dari metadata exe (FileDescription), di-cache per path.

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
        // bahasa/codepage pertama dari tabel terjemahan
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

/// FileDescription untuk `path` (kosong bila tidak ada). Jalur `shell:`/berkunci `?` dilewati.
pub fn friendly(path: &str) -> String {
    if path.contains('?') || path.to_lowercase().starts_with("shell:") {
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
