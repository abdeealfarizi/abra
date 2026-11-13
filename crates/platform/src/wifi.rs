//! Nama jaringan Wi-Fi yang sedang tersambung (WLAN API). Dibaca sesuai kebutuhan, tanpa pekerja latar.

use windows::Win32::Foundation::HANDLE;
use windows::Win32::NetworkManagement::WiFi::*;

/// SSID jaringan tersambung, atau `None` bila tidak ada adaptor/tidak tersambung.
pub fn ssid() -> Option<String> {
    unsafe {
        let mut ver = 0u32;
        let mut h = HANDLE::default();
        if WlanOpenHandle(2, None, &mut ver, &mut h) != 0 {
            return None;
        }
        let mut result = None;
        let mut list: *mut WLAN_INTERFACE_INFO_LIST = std::ptr::null_mut();
        if WlanEnumInterfaces(h, None, &mut list) == 0 && !list.is_null() {
            let l = &*list;
            for i in 0..l.dwNumberOfItems as usize {
                let info = &*l.InterfaceInfo.as_ptr().add(i);
                if info.isState != wlan_interface_state_connected {
                    continue;
                }
                let mut size = 0u32;
                let mut data: *mut std::ffi::c_void = std::ptr::null_mut();
                let rc = WlanQueryInterface(h, &info.InterfaceGuid, wlan_intf_opcode_current_connection, None, &mut size, &mut data, None);
                if rc == 0 && !data.is_null() {
                    let attrs = &*(data as *const WLAN_CONNECTION_ATTRIBUTES);
                    let s = &attrs.wlanAssociationAttributes.dot11Ssid;
                    let n = (s.uSSIDLength as usize).min(32);
                    let name = String::from_utf8_lossy(&s.ucSSID[..n]).to_string();
                    if !name.is_empty() {
                        result = Some(name);
                    }
                    WlanFreeMemory(data);
                }
                if result.is_some() {
                    break;
                }
            }
            WlanFreeMemory(list as *const _);
        }
        let _ = WlanCloseHandle(h, None);
        result
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Pemilih jaringan: pindai, daftar, sambung. Berjalan di thread pekerja (WlanScan butuh beberapa detik).
// Sejak Windows 11 24H2 daftar jaringan sekitar butuh izin Lokasi untuk aplikasi desktop; tanpa itu Windows
// mengembalikan ERROR_ACCESS_DENIED dan menu memakai mode terbatas (nama jaringan tersambung saja).
// ---------------------------------------------------------------------------------------------------------------

use nacre_core::netlist::{merge, Network};
use std::sync::mpsc::{channel, Receiver, Sender};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Scan {
    /// Jaringan terlihat (sudah digabung dan diurutkan); kosong bila `denied` atau tidak ada adaptor.
    pub networks: Vec<Network>,
    /// Daftar ditolak Windows (izin Lokasi mati).
    pub denied: bool,
    /// Ada adaptor Wi-Fi.
    pub adapter: bool,
    /// SSID jaringan tersambung (tidak bisa dibaca saat `denied`).
    pub current: Option<String>,
    /// Adaptor sedang tersambung ke suatu jaringan (selalu bisa dibaca, juga saat `denied`).
    pub connected: bool,
}

pub enum Cmd {
    /// Baca daftar tersimpan sekarang, lalu pindai ulang dan kirim lagi saat selesai.
    Scan,
    Connect(Network),
}

pub struct Scanner {
    pub commands: Sender<Cmd>,
    pub results: Receiver<Scan>,
}

const ERROR_ACCESS_DENIED: u32 = 5;

fn wide_to_string(w: &[u16]) -> String {
    let n = w.iter().position(|c| *c == 0).unwrap_or(w.len());
    String::from_utf16_lossy(&w[..n])
}

fn interfaces(h: HANDLE) -> Vec<windows::core::GUID> {
    interfaces_state(h).into_iter().map(|(g, _)| g).collect()
}

/// Antarmuka WLAN beserta apakah sedang tersambung.
fn interfaces_state(h: HANDLE) -> Vec<(windows::core::GUID, bool)> {
    let mut out = Vec::new();
    unsafe {
        let mut list: *mut WLAN_INTERFACE_INFO_LIST = std::ptr::null_mut();
        if WlanEnumInterfaces(h, None, &mut list) == 0 && !list.is_null() {
            let l = &*list;
            for i in 0..l.dwNumberOfItems as usize {
                let info = &*l.InterfaceInfo.as_ptr().add(i);
                out.push((info.InterfaceGuid, info.isState == wlan_interface_state_connected));
            }
            WlanFreeMemory(list as *const _);
        }
    }
    out
}

fn read(h: HANDLE) -> Scan {
    let state = interfaces_state(h);
    let guids: Vec<_> = state.iter().map(|(g, _)| *g).collect();
    let mut scan = Scan { adapter: !guids.is_empty(), connected: state.iter().any(|(_, c)| *c), ..Default::default() };
    let mut raw = Vec::new();
    for g in &guids {
        unsafe {
            let mut list: *mut WLAN_AVAILABLE_NETWORK_LIST = std::ptr::null_mut();
            let rc = WlanGetAvailableNetworkList(h, g, 0, None, &mut list);
            if rc == ERROR_ACCESS_DENIED {
                scan.denied = true;
            }
            if rc != 0 || list.is_null() {
                continue;
            }
            let l = &*list;
            for i in 0..l.dwNumberOfItems as usize {
                let n = &*l.Network.as_ptr().add(i);
                let len = (n.dot11Ssid.uSSIDLength as usize).min(32);
                let profile = wide_to_string(&n.strProfileName);
                raw.push(Network {
                    ssid: String::from_utf8_lossy(&n.dot11Ssid.ucSSID[..len]).to_string(),
                    signal: n.wlanSignalQuality.min(100) as u8,
                    secured: n.bSecurityEnabled.as_bool(),
                    connected: n.dwFlags & WLAN_AVAILABLE_NETWORK_CONNECTED != 0,
                    profile: (n.dwFlags & WLAN_AVAILABLE_NETWORK_HAS_PROFILE != 0 && !profile.is_empty()).then_some(profile),
                });
            }
            WlanFreeMemory(list as *const _);
        }
    }
    scan.networks = merge(raw);
    // nama jaringan Windows (NLM) tidak dipakai: saat izin Lokasi mati Windows menggantinya dengan teks samaran
    scan.current = scan.networks.iter().find(|n| n.connected).map(|n| n.ssid.clone()).or_else(ssid);
    scan
}

fn connect(h: HANDLE, n: &Network) {
    let Some(g) = interfaces(h).into_iter().next() else { return };
    let bytes = n.ssid.as_bytes();
    let mut dot = DOT11_SSID { uSSIDLength: bytes.len().min(32) as u32, ucSSID: [0; 32] };
    dot.ucSSID[..bytes.len().min(32)].copy_from_slice(&bytes[..bytes.len().min(32)]);
    let prof: Vec<u16> = n.profile.clone().unwrap_or_default().encode_utf16().chain(std::iter::once(0)).collect();
    let params = WLAN_CONNECTION_PARAMETERS {
        wlanConnectionMode: if n.profile.is_some() { wlan_connection_mode_profile } else { wlan_connection_mode_discovery_unsecure },
        strProfile: if n.profile.is_some() { windows::core::PCWSTR(prof.as_ptr()) } else { windows::core::PCWSTR::null() },
        pDot11Ssid: &mut dot,
        pDesiredBssidList: std::ptr::null_mut(),
        dot11BssType: dot11_BSS_type_infrastructure,
        dwFlags: 0,
    };
    let rc = unsafe { WlanConnect(h, &g, &params, None) };
    crate::log::line("wifi", &format!("sambung ({}): {rc}", if n.profile.is_some() { "profil" } else { "terbuka" }));
}

/// Data palsu untuk uji tampilan (`NACRE_FAKE_WIFI=1`), tanpa menyentuh adaptor.
fn fake() -> Scan {
    let n = |s: &str, sig: u8, sec: bool, con: bool, known: bool| Network { ssid: s.into(), signal: sig, secured: sec, connected: con, profile: known.then(|| s.to_string()) };
    let networks = merge(vec![n("Jaringan Uji", 90, true, true, true), n("Kantor Uji", 55, true, false, true), n("Kafe Uji", 70, false, false, false), n("Tetangga Uji", 25, true, false, false)]);
    Scan { networks, denied: false, adapter: true, current: Some("Jaringan Uji".into()), connected: true }
}

pub fn spawn(notify: impl Fn() + Send + 'static) -> Scanner {
    let (cmd_tx, cmd_rx) = channel::<Cmd>();
    let (res_tx, res_rx) = channel::<Scan>();
    let fake_mode = std::env::var_os("NACRE_FAKE_WIFI").is_some();
    std::thread::spawn(move || {
        let mut h = HANDLE::default();
        let mut ver = 0u32;
        let open = fake_mode || unsafe { WlanOpenHandle(2, None, &mut ver, &mut h) } == 0;
        while let Ok(cmd) = cmd_rx.recv() {
            if fake_mode || !open {
                let _ = res_tx.send(if fake_mode { fake() } else { Scan::default() });
                notify();
                continue;
            }
            match cmd {
                Cmd::Scan => {
                    let first = read(h);
                    let denied = first.denied;
                    let _ = res_tx.send(first);
                    notify();
                    if !denied {
                        for g in interfaces(h) {
                            unsafe {
                                let _ = WlanScan(h, &g, None, None, None);
                            }
                        }
                        // pemindaian selesai dalam ±4 detik (driver memberi tahu lewat notifikasi; cukup tunggu)
                        std::thread::sleep(std::time::Duration::from_millis(3500));
                        let _ = res_tx.send(read(h));
                        notify();
                    }
                }
                Cmd::Connect(n) => {
                    connect(h, &n);
                    std::thread::sleep(std::time::Duration::from_millis(2500));
                    let _ = res_tx.send(read(h));
                    notify();
                }
            }
        }
        if open && !fake_mode {
            unsafe {
                let _ = WlanCloseHandle(h, None);
            }
        }
    });
    Scanner { commands: cmd_tx, results: res_rx }
}
