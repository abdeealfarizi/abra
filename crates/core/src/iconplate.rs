//! Ikon dock seragam ala macOS: setiap ikon menempati "badan" squircle (superelips) yang sama besar dengan bayangan
//! lembut di bawahnya. Ikon yang sudah berbentuk kotak membulat dipakai apa adanya (hanya diperkecil ke ukuran badan);
//! ikon berbentuk bebas (perisai, map, logo) diletakkan di atas pelat putih. Murni (BGRA ter-premultiply), diuji.

/// Badan ikon terhadap kanvas (macOS: 824/1024; sedikit lebih besar agar terbaca di dock kecil).
pub const BODY: f32 = 0.86;
/// Ikon bentuk bebas di atas pelat, terhadap kanvas.
const GLYPH: f32 = 0.60;
/// Pangkat superelips (sudut kontinu ala Apple).
const POWER: f32 = 5.0;

fn alpha(b: &[u8], n: u32, x: u32, y: u32) -> u8 {
    b[((y * n + x) * 4 + 3) as usize]
}

/// Apakah ikon sudah berupa kotak membulat penuh (mengisi kanvas, sudut transparan)?
pub fn is_rounded_square(bgra: &[u8], n: u32) -> bool {
    if n < 8 || bgra.len() < (n * n * 4) as usize {
        return false;
    }
    let m = n / 2;
    let e = (n as f32 * 0.06) as u32;
    let solid = |x: u32, y: u32| alpha(bgra, n, x, y) > 200;
    let edges = solid(m, e) && solid(m, n - 1 - e) && solid(e, m) && solid(n - 1 - e, m);
    let q = n / 4;
    let quarters = solid(q, e + 1) && solid(n - 1 - q, e + 1) && solid(q, n - 2 - e) && solid(n - 1 - q, n - 2 - e) && solid(e + 1, q) && solid(e + 1, n - 1 - q);
    let filled = bgra.chunks_exact(4).filter(|p| p[3] > 128).count() as f32 / (n * n) as f32;
    edges && quarters && filled > 0.80
}

/// Ubah ukuran `n`×`n` → `m`×`m` dengan rata-rata area (untuk memperkecil) / bilinear sederhana.
fn resample(src: &[u8], n: u32, m: u32) -> Vec<u8> {
    let mut out = vec![0u8; (m * m * 4) as usize];
    let k = n as f32 / m as f32;
    for y in 0..m {
        let (y0, y1) = (y as f32 * k, (y + 1) as f32 * k);
        for x in 0..m {
            let (x0, x1) = (x as f32 * k, (x + 1) as f32 * k);
            let mut acc = [0f32; 4];
            let mut wsum = 0f32;
            let mut sy = y0.floor() as u32;
            while (sy as f32) < y1 && sy < n {
                let wy = (y1.min(sy as f32 + 1.0) - y0.max(sy as f32)).max(0.0);
                let mut sx = x0.floor() as u32;
                while (sx as f32) < x1 && sx < n {
                    let wx = (x1.min(sx as f32 + 1.0) - x0.max(sx as f32)).max(0.0);
                    let w = wx * wy;
                    let i = ((sy * n + sx) * 4) as usize;
                    for c in 0..4 {
                        acc[c] += src[i + c] as f32 * w;
                    }
                    wsum += w;
                    sx += 1;
                }
                sy += 1;
            }
            let o = ((y * m + x) * 4) as usize;
            if wsum > 0.0 {
                for c in 0..4 {
                    out[o + c] = (acc[c] / wsum).round().clamp(0.0, 255.0) as u8;
                }
            }
        }
    }
    out
}

/// Cakupan superelips berpusat di tengah kanvas `n` dengan setengah-sisi `half` px, pada piksel (x,y) (supersampling 3×3).
fn squircle_cov(n: u32, half: f32, x: u32, y: u32) -> f32 {
    let c = n as f32 / 2.0;
    let mut hit = 0;
    for sy in 0..3 {
        for sx in 0..3 {
            let px = (x as f32 + (sx as f32 + 0.5) / 3.0 - c).abs() / half;
            let py = (y as f32 + (sy as f32 + 0.5) / 3.0 - c).abs() / half;
            if px.powf(POWER) + py.powf(POWER) <= 1.0 {
                hit += 1;
            }
        }
    }
    hit as f32 / 9.0
}

/// Letakkan `top` (m×m, premultiply) di atas `dst` (n×n) pada (ox, oy).
fn over(dst: &mut [u8], n: u32, top: &[u8], m: u32, ox: u32, oy: u32) {
    for y in 0..m {
        for x in 0..m {
            let (dx, dy) = (x + ox, y + oy);
            if dx >= n || dy >= n {
                continue;
            }
            let s = ((y * m + x) * 4) as usize;
            let d = ((dy * n + dx) * 4) as usize;
            let ia = 255 - top[s + 3] as u32;
            for c in 0..4 {
                dst[d + c] = (top[s + c] as u32 + dst[d + c] as u32 * ia / 255).min(255) as u8;
            }
        }
    }
}

fn box_blur(a: &mut [f32], n: usize, r: usize) {
    if r == 0 {
        return;
    }
    let mut tmp = vec![0f32; n * n];
    let win = (2 * r + 1) as f32;
    for pass in 0..2 {
        for y in 0..n {
            let mut sum = 0f32;
            for k in 0..=(2 * r) {
                let i = k as isize - r as isize;
                if i >= 0 && (i as usize) < n {
                    sum += if pass == 0 { a[y * n + i as usize] } else { a[i as usize * n + y] };
                }
            }
            for x in 0..n {
                if pass == 0 {
                    tmp[y * n + x] = sum / win;
                } else {
                    tmp[x * n + y] = sum / win;
                }
                let add = x + r + 1;
                let sub = x as isize - r as isize;
                if add < n {
                    sum += if pass == 0 { a[y * n + add] } else { a[add * n + y] };
                }
                if sub >= 0 {
                    sum -= if pass == 0 { a[y * n + sub as usize] } else { a[sub as usize * n + y] };
                }
            }
        }
        a.copy_from_slice(&tmp);
    }
}

/// Tambahkan bayangan lembut di bawah gambar (di dalam kanvas yang sama).
fn with_shadow(img: &[u8], n: u32) -> Vec<u8> {
    let nn = n as usize;
    let dy = ((n as f32 * 0.022).round() as usize).max(1);
    let mut a = vec![0f32; nn * nn];
    for y in dy..nn {
        for x in 0..nn {
            a[y * nn + x] = img[((y - dy) * nn + x) * 4 + 3] as f32 / 255.0;
        }
    }
    let r = ((n as f32 * 0.03).round() as usize).max(1);
    box_blur(&mut a, nn, r);
    box_blur(&mut a, nn, r);
    let mut out = vec![0u8; img.len()];
    for i in 0..nn * nn {
        let sa = (a[i] * 0.30 * 255.0) as u32; // hitam, alfa 30%
        let ia = 255 - img[i * 4 + 3] as u32;
        for c in 0..3 {
            out[i * 4 + c] = img[i * 4 + c];
        }
        out[i * 4 + 3] = (img[i * 4 + 3] as u32 + sa * ia / 255).min(255) as u8;
    }
    out
}

/// Warna tint untuk gaya Tinted: Biru, Ungu, Merah Muda, Oranye, Hijau.
pub const TINTS: [[u8; 3]; 5] = [[10, 132, 255], [175, 82, 222], [255, 55, 95], [255, 149, 10], [48, 209, 88]];

/// Terapkan gaya ikon Tahoe pada ikon BGRA ter-premultiply (`Default` mengembalikan salinan apa adanya).
/// Dark: latar digelapkan; Clear: abu-abu bening berkilau; Tinted: warna ikon dipetakan ke satu warna tint.
pub fn style_icon(bgra: &[u8], style: crate::settings::IconStyle, tint_index: u8) -> Vec<u8> {
    use crate::settings::IconStyle;
    let mut out = bgra.to_vec();
    if style == IconStyle::Default {
        return out;
    }
    let tint = TINTS[(tint_index as usize).min(TINTS.len() - 1)];
    for px in out.chunks_exact_mut(4) {
        let a = px[3] as u32;
        if a == 0 {
            continue;
        }
        let (b, g, r) = ((px[0] as u32 * 255 / a).min(255) as f32, (px[1] as u32 * 255 / a).min(255) as f32, (px[2] as u32 * 255 / a).min(255) as f32);
        let lum = 0.299 * r + 0.587 * g + 0.114 * b;
        let (nr, ng, nb, na) = match style {
            IconStyle::Dark => {
                // kurangi saturasi sedikit lalu gelapkan
                let (sr, sg, sb) = (r * 0.75 + lum * 0.25, g * 0.75 + lum * 0.25, b * 0.75 + lum * 0.25);
                (sr * 0.38 + 8.0, sg * 0.38 + 8.0, sb * 0.40 + 14.0, a as f32)
            }
            IconStyle::Clear => {
                let v = lum * 0.55 + 95.0;
                (v, v, v + 8.0, a as f32 * 0.82)
            }
            _ => {
                let f = lum / 255.0;
                let base = 0.22 + 0.62 * f;
                let hi = ((f - 0.62).max(0.0) * 1.6) * 255.0;
                (tint[0] as f32 * base + hi, tint[1] as f32 * base + hi, tint[2] as f32 * base + hi, a as f32)
            }
        };
        let (nr, ng, nb) = (nr.clamp(0.0, 255.0), ng.clamp(0.0, 255.0), nb.clamp(0.0, 255.0));
        let na = na.clamp(0.0, 255.0);
        px[0] = (nb * na / 255.0) as u8;
        px[1] = (ng * na / 255.0) as u8;
        px[2] = (nr * na / 255.0) as u8;
        px[3] = na as u8;
    }
    out
}

/// Hasilkan ikon dock seragam `n`×`n` dari ikon sumber `n`×`n`.
pub fn plate(bgra: &[u8], n: u32) -> Vec<u8> {
    if n < 8 || bgra.len() < (n * n * 4) as usize {
        return bgra.to_vec();
    }
    let mut canvas = vec![0u8; (n * n * 4) as usize];
    if is_rounded_square(bgra, n) {
        let m = (n as f32 * BODY).round() as u32;
        let small = resample(bgra, n, m);
        over(&mut canvas, n, &small, m, (n - m) / 2, (n - m) / 2);
    } else {
        // pelat putih bergradasi halus (atas putih, bawah abu sangat muda)
        let half = n as f32 * BODY / 2.0;
        for y in 0..n {
            let t = y as f32 / n as f32;
            let v = (255.0 - 22.0 * t) as u32;
            for x in 0..n {
                let cov = squircle_cov(n, half, x, y);
                if cov > 0.0 {
                    let a = (cov * 255.0) as u32;
                    let i = ((y * n + x) * 4) as usize;
                    let p = (v * a / 255) as u8;
                    canvas[i] = (((v + 3).min(255)) * a / 255) as u8; // sedikit kebiruan
                    canvas[i + 1] = p;
                    canvas[i + 2] = p;
                    canvas[i + 3] = a as u8;
                }
            }
        }
        let m = (n as f32 * GLYPH).round() as u32;
        let small = resample(bgra, n, m);
        over(&mut canvas, n, &small, m, (n - m) / 2, (n - m) / 2);
    }
    with_shadow(&canvas, n)
}

/// Ikon bentuk bebas tanpa pelat (mis. tempat sampah): diperkecil sedikit agar setara badan ikon lain, plus bayangan.
pub fn free(bgra: &[u8], n: u32) -> Vec<u8> {
    if n < 8 || bgra.len() < (n * n * 4) as usize {
        return bgra.to_vec();
    }
    let mut canvas = vec![0u8; (n * n * 4) as usize];
    let m = (n as f32 * 0.90).round() as u32;
    let small = resample(bgra, n, m);
    over(&mut canvas, n, &small, m, (n - m) / 2, (n - m) / 2);
    with_shadow(&canvas, n)
}

/// Ikon tombol Launchpad ala macOS: badan squircle terang bergradasi dengan kisi 3×3 titik berwarna, plus bayangan.
pub fn launchpad_icon(n: u32) -> Vec<u8> {
    const DOTS: [[u8; 3]; 9] = [
        [255, 69, 58],
        [255, 159, 10],
        [255, 214, 10],
        [48, 209, 88],
        [90, 200, 250],
        [10, 132, 255],
        [191, 90, 242],
        [255, 55, 95],
        [50, 173, 230],
    ];
    let mut canvas = vec![0u8; (n * n * 4) as usize];
    let half = n as f32 * BODY / 2.0;
    let c = n as f32 / 2.0;
    let (pitch, radius) = (n as f32 * 0.205, n as f32 * 0.085);
    for y in 0..n {
        let t = y as f32 / n as f32;
        // atas hampir putih, bawah abu kebiruan lembut
        let (br, bg, bb) = (250.0 - 40.0 * t, 250.0 - 38.0 * t, 252.0 - 28.0 * t);
        for x in 0..n {
            let cov = squircle_cov(n, half, x, y);
            if cov <= 0.0 {
                continue;
            }
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let (mut r, mut g, mut b) = (br, bg, bb);
            for (i, col) in DOTS.iter().enumerate() {
                let (cx, cy) = (c + ((i % 3) as f32 - 1.0) * pitch, c + ((i / 3) as f32 - 1.0) * pitch);
                let d = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
                let k = (radius + 0.5 - d).clamp(0.0, 1.0); // anti-alias 1 px
                if k > 0.0 {
                    r += (col[0] as f32 - r) * k;
                    g += (col[1] as f32 - g) * k;
                    b += (col[2] as f32 - b) * k;
                }
            }
            let a = (cov * 255.0) as u32;
            let i = ((y * n + x) * 4) as usize;
            canvas[i] = (b as u32 * a / 255) as u8;
            canvas[i + 1] = (g as u32 * a / 255) as u8;
            canvas[i + 2] = (r as u32 * a / 255) as u8;
            canvas[i + 3] = a as u8;
        }
    }
    with_shadow(&canvas, n)
}

#[cfg(test)]
mod style_tests {
    use super::*;
    use crate::settings::IconStyle;

    fn solid(n: u32, rgb: [u8; 3]) -> Vec<u8> {
        let mut v = vec![0u8; (n * n * 4) as usize];
        for p in v.chunks_exact_mut(4) {
            p.copy_from_slice(&[rgb[2], rgb[1], rgb[0], 255]);
        }
        v
    }

    #[test]
    fn default_is_identity_and_styles_change_color() {
        let src = solid(8, [200, 120, 60]);
        assert_eq!(style_icon(&src, IconStyle::Default, 0), src);
        let dark = style_icon(&src, IconStyle::Dark, 0);
        assert!(dark[2] < src[2] / 2, "Dark menggelapkan");
        let clear = style_icon(&src, IconStyle::Clear, 0);
        assert!(clear[3] < 255, "Clear sedikit tembus pandang");
        assert!(clear[2].abs_diff(clear[0]) < 20, "Clear hampir abu-abu");
    }

    #[test]
    fn tinted_takes_the_tint_hue_and_keeps_alpha_zero_clear() {
        let mut src = solid(8, [200, 120, 60]);
        src[0..4].copy_from_slice(&[0, 0, 0, 0]);
        let t = style_icon(&src, IconStyle::Tinted, 0); // biru
        assert_eq!(&t[0..4], &[0, 0, 0, 0]);
        let p = &t[4..8]; // B, G, R, A
        assert!(p[0] > p[2] && p[0] > p[1], "biru dominan: {p:?}");
        let pink = style_icon(&src, IconStyle::Tinted, 2);
        let q = &pink[4..8];
        assert!(q[2] > q[0], "merah muda: R > B: {q:?}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(n: u32, inset: u32, radius_cut: u32) -> Vec<u8> {
        let mut b = vec![0u8; (n * n * 4) as usize];
        for y in inset..n - inset {
            for x in inset..n - inset {
                let corner = (x < inset + radius_cut || x >= n - inset - radius_cut) && (y < inset + radius_cut || y >= n - inset - radius_cut);
                if !corner {
                    let i = ((y * n + x) * 4) as usize;
                    b[i..i + 4].copy_from_slice(&[200, 100, 50, 255]);
                }
            }
        }
        b
    }

    #[test]
    fn detects_rounded_square_but_not_free_shapes() {
        assert!(is_rounded_square(&solid(64, 0, 6), 64));
        // lingkaran kecil di tengah: bukan
        let n = 64u32;
        let mut c = vec![0u8; (n * n * 4) as usize];
        for y in 0..n {
            for x in 0..n {
                let (dx, dy) = (x as f32 - 32.0, y as f32 - 32.0);
                if dx * dx + dy * dy < 20.0 * 20.0 {
                    c[((y * n + x) * 4 + 3) as usize] = 255;
                }
            }
        }
        assert!(!is_rounded_square(&c, n));
    }

    #[test]
    fn free_shape_gets_white_plate_and_margin() {
        let n = 96u32;
        let mut c = vec![0u8; (n * n * 4) as usize];
        for y in 30..66 {
            for x in 30..66 {
                let i = ((y * n + x) * 4) as usize;
                c[i..i + 4].copy_from_slice(&[0, 0, 255, 255]);
            }
        }
        let p = plate(&c, n);
        assert_eq!(p.len(), c.len());
        // sudut kanvas kosong, tengah = ikon merah, antara tepi badan dan ikon = pelat terang
        assert_eq!(alpha(&p, n, 0, 0), 0);
        let mid = ((48 * n + 48) * 4) as usize;
        assert!(p[mid + 2] > 200 && p[mid] < 60);
        let pl = ((48 * n + 14) * 4) as usize;
        assert!(p[pl + 3] > 240 && p[pl + 1] > 220);
        // premultiply tetap sah
        for px in p.chunks_exact(4) {
            assert!(px[0] <= px[3] && px[1] <= px[3] && px[2] <= px[3]);
        }
    }

    #[test]
    fn rounded_square_is_shrunk_to_body_with_shadow_below() {
        let n = 100u32;
        let p = plate(&solid(n, 0, 8), n);
        // tepi kanvas kini kosong (badan 86%), tengah tetap berwarna ikon
        assert_eq!(alpha(&p, n, 50, 1), 0);
        let mid = ((50 * n + 50) * 4) as usize;
        assert_eq!(&p[mid..mid + 4], &[200, 100, 50, 255]);
        // bayangan: di bawah badan ada alfa tipis, di atas badan tidak
        assert!(alpha(&p, n, 50, 95) > 0 && alpha(&p, n, 50, 95) < 120);
        assert!(alpha(&p, n, 50, 4) < alpha(&p, n, 50, 95));
    }

    #[test]
    fn launchpad_icon_has_body_dots_and_clear_corners() {
        let n = 128u32;
        let p = launchpad_icon(n);
        assert_eq!(p.len(), (n * n * 4) as usize);
        assert_eq!(alpha(&p, n, 1, 1), 0);
        // tengah = titik biru (kisi 3x3 tengah: biru muda 90,200,250)
        let mid = ((64 * n + 64) * 4) as usize;
        assert!(p[mid] > 200 && p[mid + 3] == 255, "titik tengah berwarna cerah");
        // badan terang di antara titik
        let body = ((20 * n + 64) * 4) as usize;
        assert!(p[body + 1] > 200 && p[body + 3] > 240);
        for px in p.chunks_exact(4) {
            assert!(px[0] <= px[3] && px[1] <= px[3] && px[2] <= px[3]);
        }
    }
}
