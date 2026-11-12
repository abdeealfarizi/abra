//! Teks â†’ piksel BGRA ter-premultiply lewat GDI (anti-alias skala abu-abu), tanpa DirectWrite/Direct2D.
//! Dipilih demi memori: tidak memuat d2d1/dwrite; hasil teks diunggah sebagai permukaan kecil dan di-cache pemanggil.
//! Warna dibake putih; warna lain dicapai dengan `CompositionColorBrush` sebagai mask atau opasitas visual.

use crate::icons::Bitmap;
use windows::core::HSTRING;
use windows::Win32::Foundation::{COLORREF, RECT, SIZE};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject, DrawTextW, GetTextExtentPoint32W,
    SelectObject, SetBkMode, SetTextColor, ANTIALIASED_QUALITY, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CLIP_DEFAULT_PRECIS,
    DEFAULT_CHARSET, DIB_RGB_COLORS, DT_END_ELLIPSIS, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, FF_DONTCARE, HGDIOBJ,
    OUT_DEFAULT_PRECIS, TRANSPARENT, VARIABLE_PITCH,
};

/// Font ikon (glyph tombol media, dsb.) bawaan Windows 11.
pub const ICON_FACE: &str = "Segoe Fluent Icons";

/// Font teks ala macOS: SF Pro bila pengguna memasangnya sendiri, lalu Inter (dimuat privat dari folder `fonts` di samping
/// `abra.exe` bila ada, atau terpasang di sistem), terakhir Segoe UI Variable bawaan Windows. Dipilih sekali saat pertama dipakai.
pub fn text_face() -> &'static str {
    static FACE: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();
    FACE.get_or_init(|| {
        load_bundled_fonts();
        for cand in ["SF Pro Text", "SF Pro Display", "Inter", "Inter Variable"] {
            if font_exists(cand) {
                return cand;
            }
        }
        "Segoe UI Variable Text"
    })
}

fn load_bundled_fonts() {
    use windows::core::PCWSTR;
    use windows::Win32::Graphics::Gdi::{AddFontResourceExW, FONT_RESOURCE_CHARACTERISTICS};
    let Ok(exe) = std::env::current_exe() else { return };
    let Some(dir) = exe.parent().map(|d| d.join("fonts")) else { return };
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let ok = p.extension().and_then(|x| x.to_str()).map(|x| x.eq_ignore_ascii_case("ttf") || x.eq_ignore_ascii_case("otf")).unwrap_or(false);
        if ok {
            let w: Vec<u16> = p.to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect();
            // FR_PRIVATE (0x10): hanya untuk proses ini, tidak dipasang ke sistem
            unsafe {
                AddFontResourceExW(PCWSTR(w.as_ptr()), FONT_RESOURCE_CHARACTERISTICS(0x10), None);
            }
        }
    }
}

fn font_exists(face: &str) -> bool {
    use windows::Win32::Foundation::LPARAM;
    use windows::Win32::Graphics::Gdi::{EnumFontFamiliesExW, LOGFONTW, TEXTMETRICW};
    unsafe extern "system" fn found(_: *const LOGFONTW, _: *const TEXTMETRICW, _: u32, lp: LPARAM) -> i32 {
        *(lp.0 as *mut bool) = true;
        0
    }
    unsafe {
        let dc = CreateCompatibleDC(None);
        let mut lf = LOGFONTW { lfCharSet: DEFAULT_CHARSET, ..Default::default() };
        for (i, c) in face.encode_utf16().take(31).enumerate() {
            lf.lfFaceName[i] = c;
        }
        let mut hit = false;
        EnumFontFamiliesExW(dc, &lf, Some(found), LPARAM(&mut hit as *mut bool as isize), 0);
        let _ = DeleteDC(dc);
        hit
    }
}

pub struct Style {
    /// Tinggi huruf dalam piksel perangkat.
    pub px: i32,
    /// 400 normal, 600 semi-tebal.
    pub weight: i32,
}

fn font(style: &Style, face: &str) -> windows::Win32::Graphics::Gdi::HFONT {
    unsafe {
        CreateFontW(
            -style.px,
            0,
            0,
            0,
            style.weight,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            ANTIALIASED_QUALITY,
            (VARIABLE_PITCH.0 | FF_DONTCARE.0) as u32,
            &HSTRING::from(face),
        )
    }
}

/// Lebar teks (px) sebelum dipotong; untuk menata kapsul tooltip.
pub fn measure(text: &str, style: &Style) -> (i32, i32) {
    unsafe {
        let dc = CreateCompatibleDC(None);
        let big = Style { px: style.px * SS, weight: style.weight };
        let f = font(&big, text_face());
        let old = SelectObject(dc, HGDIOBJ(f.0));
        let wide: Vec<u16> = text.encode_utf16().collect();
        let mut sz = SIZE::default();
        let _ = GetTextExtentPoint32W(dc, &wide, &mut sz);
        SelectObject(dc, old);
        let _ = DeleteObject(HGDIOBJ(f.0));
        let _ = DeleteDC(dc);
        ((sz.cx + SS - 1) / SS, (sz.cy + SS - 1) / SS)
    }
}

/// Gambar `text` putih pada kotak `w`Ã—`h`, satu baris, dipotong dengan elipsis bila melebihi `w`.
pub fn render(text: &str, style: &Style, w: i32, h: i32) -> Option<Bitmap> {
    render_face(text, style, text_face(), w, h)
}

/// Sama seperti `render` tetapi dengan font pilihan (mis. `ICON_FACE`).
pub fn render_face(text: &str, style: &Style, face: &str, w: i32, h: i32) -> Option<Bitmap> {
    if w <= 0 || h <= 0 || text.is_empty() {
        return None;
    }
    // Gambar pada 3× lalu perkecil dengan rata-rata area: bentuk huruf tidak "dipaksa" ke kisi piksel (tanpa hinting),
    // sehingga tebal garis dan lengkungannya halus seperti rendering teks macOS.
    let big = render_raw(text, &Style { px: style.px * SS, weight: style.weight }, face, w * SS, h * SS)?;
    let (w, h, s) = (w as usize, h as usize, SS as usize);
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let mut sum = 0u32;
            for dy in 0..s {
                let row = ((y * s + dy) * w * s + x * s) * 4;
                for dx in 0..s {
                    sum += big.bgra[row + dx * 4 + 3] as u32;
                }
            }
            let a = (sum / (s * s) as u32) as u8;
            out[(y * w + x) * 4..(y * w + x) * 4 + 4].copy_from_slice(&[a, a, a, a]);
        }
    }
    Some(Bitmap { w: w as u32, h: h as u32, bgra: out })
}

/// Faktor supersampling teks.
const SS: i32 = 3;

fn render_raw(text: &str, style: &Style, face: &str, w: i32, h: i32) -> Option<Bitmap> {
    if w <= 0 || h <= 0 || text.is_empty() {
        return None;
    }
    unsafe {
        let dc = CreateCompatibleDC(None);
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let dib = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
        if bits.is_null() {
            let _ = DeleteDC(dc);
            return None;
        }
        let old_bmp = SelectObject(dc, HGDIOBJ(dib.0));
        let f = font(style, face);
        let old_font = SelectObject(dc, HGDIOBJ(f.0));
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(dc, COLORREF(0x00FF_FFFF));
        let mut wide: Vec<u16> = text.encode_utf16().collect();
        let mut rc = RECT { left: 0, top: 0, right: w, bottom: h };
        DrawTextW(dc, &mut wide, &mut rc, DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX);

        let src = std::slice::from_raw_parts(bits as *const u8, (w * h * 4) as usize);
        let mut out = vec![0u8; src.len()];
        for (s, d) in src.chunks_exact(4).zip(out.chunks_exact_mut(4)) {
            // GDI tidak mengisi alfa; teks putih di atas hitam â†’ alfa = kecerahan, warna = putih ter-premultiply.
            let a = s[0].max(s[1]).max(s[2]);
            d.copy_from_slice(&[a, a, a, a]);
        }
        SelectObject(dc, old_font);
        SelectObject(dc, old_bmp);
        let _ = DeleteObject(HGDIOBJ(f.0));
        let _ = DeleteObject(HGDIOBJ(dib.0));
        let _ = DeleteDC(dc);
        Some(Bitmap { w: w as u32, h: h as u32, bgra: out })
    }
}

