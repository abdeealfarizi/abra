//! Status Recycle Bin (hanya membaca jumlah item; tidak pernah mengubah isinya).

use windows::core::PCWSTR;
use windows::Win32::UI::Shell::{SHQueryRecycleBinW, SHQUERYRBINFO};

/// true bila Recycle Bin di drive mana pun berisi item (±3 ms).
pub fn is_full() -> bool {
    let mut info = SHQUERYRBINFO { cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32, ..Default::default() };
    unsafe { SHQueryRecycleBinW(PCWSTR::null(), &mut info) }.is_ok() && info.i64NumItems > 0
}
