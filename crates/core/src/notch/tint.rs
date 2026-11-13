//! Warna dari sampul album untuk visualizer dan cahaya di notch: rona yang paling dominan di antara piksel yang cukup
//! jenuh dan terang, lalu dicerahkan agar terbaca di atas notch gelap. Sampul abu-abu/hitam-putih tidak punya warna
//! (`None`), dan app memakai warna aksen ABRA.

const BUCKETS: usize = 12;

fn hsv(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d <= f32::EPSILON {
        0.0
    } else if max == r {
        60.0 * (((g - b) / d).rem_euclid(6.0))
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (h, if max > 0.0 { d / max } else { 0.0 }, max)
}

/// `bgra` = piksel w×h (BGRA). Mengambil paling banyak ±4096 sampel.
pub fn from_art(bgra: &[u8], w: u32, h: u32) -> Option<[u8; 3]> {
    let n = (w as usize) * (h as usize);
    if n == 0 || bgra.len() < n * 4 {
        return None;
    }
    let step = ((n as f32 / 4096.0).sqrt().ceil() as usize).max(1);
    let mut weight = [0f32; BUCKETS];
    let mut sum = [[0f32; 3]; BUCKETS];
    let mut total = 0f32;
    for y in (0..h as usize).step_by(step) {
        for x in (0..w as usize).step_by(step) {
            let i = (y * w as usize + x) * 4;
            let (b, g, r, a) = (bgra[i] as f32, bgra[i + 1] as f32, bgra[i + 2] as f32, bgra[i + 3] as f32);
            if a < 128.0 {
                continue;
            }
            total += 1.0;
            let (hue, s, v) = hsv(r / 255.0, g / 255.0, b / 255.0);
            if s < 0.28 || v < 0.22 {
                continue;
            }
            let wgt = s * s * v;
            let k = ((hue / 360.0 * BUCKETS as f32) as usize).min(BUCKETS - 1);
            weight[k] += wgt;
            sum[k][0] += r * wgt;
            sum[k][1] += g * wgt;
            sum[k][2] += b * wgt;
        }
    }
    let (k, best) = weight.iter().copied().enumerate().max_by(|a, b| a.1.total_cmp(&b.1))?;
    // terlalu sedikit piksel berwarna: anggap sampul tak berwarna
    if total == 0.0 || best / total < 0.04 {
        return None;
    }
    let c = [sum[k][0] / best, sum[k][1] / best, sum[k][2] / best];
    // cerahkan: kanal terbesar menjadi 235, rona dipertahankan
    let max = c[0].max(c[1]).max(c[2]).max(1.0);
    let f = 235.0 / max;
    Some([(c[0] * f).round().min(255.0) as u8, (c[1] * f).round().min(255.0) as u8, (c[2] * f).round().min(255.0) as u8])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 3]) -> Vec<u8> {
        let mut v = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                let [r, g, b] = f(x, y);
                v.extend_from_slice(&[b, g, r, 255]);
            }
        }
        v
    }

    #[test]
    fn picks_the_dominant_hue_and_brightens_it() {
        let red = img(64, 64, |_, _| [150, 20, 30]);
        let c = from_art(&red, 64, 64).unwrap();
        assert_eq!(c[0], 235);
        assert!(c[1] < 60 && c[2] < 60);
        // sebagian besar biru tua dengan bintik oranye: biru menang
        let mixed = img(64, 64, |x, y| if x < 8 && y < 8 { [255, 140, 0] } else { [20, 40, 160] });
        let c = from_art(&mixed, 64, 64).unwrap();
        assert!(c[2] > c[0] && c[2] == 235);
    }

    #[test]
    fn grey_or_dark_art_has_no_colour() {
        assert_eq!(from_art(&img(32, 32, |x, _| { let v = (x * 8) as u8; [v, v, v] }), 32, 32), None);
        assert_eq!(from_art(&img(32, 32, |_, _| [10, 5, 30]), 32, 32), None, "terlalu gelap");
        assert_eq!(from_art(&[], 0, 0), None);
    }

    #[test]
    fn large_art_is_sampled_not_scanned_fully() {
        let big = img(600, 600, |_, _| [30, 160, 90]);
        let c = from_art(&big, 600, 600).unwrap();
        assert_eq!(c[1], 235);
    }
}
