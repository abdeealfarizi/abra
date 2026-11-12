//! Mengambil alih dan memulihkan taskbar Windows (SPEC §4.1).
//!
//! Aturan keselamatan, semuanya disengaja:
//! 1. Marker + guard ditulis/dijalankan **sebelum** menyembunyikan apa pun. Bila salah satu
//!    gagal, taskbar tidak disentuh.
//! 2. `restore_marker` hanya butuh isi marker, sehingga proses guard dan start berikutnya
//!    memakai jalur yang sama persis dengan keluar normal.
//! 3. Marker hanya dihapus setelah taskbar terverifikasi terlihat dan terpasang di layar.
//! 4. `HIDDEN` dimatikan **sebelum** menampilkan kembali, supaya penegak (hook WinEvent)
//!    tidak menyembunyikannya lagi.

use nacre_core::marker::{Marker, Rect};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use windows::core::{w, BOOL};
use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::Shell::{
    SHAppBarMessage, ABM_GETSTATE, ABM_GETTASKBARPOS, ABM_SETSTATE, APPBARDATA,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowW, GetClassNameW, GetWindowRect, GetWindowThreadProcessId, IsWindowVisible,
    SetWindowPos, ShowWindow, EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_SHOW, SWP_NOACTIVATE, SWP_NOSIZE,
    SWP_NOZORDER, SW_HIDE, SW_SHOW, WINEVENT_OUTOFCONTEXT,
};

const PRIMARY_CLASS: &str = "Shell_TrayWnd";
const SECONDARY_CLASS: &str = "Shell_SecondaryTrayWnd";
const PARK_X: i32 = -10000;
const ABS_AUTOHIDE: i32 = 1;
const ABS_ALWAYSONTOP: i32 = 2;

static HIDDEN: AtomicBool = AtomicBool::new(false);

struct Active {
    marker: Marker,
    path: PathBuf,
}
static ACTIVE: Mutex<Option<Active>> = Mutex::new(None);
static HOOKS: Mutex<Vec<isize>> = Mutex::new(Vec::new());

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn is_hidden() -> bool {
    HIDDEN.load(Ordering::Relaxed)
}

// ---------- pembungkus Win32 kecil ----------

fn class_of(hwnd: HWND) -> String {
    let mut buf = [0u16; 64];
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn primary() -> Option<HWND> {
    unsafe { FindWindowW(w!("Shell_TrayWnd"), None).ok() }
}

fn secondaries() -> Vec<HWND> {
    unsafe extern "system" fn each(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let out = &mut *(lparam.0 as *mut Vec<HWND>);
        if class_of(hwnd) == SECONDARY_CLASS {
            out.push(hwnd);
        }
        BOOL(1)
    }
    let mut out: Vec<HWND> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(each), LPARAM(&mut out as *mut Vec<HWND> as isize));
    }
    out
}

fn rect_of(h: HWND) -> Option<Rect> {
    let mut r = RECT::default();
    unsafe { GetWindowRect(h, &mut r).ok()? };
    Some(Rect { left: r.left, top: r.top, right: r.right, bottom: r.bottom })
}

fn abm_data(h: HWND) -> APPBARDATA {
    APPBARDATA { cbSize: std::mem::size_of::<APPBARDATA>() as u32, hWnd: h, ..Default::default() }
}

fn abm_get_state(h: HWND) -> i32 {
    let mut d = abm_data(h);
    unsafe { SHAppBarMessage(ABM_GETSTATE, &mut d) as i32 }
}

fn abm_set_state(h: HWND, state: i32) {
    let mut d = abm_data(h);
    d.lParam = LPARAM(state as isize);
    unsafe {
        SHAppBarMessage(ABM_SETSTATE, &mut d);
    }
}

fn abm_taskbar_pos(h: HWND) -> Option<Rect> {
    let mut d = abm_data(h);
    if unsafe { SHAppBarMessage(ABM_GETTASKBARPOS, &mut d) } == 0 {
        return None;
    }
    Some(Rect { left: d.rc.left, top: d.rc.top, right: d.rc.right, bottom: d.rc.bottom })
}

/// "Tersembunyi" cukup berarti tidak terlihat. Posisi tidak diperiksa: dalam mode auto-hide
/// Explorer sendiri memindahkan window yang tak terlihat ke tepi layar, dan melawannya
/// hanya membuat hook ini berputar. Bila Explorer menampilkannya (reveal auto-hide), kita sembunyikan lagi.
fn is_parked(h: HWND) -> bool {
    unsafe { !IsWindowVisible(h).as_bool() }
}

/// Sembunyikan lalu pindahkan jauh di luar layar, agar tidak ada garis tipis/kedip.
fn park(h: HWND) {
    unsafe {
        let _ = ShowWindow(h, SW_HIDE);
        let _ = SetWindowPos(h, None, PARK_X, PARK_X, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
    }
}

fn place_and_show(h: HWND, rect: Option<Rect>) {
    unsafe {
        if let Some(r) = rect {
            let _ = SetWindowPos(h, None, r.left, r.top, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
        }
        let _ = ShowWindow(h, SW_SHOW);
    }
}

// ---------- marker di disk ----------

pub fn read_marker(path: &Path) -> Option<Marker> {
    Marker::parse(&std::fs::read_to_string(path).ok()?)
}

/// Tulis berkas lengkap di folder yang sama lalu ganti; gagal di tengah tidak merusak marker lama.
fn write_marker(path: &Path, m: &Marker) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    let mut f = std::fs::File::create(&tmp)?;
    f.write_all(m.serialize().as_bytes())?;
    f.sync_all()?;
    drop(f);
    std::fs::rename(&tmp, path)
}

// ---------- API publik ----------

/// Ada marker sisa dari sesi yang mati tanpa memulihkan taskbar?
pub fn marker_exists(path: &Path) -> bool {
    path.exists()
}

/// Memulihkan taskbar murni dari isi marker. Dipakai: keluar normal, guard, dan start setelah crash.
/// `true` hanya bila taskbar utama terlihat dan terpasang di layar (dan tiap sekunder yang ada juga).
pub fn restore_marker(m: &Marker) -> bool {
    let Some(p) = primary() else { return false };
    HIDDEN.store(false, Ordering::Relaxed);
    abm_set_state(p, if m.abstate >= 0 { m.abstate } else { ABS_ALWAYSONTOP });
    place_and_show(p, m.primary);
    let mut ok = unsafe { IsWindowVisible(p).as_bool() } && rect_of(p).is_some_and(|r| r.is_placed());
    for s in secondaries() {
        place_and_show(s, m.secondary_rect(s.0 as isize));
        ok &= unsafe { IsWindowVisible(s).as_bool() } && rect_of(s).is_some_and(|r| r.is_placed());
    }
    ok
}

/// Memulihkan sisa sesi sebelumnya (crash/dibunuh) dan menghapus marker bila berhasil.
pub fn recover_leftover(path: &Path) -> bool {
    if !path.exists() {
        return true;
    }
    // Marker rusak: kita tidak tahu posisi aslinya, tapi lebih baik menampilkan taskbar
    // dengan tebakan aman (selalu di atas, posisi dibiarkan Explorer) daripada membiarkannya hilang.
    let m = read_marker(path).unwrap_or_else(|| Marker::new(-1, 0));
    if m.pid != 0 && m.pid != std::process::id() && crate::process::is_alive(m.pid) {
        crate::log::line("taskbar", "marker dimiliki proses Nacre lain yang masih hidup; tidak disentuh");
        return false;
    }
    let ok = restore_marker(&m);
    if ok {
        let _ = std::fs::remove_file(path);
    }
    crate::log::line("taskbar", &format!("recover_leftover ok={ok}"));
    ok
}

/// Menyembunyikan taskbar. Gagal-tertutup: bila marker atau guard tidak bisa disiapkan,
/// taskbar tidak diubah sama sekali.
pub fn hide(marker_path: &Path, guard_exe: &Path) -> Result<(), String> {
    if is_hidden() {
        reassert();
        return Ok(());
    }
    if marker_path.exists() && !recover_leftover(marker_path) {
        return Err("sisa sesi sebelumnya belum bisa dipulihkan".into());
    }
    let p = primary().ok_or("taskbar tidak ditemukan")?;
    let pid = std::process::id();

    let mut m = Marker::new(abm_get_state(p), pid);
    m.primary = rect_of(p).filter(|r| r.is_placed()).or_else(|| abm_taskbar_pos(p).filter(|r| r.is_placed()));
    for s in secondaries() {
        if let Some(r) = rect_of(s).filter(|r| r.is_placed()) {
            m.secondaries.push((s.0 as isize, r));
        }
    }

    write_marker(marker_path, &m).map_err(|e| format!("marker: {e}"))?;
    if let Err(e) = super::guard_spawn::spawn_once(guard_exe, pid, marker_path) {
        let _ = std::fs::remove_file(marker_path);
        return Err(format!("guard tidak bisa dijalankan ({}): {e}", guard_exe.display()));
    }

    *lock(&ACTIVE) = Some(Active { marker: m, path: marker_path.to_path_buf() });
    abm_set_state(p, ABS_AUTOHIDE);
    park(p);
    for s in secondaries() {
        park(s);
    }
    HIDDEN.store(true, Ordering::Relaxed);
    install_enforcer();
    crate::log::line("taskbar", "hidden");
    Ok(())
}

/// Menampilkan kembali taskbar dari keadaan di memori. Aman dipanggil berulang kali.
pub fn restore() -> bool {
    let active = lock(&ACTIVE).take();
    remove_enforcer();
    let Some(a) = active else {
        HIDDEN.store(false, Ordering::Relaxed);
        return true;
    };
    let ok = restore_marker(&a.marker);
    if ok {
        let _ = std::fs::remove_file(&a.path);
    } else {
        // Biarkan marker dan state tetap ada: guard / percobaan berikutnya yang menyelesaikan.
        *lock(&ACTIVE) = Some(a);
    }
    crate::log::line("taskbar", &format!("restore ok={ok}"));
    ok
}

/// Dipanggil saat Explorer membuat ulang taskbar (pesan "TaskbarCreated") atau DPI berubah.
pub fn reassert() {
    if !is_hidden() {
        return;
    }
    if let Some(p) = primary() {
        abm_set_state(p, ABS_AUTOHIDE);
        park(p);
    }
    for s in secondaries() {
        park(s);
    }
    install_enforcer();
}

// ---------- penegak ----------

unsafe extern "system" fn enforcer(
    _hook: HWINEVENTHOOK,
    _event: u32,
    hwnd: HWND,
    id_object: i32,
    _id_child: i32,
    _thread: u32,
    _time: u32,
) {
    if !is_hidden() || id_object != 0 || hwnd.0.is_null() {
        return;
    }
    let c = class_of(hwnd);
    if (c == PRIMARY_CLASS || c == SECONDARY_CLASS) && !is_parked(hwnd) {
        park(hwnd);
    }
}

/// Hook hanya untuk proses Explorer pemilik taskbar, supaya event window lain tidak sampai ke sini.
/// Hook WinEvent butuh message loop di thread pemasang (thread UI).
fn install_enforcer() {
    remove_enforcer();
    let Some(p) = primary() else { return };
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(p, Some(&mut pid)) };
    if pid == 0 {
        return;
    }
    let mut hooks = lock(&HOOKS);
    for ev in [EVENT_OBJECT_SHOW, EVENT_OBJECT_LOCATIONCHANGE] {
        let h = unsafe { SetWinEventHook(ev, ev, None, Some(enforcer), pid, 0, WINEVENT_OUTOFCONTEXT) };
        if !h.is_invalid() {
            hooks.push(h.0 as isize);
        }
    }
}

fn remove_enforcer() {
    for h in lock(&HOOKS).drain(..) {
        unsafe {
            let _ = UnhookWinEvent(HWINEVENTHOOK(h as *mut _));
        }
    }
}
