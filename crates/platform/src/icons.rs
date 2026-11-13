//! Ikon aplikasi dari shell sebagai piksel BGRA ter-premultiply (siap diunggah ke Composition).
//! Pemuatan jalan di thread pekerja agar thread UI tidak tersendat oleh disk/shell.

use std::sync::mpsc::{channel, Receiver, Sender};
use windows::core::HSTRING;
use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{
    DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, HGDIOBJ,
};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};
use windows::Win32::UI::Shell::{IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY, SIIGBF_RESIZETOFIT};

#[derive(Clone)]
pub struct Bitmap {
    pub w: u32,
    pub h: u32,
    /// BGRA, baris dari atas ke bawah, alfa ter-premultiply.
    pub bgra: Vec<u8>,
}

/// Muat ikon `px`×`px`. Kunci internal host UWP (`path?judul`) diabaikan; hasilnya ikon exe-nya.
/// Thread pemanggil harus sudah menginisialisasi COM.
/// Susutkan ikon yang lebih besar dari `px` ke ukuran `px` dengan filter area-rata-rata (kotak berbobot pecahan).
/// Windows sering mengembalikan ikon 128/256 px walau diminta lebih kecil; bila langsung dirender kecil oleh
/// compositor (penyusutan >2× tanpa penyaringan), pinggirnya bergerigi.
pub fn fit(b: Bitmap, px: u32) -> Bitmap {
    if b.w <= px && b.h <= px {
        return b;
    }
    let k = (px as f32 / b.w.max(b.h) as f32).min(1.0);
    let (ow, oh) = (((b.w as f32 * k).round() as u32).max(1), ((b.h as f32 * k).round() as u32).max(1));
    let (sx, sy) = (b.w as f32 / ow as f32, b.h as f32 / oh as f32);
    let mut out = vec![0u8; (ow * oh * 4) as usize];
    for y in 0..oh {
        let (y0, y1) = (y as f32 * sy, (y + 1) as f32 * sy);
        for x in 0..ow {
            let (x0, x1) = (x as f32 * sx, (x + 1) as f32 * sx);
            let mut acc = [0f32; 4];
            let mut wsum = 0f32;
            let mut yy = y0.floor() as u32;
            while (yy as f32) < y1 && yy < b.h {
                let wy = ((yy + 1) as f32).min(y1) - (yy as f32).max(y0);
                let mut xx = x0.floor() as u32;
                while (xx as f32) < x1 && xx < b.w {
                    let wx = ((xx + 1) as f32).min(x1) - (xx as f32).max(x0);
                    let w = wx * wy;
                    let i = ((yy * b.w + xx) * 4) as usize;
                    for c in 0..4 {
                        acc[c] += b.bgra[i + c] as f32 * w;
                    }
                    wsum += w;
                    xx += 1;
                }
                yy += 1;
            }
            let o = ((y * ow + x) * 4) as usize;
            for c in 0..4 {
                out[o + c] = (acc[c] / wsum.max(1e-6)).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    Bitmap { w: ow, h: oh, bgra: out }
}

static OVERRIDES: std::sync::Mutex<Option<std::collections::HashMap<String, String>>> = std::sync::Mutex::new(None);

fn key(path: &str) -> String {
    path.replace('/', "\\").to_lowercase()
}

/// Ikon kustom untuk `path` (berkas gambar), atau `None` untuk mengembalikan ikon asli.
pub fn set_override(path: &str, image: Option<String>) {
    let mut g = OVERRIDES.lock().unwrap();
    let m = g.get_or_insert_with(Default::default);
    match image {
        Some(i) => {
            m.insert(key(path), i);
        }
        None => {
            m.remove(&key(path));
        }
    }
}

pub fn override_for(path: &str) -> Option<String> {
    OVERRIDES.lock().unwrap().as_ref().and_then(|m| m.get(&key(path)).cloned())
}

/// Apakah ikon `path` berasal dari pilihan pengguna atau paket ikon (sudah berbentuk jadi: tanpa pelat/gaya tambahan).
pub fn is_themed(path: &str) -> bool {
    override_for(path).is_some() || crate::iconpack::lookup(path).is_some()
}

pub fn load(path: &str, px: i32) -> Option<Bitmap> {
    // urutan: ikon pilihan pengguna → paket ikon → ikon asli dari shell
    if let Some(img) = override_for(path) {
        if let Some(b) = crate::wic::decode_file_fit(&img, px as u32) {
            return Some(b);
        }
    }
    if let Some(img) = crate::iconpack::lookup(path) {
        if let Some(b) = crate::wic::decode_file_fit(&img, px as u32) {
            return Some(b);
        }
    }
    load_shell(path, px)
}

/// Thumbnail isi berkas (foto, dokumen, video) muat dalam kotak `px`×`px`, rasio aspek dipertahankan; berkas tanpa
/// thumbnail mendapat ikonnya. Thread pemanggil harus sudah menginisialisasi COM.
pub fn thumbnail(path: &str, px: i32) -> Option<Bitmap> {
    unsafe {
        let factory: IShellItemImageFactory = SHCreateItemFromParsingName(&HSTRING::from(path), None).ok()?;
        let hbmp = factory.GetImage(SIZE { cx: px, cy: px }, SIIGBF_RESIZETOFIT | SIIGBF_BIGGERSIZEOK).ok()?;
        let mut bm = BITMAP::default();
        let got = GetObjectW(HGDIOBJ(hbmp.0), std::mem::size_of::<BITMAP>() as i32, Some(&mut bm as *mut _ as *mut _));
        if got == 0 || bm.bmWidth <= 0 || bm.bmHeight <= 0 {
            let _ = DeleteObject(HGDIOBJ(hbmp.0));
            return None;
        }
        let (w, h) = (bm.bmWidth as u32, bm.bmHeight as u32);
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                biHeight: -(h as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut buf = vec![0u8; (w * h * 4) as usize];
        let dc = GetDC(None);
        let lines = GetDIBits(dc, hbmp, 0, h, Some(buf.as_mut_ptr() as *mut _), &mut info, DIB_RGB_COLORS);
        ReleaseDC(None, dc);
        let _ = DeleteObject(HGDIOBJ(hbmp.0));
        if lines == 0 {
            return None;
        }
        // Foto/JPEG datang tanpa alfa (semua 0): jadikan opak agar tidak tak terlihat.
        if buf.chunks_exact(4).all(|p| p[3] == 0) {
            for p in buf.chunks_exact_mut(4) {
                p[3] = 255;
            }
        }
        Some(Bitmap { w, h, bgra: buf })
    }
}

fn load_shell(path: &str, px: i32) -> Option<Bitmap> {
    let path = path.split('?').next().unwrap_or(path);
    unsafe {
        let factory: IShellItemImageFactory = SHCreateItemFromParsingName(&HSTRING::from(path), None).ok()?;
        // Minta ukuran lebih besar (hingga 256) lalu susutkan sendiri dengan filter area: ikon yang dirender shell pada
        // ukuran kecil sering bertepi bergerigi, sedangkan versi besar lalu disusutkan menghasilkan tepi halus.
        let req = (px * 2).clamp(128, 256);
        let hbmp = factory.GetImage(SIZE { cx: req, cy: req }, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK).ok()?;

        let mut bm = BITMAP::default();
        let got = GetObjectW(HGDIOBJ(hbmp.0), std::mem::size_of::<BITMAP>() as i32, Some(&mut bm as *mut _ as *mut _));
        if got == 0 || bm.bmWidth <= 0 || bm.bmHeight <= 0 {
            let _ = DeleteObject(HGDIOBJ(hbmp.0));
            return None;
        }
        let (w, h) = (bm.bmWidth as u32, bm.bmHeight as u32);
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                biHeight: -(h as i32), // negatif = dari atas ke bawah
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut buf = vec![0u8; (w * h * 4) as usize];
        let dc = GetDC(None);
        let lines = GetDIBits(dc, hbmp, 0, h, Some(buf.as_mut_ptr() as *mut _), &mut info, DIB_RGB_COLORS);
        ReleaseDC(None, dc);
        let _ = DeleteObject(HGDIOBJ(hbmp.0));
        if lines == 0 {
            return None;
        }
        Some(fit(Bitmap { w, h, bgra: buf }, px.max(1) as u32))
    }
}

/// Pekerja ikon: kirim path, terima `(path, bitmap)`. `notify` dipanggil (dari thread pekerja)
/// setelah hasil masuk antrean, mis. memposting pesan ke thread UI.
pub struct Worker {
    pub requests: Sender<String>,
    pub results: Receiver<(String, Option<Bitmap>)>,
}

pub fn spawn_worker(px: i32, notify: impl Fn() + Send + 'static) -> Worker {
    let (req_tx, req_rx) = channel::<String>();
    let (res_tx, res_rx) = channel();
    std::thread::spawn(move || {
        let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).is_ok() };
        while let Ok(path) = req_rx.recv() {
            let bmp = load(&path, px);
            if res_tx.send((path, bmp)).is_err() {
                break;
            }
            notify();
        }
        if com {
            unsafe { CoUninitialize() };
        }
    });
    Worker { requests: req_tx, results: res_rx }
}
