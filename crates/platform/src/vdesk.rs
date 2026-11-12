//! Daftar Virtual Desktop Windows dari registri (nama dan desktop aktif). Hanya baca; berpindah memakai Win+Ctrl+panah.

use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_BINARY, RRF_RT_REG_SZ};

const KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\VirtualDesktops");

fn binary(subkey: &str, value: PCWSTR) -> Option<Vec<u8>> {
    unsafe {
        let k = HSTRING::from(subkey);
        let mut size = 0u32;
        if RegGetValueW(HKEY_CURRENT_USER, &k, value, RRF_RT_REG_BINARY, None, None, Some(&mut size)).is_err() || size == 0 {
            return None;
        }
        let mut buf = vec![0u8; size as usize];
        RegGetValueW(HKEY_CURRENT_USER, &k, value, RRF_RT_REG_BINARY, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut size)).ok().ok()?;
        buf.truncate(size as usize);
        Some(buf)
    }
}

fn guid_text(b: &[u8]) -> String {
    // GUID mixed-endian: Data1(4 LE) Data2(2 LE) Data3(2 LE) Data4(8)
    format!(
        "{{{:02X}{:02X}{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        b[3], b[2], b[1], b[0], b[5], b[4], b[7], b[6], b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]
    )
}

fn name_of(guid: &str) -> Option<String> {
    unsafe {
        let sub = HSTRING::from(format!("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\VirtualDesktops\\Desktops\\{guid}"));
        let mut size = 0u32;
        if RegGetValueW(HKEY_CURRENT_USER, &sub, w!("Name"), RRF_RT_REG_SZ, None, None, Some(&mut size)).is_err() || size < 4 {
            return None;
        }
        let mut buf = vec![0u16; (size as usize).div_ceil(2)];
        RegGetValueW(HKEY_CURRENT_USER, &sub, w!("Name"), RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut size)).ok().ok()?;
        let n = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..n])).filter(|s| !s.is_empty())
    }
}

/// (nama tiap desktop, indeks desktop aktif). Selalu minimal satu desktop.
pub fn list() -> (Vec<String>, usize) {
    let ids = binary("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\VirtualDesktops", w!("VirtualDesktopIDs")).unwrap_or_default();
    let cur = binary("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\VirtualDesktops", w!("CurrentVirtualDesktop"));
    let _ = KEY;
    let mut names = Vec::new();
    let mut idx = 0;
    for (i, c) in ids.chunks_exact(16).enumerate() {
        let g = guid_text(c);
        if cur.as_deref().map(|b| b.len() >= 16 && &b[..16] == c).unwrap_or(false) {
            idx = i;
        }
        names.push(name_of(&g).unwrap_or_else(|| format!("Desktop {}", i + 1)));
    }
    if names.is_empty() {
        names.push("Desktop 1".to_string());
    }
    (names, idx)
}
