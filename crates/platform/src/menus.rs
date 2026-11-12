//! Membaca menu klasik (HMENU) milik jendela app lain agar bisa ditampilkan di menu bar ABRA.
//! Hanya app Win32 klasik yang punya HMENU; app modern (Chrome, Electron, UWP) mengembalikan `None`.

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::*;

#[derive(Clone, Debug)]
pub struct Entry {
    /// Id perintah (WM_COMMAND). 0 untuk pemisah.
    pub id: u32,
    pub label: String,
    pub accel: String,
    pub enabled: bool,
    pub checked: bool,
    pub separator: bool,
    /// Tingkat bersarang (0 = langsung di bawah judul); submenu diratakan dengan penjorokan.
    pub depth: u8,
    /// Judul submenu (tidak bisa dipilih).
    pub header: bool,
}

#[derive(Clone, Debug)]
pub struct Top {
    pub title: String,
    pub entries: Vec<Entry>,
}

fn clean(s: &str) -> String {
    // buang mnemonic '&' (tetapi "&&" tetap satu '&')
    let mut out = String::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '&' {
            if it.peek() == Some(&'&') {
                out.push('&');
                it.next();
            }
        } else {
            out.push(c);
        }
    }
    out
}

unsafe fn item_text(m: HMENU, pos: u32) -> String {
    let mut buf = [0u16; 256];
    let n = GetMenuStringW(m, pos, Some(&mut buf), MF_BYPOSITION);
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

unsafe fn collect(m: HMENU, depth: u8, out: &mut Vec<Entry>) {
    let n = GetMenuItemCount(Some(m));
    if n <= 0 || depth > 2 {
        return;
    }
    for i in 0..n as u32 {
        let mut info = MENUITEMINFOW {
            cbSize: std::mem::size_of::<MENUITEMINFOW>() as u32,
            fMask: MIIM_ID | MIIM_FTYPE | MIIM_STATE | MIIM_SUBMENU,
            ..Default::default()
        };
        if GetMenuItemInfoW(m, i, true, &mut info).is_err() {
            continue;
        }
        if info.fType.0 & MFT_SEPARATOR.0 != 0 {
            if out.last().map(|e| !e.separator).unwrap_or(false) {
                out.push(Entry { id: 0, label: String::new(), accel: String::new(), enabled: false, checked: false, separator: true, depth, header: false });
            }
            continue;
        }
        if info.fType.0 & MFT_OWNERDRAW.0 != 0 {
            continue;
        }
        let text = item_text(m, i);
        let (label, accel) = match text.split_once('\t') {
            Some((l, a)) => (clean(l), a.to_string()),
            None => (clean(&text), String::new()),
        };
        if label.is_empty() {
            continue;
        }
        let enabled = info.fState.0 & (MFS_DISABLED.0 | MFS_GRAYED.0) == 0;
        let checked = info.fState.0 & MFS_CHECKED.0 != 0;
        if !info.hSubMenu.0.is_null() {
            out.push(Entry { id: 0, label, accel: String::new(), enabled: false, checked: false, separator: false, depth, header: true });
            collect(info.hSubMenu, depth + 1, out);
        } else {
            out.push(Entry { id: info.wID, label, accel, enabled, checked, separator: false, depth, header: false });
        }
    }
    while out.last().map(|e| e.separator).unwrap_or(false) {
        out.pop();
    }
}

/// Menu klasik jendela `hwnd`, atau `None` bila tidak punya.
pub fn read(hwnd: isize) -> Option<Vec<Top>> {
    unsafe {
        let h = HWND(hwnd as *mut _);
        let bar = GetMenu(h);
        if bar.0.is_null() {
            return None;
        }
        let n = GetMenuItemCount(Some(bar));
        if n <= 0 {
            return None;
        }
        let mut tops = Vec::new();
        for i in 0..n as u32 {
            let sub = GetSubMenu(bar, i as i32);
            if sub.0.is_null() {
                continue;
            }
            let title = clean(&item_text(bar, i));
            if title.is_empty() {
                continue;
            }
            let mut entries = Vec::new();
            collect(sub, 0, &mut entries);
            if !entries.is_empty() {
                tops.push(Top { title, entries });
            }
        }
        if tops.is_empty() { None } else { Some(tops) }
    }
}

/// Jalankan perintah menu pada jendela pemilik.
pub fn invoke(hwnd: isize, id: u32) {
    unsafe {
        let _ = PostMessageW(Some(HWND(hwnd as *mut _)), WM_COMMAND, WPARAM(id as usize & 0xFFFF), LPARAM(0));
    }
}
