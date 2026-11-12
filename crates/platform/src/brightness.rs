//! Kecerahan panel internal lewat WMI (`root\WMI`): baca/atur, plus langganan event agar perubahan dari
//! tombol keras terlihat tanpa polling. Hanya ada pada perangkat dengan panel yang mendukung (laptop).
//! Monitor eksternal (DDC/CI) belum didukung.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::mpsc::{channel, Sender};
use wmi::{COMLibrary, WMIConnection};

/// −1 = tidak tersedia/belum dibaca, 0..100 = persen.
static LEVEL: AtomicI32 = AtomicI32::new(-1);

pub fn latest() -> Option<u8> {
    let v = LEVEL.load(Ordering::Relaxed);
    (v >= 0).then_some(v as u8)
}

pub enum Cmd {
    Set(u8),
    Refresh,
}

pub struct Brightness {
    pub commands: Sender<Cmd>,
}

#[derive(Deserialize, Debug)]
#[serde(rename = "WmiMonitorBrightness")]
#[serde(rename_all = "PascalCase")]
struct Current {
    current_brightness: u8,
}

#[derive(Deserialize, Debug)]
#[serde(rename = "WmiMonitorBrightnessEvent")]
#[serde(rename_all = "PascalCase")]
struct Event {
    brightness: u8,
}

#[derive(Deserialize, Debug)]
#[serde(rename = "WmiMonitorBrightnessMethods")]
struct Methods {
    #[serde(rename = "__Path")]
    path: String,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct SetA {
    timeout: u32,
    brightness: u8,
}
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct SetB {
    timeout: i32,
    brightness: i32,
}
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct SetC {
    timeout: u8,
    brightness: u8,
}
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct SetD {
    timeout: String,
    brightness: String,
}

static VARIANT: AtomicI32 = AtomicI32::new(0);

fn set(con: &WMIConnection, path: &str, p: u8) -> bool {
    let m = "WmiSetBrightness";
    let start = VARIANT.load(Ordering::Relaxed);
    for off in 0..4 {
        let v = (start + off) % 4;
        let ok = match v {
            0 => con.exec_instance_method::<Methods, _, ()>(m, path, SetA { timeout: 1, brightness: p }).is_ok(),
            1 => con.exec_instance_method::<Methods, _, ()>(m, path, SetB { timeout: 1, brightness: p as i32 }).is_ok(),
            2 => con.exec_instance_method::<Methods, _, ()>(m, path, SetC { timeout: 1, brightness: p }).is_ok(),
            _ => con.exec_instance_method::<Methods, _, ()>(m, path, SetD { timeout: "1".into(), brightness: p.to_string() }).is_ok(),
        };
        if ok {
            if v != start {
                crate::log::line("brightness", &format!("varian {v} bekerja"));
            }
            VARIANT.store(v, Ordering::Relaxed);
            return true;
        }
    }
    false
}

fn read(con: &WMIConnection) -> Option<u8> {
    con.query::<Current>().ok()?.first().map(|c| c.current_brightness.min(100))
}

pub fn spawn(notify: impl Fn() + Send + Sync + 'static) -> Brightness {
    let (tx, rx) = channel::<Cmd>();
    let notify: std::sync::Arc<dyn Fn() + Send + Sync> = std::sync::Arc::new(notify);

    // Pekerja perintah + pembacaan awal.
    let n1 = notify.clone();
    std::thread::spawn(move || {
        let Ok(com) = COMLibrary::new() else { return };
        let Ok(con) = WMIConnection::with_namespace_path("root\\WMI", com) else { return };
        if let Some(v) = read(&con) {
            LEVEL.store(v as i32, Ordering::Relaxed);
            n1();
        } else {
            crate::log::line("brightness", "panel tidak mendukung kecerahan WMI");
            return;
        }
        while let Ok(cmd) = rx.recv() {
            match cmd {
                Cmd::Set(p) => {
                    let p = p.min(100);
                    let path = con.query::<Methods>().ok().and_then(|m| m.into_iter().next()).map(|m| m.path);
                    if let Some(path) = path {
                        if set(&con, &path, p) {
                            LEVEL.store(p as i32, Ordering::Relaxed);
                            n1();
                        } else {
                            crate::log::line("brightness", "set gagal (semua varian)");
                        }
                    }
                }
                Cmd::Refresh => {
                    if let Some(v) = read(&con) {
                        LEVEL.store(v as i32, Ordering::Relaxed);
                        n1();
                    }
                }
            }
        }
    });

    // Pendengar event (memblokir sampai ada kejadian): perubahan dari tombol keras/aplikasi lain.
    let n2 = notify;
    std::thread::spawn(move || {
        let Ok(com) = COMLibrary::new() else { return };
        let Ok(con) = WMIConnection::with_namespace_path("root\\WMI", com) else { return };
        let Ok(events) = con.raw_notification::<Event>("SELECT * FROM WmiMonitorBrightnessEvent") else { return };
        for ev in events {
            let Ok(ev) = ev else { break };
            LEVEL.store(ev.brightness.min(100) as i32, Ordering::Relaxed);
            n2();
        }
    });
    Brightness { commands: tx }
}
