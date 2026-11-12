//! Mengirim kombinasi tombol ke jendela yang sedang aktif (SendInput). Dipakai menu bar untuk meneruskan
//! perintah standar (Salin, Simpan, ...) ke aplikasi: jendela ABRA bersifat NOACTIVATE sehingga fokus tetap di app.

use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, VIRTUAL_KEY,
};

pub const CTRL: u16 = 0x11;
pub const SHIFT: u16 = 0x10;
pub const ALT: u16 = 0x12;
pub const WIN: u16 = 0x5B;

fn key(vk: u16, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// Tekan semua `vks` berurutan lalu lepas berkebalikan (mis. `[CTRL, 0x53]` = Ctrl+S).
pub fn chord(vks: &[u16]) {
    let mut seq: Vec<INPUT> = vks.iter().map(|v| key(*v, false)).collect();
    seq.extend(vks.iter().rev().map(|v| key(*v, true)));
    unsafe {
        SendInput(&seq, std::mem::size_of::<INPUT>() as i32);
    }
}
