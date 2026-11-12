//! Sisa fitur kursor bulat yang sudah dihapus: versi ABRA lama bisa meninggalkan penanda `cursors_replaced.flag` bila
//! mati mendadak saat kursor sistem sedang diganti. Modul ini hanya memulihkan skema kursor bawaan pengguna
//! (`SPI_SETCURSORS` memuat ulang dari registri; ABRA tidak pernah mengubah registri) dan menghapus penanda itu.

use std::path::Path;
use windows::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW, SPI_SETCURSORS, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS};

/// Pulihkan kursor bawaan pengguna dan hapus penanda.
pub fn restore(flag: &Path) {
    unsafe {
        let _ = SystemParametersInfoW(SPI_SETCURSORS, 0, None, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
    }
    let _ = std::fs::remove_file(flag);
}

/// Apakah penanda ada (kursor mungkin masih terganti oleh proses lama yang mati)?
pub fn flagged(flag: &Path) -> bool {
    flag.exists()
}

/// Pulihkan hanya bila penanda ada. Mengembalikan true bila ada pemulihan.
pub fn recover_leftover(flag: &Path) -> bool {
    if flagged(flag) {
        restore(flag);
        true
    } else {
        false
    }
}
