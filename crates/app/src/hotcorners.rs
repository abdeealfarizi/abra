//! Hot Corners: kursor menyentuh sudut layar menjalankan aksi (Mission Control, Launchpad, Desktop, Stage Manager,
//! Pusat Notifikasi, Kunci Layar). Strip 2 px di tiap sudut yang aksinya aktif (pola `strip.rs`, tanpa polling,
//! tanpa hook global); aksi berjalan setelah kursor bertahan 250 ms dan bukan sedang menyeret (tombol mouse ditekan).

use crate::prefs;
use crate::strip::Strip;
use nacre_core::settings::HotAction;
use nacre_platform::{keys, launch, log, sys};
use std::cell::RefCell;
use windows::core::w;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_RBUTTON};
use windows::Win32::UI::WindowsAndMessaging::*;

const ENTER_BASE: u32 = WM_APP + 100; // + 2*i
const LEAVE_BASE: u32 = WM_APP + 101; // + 2*i
const T_DWELL: usize = 10; // + i

struct Corners {
    hwnd: HWND,
    strips: [Option<Strip>; 4],
}

thread_local! {
    static HC: RefCell<Option<Corners>> = const { RefCell::new(None) };
}

pub fn init() -> windows_core::Result<()> {
    unsafe {
        let hi: HINSTANCE = GetModuleHandleW(None)?.into();
        let class = w!("AbraHotCorners");
        RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(proc), hInstance: hi, lpszClassName: class, ..Default::default() });
        let hwnd = CreateWindowExW(WINDOW_EX_STYLE(0), class, w!("abra-hc"), WS_POPUP, 0, 0, 0, 0, Some(HWND_MESSAGE), None, Some(hi), None)?;
        HC.with(|c| *c.borrow_mut() = Some(Corners { hwnd, strips: [None, None, None, None] }));
        prefs::listen(hwnd);
    }
    apply();
    Ok(())
}

/// Angkat strip sudut ke atas (menu bar/notch yang dibuat belakangan akan menutupinya).
pub fn raise() {
    apply();
}

/// Sinkronkan strip dengan pengaturan: hanya sudut yang aksinya bukan "Tidak ada" yang punya strip.
fn apply() {
    let hot = prefs::get().hot;
    let (sw, sh) = sys::screen_size();
    HC.with(|c| {
        let mut g = c.borrow_mut();
        let Some(hc) = g.as_mut() else { return };
        for i in 0..4 {
            if hot[i] == HotAction::None {
                hc.strips[i] = None;
                continue;
            }
            if hc.strips[i].is_none() {
                hc.strips[i] = Strip::new(hc.hwnd, ENTER_BASE + 2 * i as u32, LEAVE_BASE + 2 * i as u32).ok();
            }
            if let Some(s) = &hc.strips[i] {
                let (x, y) = (if i % 2 == 0 { 0 } else { sw - 3 }, if i < 2 { 0 } else { sh - 3 });
                s.place(x, y, 3, 3);
                s.set_visible(true);
            }
        }
    });
}

fn run(a: HotAction) {
    log::line("hotcorner", &format!("aksi {}", a.key()));
    match a {
        HotAction::None => {}
        HotAction::Mission => crate::mission::toggle(),
        HotAction::Launchpad => crate::launchpad::toggle(),
        HotAction::Desktop => keys::chord(&[keys::WIN, 0x44]),
        HotAction::Stage => prefs::update(|s| s.stage = !s.stage),
        HotAction::Notifications => keys::chord(&[keys::WIN, 0x4E]),
        HotAction::Lock => launch::run("rundll32.exe", "user32.dll,LockWorkStation"),
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        prefs::WM_PREFS => {
            apply();
            LRESULT(0)
        }
        m if (ENTER_BASE..ENTER_BASE + 8).contains(&m) && (m - ENTER_BASE) % 2 == 0 => {
            let i = ((m - ENTER_BASE) / 2) as usize;
            SetTimer(Some(hwnd), T_DWELL + i, 250, None);
            LRESULT(0)
        }
        m if (LEAVE_BASE..LEAVE_BASE + 8).contains(&m) && (m - LEAVE_BASE) % 2 == 0 => {
            let i = ((m - LEAVE_BASE) / 2) as usize;
            let _ = KillTimer(Some(hwnd), T_DWELL + i);
            LRESULT(0)
        }
        WM_TIMER if (T_DWELL..T_DWELL + 4).contains(&wp.0) => {
            let i = wp.0 - T_DWELL;
            let _ = KillTimer(Some(hwnd), wp.0);
            // sedang menyeret/menekan tombol mouse: abaikan (mis. menyeret jendela ke tepi)
            let down = GetAsyncKeyState(VK_LBUTTON.0 as i32) < 0 || GetAsyncKeyState(VK_RBUTTON.0 as i32) < 0;
            if !down {
                run(prefs::get().hot[i]);
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
