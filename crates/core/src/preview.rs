//! Gambar pratinjau kecil untuk Pengaturan → Tampilan, seperti di macOS Tahoe: mockup layar Auto/Terang/Gelap,
//! kotak contoh gaya ikon, dan lingkaran "Multiwarna". Semua BGRA ter-premultiply, murni dan diuji.

use crate::iconplate;
use crate::settings::IconStyle;

type P = (f32, f32);

/// Jarak bertanda ke persegi membulat berpusat `c`, setengah-ukuran `h`, radius `r`.
fn rrect(p: P, c: P, h: P, r: f32) -> f32 {
    let (qx, qy) = ((p.0 - c.0).abs() - (h.0 - r), (p.1 - c.1).abs() - (h.1 - r));
    (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r
}

/// Cakupan 0..1 dari jarak bertanda (tepi halus selebar 1 px).
fn cov(d: f32) -> f32 {
    (0.5 - d).clamp(0.0, 1.0)
}

/// Campur warna `top` (0..255, alfa 0..1) di atas `dst` (RGB lurus).
fn mix(dst: [f32; 3], top: [f32; 3], a: f32) -> [f32; 3] {
    [dst[0] + (top[0] - dst[0]) * a, dst[1] + (top[1] - dst[1]) * a, dst[2] + (top[2] - dst[2]) * a]
}

/// Wallpaper biru bergelombang ala Tahoe (versi gelap bila `dark`).
fn wallpaper(x: f32, y: f32, w: f32, h: f32, dark: bool) -> [f32; 3] {
    let (u, v) = (x / w, y / h);
    let t = (u * 0.55 + v * 0.45).clamp(0.0, 1.0);
    let (a, b) = if dark { ([18.0, 34.0, 92.0], [40.0, 96.0, 168.0]) } else { ([46.0, 110.0, 225.0], [130.0, 200.0, 245.0]) };
    let mut c = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
    // gelombang terang diagonal
    let wave = (v - (0.62 - 0.35 * u + 0.08 * (u * 9.0).sin())).abs();
    let glow = (1.0 - wave / 0.10).clamp(0.0, 1.0);
    c = mix(c, if dark { [90.0, 150.0, 220.0] } else { [235.0, 245.0, 255.0] }, glow * 0.55);
    c
}

/// Mockup layar untuk kartu Tampilan: `kind` 0 = Auto (kiri terang, kanan gelap), 1 = Terang, 2 = Gelap.
pub fn appearance(kind: u8, w: u32, h: u32) -> Vec<u8> {
    let (wf, hf) = (w as f32, h as f32);
    let mut out = vec![0u8; (w * h * 4) as usize];
    const SS: u32 = 2;
    for py in 0..h {
        for px in 0..w {
            let mut acc = [0.0f32; 4];
            for sy in 0..SS {
                for sx in 0..SS {
                    let (x, y) = (px as f32 + (sx as f32 + 0.5) / SS as f32, py as f32 + (sy as f32 + 0.5) / SS as f32);
                    // Auto: garis belah miring
                    let dark = match kind {
                        1 => false,
                        2 => true,
                        _ => x > wf * (0.5 + (y / hf - 0.5) * 0.35),
                    };
                    let outer = cov(rrect((x, y), (wf / 2.0, hf / 2.0), (wf / 2.0, hf / 2.0), hf * 0.14));
                    if outer <= 0.0 {
                        continue;
                    }
                    let mut c = wallpaper(x, y, wf, hf, dark);
                    // menu bar
                    if y < hf * 0.11 {
                        c = mix(c, if dark { [0.0, 0.0, 0.0] } else { [255.0, 255.0, 255.0] }, 0.45);
                    }
                    // jendela
                    let (wx0, wy0, ww, wh) = (wf * 0.20, hf * 0.26, wf * 0.66, hf * 0.62);
                    let wc = (wx0 + ww / 2.0, wy0 + wh / 2.0);
                    let wcov = cov(rrect((x, y), wc, (ww / 2.0, wh / 2.0), hf * 0.07));
                    if wcov > 0.0 {
                        let side = x < wx0 + ww * 0.30;
                        let fill = match (dark, side) {
                            (false, false) => [248.0, 248.0, 250.0],
                            (false, true) => [228.0, 230.0, 236.0],
                            (true, false) => [40.0, 40.0, 44.0],
                            (true, true) => [56.0, 56.0, 62.0],
                        };
                        let mut wcol = fill;
                        // tiga lampu jendela
                        for (i, l) in [[255.0, 95.0, 87.0], [254.0, 188.0, 46.0], [40.0, 200.0, 64.0]].iter().enumerate() {
                            let lc = (wx0 + ww * 0.07 + i as f32 * hf * 0.075, wy0 + wh * 0.13);
                            let dl = ((x - lc.0).powi(2) + (y - lc.1).powi(2)).sqrt() - hf * 0.028;
                            wcol = mix(wcol, *l, cov(dl));
                        }
                        // dua garis isi
                        for (k, len) in [(0.40, 0.42), (0.58, 0.30)] {
                            let ly = wy0 + wh * k;
                            let lx0 = wx0 + ww * 0.38;
                            let d = rrect((x, y), (lx0 + ww * len / 2.0, ly), (ww * len / 2.0, hf * 0.018), hf * 0.018);
                            wcol = mix(wcol, if dark { [110.0, 110.0, 118.0] } else { [196.0, 198.0, 206.0] }, cov(d));
                        }
                        c = mix(c, wcol, wcov);
                    }
                    acc[0] += c[0] * outer;
                    acc[1] += c[1] * outer;
                    acc[2] += c[2] * outer;
                    acc[3] += outer;
                }
            }
            let n = (SS * SS) as f32;
            let i = ((py * w + px) * 4) as usize;
            out[i] = (acc[2] / n).round().clamp(0.0, 255.0) as u8;
            out[i + 1] = (acc[1] / n).round().clamp(0.0, 255.0) as u8;
            out[i + 2] = (acc[0] / n).round().clamp(0.0, 255.0) as u8;
            out[i + 3] = (acc[3] / n * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

/// Kotak contoh gaya ikon `n`×`n`: wallpaper membulat dengan ikon Apps bergaya `style` di tengahnya.
pub fn icon_tile(style: IconStyle, tint: u8, dark: bool, n: u32) -> Vec<u8> {
    let nf = n as f32;
    let mut out = vec![0u8; (n * n * 4) as usize];
    // latar
    for y in 0..n {
        for x in 0..n {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let a = cov(rrect((fx, fy), (nf / 2.0, nf / 2.0), (nf / 2.0, nf / 2.0), nf * 0.22));
            if a <= 0.0 {
                continue;
            }
            let c = wallpaper(fx, fy, nf, nf, dark);
            let i = ((y * n + x) * 4) as usize;
            out[i] = (c[2] * a) as u8;
            out[i + 1] = (c[1] * a) as u8;
            out[i + 2] = (c[0] * a) as u8;
            out[i + 3] = (a * 255.0) as u8;
        }
    }
    // ikon di tengah
    let m = ((nf * 0.64).round() as u32).max(8);
    let icon = iconplate::style_icon_v(&iconplate::launchpad_icon(m), style, tint, dark);
    let o = (n - m) / 2;
    for y in 0..m {
        for x in 0..m {
            let s = ((y * m + x) * 4) as usize;
            let ta = icon[s + 3] as f32 / 255.0;
            if ta <= 0.0 {
                continue;
            }
            let d = (((y + o) * n + x + o) * 4) as usize;
            for k in 0..3 {
                out[d + k] = (icon[s + k] as f32 + out[d + k] as f32 * (1.0 - ta)).round().min(255.0) as u8;
            }
            out[d + 3] = (icon[s + 3] as f32 + out[d + 3] as f32 * (1.0 - ta)).round().min(255.0) as u8;
        }
    }
    out
}

/// Lingkaran pelangi (warna "Multiwarna") `n`×`n`.
pub fn multicolor(n: u32) -> Vec<u8> {
    let nf = n as f32;
    let mut out = vec![0u8; (n * n * 4) as usize];
    let c = nf / 2.0;
    for y in 0..n {
        for x in 0..n {
            let (dx, dy) = (x as f32 + 0.5 - c, y as f32 + 0.5 - c);
            let a = cov((dx * dx + dy * dy).sqrt() - (c - 0.5));
            if a <= 0.0 {
                continue;
            }
            let hue = (dy.atan2(dx) / std::f32::consts::TAU + 1.0) % 1.0;
            let rgb = hsv(hue, 0.78, 1.0);
            let i = ((y * n + x) * 4) as usize;
            out[i] = (rgb[2] * a) as u8;
            out[i + 1] = (rgb[1] * a) as u8;
            out[i + 2] = (rgb[0] * a) as u8;
            out[i + 3] = (a * 255.0) as u8;
        }
    }
    out
}

fn hsv(h: f32, s: f32, v: f32) -> [f32; 3] {
    let i = (h * 6.0).floor();
    let f = h * 6.0 - i;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - f * s), v * (1.0 - (1.0 - f) * s));
    let (r, g, b) = match i as i32 % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    [r * 255.0, g * 255.0, b * 255.0]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mean_luma(b: &[u8]) -> f32 {
        let (mut s, mut n) = (0.0f32, 0.0f32);
        for p in b.chunks_exact(4) {
            if p[3] > 200 {
                s += 0.114 * p[0] as f32 + 0.587 * p[1] as f32 + 0.299 * p[2] as f32;
                n += 1.0;
            }
        }
        s / n.max(1.0)
    }

    #[test]
    fn appearance_light_is_brighter_than_dark_and_auto_in_between() {
        let (w, h) = (96, 64);
        let (a, l, d) = (appearance(0, w, h), appearance(1, w, h), appearance(2, w, h));
        assert_eq!(l.len(), (w * h * 4) as usize);
        let (la, ll, ld) = (mean_luma(&a), mean_luma(&l), mean_luma(&d));
        assert!(ll > la && la > ld, "terang {ll} auto {la} gelap {ld}");
        // sudut membulat transparan
        assert_eq!(l[3], 0);
    }

    #[test]
    fn previews_are_premultiplied() {
        for b in [appearance(0, 40, 28), icon_tile(IconStyle::Clear, 0, false, 40), multicolor(24)] {
            for p in b.chunks_exact(4) {
                assert!(p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3]);
            }
        }
    }

    #[test]
    fn multicolor_has_many_hues() {
        let b = multicolor(32);
        let red = b.chunks_exact(4).any(|p| p[3] > 200 && p[2] > 200 && p[0] < 120);
        let blue = b.chunks_exact(4).any(|p| p[3] > 200 && p[0] > 200 && p[2] < 120);
        assert!(red && blue);
    }
}
