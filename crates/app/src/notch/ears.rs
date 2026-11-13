//! "Telinga" cekung yang menyatukan notch dengan tepi atas layar (seperti notch).
//! Dibuat sebagai bitmap kecil beranti-alias (supersampling), bukan geometri path, agar tidak butuh Win2D.

use nacre_platform::icons::Bitmap;

/// Persegi `e`Ã—`e` berisi warna `rgb` (alfa `alpha`) di luar seperempat lingkaran berjari-jari `e`.
/// `left`: telinga di kiri panel (pusat lingkaran di kiri-bawah persegi); kalau tidak, cermin.
pub fn bitmap(e: u32, rgb: (u8, u8, u8), alpha: u8, left: bool) -> Bitmap {
    let n = e as usize;
    let mut bgra = vec![0u8; n * n * 4];
    const SS: usize = 4;
    let r = e as f32;
    let cx = if left { 0.0 } else { r };
    let cy = r;
    for y in 0..n {
        for x in 0..n {
            let mut hit = 0;
            for sy in 0..SS {
                for sx in 0..SS {
                    let px = x as f32 + (sx as f32 + 0.5) / SS as f32;
                    let py = y as f32 + (sy as f32 + 0.5) / SS as f32;
                    let d = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
                    if d > r {
                        hit += 1;
                    }
                }
            }
            let cov = hit as f32 / (SS * SS) as f32;
            let a = alpha as f32 / 255.0 * cov;
            let o = (y * n + x) * 4;
            // ter-premultiply
            bgra[o] = (rgb.2 as f32 * a).round() as u8;
            bgra[o + 1] = (rgb.1 as f32 * a).round() as u8;
            bgra[o + 2] = (rgb.0 as f32 * a).round() as u8;
            bgra[o + 3] = (255.0 * a).round() as u8;
        }
    }
    Bitmap { w: e, h: e, bgra }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha(b: &Bitmap, x: u32, y: u32) -> u8 {
        b.bgra[((y * b.w + x) * 4 + 3) as usize]
    }

    #[test]
    fn left_ear_is_filled_at_the_top_and_open_at_the_bottom_left() {
        let b = bitmap(16, (10, 10, 12), 240, true);
        assert!(alpha(&b, 15, 0) >= 235, "sudut atas dekat panel terisi penuh");
        assert!(alpha(&b, 15, 8) >= 235, "tepi panel di tengah tinggi terisi penuh");
        assert!(alpha(&b, 15, 15) < 200, "ujung bawah busur adalah batas lengkung (tidak terisi penuh)");
        assert_eq!(alpha(&b, 0, 15), 0, "pusat lengkung kosong (ke arah luar)");
        assert!(alpha(&b, 2, 12) < 40);
    }

    #[test]
    fn right_ear_mirrors_the_left_one() {
        let l = bitmap(16, (10, 10, 12), 240, true);
        let r = bitmap(16, (10, 10, 12), 240, false);
        for y in 0..16 {
            for x in 0..16 {
                let d = alpha(&l, x, y) as i32 - alpha(&r, 15 - x, y) as i32;
                assert!(d.abs() <= 2, "({x},{y})");
            }
        }
    }

    #[test]
    fn pixels_are_premultiplied() {
        let b = bitmap(8, (200, 100, 50), 128, true);
        for px in b.bgra.chunks(4) {
            assert!(px[0] <= px[3] && px[1] <= px[3] && px[2] <= px[3]);
        }
    }
}

