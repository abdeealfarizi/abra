//! Logo ABRA: huruf "A" (puncak ke atas, dengan palang pendek) di atas "V" (puncak ke bawah), keduanya
//! tebal berujung bulat dan bertumpuk rapat sehingga terbaca sebagai panah atas dan bawah. Digambar vektor
//! (jarak ke ruas garis) lalu dirasterisasi dengan supersampling, jadi tajam pada ukuran apa pun.

/// Ruas garis dalam kotak satuan (0..1) dengan lebar goresan satuan.
const STROKE: f32 = 0.10;
const SEGMENTS: &[((f32, f32), (f32, f32))] = &[
    // "A": dua kaki dari puncak atas
    ((0.50, 0.06), (0.17, 0.39)),
    ((0.50, 0.06), (0.83, 0.39)),
    // palang "A"
    ((0.355, 0.27), (0.645, 0.27)),
    // "V": dua kaki ke puncak bawah
    ((0.17, 0.61), (0.50, 0.94)),
    ((0.83, 0.61), (0.50, 0.94)),
];

fn seg_dist(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (abx, aby) = (b.0 - a.0, b.1 - a.1);
    let (apx, apy) = (p.0 - a.0, p.1 - a.1);
    let t = ((apx * abx + apy * aby) / (abx * abx + aby * aby)).clamp(0.0, 1.0);
    let (cx, cy) = (a.0 + abx * t, a.1 + aby * t);
    ((p.0 - cx).powi(2) + (p.1 - cy).powi(2)).sqrt()
}

/// Cakupan (0..1) logo pada titik satuan `p`.
fn inside(p: (f32, f32)) -> bool {
    inside_with(p, STROKE)
}

fn inside_with(p: (f32, f32), stroke: f32) -> bool {
    SEGMENTS.iter().any(|(a, b)| seg_dist(p, *a, *b) <= stroke / 2.0)
}

/// Rasterisasi ke `size`×`size` piksel BGRA ter-premultiply berwarna `rgb` (alfa = cakupan, supersampling 4×4).
pub fn render(size: u32, rgb: [u8; 3]) -> Vec<u8> {
    let n = size.max(1);
    let ss = 4u32;
    let mut out = vec![0u8; (n * n * 4) as usize];
    for y in 0..n {
        for x in 0..n {
            let mut hit = 0u32;
            for sy in 0..ss {
                for sx in 0..ss {
                    let p = (
                        (x as f32 + (sx as f32 + 0.5) / ss as f32) / n as f32,
                        (y as f32 + (sy as f32 + 0.5) / ss as f32) / n as f32,
                    );
                    if inside(p) {
                        hit += 1;
                    }
                }
            }
            let a = (hit * 255 / (ss * ss)) as u32;
            let i = ((y * n + x) * 4) as usize;
            out[i] = (rgb[2] as u32 * a / 255) as u8;
            out[i + 1] = (rgb[1] as u32 * a / 255) as u8;
            out[i + 2] = (rgb[0] as u32 * a / 255) as u8;
            out[i + 3] = a as u8;
        }
    }
    out
}

/// Ikon bulat di menu bar: hanya cincin berwarna `rgb`, tanpa latar dan tanpa lambang.
/// Anti-alias dengan supersampling 4x4.
pub fn render_badge(size: u32, rgb: [u8; 3]) -> Vec<u8> {
    let n = size.max(1);
    let ss = 4u32;
    let (outer, inner) = (0.46f32, 0.335f32); // jari-jari luar/dalam cincin (satuan kotak)
    let mut out = vec![0u8; (n * n * 4) as usize];
    for y in 0..n {
        for x in 0..n {
            let mut hit = 0u32;
            for sy in 0..ss {
                for sx in 0..ss {
                    let p = ((x as f32 + (sx as f32 + 0.5) / ss as f32) / n as f32, (y as f32 + (sy as f32 + 0.5) / ss as f32) / n as f32);
                    let (dx, dy) = (p.0 - 0.5, p.1 - 0.5);
                    let d2 = dx * dx + dy * dy;
                    if d2 <= outer * outer && d2 >= inner * inner {
                        hit += 1;
                    }
                }
            }
            let a = hit * 255 / (ss * ss);
            let i = ((y * n + x) * 4) as usize;
            out[i] = (rgb[2] as u32 * a / 255) as u8;
            out[i + 1] = (rgb[1] as u32 * a / 255) as u8;
            out[i + 2] = (rgb[0] as u32 * a / 255) as u8;
            out[i + 3] = a as u8;
        }
    }
    out
}

/// Jarak bertanda ke kotak membulat berpusat (cx,cy), setengah-ukuran (hw,hh), radius r.
fn rr_dist(px: f32, py: f32, cx: f32, cy: f32, hw: f32, hh: f32, r: f32) -> f32 {
    let (qx, qy) = ((px - cx).abs() - (hw - r), (py - cy).abs() - (hh - r));
    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    outside + qx.max(qy).min(0.0) - r
}

/// Ikon baterai ala macOS: badan bergaris tepi tipis (alfa 45%), isian di dalamnya sesuai `pct`, tonjolan kecil di kanan.
/// `fill` = warna isian, `rgb` = warna garis tepi. Hasil `w`×`h` BGRA ter-premultiply.
pub fn render_battery(w: u32, h: u32, pct: u8, rgb: [u8; 3], fill: [u8; 3]) -> Vec<u8> {
    let (wf, hf) = (w as f32, h as f32);
    let nub = (wf * 0.07).max(1.5);
    let bw = wf - nub - 1.0; // lebar badan
    let stroke = (hf * 0.09).max(1.0);
    let r = hf * 0.30;
    let gap = stroke + (hf * 0.10).max(1.0);
    let inner_w = (bw - 2.0 * gap) * (pct.min(100) as f32 / 100.0);
    let ss = 4;
    let mut out = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let (mut a_line, mut a_fill) = (0f32, 0f32);
            for sy in 0..ss {
                for sx in 0..ss {
                    let (px, py) = (x as f32 + (sx as f32 + 0.5) / ss as f32, y as f32 + (sy as f32 + 0.5) / ss as f32);
                    let d = rr_dist(px, py, bw / 2.0, hf / 2.0, bw / 2.0, hf / 2.0, r);
                    if d <= 0.0 && d >= -stroke {
                        a_line += 1.0;
                    }
                    // tonjolan
                    if rr_dist(px, py, bw + 1.0 + nub / 2.0 - nub * 0.5, hf / 2.0, nub, hf * 0.19, nub * 0.9) <= 0.0 && px > bw + 0.8 {
                        a_line += 1.0;
                    }
                    if inner_w > 0.5 && rr_dist(px, py, gap + inner_w / 2.0, hf / 2.0, inner_w / 2.0, hf / 2.0 - gap, (r - gap).max(0.8).min(inner_w / 2.0)) <= 0.0 {
                        a_fill += 1.0;
                    }
                }
            }
            let n = (ss * ss) as f32;
            let (al, af) = (a_line / n * 0.45, a_fill / n);
            let a = (af + al * (1.0 - af)).min(1.0);
            let i = ((y * w + x) * 4) as usize;
            for c in 0..3 {
                let col = fill[2 - c] as f32 * af + rgb[2 - c] as f32 * al * (1.0 - af);
                out[i + c] = col.round().min(255.0) as u8;
            }
            out[i + 3] = (a * 255.0).round() as u8;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn battery_outline_fill_and_nub() {
        let (w, h) = (52u32, 24u32);
        let full = render_battery(w, h, 100, [255, 255, 255], [255, 255, 255]);
        let empty = render_battery(w, h, 0, [255, 255, 255], [255, 255, 255]);
        let a = |b: &[u8], x: u32, y: u32| b[((y * w + x) * 4 + 3) as usize];
        assert!(a(&full, 24, 12) > 240, "isian penuh di tengah");
        assert_eq!(a(&empty, 24, 12), 0, "kosong: tengah transparan");
        assert!(a(&empty, 24, 0) > 60 && a(&empty, 24, 0) < 160, "garis tepi semi-transparan");
        assert!(a(&empty, w - 2, 12) > 40, "tonjolan kanan");
        assert_eq!(a(&empty, w - 1, 0), 0);
    }

    use super::*;

    fn alpha(b: &[u8], n: u32, x: u32, y: u32) -> u8 {
        b[((y * n + x) * 4 + 3) as usize]
    }

    #[test]
    fn symmetric_left_right() {
        let n = 64;
        let b = render(n, [255, 255, 255]);
        for y in 0..n {
            for x in 0..n / 2 {
                let (l, r) = (alpha(&b, n, x, y) as i32, alpha(&b, n, n - 1 - x, y) as i32);
                assert!((l - r).abs() <= 2, "({x},{y}) {l} vs {r}");
            }
        }
    }

    #[test]
    fn has_solid_core_soft_edges_and_empty_corners() {
        let n = 96;
        let b = render(n, [255, 255, 255]);
        // sudut kosong, puncak atas dan bawah terisi, ada piksel tepi bertingkat (anti-alias)
        assert_eq!(alpha(&b, n, 1, 1), 0);
        assert_eq!(alpha(&b, n, n - 2, n - 2), 0);
        assert!(alpha(&b, n, n / 2, (0.09 * n as f32) as u32) > 200);
        assert!(alpha(&b, n, n / 2, (0.91 * n as f32) as u32) > 200);
        assert!(b.chunks_exact(4).any(|p| p[3] > 20 && p[3] < 235));
    }

    #[test]
    fn badge_is_a_hollow_ring() {
        let n = 96;
        let b = render_badge(n, [255, 255, 255]);
        assert_eq!(alpha(&b, n, 1, 1), 0, "sudut luar kosong");
        assert!(alpha(&b, n, n / 2, 6) > 200, "cincin atas terisi");
        assert!(alpha(&b, n, 6, n / 2) > 200, "cincin kiri terisi");
        assert_eq!(alpha(&b, n, n / 2, n / 2), 0, "tengah berlubang (tanpa latar)");
        for y in 0..n {
            let (l, r) = (alpha(&b, n, 10, y) as i32, alpha(&b, n, n - 11, y) as i32);
            assert!((l - r).abs() <= 3);
        }
    }

    #[test]
    fn premultiplied_color() {
        let b = render(32, [200, 100, 50]);
        for p in b.chunks_exact(4) {
            assert!(p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3]);
        }
    }
}
