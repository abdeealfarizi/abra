//! Bitmap bayangan untuk notch, dipakai sebagai kuas nine-grid (sudut tetap, sisi dan tengah direntang mengikuti ukuran
//! notch yang sedang beranimasi). Bagian dalamnya **berlubang** seluas badan notch: badan kaca tembus pandang, jadi
//! bayangan tidak boleh terlihat dari balik badan. Bayangan digeser `dy` ke bawah di dalam bitmap, sehingga tepat di
//! bawah badan ada pita gelap tipis yang lalu memudar.

pub struct Shade {
    pub w: u32,
    pub h: u32,
    /// Inset nine-grid kiri/atas/kanan/bawah (px).
    pub insets: (f32, f32, f32, f32),
    /// Lebar pudar di sekeliling badan (px): sprite = badan diperbesar `margin` ke tiap sisi, ditambah `dy` di bawah.
    pub margin: u32,
    pub dy: u32,
    /// BGRA hitam ter-premultiply (RGB 0).
    pub bgra: Vec<u8>,
}

/// Jarak bertanda ke persegi membulat [x0,x1]×[y0,y1] radius r (positif di luar).
fn sdf(px: f32, py: f32, x0: f32, y0: f32, x1: f32, y1: f32, r: f32) -> f32 {
    let cx = px.clamp(x0 + r, (x1 - r).max(x0 + r));
    let cy = py.clamp(y0 + r, (y1 - r).max(y0 + r));
    ((px - cx).powi(2) + (py - cy).powi(2)).sqrt() - r
}

/// `radius` = radius sudut badan, `blur` = jarak pudar, `dy` = geser bayangan ke bawah; semuanya px.
pub fn rounded(radius: f32, blur: f32, dy: f32) -> Shade {
    let r = radius.max(1.0).ceil();
    let m = blur.max(1.0).ceil();
    let dy = dy.max(0.0).round();
    let w = (2.0 * (m + r) + 2.0) as u32;
    let h = w + dy as u32;
    let (bx0, by0, bx1, by1) = (m, m, w as f32 - m, w as f32 - m);
    let mut bgra = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let body = sdf(px, py, bx0, by0, bx1, by1, r);
            // di dalam badan: kosong. Lubang dimulai 3,5 px di dalam tepi badan: tepi badan beranti-alias dan peregangan
            // nine-grid tidak boleh menyisakan garis terang di antara badan dan bayangan.
            let outside = (body + 3.5).clamp(0.0, 1.0);
            if outside <= 0.0 {
                continue;
            }
            // bayangan sedikit lebih ramping dari badan: kesan timbul datang dari bawah, sisi dan telinga hampir bersih
            // (radiusnya ikut mengecil agar sudutnya tetap bersarang di dalam sudut badan)
            let k = (0.45 * m).min(r - 1.0).max(0.0);
            let d = sdf(px, py, bx0 + k, by0 + dy, bx1 - k, by1 + dy, r - k);
            let a = if d <= 0.0 { 0.9 } else { 0.9 * (1.0 - (d / m).clamp(0.0, 1.0)).powi(3) };
            bgra[((y * w + x) * 4 + 3) as usize] = (a * outside * 255.0).round() as u8;
        }
    }
    let side = m + r;
    Shade { w, h, insets: (side, side, side, side + dy), margin: m as u32, dy: dy as u32, bgra }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha(s: &Shade, x: u32, y: u32) -> u8 {
        s.bgra[((y * s.w + x) * 4 + 3) as usize]
    }

    #[test]
    fn hollow_inside_dark_just_below_and_fading_out() {
        let s = rounded(15.0, 14.0, 3.0);
        let cx = s.w / 2;
        let body_bottom = s.w - s.margin; // y tepi bawah badan
        assert_eq!(alpha(&s, cx, s.w / 2), 0, "tidak terlihat dari balik badan kaca");
        assert!(alpha(&s, cx, body_bottom + 1) > 200, "pita gelap tepat di bawah badan");
        let fade = alpha(&s, cx, body_bottom + s.dy + s.margin / 2);
        assert!(fade > 0 && fade < 120, "memudar: {fade}");
        assert_eq!(alpha(&s, cx, s.h - 1), 0);
        assert_eq!(alpha(&s, 0, s.h / 2), 0);
        assert!(s.bgra.chunks_exact(4).all(|p| p[0] == 0 && p[1] == 0 && p[2] == 0));
    }

    #[test]
    fn insets_cover_corners_and_the_drop() {
        let s = rounded(28.0, 20.0, 5.0);
        assert_eq!((s.w, s.h), (98, 103));
        assert_eq!(s.insets, (48.0, 48.0, 48.0, 53.0));
        assert!(s.insets.0 + s.insets.2 < s.w as f32 && s.insets.1 + s.insets.3 < s.h as f32, "masih ada bagian tengah untuk direntang");
    }
}
