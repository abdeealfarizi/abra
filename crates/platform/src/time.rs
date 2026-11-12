//! Waktu lokal dan bunyi sistem.
use windows::Win32::System::Diagnostics::Debug::MessageBeep;
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::UI::WindowsAndMessaging::MB_ICONASTERISK;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Local {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

pub fn local_now() -> Local {
    let t = unsafe { GetLocalTime() };
    Local {
        year: t.wYear as i32,
        month: t.wMonth as u32,
        day: t.wDay as u32,
        hour: t.wHour as u32,
        minute: t.wMinute as u32,
        second: t.wSecond as u32,
    }
}

/// Milidetik sampai menit berikutnya (jam hanya bangun sekali per menit).
pub fn ms_to_next_minute() -> u32 {
    let n = local_now();
    ((60 - n.second) * 1000).max(500)
}

/// Bunyi pemberitahuan sistem (dipakai saat timer selesai).
pub fn chime() {
    unsafe {
        let _ = MessageBeep(MB_ICONASTERISK);
    }
}
