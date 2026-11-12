//! Indeks aplikasi terpasang dari folder shell `AppsFolder` (aplikasi desktop dan Microsoft Store), dimuat di
//! thread pekerja dan di-cache. Dipakai Spotlight dan Launchpad.

use std::sync::Mutex;
use windows::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, COINIT_MULTITHREADED};
use windows::Win32::UI::Shell::{
    BHID_EnumItems, FOLDERID_AppsFolder, IEnumShellItems, IShellItem, SHGetKnownFolderItem, KF_FLAG_DEFAULT, SIGDN_NORMALDISPLAY,
    SIGDN_PARENTRELATIVEPARSING,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct App {
    pub name: String,
    /// Jalur `shell:AppsFolder\...` yang bisa dibuka (`launch::open`) dan dimuat ikonnya (`icons::load`).
    pub path: String,
}

static CACHE: Mutex<Option<Vec<App>>> = Mutex::new(None);
static LOADING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn load() -> Vec<App> {
    let mut out = Vec::new();
    unsafe {
        let Ok(folder) = SHGetKnownFolderItem::<IShellItem>(&FOLDERID_AppsFolder, KF_FLAG_DEFAULT, None) else { return out };
        let Ok(en) = folder.BindToHandler::<_, IEnumShellItems>(None, &BHID_EnumItems) else { return out };
        loop {
            let mut batch: [Option<IShellItem>; 16] = Default::default();
            let mut got = 0u32;
            if en.Next(&mut batch, Some(&mut got)).is_err() || got == 0 {
                break;
            }
            for item in batch.iter().take(got as usize).flatten() {
                let name = item.GetDisplayName(SIGDN_NORMALDISPLAY).ok().and_then(|p| {
                    let s = p.to_string().ok();
                    CoTaskMemFree(Some(p.0 as *const _));
                    s
                });
                let rel = item.GetDisplayName(SIGDN_PARENTRELATIVEPARSING).ok().and_then(|p| {
                    let s = p.to_string().ok();
                    CoTaskMemFree(Some(p.0 as *const _));
                    s
                });
                if let (Some(name), Some(rel)) = (name, rel) {
                    if !name.trim().is_empty() {
                        out.push(App { name, path: format!("shell:AppsFolder\\{rel}") });
                    }
                }
            }
        }
    }
    out.sort_by_key(|a| a.name.to_lowercase());
    out.dedup_by(|a, b| a.path == b.path);
    out
}

/// Mulai (atau segarkan) pemuatan indeks di thread pekerja; `notify` dipanggil setelah selesai.
pub fn refresh(notify: impl Fn() + Send + 'static) {
    if LOADING.swap(true, std::sync::atomic::Ordering::AcqRel) {
        return;
    }
    std::thread::spawn(move || {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let list = load();
        crate::log::line("apps", &format!("indeks: {} aplikasi", list.len()));
        *CACHE.lock().unwrap_or_else(|e| e.into_inner()) = Some(list);
        LOADING.store(false, std::sync::atomic::Ordering::Release);
        notify();
    });
}

/// Indeks terakhir yang dimuat (kosong bila belum selesai).
pub fn snapshot() -> Vec<App> {
    CACHE.lock().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or_default()
}

pub fn is_loaded() -> bool {
    CACHE.lock().unwrap_or_else(|e| e.into_inner()).is_some()
}
