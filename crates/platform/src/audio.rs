//! Volume master berbasis event (IAudioEndpointVolumeCallback + IMMNotificationClient): tanpa polling.
//! Satu thread MTA memegang endpoint; perubahan dari tombol keras/aplikasi lain dilaporkan lewat callback,
//! perubahan perangkat keluaran bawaan memicu pemasangan ulang.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{channel, Sender};
use windows::core::{implement, PCWSTR};
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::Endpoints::{
    IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl,
};
use windows::Win32::Media::Audio::{
    eConsole, eRender, EDataFlow, ERole, IMMDeviceEnumerator, IMMNotificationClient, IMMNotificationClient_Impl,
    MMDeviceEnumerator, AUDIO_VOLUME_NOTIFICATION_DATA, DEVICE_STATE,
};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};

static LEVEL: AtomicU32 = AtomicU32::new(0);
static MUTED: AtomicBool = AtomicBool::new(false);
static AVAILABLE: AtomicBool = AtomicBool::new(false);
/// Nama perangkat keluaran bawaan (mis. "Speakers (Realtek(R) Audio)") untuk judul HUD volume.
static NAME: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

/// Nama perangkat keluaran suara saat ini ("" bila belum diketahui).
pub fn output_name() -> String {
    NAME.lock().map(|g| g.clone()).unwrap_or_default()
}

fn read_name(device: &windows::Win32::Media::Audio::IMMDevice) -> Option<String> {
    use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PropVariantToStringAlloc};
    use windows::Win32::System::Com::{CoTaskMemFree, STGM_READ};
    use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
    // PKEY_Device_FriendlyName
    const KEY: PROPERTYKEY = PROPERTYKEY { fmtid: windows::core::GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0), pid: 14 };
    unsafe {
        let store: IPropertyStore = device.OpenPropertyStore(STGM_READ).ok()?;
        let mut pv = store.GetValue(&KEY).ok()?;
        let s = PropVariantToStringAlloc(&pv).ok().and_then(|p| {
            let t = p.to_string().ok();
            CoTaskMemFree(Some(p.0 as *const _));
            t
        });
        let _ = PropVariantClear(&mut pv);
        s.filter(|t| !t.is_empty())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Volume {
    pub level: f32,
    pub muted: bool,
    /// Ada perangkat keluaran yang bisa dikendalikan.
    pub available: bool,
}

pub fn latest() -> Volume {
    Volume {
        level: f32::from_bits(LEVEL.load(Ordering::Relaxed)),
        muted: MUTED.load(Ordering::Relaxed),
        available: AVAILABLE.load(Ordering::Relaxed),
    }
}

pub enum Cmd {
    SetLevel(f32),
    SetMuted(bool),
    ToggleMute,
    Refresh,
}

pub struct Audio {
    pub commands: Sender<Cmd>,
}

#[implement(IAudioEndpointVolumeCallback)]
struct VolCb {
    notify: Box<dyn Fn() + Send + Sync>,
}

impl IAudioEndpointVolumeCallback_Impl for VolCb_Impl {
    fn OnNotify(&self, pnotify: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> windows::core::Result<()> {
        if !pnotify.is_null() {
            let d = unsafe { &*pnotify };
            LEVEL.store(d.fMasterVolume.to_bits(), Ordering::Relaxed);
            MUTED.store(d.bMuted.as_bool(), Ordering::Relaxed);
            (self.notify)();
        }
        Ok(())
    }
}

#[implement(IMMNotificationClient)]
struct DevCb {
    tx: Sender<Cmd>,
}

impl IMMNotificationClient_Impl for DevCb_Impl {
    fn OnDeviceStateChanged(&self, _id: &PCWSTR, _state: DEVICE_STATE) -> windows::core::Result<()> {
        Ok(())
    }
    fn OnDeviceAdded(&self, _id: &PCWSTR) -> windows::core::Result<()> {
        Ok(())
    }
    fn OnDeviceRemoved(&self, _id: &PCWSTR) -> windows::core::Result<()> {
        Ok(())
    }
    fn OnDefaultDeviceChanged(&self, flow: EDataFlow, role: ERole, _id: &PCWSTR) -> windows::core::Result<()> {
        if flow == eRender && role == eConsole {
            let _ = self.tx.send(Cmd::Refresh);
        }
        Ok(())
    }
    fn OnPropertyValueChanged(&self, _id: &PCWSTR, _key: &PROPERTYKEY) -> windows::core::Result<()> {
        Ok(())
    }
}

struct Attached {
    ep: IAudioEndpointVolume,
    cb: IAudioEndpointVolumeCallback,
}

impl Drop for Attached {
    fn drop(&mut self) {
        unsafe {
            let _ = self.ep.UnregisterControlChangeNotify(&self.cb);
        }
    }
}

fn attach(enumerator: &IMMDeviceEnumerator, notify: &std::sync::Arc<dyn Fn() + Send + Sync>) -> Option<Attached> {
    unsafe {
        let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole).ok()?;
        if let (Some(n), Ok(mut g)) = (read_name(&device), NAME.lock()) {
            *g = n;
        }
        let ep: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None).ok()?;
        let n = notify.clone();
        let cb: IAudioEndpointVolumeCallback = VolCb { notify: Box::new(move || n()) }.into();
        ep.RegisterControlChangeNotify(&cb).ok()?;
        if let Ok(v) = ep.GetMasterVolumeLevelScalar() {
            LEVEL.store(v.to_bits(), Ordering::Relaxed);
        }
        if let Ok(m) = ep.GetMute() {
            MUTED.store(m.as_bool(), Ordering::Relaxed);
        }
        Some(Attached { ep, cb })
    }
}

/// `notify` dipanggil dari thread COM mana pun saat volume/mute berubah; pemanggil sebaiknya hanya memposting pesan.
pub fn spawn(notify: impl Fn() + Send + Sync + 'static) -> Audio {
    let (tx, rx) = channel::<Cmd>();
    let tx_dev = tx.clone();
    let notify: std::sync::Arc<dyn Fn() + Send + Sync> = std::sync::Arc::new(notify);
    std::thread::spawn(move || unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let Ok(enumerator) = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL) else {
            crate::log::line("audio", "enumerator gagal");
            return;
        };
        let dev_cb: IMMNotificationClient = DevCb { tx: tx_dev }.into();
        let _ = enumerator.RegisterEndpointNotificationCallback(&dev_cb);
        let mut at = attach(&enumerator, &notify);
        AVAILABLE.store(at.is_some(), Ordering::Relaxed);
        notify();
        while let Ok(cmd) = rx.recv() {
            match cmd {
                Cmd::Refresh => {
                    drop(at.take());
                    at = attach(&enumerator, &notify);
                    AVAILABLE.store(at.is_some(), Ordering::Relaxed);
                    notify();
                }
                Cmd::SetLevel(v) => {
                    if let Some(a) = &at {
                        let v = v.clamp(0.0, 1.0);
                        let _ = a.ep.SetMasterVolumeLevelScalar(v, std::ptr::null());
                        if v > 0.0 && MUTED.load(Ordering::Relaxed) {
                            let _ = a.ep.SetMute(false, std::ptr::null());
                        }
                    }
                }
                Cmd::SetMuted(m) => {
                    if let Some(a) = &at {
                        let _ = a.ep.SetMute(m, std::ptr::null());
                    }
                }
                Cmd::ToggleMute => {
                    if let Some(a) = &at {
                        let now = a.ep.GetMute().map(|m| m.as_bool()).unwrap_or(false);
                        let _ = a.ep.SetMute(!now, std::ptr::null());
                    }
                }
            }
        }
    });
    Audio { commands: tx }
}

