//! Perangkat Bluetooth yang sudah dipasangkan (klasik + LE) beserta status tersambung, untuk menu Bluetooth.
//!
//! Headphone/speaker (audio Bluetooth klasik) bisa disambung/diputus langsung: setiap perangkat audio punya filter
//! kernel-streaming yang menerima properti terdokumentasi `KSPROPERTY_ONESHOT_RECONNECT` / `KSPROPERTY_ONESHOT_DISCONNECT`
//! (`KSPROPSETID_BtAudio`). Filternya dicari lewat topologi endpoint audio, lalu dicocokkan dengan alamat Bluetooth di
//! jalur perangkatnya. Perangkat lain (keyboard, mouse, ponsel) tetap dibuka di Pengaturan Bluetooth Windows.
//! Berjalan di thread pekerja karena `FromIdAsync` dan penyambungan bisa makan ratusan milidetik sampai detik.

use nacre_core::netlist::{bt_addr_from_path, kind_from_appearance, kind_from_cod, merge_devices, Device};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;
use windows::Devices::Bluetooth::{BluetoothConnectionStatus, BluetoothDevice, BluetoothLEDevice};
use windows::Devices::Enumeration::DeviceInformation;
use windows::Win32::Media::Audio::{eAll, IDeviceTopology, IMMDeviceEnumerator, MMDeviceEnumerator, DEVICE_STATE};
use windows::Win32::Media::KernelStreaming::{IKsControl, KSIDENTIFIER, KSIDENTIFIER_0, KSIDENTIFIER_0_0, KSPROPERTY_ONESHOT_DISCONNECT, KSPROPERTY_ONESHOT_RECONNECT, KSPROPERTY_TYPE_GET, KSPROPSETID_BtAudio};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED};

pub enum Cmd {
    Refresh,
    /// Sambungkan (`true`) atau putuskan perangkat audio beralamat ini.
    Audio(u64, bool),
}

pub struct Watcher {
    pub commands: Sender<Cmd>,
    pub lists: Receiver<Vec<Device>>,
}

fn ids(selector: windows_core::Result<windows_core::HSTRING>) -> Vec<windows_core::HSTRING> {
    let Ok(sel) = selector else { return vec![] };
    let Some(list) = DeviceInformation::FindAllAsyncAqsFilter(&sel).ok().and_then(|o| o.join().ok()) else { return vec![] };
    (0..list.Size().unwrap_or(0)).filter_map(|i| list.GetAt(i).ok()).filter_map(|d| d.Id().ok()).collect()
}

/// Filter audio Bluetooth klasik per alamat: endpoint audio (aktif atau terputus) → konektor → filter KS di seberangnya.
fn audio_filters() -> Vec<(u64, IKsControl)> {
    let dbg = std::env::var_os("NACRE_DEBUG_BTAUDIO").is_some();
    let mut out: Vec<(u64, IKsControl)> = Vec::new();
    unsafe {
        let Ok(en) = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL) else { return out };
        // aktif (1) + tidak dicolok (8): headphone yang sedang terputus tercatat sebagai "unplugged"
        let Ok(col) = en.EnumAudioEndpoints(eAll, DEVICE_STATE(1 | 8)) else { return out };
        for i in 0..col.GetCount().unwrap_or(0) {
            let Ok(dev) = col.Item(i) else { continue };
            let Ok(topo) = dev.Activate::<IDeviceTopology>(CLSCTX_ALL, None) else { continue };
            let Ok(conn) = topo.GetConnector(0) else { continue };
            // jalur perangkat filter di seberang konektor endpoint (mis. "{2}.\?thenum#...")
            let Ok(pid) = conn.GetDeviceIdConnectedTo() else { continue };
            let path = pid.to_string().unwrap_or_default();
            let Some(addr) = bt_addr_from_path(&path) else {
                CoTaskMemFree(Some(pid.0 as *const _));
                continue;
            };
            if out.iter().any(|(a, _)| *a == addr) {
                CoTaskMemFree(Some(pid.0 as *const _));
                continue;
            }
            // filter itu sendiri dibuka sebagai IMMDevice, lalu IKsControl-nya diaktifkan
            let ks = en.GetDevice(windows::core::PCWSTR(pid.0)).and_then(|f| f.Activate::<IKsControl>(CLSCTX_ALL, None));
            CoTaskMemFree(Some(pid.0 as *const _));
            match ks {
                Ok(k) => out.push((addr, k)),
                Err(e) => {
                    if dbg {
                        crate::log::line("bt-audio", &format!("#{i} IKsControl gagal: {e}"));
                    }
                }
            }
        }
    }
    if dbg {
        crate::log::line("bt-audio", &format!("{} filter audio Bluetooth siap", out.len()));
    }
    out
}

/// Kirim perintah sambung/putus ke filter audio perangkat `addr`. `false` bila filternya tidak ditemukan atau ditolak.
fn set_audio(addr: u64, connect: bool) -> bool {
    let Some((_, ks)) = audio_filters().into_iter().find(|(a, _)| *a == addr) else { return false };
    let id = if connect { KSPROPERTY_ONESHOT_RECONNECT } else { KSPROPERTY_ONESHOT_DISCONNECT };
    let prop = KSIDENTIFIER { Anonymous: KSIDENTIFIER_0 { Anonymous: KSIDENTIFIER_0_0 { Set: KSPROPSETID_BtAudio, Id: id.0 as u32, Flags: KSPROPERTY_TYPE_GET } } };
    let mut returned = 0u32;
    unsafe { ks.KsProperty(&prop, std::mem::size_of::<KSIDENTIFIER>() as u32, std::ptr::null_mut(), 0, &mut returned) }.is_ok()
}

fn read() -> Vec<Device> {
    let audio: Vec<u64> = audio_filters().into_iter().map(|(a, _)| a).collect();
    let mut raw = Vec::new();
    for id in ids(BluetoothDevice::GetDeviceSelectorFromPairingState(true)) {
        if let Some(d) = BluetoothDevice::FromIdAsync(&id).ok().and_then(|o| o.join().ok()) {
            let addr = d.BluetoothAddress().unwrap_or(0);
            raw.push(Device {
                name: d.Name().map(|s| s.to_string()).unwrap_or_default(),
                connected: d.ConnectionStatus().ok() == Some(BluetoothConnectionStatus::Connected),
                kind: d.ClassOfDevice().and_then(|c| c.RawValue()).map(kind_from_cod).unwrap_or_default(),
                addr,
                audio: addr != 0 && audio.contains(&addr),
            });
        }
    }
    for id in ids(BluetoothLEDevice::GetDeviceSelectorFromPairingState(true)) {
        if let Some(d) = BluetoothLEDevice::FromIdAsync(&id).ok().and_then(|o| o.join().ok()) {
            raw.push(Device {
                name: d.Name().map(|s| s.to_string()).unwrap_or_default(),
                connected: d.ConnectionStatus().ok() == Some(BluetoothConnectionStatus::Connected),
                kind: d.Appearance().and_then(|a| a.RawValue()).map(kind_from_appearance).unwrap_or_default(),
                addr: d.BluetoothAddress().unwrap_or(0),
                audio: false,
            });
        }
    }
    let list = merge_devices(raw);
    crate::log::line("bluetooth", &format!("{} perangkat terpasang, {} bisa disambung langsung (audio)", list.len(), list.iter().filter(|d| d.audio).count()));
    list
}

/// Data palsu untuk uji tampilan (`NACRE_FAKE_WIFI=1` juga memalsukan Bluetooth).
fn fake(connected_headphone: bool) -> Vec<Device> {
    use nacre_core::netlist::DevKind;
    let d = |n: &str, c: bool, k: DevKind, addr: u64, audio: bool| Device { name: n.into(), connected: c, kind: k, addr, audio };
    merge_devices(vec![d("Headphone Uji", connected_headphone, DevKind::Headphones, 1, true), d("Keyboard Uji", false, DevKind::Keyboard, 2, false), d("Mouse Uji", false, DevKind::Mouse, 3, false)])
}

pub fn spawn(notify: impl Fn() + Send + 'static) -> Watcher {
    let (tx, rx) = channel::<Cmd>();
    let (list_tx, list_rx) = channel::<Vec<Device>>();
    let fake_mode = std::env::var_os("NACRE_FAKE_WIFI").is_some();
    std::thread::spawn(move || {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let mut fake_on = true;
        let mut queue: std::collections::VecDeque<Cmd> = std::collections::VecDeque::new();
        loop {
            let cmd = match queue.pop_front() {
                Some(c) => c,
                None => match rx.recv() {
                    Ok(c) => c,
                    Err(_) => break,
                },
            };
            match cmd {
                Cmd::Refresh => {
                    // baca-ulang beruntun cukup dijawab sekali; perintah lain menunggu giliran
                    while let Ok(c) = rx.try_recv() {
                        if !matches!(c, Cmd::Refresh) {
                            queue.push_back(c);
                        }
                    }
                    let _ = list_tx.send(if fake_mode { fake(fake_on) } else { read() });
                    notify();
                }
                Cmd::Audio(addr, on) => {
                    if fake_mode {
                        fake_on = on;
                        std::thread::sleep(Duration::from_millis(1500));
                        let _ = list_tx.send(fake(fake_on));
                        notify();
                        continue;
                    }
                    let ok = set_audio(addr, on);
                    crate::log::line("bluetooth", &format!("{} perangkat audio: {}", if on { "sambung" } else { "putus" }, if ok { "perintah terkirim" } else { "filter tidak ditemukan" }));
                    // penyambungan butuh beberapa detik: kirim keadaan dua kali
                    for wait in [2500u64, 4500] {
                        std::thread::sleep(Duration::from_millis(wait));
                        let _ = list_tx.send(read());
                        notify();
                    }
                }
            }
        }
    });
    Watcher { commands: tx, lists: list_rx }
}
