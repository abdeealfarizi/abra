//! Ikon tempat sampah dock gaya macOS Tahoe, digambar sendiri (bukan berkas Apple): tabung kaca buram yang sedikit
//! meruncing ke bawah dengan alur vertikal, bibir elips di atas, dan bagian dalam gelap. Versi berisi menampilkan
//! gumpalan kertas yang menyembul dari bibir. Keluaran BGRA premultiplied `n`×`n` tanpa bayangan (bayangan ditambah
//! `iconplate::free`).

const TOP: f32 = 0.215;
const BOT: f32 = 0.905;
/// Setengah lebar badan di atas dan di bawah.
const HW_TOP: f32 = 0.300;
const HW_BOT: f32 = 0.250;
/// Radius sudut bawah badan.
const CORNER: f32 = 0.07;
/// Jari-jari bibir (luar) dan mulut (dalam).
const RIM: (f32, f32) = (0.318, 0.078);
const MOUTH: (f32, f32) = (0.272, 0.054);

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn half_width(v: f32) -> f32 {
    lerp(HW_TOP, HW_BOT, ((v - TOP) / (BOT - TOP)).clamp(0.0, 1.0))
}

fn in_ellipse(u: f32, v: f32, c: (f32, f32), r: (f32, f32)) -> bool {
    let (dx, dy) = ((u - c.0) / r.0, (v - c.1) / r.1);
    dx * dx + dy * dy <= 1.0
}

fn in_body(u: f32, v: f32) -> bool {
    if v < TOP || v > BOT {
        return false;
    }
    let hw = half_width(v);
    let dx = (u - 0.5).abs();
    if dx > hw {
        return false;
    }
    if v > BOT - CORNER && dx > hw - CORNER {
        let (cx, cy) = (hw - CORNER, BOT - CORNER);
        return (dx - cx).powi(2) + (v - cy).powi(2) <= CORNER * CORNER;
    }
    true
}

/// Gumpalan kertas: (pusat, jari-jari).
/// Urutan = belakang ke depan.
const PAPERS: [((f32, f32), f32); 6] = [
    ((0.51, 0.125), 0.090),
    ((0.37, 0.170), 0.080),
    ((0.65, 0.165), 0.078),
    ((0.30, 0.215), 0.052),
    ((0.585, 0.205), 0.070),
    ((0.445, 0.210), 0.072),
];

/// Kertas di titik (u, v): kecerahan bila tertutup kertas.
fn paper(u: f32, v: f32) -> Option<f32> {
    // hanya terlihat di atas garis bibir atau di dalam mulut
    if !(in_ellipse(u, v, (0.5, TOP), MOUTH) || (v < TOP && (u - 0.5).abs() < MOUTH.0)) {
        return None;
    }
    // kertas remuk: tepi tak rata dan sisi bersegi dengan kecerahan berbeda; gumpalan depan menutupi yang belakang
    let mut hit: Option<f32> = None;
    for (i, (c, r)) in PAPERS.iter().enumerate() {
        let (dx, dy) = (u - c.0, v - c.1);
        let th = dy.atan2(dx);
        let fi = i as f32;
        let r_eff = r * (1.0 + 0.15 * (5.0 * th + fi).sin() + 0.07 * (11.0 * th + 2.0 * fi).sin());
        let d = (dx * dx + dy * dy).sqrt() / r_eff;
        if d > 1.0 {
            continue;
        }
        let seg = std::f32::consts::FRAC_PI_3;
        let a = (th + std::f32::consts::PI + fi * 0.7) / seg;
        let facet = a.floor() as i32;
        let h = ((facet * 7 + i as i32 * 13).rem_euclid(5)) as f32 / 4.0;
        // cahaya dari kiri atas + segi acak + garis lipatan di batas segi
        let lit = 0.5 - 0.5 * ((dx + dy) / r).clamp(-1.0, 1.0);
        let crease = if (a - a.floor() < 0.06 || a - a.floor() > 0.94) && d > 0.2 { 0.84 } else { 1.0 };
        let l = 255.0 * (0.74 + 0.12 * lit + 0.12 * h) * crease * (1.0 - 0.08 * d * d);
        hit = Some(l);
    }
    hit
}

/// Warna (r, g, b, a) lurus di titik (u, v), atau None bila transparan.
fn sample(u: f32, v: f32, full: bool) -> Option<[f32; 4]> {
    let rim = in_ellipse(u, v, (0.5, TOP), RIM);
    let mouth = in_ellipse(u, v, (0.5, TOP), MOUTH);
    let front = v >= TOP;
    // urutan lukis dari depan ke belakang
    if rim && !mouth && front {
        return Some([250.0, 251.0, 253.0, 250.0]);
    }
    if full {
        if let Some(l) = paper(u, v) {
            return Some([l, l, l * 1.01, 255.0]);
        }
    }
    if mouth {
        // dinding dalam: belakang lebih gelap, dekat bibir depan lebih terang
        let t = ((v - (TOP - MOUTH.1)) / (2.0 * MOUTH.1)).clamp(0.0, 1.0);
        let l = lerp(92.0, 150.0, t);
        return Some([l - 4.0, l - 2.0, l + 2.0, 245.0]);
    }
    if rim {
        return Some([236.0, 238.0, 242.0, 250.0]);
    }
    if in_body(u, v) {
        let hw = half_width(v);
        let s = ((u - 0.5) / hw).clamp(-1.0, 1.0);
        let t = (v - TOP) / (BOT - TOP);
        // kaca buram: atas terang, bawah sedikit abu, tepi tabung lebih gelap
        let mut l = lerp(244.0, 212.0, t) - 34.0 * s.abs().powi(3);
        // alur vertikal mengikuti lengkung tabung
        if v > TOP + 0.085 && v < BOT - 0.065 {
            for k in -3i32..=3 {
                let ang = k as f32 * 0.39;
                let gx = 0.5 + hw * 0.90 * ang.sin();
                let gw = 0.006 + 0.020 * ang.cos();
                if (u - gx).abs() < gw / 2.0 {
                    l -= 40.0;
                    break;
                }
            }
        }
        return Some([l - 5.0, l - 2.0, l + 1.0, 238.0]);
    }
    None
}

/// Ikon BGRA premultiplied `n`×`n` (4×4 sampel per piksel).
pub fn icon(n: u32, full: bool) -> Vec<u8> {
    let mut out = vec![0u8; (n * n * 4) as usize];
    const S: u32 = 4;
    let inv = 1.0 / (n as f32 * S as f32);
    for y in 0..n {
        for x in 0..n {
            let (mut r, mut g, mut b, mut a) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            for sy in 0..S {
                for sx in 0..S {
                    let u = ((x * S + sx) as f32 + 0.5) * inv;
                    let v = ((y * S + sy) as f32 + 0.5) * inv;
                    if let Some([cr, cg, cb, ca]) = sample(u, v, full) {
                        let al = ca / 255.0;
                        r += cr * al;
                        g += cg * al;
                        b += cb * al;
                        a += ca;
                    }
                }
            }
            let k = (S * S) as f32;
            let i = ((y * n + x) * 4) as usize;
            out[i] = (b / k).round().clamp(0.0, 255.0) as u8;
            out[i + 1] = (g / k).round().clamp(0.0, 255.0) as u8;
            out[i + 2] = (r / k).round().clamp(0.0, 255.0) as u8;
            out[i + 3] = (a / k).round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha(b: &[u8], n: u32, x: u32, y: u32) -> u8 {
        b[((y * n + x) * 4 + 3) as usize]
    }

    #[test]
    fn body_is_opaque_and_corners_transparent() {
        let n = 64;
        let b = icon(n, false);
        assert_eq!(alpha(&b, n, 0, 0), 0);
        assert_eq!(alpha(&b, n, n - 1, n - 1), 0);
        assert!(alpha(&b, n, n / 2, n * 3 / 5) > 200);
        // premultiplied: warna tidak melebihi alfa
        assert!(b.chunks_exact(4).all(|p| p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3]));
    }

    #[test]
    fn full_bin_shows_paper_above_the_rim() {
        let n = 64;
        let (e, f) = (icon(n, false), icon(n, true));
        let (x, y) = (n / 2, (0.12 * n as f32) as u32);
        assert_eq!(alpha(&e, n, x, y), 0, "kosong: di atas bibir transparan");
        assert!(alpha(&f, n, x, y) > 200, "berisi: kertas menyembul");
        assert_ne!(e, f);
    }
}
