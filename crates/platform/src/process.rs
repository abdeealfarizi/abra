use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_INVALID_PARAMETER, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, WaitForSingleObject, INFINITE, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SYNCHRONIZE,
};

/// Hasil menunggu sebuah proses berakhir.
#[derive(Debug, PartialEq, Eq)]
pub enum WaitOutcome {
    /// Proses sudah/baru saja berakhir (termasuk PID yang memang tidak ada lagi).
    Exited,
    /// Tidak bisa dipastikan (mis. akses ditolak). Jangan bertindak seolah proses mati.
    Unknown,
}

/// Menunggu proses `pid` berakhir. Hanya "PID tidak ada" yang dianggap mati;
/// akses ditolak atau gagal tunggu tidak boleh memicu pemulihan atas Bloom yang masih hidup.
pub fn wait_for_exit(pid: u32) -> WaitOutcome {
    unsafe {
        match OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
            Ok(h) => {
                let r = WaitForSingleObject(h, INFINITE);
                let _ = CloseHandle(h);
                if r == WAIT_OBJECT_0 {
                    WaitOutcome::Exited
                } else {
                    WaitOutcome::Unknown
                }
            }
            Err(_) if GetLastError() == ERROR_INVALID_PARAMETER => WaitOutcome::Exited,
            Err(_) => WaitOutcome::Unknown,
        }
    }
}

/// Apakah proses `pid` masih berjalan.
pub fn is_alive(pid: u32) -> bool {
    const STILL_ACTIVE: u32 = 259;
    unsafe {
        match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(h) => {
                let mut code = 0u32;
                let ok = GetExitCodeProcess(h, &mut code).is_ok();
                let _ = CloseHandle(h);
                ok && code == STILL_ACTIVE
            }
            // PID tidak ada → mati. Akses ditolak → anggap hidup (aman: jangan merebut marker).
            Err(_) => GetLastError() != ERROR_INVALID_PARAMETER,
        }
    }
}
