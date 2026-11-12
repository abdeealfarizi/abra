//! Dialog pilih berkas (IFileOpenDialog) dan penyelesaian pintasan `.lnk` ke target aslinya.
//! Dipanggil dari thread UI (STA). Dialog bersifat modal dan memakai loop pesannya sendiri.

use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CoCreateInstance, IPersistFile, CLSCTX_INPROC_SERVER, STGM_READ};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    FileOpenDialog, IFileOpenDialog, IShellLinkW, ShellLink, FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST, SIGDN_FILESYSPATH,
};

/// Tampilkan dialog buka berkas. `filters`: pasangan (nama tampil, pola seperti "*.exe;*.lnk").
pub fn open_file(title: &str, filters: &[(&str, &str)]) -> Option<String> {
    unsafe {
        let dlg: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        let opts = dlg.GetOptions().ok()?;
        dlg.SetOptions(opts | FOS_FILEMUSTEXIST | FOS_PATHMUSTEXIST | FOS_FORCEFILESYSTEM).ok()?;
        let _ = dlg.SetTitle(&HSTRING::from(title));
        // HSTRING hidup sampai dialog selesai: PCWSTR di spec menunjuk ke sini.
        let owned: Vec<(HSTRING, HSTRING)> = filters.iter().map(|(n, p)| (HSTRING::from(*n), HSTRING::from(*p))).collect();
        let specs: Vec<COMDLG_FILTERSPEC> =
            owned.iter().map(|(n, p)| COMDLG_FILTERSPEC { pszName: PCWSTR(n.as_ptr()), pszSpec: PCWSTR(p.as_ptr()) }).collect();
        if !specs.is_empty() {
            let _ = dlg.SetFileTypes(&specs);
        }
        dlg.Show(Some(HWND::default())).ok()?;
        let item = dlg.GetResult().ok()?;
        let name = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let s = name.to_string().ok();
        windows::Win32::System::Com::CoTaskMemFree(Some(name.0 as *const _));
        s
    }
}

/// Selesaikan `.lnk` ke jalur target; selain `.lnk` dikembalikan apa adanya.
pub fn resolve_shortcut(path: &str) -> Option<String> {
    if !path.to_ascii_lowercase().ends_with(".lnk") {
        return Some(path.to_string());
    }
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        let file: IPersistFile = windows::core::Interface::cast(&link).ok()?;
        file.Load(&HSTRING::from(path), STGM_READ).ok()?;
        let mut buf = [0u16; 520];
        link.GetPath(&mut buf, std::ptr::null_mut(), 0).ok()?;
        let n = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
        let target = String::from_utf16_lossy(&buf[..n]);
        let _ = w!("");
        if !target.is_empty() {
            return Some(target);
        }
        // Aplikasi Microsoft Store (WhatsApp, Calculator, ...): pintasannya tidak punya jalur berkas, targetnya
        // identitas aplikasi (AUMID) di dalam ID-list. Ubah ke `shell:AppsFolder\<AUMID>` seperti yang dipakai dock.
        let pidl = link.GetIDList().ok()?;
        let name = windows::Win32::UI::Shell::SHGetNameFromIDList(pidl, windows::Win32::UI::Shell::SIGDN_DESKTOPABSOLUTEPARSING).ok()?;
        let parsed = name.to_string().ok();
        windows::Win32::System::Com::CoTaskMemFree(Some(name.0 as *const _));
        windows::Win32::System::Com::CoTaskMemFree(Some(pidl as *const _));
        let parsed = parsed?;
        let lower = parsed.to_ascii_lowercase();
        if lower.starts_with("shell:appsfolder") {
            Some(parsed)
        } else if let Some(pos) = lower.rfind("appsfolder\\") {
            Some(format!("shell:AppsFolder\\{}", &parsed[pos + "appsfolder\\".len()..]))
        } else if parsed.contains('!') && !parsed.contains('\\') {
            Some(format!("shell:AppsFolder\\{parsed}"))
        } else {
            None
        }
    }
}
