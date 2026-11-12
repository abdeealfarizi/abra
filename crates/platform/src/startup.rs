//! Jalankan otomatis saat login: nilai `ABRA` di `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
//! Cara yang dianjurkan untuk app per-pengguna: tanpa hak admin, tanpa Task Scheduler, dan terlihat/bisa
//! dimatikan pengguna dari Settings → Apps → Startup. Jalur exe dikutip agar aman dengan spasi.

use std::path::Path;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
use windows::Win32::System::Registry::{RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ};

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const NAME: PCWSTR = w!("ABRA");

/// Isi nilai Run saat ini (jalur terkutip), atau `None` bila tidak ada.
pub fn current() -> Option<String> {
    unsafe {
        let mut size = 0u32;
        if RegGetValueW(HKEY_CURRENT_USER, RUN_KEY, NAME, RRF_RT_REG_SZ, None, None, Some(&mut size)).is_err() || size == 0 {
            return None;
        }
        let mut buf = vec![0u16; (size as usize).div_ceil(2)];
        if RegGetValueW(HKEY_CURRENT_USER, RUN_KEY, NAME, RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut size)).is_err() {
            return None;
        }
        let n = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..n]))
    }
}

pub fn is_enabled() -> bool {
    current().is_some()
}

/// Nilai yang akan ditulis untuk `exe`.
pub fn command_for(exe: &Path) -> String {
    format!("\"{}\"", exe.display())
}

/// Nyalakan (tulis jalur exe saat ini) atau matikan (hapus nilai). Idempoten.
pub fn set(enabled: bool, exe: &Path) -> Result<(), String> {
    unsafe {
        if enabled {
            let cmd = command_for(exe);
            let wide: Vec<u16> = cmd.encode_utf16().chain(std::iter::once(0)).collect();
            let err = RegSetKeyValueW(HKEY_CURRENT_USER, RUN_KEY, NAME, REG_SZ.0, Some(wide.as_ptr() as *const _), (wide.len() * 2) as u32);
            if err.is_err() {
                return Err(format!("RegSetKeyValue: {err:?}"));
            }
        } else {
            let err = RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, NAME);
            if err.is_err() && err != ERROR_FILE_NOT_FOUND {
                return Err(format!("RegDeleteKeyValue: {err:?}"));
            }
        }
    }
    Ok(())
}
