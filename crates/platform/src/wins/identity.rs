//! Identitas app dari window: AppUserModelID (pengenal yang juga dipakai taskbar untuk mengelompokkan).

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PropVariantToStringAlloc};
use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow};

/// `System.AppUserModel.ID` milik window; `None` bila tidak ada. Thread pemanggil perlu COM terinisialisasi.
pub fn aumid(hwnd: isize) -> Option<String> {
    unsafe {
        let store: IPropertyStore = SHGetPropertyStoreForWindow(HWND(hwnd as *mut _)).ok()?;
        let mut pv = store.GetValue(&PKEY_AppUserModel_ID).ok()?;
        let s = PropVariantToStringAlloc(&pv).ok().and_then(|p| {
            let text = p.to_string().ok();
            CoTaskMemFree(Some(p.0 as *const _));
            text
        });
        let _ = PropVariantClear(&mut pv);
        s.filter(|t| !t.is_empty())
    }
}

/// Jalur shell yang bisa dibuka (`ShellExecute`) dan dimuat ikonnya untuk sebuah AUMID.
pub fn shell_path(aumid: &str) -> String {
    format!("shell:AppsFolder\\{aumid}")
}

