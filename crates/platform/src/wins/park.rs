//! Parkir jendela: memindahkan jendela ke luar layar (bukan diminimalkan) agar isinya tetap tertangkap hidup. Hanya sudut
//! kanan-bawah jendela seluas `SLIVER` px yang tersisa di pojok kiri-atas layar (tersembunyi di bawah menu bar): browser
//! berbasis Chromium menganggap jendela yang sepenuhnya di luar layar sebagai "tertutup" lalu berhenti menggambar
//! (video berhenti, kartu Stage Manager hitam); sepotong kecil yang masih terlihat menjaganya tetap berjalan.
//! Daftar jendela terparkir disimpan ke berkas supaya bisa dipulihkan setelah ABRA mati mendadak.

use std::path::Path;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::WindowsAndMessaging::*;

/// Lebar/tinggi sudut jendela yang tetap berada di layar saat terparkir.
pub const SLIVER: i32 = 4;

/// Jendela dianggap terparkir bila seluruhnya berada di kiri-atas titik (SLIVER, SLIVER) dan hanya sudutnya menyentuh layar.
fn rect_is_parked(r: &RECT) -> bool {
    r.right > 0 && r.right <= SLIVER + 2 && r.bottom > 0 && r.bottom <= SLIVER + 2 && r.left < -100 && r.top < -50
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Parked {
    pub hwnd: isize,
    /// Persegi normal jendela (kiri, atas, kanan, bawah) sebelum diparkir.
    pub rect: (i32, i32, i32, i32),
    pub maximized: bool,
}

fn h(v: isize) -> HWND {
    HWND(v as *mut _)
}

/// Pindahkan `hwnd` keluar layar. `None` bila bukan jendela hidup atau sedang diminimalkan.
pub fn park(hwnd: isize) -> Option<Parked> {
    unsafe {
        let w = h(hwnd);
        if !IsWindow(Some(w)).as_bool() || IsIconic(w).as_bool() {
            return None;
        }
        let mut wp = WINDOWPLACEMENT { length: std::mem::size_of::<WINDOWPLACEMENT>() as u32, ..Default::default() };
        GetWindowPlacement(w, &mut wp).ok()?;
        let maximized = IsZoomed(w).as_bool();
        let r = wp.rcNormalPosition;
        let mut cur = RECT::default();
        GetWindowRect(w, &mut cur).ok()?;
        let rect = if maximized { (r.left, r.top, r.right, r.bottom) } else { (cur.left, cur.top, cur.right, cur.bottom) };
        if rect_is_parked(&cur) {
            return None; // sudah terparkir
        }
        let size = (cur.right - cur.left, cur.bottom - cur.top);
        if maximized {
            let _ = ShowWindow(w, SW_RESTORE);
        }
        let (pw, ph) = (size.0.max(200), size.1.max(120));
        let _ = SetWindowPos(w, None, SLIVER - pw, SLIVER - ph, pw, ph, SWP_NOACTIVATE | SWP_NOZORDER);
        Some(Parked { hwnd, rect, maximized })
    }
}

/// Kembalikan jendela ke posisi semula.
pub fn unpark(p: &Parked) {
    unsafe {
        let w = h(p.hwnd);
        if !IsWindow(Some(w)).as_bool() {
            return;
        }
        let (l, t, r, b) = p.rect;
        let _ = SetWindowPos(w, None, l, t, r - l, b - t, SWP_NOACTIVATE | SWP_NOZORDER);
        if p.maximized {
            let _ = ShowWindow(w, SW_MAXIMIZE);
        }
    }
}

/// Apakah jendela sedang terparkir (di luar layar oleh kita).
pub fn is_parked(hwnd: isize) -> bool {
    unsafe {
        let mut r = RECT::default();
        GetWindowRect(h(hwnd), &mut r).is_ok() && rect_is_parked(&r)
    }
}

pub fn save(path: &Path, list: &[Parked]) {
    if list.is_empty() {
        let _ = std::fs::remove_file(path);
        return;
    }
    let mut s = String::new();
    for p in list {
        s.push_str(&format!("{} {} {} {} {} {}\n", p.hwnd, p.rect.0, p.rect.1, p.rect.2, p.rect.3, p.maximized as u8));
    }
    let _ = std::fs::write(path, s);
}

pub fn load(path: &Path) -> Vec<Parked> {
    let Ok(s) = std::fs::read_to_string(path) else { return vec![] };
    s.lines()
        .filter_map(|l| {
            let v: Vec<i64> = l.split_whitespace().filter_map(|x| x.parse().ok()).collect();
            (v.len() == 6).then(|| Parked { hwnd: v[0] as isize, rect: (v[1] as i32, v[2] as i32, v[3] as i32, v[4] as i32), maximized: v[5] != 0 })
        })
        .collect()
}

/// Pulihkan semua jendela di berkas lalu hapus berkasnya (dipakai saat start, keluar, dan oleh guard).
pub fn restore_file(path: &Path) -> usize {
    let list = load(path);
    for p in &list {
        // hanya yang benar-benar masih terparkir (hwnd bisa sudah dipakai ulang)
        if is_parked(p.hwnd) {
            unpark(p);
        }
    }
    let _ = std::fs::remove_file(path);
    list.len()
}
