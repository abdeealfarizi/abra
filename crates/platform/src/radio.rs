//! Radio Wi-Fi dan Bluetooth lewat Windows.Devices.Radios: baca dan nyalakan/matikan.
//! Berjalan di thread pekerja karena `SetStateAsync` bisa makan ratusan milidetik.

use std::sync::mpsc::{channel, Receiver, Sender};
use windows::Devices::Radios::{Radio, RadioKind, RadioState};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Snapshot {
    /// `None` = tidak ada radio jenis itu (atau akses ditolak).
    pub wifi: Option<bool>,
    pub bluetooth: Option<bool>,
}

#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Wifi,
    Bluetooth,
}

pub enum Cmd {
    Refresh,
    Set(Kind, bool),
}

pub struct Radios {
    pub commands: Sender<Cmd>,
    pub snapshots: Receiver<Snapshot>,
}

fn read_all() -> Vec<Radio> {
    let Some(list) = Radio::GetRadiosAsync().ok().and_then(|o| o.join().ok()) else { return vec![] };
    (0..list.Size().unwrap_or(0)).filter_map(|i| list.GetAt(i).ok()).collect()
}

fn snapshot(radios: &[Radio]) -> Snapshot {
    let state = |k: RadioKind| {
        radios
            .iter()
            .find(|r| r.Kind().ok() == Some(k))
            .map(|r| r.State().ok() == Some(RadioState::On))
    };
    Snapshot { wifi: state(RadioKind::WiFi), bluetooth: state(RadioKind::Bluetooth) }
}

pub fn spawn(notify: impl Fn() + Send + 'static) -> Radios {
    let (cmd_tx, cmd_rx) = channel::<Cmd>();
    let (snap_tx, snap_rx) = channel::<Snapshot>();
    std::thread::spawn(move || {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        // Izin akses radio untuk aplikasi desktop; hasilnya tidak menghalangi percobaan berikutnya.
        let _ = Radio::RequestAccessAsync().ok().and_then(|o| o.join().ok());
        let _ = snap_tx.send(snapshot(&read_all()));
        notify();
        while let Ok(cmd) = cmd_rx.recv() {
            if let Cmd::Set(kind, on) = cmd {
                let want = match kind {
                    Kind::Wifi => RadioKind::WiFi,
                    Kind::Bluetooth => RadioKind::Bluetooth,
                };
                for r in read_all() {
                    if r.Kind().ok() == Some(want) {
                        let st = if on { RadioState::On } else { RadioState::Off };
                        let res = r.SetStateAsync(st).ok().and_then(|o| o.join().ok());
                        crate::log::line("radio", &format!("{kind:?} -> {on}: {res:?}"));
                    }
                }
            }
            let _ = snap_tx.send(snapshot(&read_all()));
            notify();
        }
    });
    Radios { commands: cmd_tx, snapshots: snap_rx }
}
