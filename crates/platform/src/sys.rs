//! Kondisi sistem yang memengaruhi biaya tampilan: transparansi, hemat baterai, sesi jarak jauh.

use windows::core::w;
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_REMOTESESSION};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Conditions {
    /// Pengaturan Windows "Transparency effects".
    pub transparency: bool,
    pub battery_saver: bool,
    pub remote_session: bool,
}

pub fn conditions() -> Conditions {
    let mut v = 1u32;
    let mut size = 4u32;
    let transparency = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("EnableTransparency"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut v as *mut u32 as *mut _),
            Some(&mut size),
        )
        .is_err()
            || v != 0
    };
    let mut ps = SYSTEM_POWER_STATUS::default();
    let battery_saver = unsafe { GetSystemPowerStatus(&mut ps).is_ok() && ps.SystemStatusFlag & 1 != 0 };
    let remote_session = unsafe { GetSystemMetrics(SM_REMOTESESSION) != 0 };
    Conditions { transparency, battery_saver, remote_session }
}

/// Persen baterai dan apakah sedang diisi; None bila perangkat tanpa baterai/tidak diketahui.
pub fn battery() -> Option<(u8, bool)> {
    let mut ps = SYSTEM_POWER_STATUS::default();
    unsafe { GetSystemPowerStatus(&mut ps).ok()? };
    if ps.BatteryFlag & 128 != 0 || ps.BatteryLifePercent > 100 {
        return None;
    }
    Some((ps.BatteryLifePercent, ps.BatteryFlag & 8 != 0 || ps.ACLineStatus == 1))
}


/// Ukuran monitor primer dalam piksel fisik. Untuk pengujian, NACRE_SCREEN_FILE menunjuk berkas berisi
/// LEBARxTINGGI yang dibaca setiap kali (mensimulasikan perubahan resolusi tanpa mengubah layar sungguhan).
/// Warna aksen Windows (Pengaturan → Personalisasi → Warna), untuk aksen "Multiwarna".
pub fn accent_color() -> Option<[u8; 3]> {
    let mut v: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    unsafe {
        RegGetValueW(HKEY_CURRENT_USER, w!("Software\\Microsoft\\Windows\\DWM"), w!("AccentColor"), RRF_RT_REG_DWORD, None, Some(&mut v as *mut u32 as *mut _), Some(&mut size)).ok().ok()?;
    }
    // DWORD 0xAABBGGRR
    Some([(v & 0xFF) as u8, ((v >> 8) & 0xFF) as u8, ((v >> 16) & 0xFF) as u8])
}

pub fn screen_size() -> (i32, i32) {
    if let Some(p) = std::env::var_os("NACRE_SCREEN_FILE") {
        if let Ok(text) = std::fs::read_to_string(p) {
            if let Some((w, h)) = text.trim().split_once('x') {
                if let (Ok(w), Ok(h)) = (w.parse(), h.parse()) {
                    return (w, h);
                }
            }
        }
    }
    unsafe { (GetSystemMetrics(windows::Win32::UI::WindowsAndMessaging::SM_CXSCREEN), GetSystemMetrics(windows::Win32::UI::WindowsAndMessaging::SM_CYSCREEN)) }
}


/// Tema aplikasi Windows sedang gelap (Pengaturan → Personalisasi → Warna → mode aplikasi default).
pub fn apps_dark() -> bool {
    let mut v = 1u32;
    let mut size = 4u32;
    let ok = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut v as *mut u32 as *mut _),
            Some(&mut size),
        )
        .is_ok()
    };
    ok && v == 0
}

/// Kecerahan rata-rata (0..1, luma Rec.709) piksel layar pada `points` (px layar); `None` bila tak terbaca.
/// Dipakai menu bar transparan untuk memilih teks putih atau gelap sesuai latar di bawahnya.
pub fn screen_luma(points: &[(i32, i32)]) -> Option<f32> {
    use windows::Win32::Graphics::Gdi::{GetDC, GetPixel, ReleaseDC};
    unsafe {
        let dc = GetDC(None);
        if dc.is_invalid() {
            return None;
        }
        let (mut sum, mut n) = (0.0f32, 0u32);
        for (x, y) in points {
            let c = GetPixel(dc, *x, *y).0;
            if c == 0xFFFF_FFFF {
                continue;
            }
            let (r, g, b) = ((c & 0xFF) as f32, ((c >> 8) & 0xFF) as f32, ((c >> 16) & 0xFF) as f32);
            sum += (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255.0;
            n += 1;
        }
        ReleaseDC(None, dc);
        (n > 0).then(|| sum / n as f32)
    }
}
