use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use windows::core::{HSTRING, PWSTR};
use windows::Win32::System::Registry::{
    RegCloseKey, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, KEY_READ, RRF_RT_REG_SZ,
};

#[link(name = "kernel32")]
extern "system" {
    fn K32EnumProcesses(lpidProcess: *mut u32, cb: u32, lpcbNeeded: *mut u32) -> i32;
}

const NOTIFY_SETTINGS: &str = "Control Panel\\NotifyIconSettings";
const CACHE_TTL: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayApp {
    pub name: String,
    pub exe: String,
}

struct Cached {
    at: Instant,
    list: Vec<TrayApp>,
}

static CACHE: Mutex<Option<Cached>> = Mutex::new(None);

fn reg_str(key: HKEY, name: &str) -> Option<String> {
    unsafe {
        let mut size = 0u32;
        let vn = HSTRING::from(name);
        if RegGetValueW(key, None, &vn, RRF_RT_REG_SZ, None, None, Some(&mut size)).is_err() || size == 0 {
            return None;
        }
        let mut buf = vec![0u16; (size as usize).div_ceil(2)];
        if RegGetValueW(key, None, &vn, RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut size)).is_err() {
            return None;
        }
        let n = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
        let s = String::from_utf16_lossy(&buf[..n]).trim().to_string();
        (!s.is_empty()).then_some(s)
    }
}

fn registered_tips() -> HashMap<String, String> {
    let mut map = HashMap::new();
    unsafe {
        let mut root = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, &HSTRING::from(NOTIFY_SETTINGS), Some(0), KEY_READ, &mut root).is_err() {
            return map;
        }
        for i in 0..256u32 {
            let mut sub_buf = [0u16; 256];
            let mut len = sub_buf.len() as u32;
            if RegEnumKeyExW(root, i, Some(PWSTR(sub_buf.as_mut_ptr())), &mut len, None, None, None, None).is_err() {
                break;
            }
            let sub = String::from_utf16_lossy(&sub_buf[..len as usize]);
            let mut sk = HKEY::default();
            if RegOpenKeyExW(root, &HSTRING::from(sub), Some(0), KEY_READ, &mut sk).is_ok() {
                if let Some(exe) = reg_str(sk, "ExecutablePath") {
                    let file = Path::new(&exe)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
                        .to_ascii_lowercase();
                    if !file.is_empty() {
                        let tip = reg_str(sk, "InitialTooltip").unwrap_or_default();
                        map.entry(file).or_insert(tip);
                    }
                }
                let _ = RegCloseKey(sk);
            }
        }
        let _ = RegCloseKey(root);
    }
    map
}

fn friendly_name(exe: &str, tip: &str) -> String {
    let t = tip.lines().next().unwrap_or("").trim();
    if !t.is_empty() {
        return t.chars().take(48).collect();
    }
    Path::new(exe)
        .file_stem()
        .and_then(|n| n.to_str())
        .unwrap_or("App")
        .to_string()
}

pub fn running() -> Vec<TrayApp> {
    let now = Instant::now();
    if let Ok(g) = CACHE.lock() {
        if let Some(c) = g.as_ref() {
            if now.duration_since(c.at) < CACHE_TTL {
                return c.list.clone();
            }
        }
    }

    let tips = registered_tips();
    let mut pids = vec![0u32; 1024];
    let mut needed = 0u32;
    let ok = unsafe { K32EnumProcesses(pids.as_mut_ptr(), (pids.len() * 4) as u32, &mut needed) };
    let mut out = Vec::new();
    let mut seen = Vec::new();
    if ok != 0 {
        let count = (needed as usize / 4).min(pids.len());
        for &pid in &pids[..count] {
            if pid == 0 || pid == std::process::id() {
                continue;
            }
            let Some(path) = crate::wins::enumerate::exe_path_of_pid(pid) else { continue };
            let file = Path::new(&path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if file.is_empty()
                || seen.contains(&file)
                || matches!(
                    file.as_str(),
                    "abra.exe" | "abra-guard.exe" | "explorer.exe" | "monotificationux.exe" | "textinputhost.exe" | "shellexperiencehost.exe"
                )
            {
                continue;
            }
            if let Some(tip) = tips.get(&file) {
                seen.push(file);
                out.push(TrayApp { name: friendly_name(&path, tip), exe: path });
            }
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    if let Ok(mut g) = CACHE.lock() {
        *g = Some(Cached { at: now, list: out.clone() });
    }
    out
}

pub fn activate(app: &TrayApp) {
    let target = app.exe.to_ascii_lowercase();
    let wins = crate::wins::enumerate::list(std::process::id());
    if let Some(w) = wins.iter().find(|w| w.path.to_ascii_lowercase() == target) {
        crate::wins::focus::force_foreground(w.hwnd);
    } else {
        crate::launch::show(&app.exe, "");
    }
}
