//! Strip tepi 2 px: window layered berkas alfa 1 yang tetap menerima hover. Menggantikan hook mouse global untuk
//! mendeteksi kursor menempel di tepi layar (nol polling, nol hook). Satu `Strip` per tepi; pesan masuk/keluar
//! diposting ke window pemilik.

use std::sync::Once;
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{GetStockObject, BLACK_BRUSH, HBRUSH};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows_core::Result;

const WM_MOUSELEAVE: u32 = 0x02A3;
static REGISTER: Once = Once::new();

/// Data per instance, disimpan di GWLP_USERDATA.
struct State {
    target: HWND,
    enter: u32,
    leave: u32,
    inside: bool,
}

unsafe extern "system" fn proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    let st = GetWindowLongPtrW(h, GWLP_USERDATA) as *mut State;
    match m {
        WM_MOUSEMOVE if !st.is_null() => {
            let s = &mut *st;
            if !s.inside {
                s.inside = true;
                let mut t = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: h,
                    dwHoverTime: 0,
                };
                let _ = TrackMouseEvent(&mut t);
                let _ = PostMessageW(Some(s.target), s.enter, WPARAM(0), LPARAM(0));
            }
            LRESULT(0)
        }
        WM_MOUSELEAVE if !st.is_null() => {
            let s = &mut *st;
            s.inside = false;
            let _ = PostMessageW(Some(s.target), s.leave, WPARAM(0), LPARAM(0));
            LRESULT(0)
        }
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_NCDESTROY => {
            if !st.is_null() {
                drop(Box::from_raw(st));
                SetWindowLongPtrW(h, GWLP_USERDATA, 0);
            }
            DefWindowProcW(h, m, w, l)
        }
        _ => DefWindowProcW(h, m, w, l),
    }
}

pub struct Strip {
    hwnd: HWND,
}

impl Drop for Strip {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

impl Strip {
    pub fn new(target: HWND, enter: u32, leave: u32) -> Result<Strip> {
        unsafe {
            let hi: HINSTANCE = GetModuleHandleW(None)?.into();
            let class = w!("NacreEdgeStrip");
            REGISTER.call_once(|| {
                RegisterClassW(&WNDCLASSW {
                    lpfnWndProc: Some(proc),
                    hInstance: hi,
                    lpszClassName: class,
                    hbrBackground: HBRUSH(GetStockObject(BLACK_BRUSH).0),
                    ..Default::default()
                });
            });
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST,
                class,
                w!("nacre-strip"),
                WS_POPUP,
                0, 0, 10, 2,
                None, None, Some(hi), None,
            )?;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(Box::new(State { target, enter, leave, inside: false })) as isize);
            // Alfa 1/255: tak terlihat, tapi tetap ikut hit-test.
            SetLayeredWindowAttributes(hwnd, COLORREF(0), 1, LWA_ALPHA)?;
            Ok(Strip { hwnd })
        }
    }

    pub fn place(&self, x: i32, y: i32, w: i32, h: i32) {
        unsafe {
            let _ = SetWindowPos(self.hwnd, Some(HWND_TOPMOST), x, y, w, h, SWP_NOACTIVATE);
        }
    }

    pub fn set_visible(&self, on: bool) {
        unsafe {
            let _ = ShowWindow(self.hwnd, if on { SW_SHOWNOACTIVATE } else { SW_HIDE });
        }
    }
}
