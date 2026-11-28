use nacre_core::inputmethod::{badge, parse_evkey_ini, short_code, unikey_switch_key, EngineKind, SwitchKey};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};
use windows::core::HSTRING;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows::Win32::UI::WindowsAndMessaging::{GetWindowThreadProcessId, PostMessageW};

const WM_INPUTLANGCHANGEREQUEST: u32 = 0x0050;

const LOCALE_SLOCALIZEDDISPLAYNAME: u32 = 0x0000_0002;
const LOCALE_SISO639LANGNAME: u32 = 0x0000_0059;
const PROC_CACHE_TTL: Duration = Duration::from_secs(5);

#[link(name = "user32")]
extern "system" {
    fn GetKeyboardLayout(idThread: u32) -> isize;
    fn GetKeyboardLayoutList(nBuff: i32, lpList: *mut isize) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetLocaleInfoW(Locale: u32, LCType: u32, lpLCData: *mut u16, cchData: i32) -> i32;
    fn K32EnumProcesses(lpidProcess: *mut u32, cb: u32, lpcbNeeded: *mut u32) -> i32;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    pub hkl: isize,
    pub code: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalIme {
    pub kind: EngineKind,
    pub exe: PathBuf,
    pub vietnamese: bool,
    pub switch_key: SwitchKey,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct State {
    pub active_hkl: isize,
    pub active_code: String,
    pub layouts: Vec<Layout>,
    pub external: Option<ExternalIme>,
}

impl State {
    pub fn badge(&self) -> String {
        if self.active_code.is_empty() && self.external.is_none() {
            return String::new();
        }
        let code = if self.active_code.is_empty() { "EN" } else { &self.active_code };
        badge(code, self.external.as_ref().map(|e| e.vietnamese))
    }
}

#[derive(Clone)]
struct CachedProc {
    at: Instant,
    found: Option<(EngineKind, PathBuf)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct OverrideState {
    kind: EngineKind,
    disk_vi: bool,
    disk_mtime: Option<SystemTime>,
    vi: bool,
}

static PROC_CACHE: Mutex<Option<CachedProc>> = Mutex::new(None);
static OVERRIDE: Mutex<Option<OverrideState>> = Mutex::new(None);

fn locale_info(lang_id: u16, lctype: u32) -> String {
    let mut buf = [0u16; 128];
    let n = unsafe { GetLocaleInfoW(lang_id as u32, lctype, buf.as_mut_ptr(), buf.len() as i32) };
    if n <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..(n as usize).saturating_sub(1)]).trim().to_string()
}

fn layout_from_hkl(hkl: isize) -> Layout {
    let lang_id = (hkl as usize & 0xFFFF) as u16;
    let iso = locale_info(lang_id, LOCALE_SISO639LANGNAME);
    let code = short_code(&iso, lang_id);
    let mut name = locale_info(lang_id, LOCALE_SLOCALIZEDDISPLAYNAME);
    if name.is_empty() {
        name = code.clone();
    }
    Layout { hkl, code, name }
}

pub fn list_layouts() -> Vec<Layout> {
    let count = unsafe { GetKeyboardLayoutList(0, std::ptr::null_mut()) };
    if count <= 0 {
        return Vec::new();
    }
    let mut hkls = vec![0isize; count as usize];
    let got = unsafe { GetKeyboardLayoutList(count, hkls.as_mut_ptr()) };
    if got <= 0 {
        return Vec::new();
    }
    hkls.truncate(got as usize);
    let mut out = Vec::with_capacity(hkls.len());
    for hkl in hkls {
        if hkl != 0 && !out.iter().any(|l: &Layout| l.hkl == hkl) {
            out.push(layout_from_hkl(hkl));
        }
    }
    out
}

fn target_hwnd(preferred: isize) -> isize {
    if preferred != 0 {
        return preferred;
    }
    let ext = crate::wins::tracker::external_foreground();
    if ext != 0 {
        return ext;
    }
    crate::wins::focus::foreground()
}

pub fn active_hkl(hwnd: isize) -> isize {
    let h = target_hwnd(hwnd);
    let tid = if h != 0 {
        unsafe { GetWindowThreadProcessId(HWND(h as *mut _), None) }
    } else {
        0
    };
    unsafe { GetKeyboardLayout(tid) }
}

fn reg_dword(subkey: &str, name: &str) -> Option<u32> {
    let mut v = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(subkey),
            &HSTRING::from(name),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut v as *mut u32 as *mut _),
            Some(&mut size),
        )
        .ok()
        .ok()?;
    }
    Some(v)
}

fn find_external_process() -> Option<(EngineKind, PathBuf)> {
    let now = Instant::now();
    if let Ok(g) = PROC_CACHE.lock() {
        if let Some(c) = g.as_ref() {
            if now.duration_since(c.at) < PROC_CACHE_TTL {
                return c.found.clone();
            }
        }
    }

    let mut pids = vec![0u32; 1024];
    let mut needed = 0u32;
    let ok = unsafe { K32EnumProcesses(pids.as_mut_ptr(), (pids.len() * 4) as u32, &mut needed) };
    let mut found = None;
    if ok != 0 {
        let count = (needed as usize / 4).min(pids.len());
        for &pid in &pids[..count] {
            if pid == 0 {
                continue;
            }
            let Some(path) = crate::wins::enumerate::exe_path_of_pid(pid) else { continue };
            let name = Path::new(&path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            let kind = match name.as_str() {
                "evkey.exe" | "evkey64.exe" | "evkey32.exe" => Some(EngineKind::EvKey),
                "unikeynt.exe" | "unikey.exe" => Some(EngineKind::UniKey),
                "openkey.exe" | "openkey64.exe" | "openkey32.exe" => Some(EngineKind::OpenKey),
                _ => None,
            };
            if let Some(kind) = kind {
                found = Some((kind, PathBuf::from(path)));
                break;
            }
        }
    }

    if let Ok(mut g) = PROC_CACHE.lock() {
        *g = Some(CachedProc { at: now, found: found.clone() });
    }
    found
}

fn evkey_ini_path(exe: &Path) -> PathBuf {
    let beside = exe.with_file_name("setting.ini");
    let appdata = std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("EVKey").join("setting.ini"));
    match appdata {
        Some(app) if app.exists() => {
            if beside.exists() {
                let mt_b = std::fs::metadata(&beside).and_then(|m| m.modified()).ok();
                let mt_a = std::fs::metadata(&app).and_then(|m| m.modified()).ok();
                if mt_a > mt_b { app } else { beside }
            } else {
                app
            }
        }
        _ => beside,
    }
}

fn read_external_raw(kind: EngineKind, exe: &Path) -> (bool, SwitchKey, Option<SystemTime>) {
    match kind {
        EngineKind::EvKey => {
            let ini = evkey_ini_path(exe);
            let mtime = std::fs::metadata(&ini).and_then(|m| m.modified()).ok();
            let cfg = std::fs::read_to_string(&ini).map(|t| parse_evkey_ini(&t)).unwrap_or_default();
            (cfg.vietnamese, cfg.switch_key, mtime)
        }
        EngineKind::UniKey => {
            let vi = reg_dword("Software\\UniKey", "Vietnamese").map(|v| v != 0).unwrap_or(true);
            let sk = reg_dword("Software\\UniKey", "SwitchKey").map(unikey_switch_key).unwrap_or(SwitchKey::CtrlShift);
            (vi, sk, None)
        }
        EngineKind::OpenKey => {
            let vi = reg_dword("Software\\OpenKey", "vLanguage").map(|v| v != 0).unwrap_or(true);
            let sk = reg_dword("Software\\OpenKey", "SwitchKey").map(unikey_switch_key).unwrap_or(SwitchKey::CtrlShift);
            (vi, sk, None)
        }
    }
}

fn detect_external() -> Option<ExternalIme> {
    let (kind, exe) = find_external_process()?;
    let (disk_vi, switch_key, disk_mtime) = read_external_raw(kind, &exe);
    let vietnamese = match OVERRIDE.lock() {
        Ok(mut g) => match *g {
            Some(ov) if ov.kind == kind && ov.disk_vi == disk_vi && ov.disk_mtime == disk_mtime => ov.vi,
            _ => {
                *g = None;
                disk_vi
            }
        },
        Err(_) => disk_vi,
    };
    Some(ExternalIme { kind, exe, vietnamese, switch_key })
}

pub fn read(preferred_hwnd: isize) -> State {
    let layouts = list_layouts();
    let hkl = active_hkl(preferred_hwnd);
    let active_code = layouts
        .iter()
        .find(|l| l.hkl == hkl)
        .map(|l| l.code.clone())
        .unwrap_or_else(|| {
            if hkl != 0 {
                layout_from_hkl(hkl).code
            } else {
                String::new()
            }
        });
    let external = detect_external();
    State { active_hkl: hkl, active_code, layouts, external }
}

pub fn switch_layout(preferred_hwnd: isize, hkl: isize) {
    let target = target_hwnd(preferred_hwnd);
    if target != 0 && hkl != 0 {
        unsafe {
            let _ = PostMessageW(Some(HWND(target as *mut _)), WM_INPUTLANGCHANGEREQUEST, WPARAM(0), LPARAM(hkl));
        }
    }
}

fn send_switch_chord(key: SwitchKey) {
    match key {
        SwitchKey::CtrlShift => crate::keys::chord(&[crate::keys::CTRL, crate::keys::SHIFT]),
        SwitchKey::AltZ => crate::keys::chord(&[crate::keys::ALT, 0x5A]),
    }
}

pub fn toggle_external(cur: &ExternalIme) -> bool {
    let (disk_vi, _, disk_mtime) = read_external_raw(cur.kind, &cur.exe);
    send_switch_chord(cur.switch_key);
    let next = !cur.vietnamese;
    if let Ok(mut g) = OVERRIDE.lock() {
        *g = Some(OverrideState { kind: cur.kind, disk_vi, disk_mtime, vi: next });
    }
    next
}

pub fn set_external_vi(cur: &ExternalIme, want_vi: bool) -> bool {
    if cur.vietnamese == want_vi {
        return cur.vietnamese;
    }
    toggle_external(cur)
}

pub fn quick_switch(preferred_hwnd: isize, state: &State) {
    if let Some(ext) = &state.external {
        if state.active_code.eq_ignore_ascii_case("EN") || state.layouts.len() < 2 {
            toggle_external(ext);
            return;
        }
    }
    if state.layouts.len() >= 2 {
        let idx = state.layouts.iter().position(|l| l.hkl == state.active_hkl).unwrap_or(0);
        let next = &state.layouts[(idx + 1) % state.layouts.len()];
        switch_layout(preferred_hwnd, next.hkl);
    }
}
