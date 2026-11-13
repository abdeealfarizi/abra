//! Tangkap audio keluaran sistem (WASAPI loopback) untuk visualizer. Hanya hidup selama dipanggil `start`;
//! berhenti total (thread, handle COM, perangkat) saat `Loopback` dilepas, yaitu saat musik tidak main.

use nacre_core::island::bands::{Analyzer, BANDS};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use windows::core::PCWSTR;
use windows::Win32::Media::Audio::{
    eConsole, eRender, IAudioCaptureClient, IAudioClient, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
    AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK, WAVEFORMATEX, WAVEFORMATEXTENSIBLE,
};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED};

/// Pita terbaru (bit f32), dibaca thread UI setelah `notify`.
static OUT: [AtomicU32; BANDS] = [const { AtomicU32::new(0) }; BANDS];
static PENDING: AtomicBool = AtomicBool::new(false);

pub fn latest() -> [f32; BANDS] {
    PENDING.store(false, Ordering::Relaxed);
    let mut o = [0.0; BANDS];
    for (i, v) in OUT.iter().enumerate() {
        o[i] = f32::from_bits(v.load(Ordering::Relaxed));
    }
    o
}

pub struct Loopback {
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Loopback {
    fn drop(&mut self) {
        // Hanya beri tanda berhenti; thread keluar sendiri (≤100 ms). Dulu thread UI menunggu (join) di sini, dan bila
        // driver audio sedang tersendat (mis. headphone Bluetooth tersambung ulang) seluruh ABRA ikut membeku.
        self.stop.store(true, Ordering::Relaxed);
        drop(self.handle.take());
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Sample {
    F32,
    I16,
    I32,
}

unsafe fn sample_kind(fmt: *const WAVEFORMATEX) -> Option<Sample> {
    let f = &*fmt;
    let tag = f.wFormatTag;
    let bits = f.wBitsPerSample;
    match (tag, bits) {
        (3, 32) => Some(Sample::F32),
        (1, 16) => Some(Sample::I16),
        (1, 32) => Some(Sample::I32),
        // WAVE_FORMAT_EXTENSIBLE: subformat menentukan float atau PCM
        (0xFFFE, _) if f.cbSize >= 22 => {
            let ext = &*(fmt as *const WAVEFORMATEXTENSIBLE);
            let sub = ext.SubFormat.data1;
            match (sub, bits) {
                (3, 32) => Some(Sample::F32),
                (1, 16) => Some(Sample::I16),
                (1, 32) => Some(Sample::I32),
                _ => None,
            }
        }
        _ => None,
    }
}

fn device_id(d: &IMMDevice) -> String {
    unsafe {
        match d.GetId() {
            Ok(id) => {
                let s = PCWSTR(id.0).to_string().unwrap_or_default();
                CoTaskMemFree(Some(id.0 as *const _));
                s
            }
            Err(_) => String::new(),
        }
    }
}

/// Satu sesi tangkap pada perangkat keluaran bawaan. `Err` = bangun ulang (perangkat berganti/gagal).
fn session(stop: &AtomicBool, notify: &(dyn Fn() + Send), analyzer: &mut Analyzer) -> Result<(), &'static str> {
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|_| "enumerator")?;
        let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole).map_err(|_| "tidak ada perangkat keluaran")?;
        let id = device_id(&device);
        let client: IAudioClient = device.Activate(CLSCTX_ALL, None).map_err(|_| "activate")?;
        let fmt = client.GetMixFormat().map_err(|_| "mixformat")?;
        let channels = (*fmt).nChannels as usize;
        let kind = sample_kind(fmt);
        let init = client.Initialize(AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK, 10_000_000, 0, fmt, None);
        let bytes_per = ((*fmt).wBitsPerSample / 8) as usize;
        CoTaskMemFree(Some(fmt as *const _));
        init.map_err(|_| "initialize")?;
        let (Some(kind), true) = (kind, channels > 0 && bytes_per > 0) else { return Err("format tidak didukung") };
        let capture: IAudioCaptureClient = client.GetService().map_err(|_| "service")?;
        client.Start().map_err(|_| "start")?;

        let mut mono: Vec<f32> = Vec::with_capacity(4096);
        let mut last_check = Instant::now();
        while !stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(32));
            if last_check.elapsed() >= Duration::from_secs(2) {
                last_check = Instant::now();
                // perangkat bawaan berganti → bangun ulang sesi
                if let Ok(d) = enumerator.GetDefaultAudioEndpoint(eRender, eConsole) {
                    if device_id(&d) != id {
                        let _ = client.Stop();
                        return Err("perangkat berganti");
                    }
                }
            }
            loop {
                let n = capture.GetNextPacketSize().map_err(|_| "perangkat tidak valid")?;
                if n == 0 {
                    break;
                }
                let (mut data, mut frames, mut flags) = (std::ptr::null_mut::<u8>(), 0u32, 0u32);
                capture.GetBuffer(&mut data, &mut frames, &mut flags, None, None).map_err(|_| "perangkat tidak valid")?;
                mono.clear();
                let silent = flags & 0x2 != 0;
                for f in 0..frames as usize {
                    let mut acc = 0.0f32;
                    if !silent && !data.is_null() {
                        for ch in 0..channels {
                            let p = data.add((f * channels + ch) * bytes_per);
                            acc += match kind {
                                Sample::F32 => (p as *const f32).read_unaligned(),
                                Sample::I16 => (p as *const i16).read_unaligned() as f32 / 32768.0,
                                Sample::I32 => (p as *const i32).read_unaligned() as f32 / 2_147_483_648.0,
                            };
                        }
                    }
                    mono.push(acc / channels as f32);
                }
                let _ = capture.ReleaseBuffer(frames);
                analyzer.push(&mono, |b| {
                    for (i, v) in b.iter().enumerate() {
                        OUT[i].store(v.to_bits(), Ordering::Relaxed);
                    }
                    // satu pesan tertunda sudah cukup; UI membaca nilai terbaru
                    if !PENDING.swap(true, Ordering::Relaxed) {
                        notify();
                    }
                });
            }
        }
        let _ = client.Stop();
        Ok(())
    }
}

pub fn start(notify: impl Fn() + Send + 'static) -> Loopback {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let handle = std::thread::spawn(move || {
        let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).is_ok() };
        let mut analyzer = Analyzer::new();
        while !flag.load(Ordering::Relaxed) {
            if let Err(why) = session(&flag, &notify, &mut analyzer) {
                crate::log::line("loopback", why);
                // beri Windows waktu memperbarui endpoint sebelum mencoba lagi
                for _ in 0..25 {
                    if flag.load(Ordering::Relaxed) {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        }
        if com {
            unsafe { windows::Win32::System::Com::CoUninitialize() };
        }
    });
    Loopback { stop, handle: Some(handle) }
}
