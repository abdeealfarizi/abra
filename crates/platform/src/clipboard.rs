//! Riwayat papan klip (teks saja) untuk mode Papan Klip di Spotlight. Hanya di memori: tidak pernah ditulis ke disk atau
//! dikirim ke mana pun, dibatasi 8 jam / 100 butir, dan bisa dimatikan. Isi yang ditandai pengelola kata sandi
//! ("ExcludeClipboardContentFromMonitorProcessing" / "CanIncludeInClipboardHistory"=0) diabaikan.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::DataExchange::{
    AddClipboardFormatListener, CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW,
    RemoveClipboardFormatListener, SetClipboardData,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::UI::WindowsAndMessaging::*;

const CF_UNICODETEXT: u32 = 13;
const KEEP_SECS: i64 = 8 * 3600;
const MAX_ITEMS: usize = 100;
const MAX_CHARS: usize = 4000;

#[derive(Clone, Debug)]
pub struct Entry {
    /// Detik sejak epoch Unix.
    pub at: i64,
    pub text: String,
}

static HISTORY: Mutex<Vec<Entry>> = Mutex::new(Vec::new());
static ENABLED: AtomicBool = AtomicBool::new(true);
static HWND_LISTENER: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

fn now_secs() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Tambahkan teks ke riwayat (di luar jendela, agar bisa diuji): buang yang kosong/terlalu panjang, gabungkan duplikat,
/// pangkas yang lebih tua dari 8 jam dan melebihi batas jumlah.
pub fn push(history: &mut Vec<Entry>, text: &str, now: i64) {
    let t = text.trim_end_matches(['\r', '\n', '\0']);
    if t.trim().is_empty() {
        return;
    }
    let t: String = t.chars().take(MAX_CHARS).collect();
    history.retain(|e| e.text != t && now - e.at <= KEEP_SECS);
    history.insert(0, Entry { at: now, text: t });
    history.truncate(MAX_ITEMS);
}

pub fn history() -> Vec<Entry> {
    let now = now_secs();
    let mut g = HISTORY.lock().unwrap();
    g.retain(|e| now - e.at <= KEEP_SECS);
    g.clone()
}

pub fn clear() {
    HISTORY.lock().unwrap().clear();
}

pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
    if !on {
        clear();
    }
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Isi tidak boleh masuk riwayat: ada format "Exclude…", atau "CanIncludeInClipboardHistory" bukan DWORD bukan-nol
/// (nilai 0 = jangan simpan; data rusak/tak terbaca juga dianggap jangan, demi keamanan).
fn opted_out(exclude: bool, can_include: Option<&[u8]>) -> bool {
    exclude || can_include.is_some_and(|b| b.len() < 4 || b[..4] == [0; 4])
}

unsafe fn read_text(hwnd: HWND) -> Option<String> {
    for _ in 0..6 {
        if OpenClipboard(Some(hwnd)).is_ok() {
            let r = (|| {
                // pengelola kata sandi menandai isi sensitif: jangan simpan
                let has = |name| {
                    let f = RegisterClipboardFormatW(name);
                    (f != 0 && IsClipboardFormatAvailable(f).is_ok()).then_some(f)
                };
                let can = has(w!("CanIncludeInClipboardHistory")).map(|f| {
                    let Ok(h) = GetClipboardData(f) else { return Vec::new() };
                    let g = windows::Win32::Foundation::HGLOBAL(h.0);
                    let p = GlobalLock(g) as *const u8;
                    if p.is_null() {
                        return Vec::new();
                    }
                    let v = std::slice::from_raw_parts(p, GlobalSize(g).min(4)).to_vec();
                    let _ = GlobalUnlock(g);
                    v
                });
                if opted_out(has(w!("ExcludeClipboardContentFromMonitorProcessing")).is_some(), can.as_deref()) {
                    return None;
                }
                if IsClipboardFormatAvailable(CF_UNICODETEXT).is_err() {
                    return None;
                }
                let h = GetClipboardData(CF_UNICODETEXT).ok()?;
                let p = GlobalLock(windows::Win32::Foundation::HGLOBAL(h.0)) as *const u16;
                if p.is_null() {
                    return None;
                }
                let mut n = 0usize;
                while *p.add(n) != 0 && n < 200_000 {
                    n += 1;
                }
                let s = String::from_utf16_lossy(std::slice::from_raw_parts(p, n));
                let _ = GlobalUnlock(windows::Win32::Foundation::HGLOBAL(h.0));
                Some(s)
            })();
            let _ = CloseClipboard();
            return r;
        }
        std::thread::sleep(std::time::Duration::from_millis(12));
    }
    None
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_CLIPBOARDUPDATE {
        if ENABLED.load(Ordering::Relaxed) {
            if let Some(t) = read_text(hwnd) {
                push(&mut HISTORY.lock().unwrap(), &t, now_secs());
            }
        }
        return LRESULT(0);
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

/// Pasang pendengar papan klip (panggil sekali dari thread UI).
pub fn init() -> windows_core::Result<()> {
    unsafe {
        let hi = GetModuleHandleW(None)?.into();
        let class = w!("AbraClipboard");
        RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(proc), hInstance: hi, lpszClassName: class, ..Default::default() });
        let hwnd = CreateWindowExW(WINDOW_EX_STYLE(0), class, w!("abra-clip"), WS_POPUP, 0, 0, 0, 0, Some(HWND_MESSAGE), None, Some(hi), None)?;
        AddClipboardFormatListener(hwnd)?;
        HWND_LISTENER.store(hwnd.0 as isize, Ordering::Relaxed);
    }
    Ok(())
}

pub fn shutdown() {
    let h = HWND_LISTENER.swap(0, Ordering::Relaxed);
    if h != 0 {
        unsafe {
            let _ = RemoveClipboardFormatListener(HWND(h as *mut _));
        }
    }
}

/// Salin `text` kembali ke papan klip sistem (agar bisa ditempel).
pub fn copy_text(text: &str) -> bool {
    unsafe {
        let hwnd = HWND(HWND_LISTENER.load(Ordering::Relaxed) as *mut _);
        for _ in 0..6 {
            if OpenClipboard(Some(hwnd)).is_ok() {
                let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
                let ok = (|| {
                    let _ = EmptyClipboard();
                    let h = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2).ok()?;
                    let p = GlobalLock(h) as *mut u16;
                    if p.is_null() {
                        return None;
                    }
                    std::ptr::copy_nonoverlapping(wide.as_ptr(), p, wide.len());
                    let _ = GlobalUnlock(h);
                    SetClipboardData(CF_UNICODETEXT, Some(windows::Win32::Foundation::HANDLE(h.0))).ok()?;
                    Some(())
                })()
                .is_some();
                let _ = CloseClipboard();
                return ok;
            }
            std::thread::sleep(std::time::Duration::from_millis(12));
        }
        false
    }
}

#[allow(dead_code)]
fn _unused(_: PCWSTR) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn penanda_sensitif_diabaikan() {
        assert!(!opted_out(false, None), "tanpa penanda: simpan");
        assert!(opted_out(true, None));
        assert!(opted_out(false, Some(&0u32.to_le_bytes())), "CanIncludeInClipboardHistory=0");
        assert!(!opted_out(false, Some(&1u32.to_le_bytes())), "CanIncludeInClipboardHistory=1");
        assert!(opted_out(false, Some(&[1, 0])), "data rusak dianggap jangan");
        assert!(opted_out(true, Some(&1u32.to_le_bytes())), "Exclude menang");
    }

    unsafe fn put(text: &str, can_include: Option<u32>) {
        assert!(OpenClipboard(None).is_ok());
        let _ = EmptyClipboard();
        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let h = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2).unwrap();
        std::ptr::copy_nonoverlapping(wide.as_ptr(), GlobalLock(h) as *mut u16, wide.len());
        let _ = GlobalUnlock(h);
        SetClipboardData(CF_UNICODETEXT, Some(windows::Win32::Foundation::HANDLE(h.0))).unwrap();
        if let Some(v) = can_include {
            let h = GlobalAlloc(GMEM_MOVEABLE, 4).unwrap();
            std::ptr::copy_nonoverlapping(v.to_le_bytes().as_ptr(), GlobalLock(h) as *mut u8, 4);
            let _ = GlobalUnlock(h);
            SetClipboardData(RegisterClipboardFormatW(w!("CanIncludeInClipboardHistory")), Some(windows::Win32::Foundation::HANDLE(h.0))).unwrap();
        }
        let _ = CloseClipboard();
    }

    #[test]
    #[ignore = "menimpa papan klip sistem; jalankan manual: cargo test -p nacre-platform clipboard -- --ignored"]
    fn papan_klip_asli_menghormati_can_include() {
        unsafe {
            let lama = read_text(HWND::default());
            put("rahasia-uji", Some(0));
            assert_eq!(read_text(HWND::default()), None);
            put("boleh-uji", Some(1));
            assert_eq!(read_text(HWND::default()).as_deref(), Some("boleh-uji"));
            put(lama.as_deref().unwrap_or(""), None);
        }
    }

    #[test]
    fn push_dedups_trims_old_and_caps() {
        let mut h = Vec::new();
        push(&mut h, "a", 1000);
        push(&mut h, "b\r\n", 1001);
        push(&mut h, "a", 1002); // duplikat naik ke atas
        assert_eq!(h.iter().map(|e| e.text.as_str()).collect::<Vec<_>>(), vec!["a", "b"]);
        push(&mut h, "   ", 1003);
        assert_eq!(h.len(), 2, "kosong diabaikan");
        push(&mut h, "baru", 1000 + KEEP_SECS + 10);
        assert_eq!(h.len(), 1, "yang lebih tua dari 8 jam dibuang");
        for i in 0..150 {
            push(&mut h, &format!("x{i}"), 5_000_000 + i);
        }
        assert_eq!(h.len(), MAX_ITEMS);
        let long = "z".repeat(10_000);
        push(&mut h, &long, 6_000_000);
        assert_eq!(h[0].text.chars().count(), MAX_CHARS);
    }
}
