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
