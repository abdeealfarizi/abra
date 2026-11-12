//! Simbol menu bar bergaya SF Symbols (Wi-Fi, kaca pembesar, Control Center, Stage Manager) yang digambar sendiri
//! sebagai bentuk vektor ke bitmap BGRA ter-premultiply, supaya tidak bergantung pada font ikon Windows maupun berkas Apple.
//! Semua bentuk dihitung di kotak satuan 0..1 dengan anti-alias supersampling 4×4.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sym {
    Wifi,
    Search,
    Control,
    Stage,
    Info,
    Gear,
    Bag,
    Grid,
    Windows,
    XCircle,
    Moon,
    Restart,
    Power,
    Lock,
    EyeSlash,
    Speaker,
    SpeakerWave,
    Sun,
    SunSmall,
    SpeakerLow,
    SpeakerMid,
    SpeakerMute,
    Bluetooth,
    Play,
    Pause,
    Backward,
    Forward,
    Plus,
    Minus,
    Calendar,
    Note,
}

/// Simbol untuk perintah menu bar (id yang dipakai `menubar.rs`); `None` = tanpa simbol.
pub fn for_command(id: usize) -> Option<Sym> {
    match id {
        100 | 200 => Some(Sym::Info),
        101 | 103 | 810 | 811 => Some(Sym::Gear),
        102 => Some(Sym::Bag),
        105 | 809 => Some(Sym::Grid),
        106 | 808 => Some(Sym::Windows),
        104 => Some(Sym::XCircle),
        110 => Some(Sym::Moon),
        111 => Some(Sym::Restart),
        112 => Some(Sym::Power),
        113 => Some(Sym::Lock),
        201 | 202 => Some(Sym::EyeSlash),
        _ => None,
    }
}

/// Potong bitmap BGRA ter-premultiply ke kotak isi tinta (alfa > 10) supaya bisa dipusatkan tepat pada isinya, bukan pada
/// kotak font/gambar. `None` bila kosong.
pub fn crop_ink(bgra: &[u8], w: u32, h: u32) -> Option<(Vec<u8>, u32, u32)> {
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0u32, 0u32);
    for y in 0..h {
        for x in 0..w {
            if bgra[((y * w + x) * 4 + 3) as usize] > 10 {
                x0 = x0.min(x);
                x1 = x1.max(x);
                y0 = y0.min(y);
                y1 = y1.max(y);
            }
        }
    }
    if x0 > x1 || y0 > y1 {
        return None;
    }
    let (cw, ch) = (x1 - x0 + 1, y1 - y0 + 1);
    let mut out = vec![0u8; (cw * ch * 4) as usize];
    for y in 0..ch {
        let s = (((y0 + y) * w + x0) * 4) as usize;
        let d = (y * cw * 4) as usize;
        out[d..d + (cw * 4) as usize].copy_from_slice(&bgra[s..s + (cw * 4) as usize]);
    }
    Some((out, cw, ch))
}

/// Padanan simbol untuk glyph Segoe Fluent yang dipakai sebelumnya (kode titik).
pub fn for_glyph(g: &str) -> Option<Sym> {
    match g {
        "\u{E701}" => Some(Sym::Wifi),
        "\u{E721}" => Some(Sym::Search),
        "\u{E9E9}" => Some(Sym::Control),
        "\u{E8A0}" => Some(Sym::Stage),
        _ => None,
    }
}

type P = (f32, f32);

fn len(a: P, b: P) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

/// Jarak ke ruas a-b (tanpa ketebalan).
fn seg(p: P, a: P, b: P) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let l2 = dx * dx + dy * dy;
    let t = if l2 == 0.0 { 0.0 } else { (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / l2).clamp(0.0, 1.0) };
    len(p, (a.0 + t * dx, a.1 + t * dy))
}

/// Jarak bertanda ke poligon (negatif di dalam). Dikurangi radius `r` oleh pemanggil untuk sudut membulat.
fn poly(p: P, pts: &[P]) -> f32 {
    let mut d = f32::MAX;
    let mut inside = false;
    let mut j = pts.len() - 1;
    for i in 0..pts.len() {
        d = d.min(seg(p, pts[i], pts[j]));
        let (pi, pj) = (pts[i], pts[j]);
        if (pi.1 > p.1) != (pj.1 > p.1) && p.0 < (pj.0 - pi.0) * (p.1 - pi.1) / (pj.1 - pi.1) + pi.0 {
            inside = !inside;
        }
        j = i;
    }
    if inside {
        -d
    } else {
        d
    }
}

/// Segitiga membulat menghadap kiri dengan ujung di `tip` dan alas di `base` (koordinat luar).
fn tri_left(p: P, tip: f32, base: f32) -> bool {
    poly(p, &[(tip + 0.07, 0.5), (base - 0.045, 0.29), (base - 0.045, 0.71)]) - 0.045 <= 0.0
}

/// Jarak bertanda ke lingkaran.
fn circle(p: P, c: P, r: f32) -> f32 {
    len(p, c) - r
}

/// Jarak bertanda ke garis tebal `w` berujung bulat dari a ke b.
fn line(p: P, a: P, b: P, w: f32) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let t = (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    len(p, (a.0 + t * dx, a.1 + t * dy)) - w / 2.0
}

/// Jarak bertanda ke kotak membulat berpusat `c`, setengah-ukuran `h`, radius `r`.
fn rrect(p: P, c: P, h: P, r: f32) -> f32 {
    let (qx, qy) = ((p.0 - c.0).abs() - (h.0 - r), (p.1 - c.1).abs() - (h.1 - r));
    (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r
}

/// Cincin tebal `w` berjari-jari `r` (jarak bertanda ke tepi).
fn ring(p: P, c: P, r: f32, w: f32) -> f32 {
    (len(p, c) - r).abs() - w / 2.0
}

/// Busur: bagian cincin dalam sudut ±`half` (rad) dari arah atas, dengan ujung bulat.
fn arc(p: P, c: P, r: f32, w: f32, half: f32) -> f32 {
    let ang = (p.0 - c.0).atan2(-(p.1 - c.1)); // 0 = lurus ke atas
    let a = ang.clamp(-half, half);
    let q = (c.0 + r * a.sin(), c.1 - r * a.cos());
    len(p, q) - w / 2.0
}

/// Cakupan (0..1) satu sampel: gabungan bentuk dikurangi lubang.
fn shape(sym: Sym, p: P) -> bool {
    match sym {
        Sym::Search => {
            let (c, r, w) = ((0.43, 0.43), 0.27, 0.10);
            ring(p, c, r, w) <= 0.0 || line(p, (0.64, 0.64), (0.88, 0.88), 0.11) <= 0.0
        }
        Sym::Wifi => {
            let apex = (0.5, 0.80);
            let half = 0.78; // ≈ 45°
            let w = 0.10;
            circle(p, apex, 0.075) <= 0.0 || [0.27f32, 0.46, 0.65].iter().any(|r| arc(p, apex, *r, w, half) <= 0.0)
        }
        Sym::Control => {
            // dua sakelar bertumpuk: atas terisi dengan lubang kenop di kanan, bawah garis tepi dengan kenop terisi di kiri
            let top = rrect(p, (0.5, 0.29), (0.43, 0.17), 0.17) <= 0.0 && circle(p, (0.70, 0.29), 0.105) > 0.0;
            let bot_outline = rrect(p, (0.5, 0.71), (0.43, 0.17), 0.17).abs() - 0.035 <= 0.0;
            let bot_knob = circle(p, (0.30, 0.71), 0.105) <= 0.0;
            top || bot_outline || bot_knob
        }
        Sym::Info => {
            ring(p, (0.5, 0.5), 0.41, 0.075) <= 0.0 || circle(p, (0.5, 0.29), 0.058) <= 0.0 || line(p, (0.5, 0.45), (0.5, 0.72), 0.09) <= 0.0
        }
        Sym::Gear => {
            let c = (0.5, 0.5);
            let teeth = (0..8).any(|i| {
                let a = i as f32 * std::f32::consts::FRAC_PI_4;
                line(p, (c.0 + 0.30 * a.cos(), c.1 + 0.30 * a.sin()), (c.0 + 0.40 * a.cos(), c.1 + 0.40 * a.sin()), 0.14) <= 0.0
            });
            ring(p, c, 0.27, 0.12) <= 0.0 || teeth || (circle(p, c, 0.12) <= 0.0 && false)
        }
        Sym::Bag => {
            let body = rrect(p, (0.5, 0.62), (0.34, 0.27), 0.07).abs() - 0.04 <= 0.0;
            let handle = arc(p, (0.5, 0.40), 0.17, 0.075, 1.5) <= 0.0 && p.1 < 0.42;
            body || handle
        }
        Sym::Grid => [(0.28, 0.28), (0.72, 0.28), (0.28, 0.72), (0.72, 0.72)].iter().any(|c| rrect(p, *c, (0.17, 0.17), 0.06) <= 0.0),
        Sym::Windows => {
            let back = rrect(p, (0.40, 0.38), (0.30, 0.22), 0.06).abs() - 0.035 <= 0.0 && !(rrect(p, (0.60, 0.62), (0.32, 0.24), 0.06) <= 0.0);
            let front = rrect(p, (0.60, 0.62), (0.30, 0.22), 0.06).abs() - 0.035 <= 0.0;
            back || front
        }
        Sym::XCircle => ring(p, (0.5, 0.5), 0.41, 0.075) <= 0.0 || line(p, (0.34, 0.34), (0.66, 0.66), 0.085) <= 0.0 || line(p, (0.66, 0.34), (0.34, 0.66), 0.085) <= 0.0,
        Sym::Moon => circle(p, (0.46, 0.5), 0.38) <= 0.0 && circle(p, (0.68, 0.36), 0.31) > 0.0,
        Sym::Restart => {
            let c = (0.5, 0.5);
            let ang = (p.0 - c.0).atan2(-(p.1 - c.1));
            let gap = ang > 0.15 && ang < 1.25;
            let tip = (c.0 + 0.33 * 0.15f32.sin(), c.1 - 0.33 * 0.15f32.cos());
            ring(p, c, 0.33, 0.09) <= 0.0 && !gap || line(p, tip, (tip.0 + 0.17, tip.1 - 0.02), 0.09) <= 0.0 || line(p, tip, (tip.0 + 0.02, tip.1 + 0.17), 0.09) <= 0.0
        }
        Sym::Power => {
            let c = (0.5, 0.54);
            let ang = (p.0 - c.0).atan2(-(p.1 - c.1));
            (ring(p, c, 0.33, 0.09) <= 0.0 && ang.abs() > 0.5) || line(p, (0.5, 0.12), (0.5, 0.46), 0.09) <= 0.0
        }
        Sym::Lock => {
            rrect(p, (0.5, 0.64), (0.32, 0.25), 0.07) <= 0.0 || (arc(p, (0.5, 0.40), 0.19, 0.085, 1.5708) <= 0.0 && p.1 < 0.42) || line(p, (0.31, 0.40), (0.31, 0.46), 0.085) <= 0.0 || line(p, (0.69, 0.40), (0.69, 0.46), 0.085) <= 0.0
        }
        Sym::EyeSlash => {
            let n = (((p.0 - 0.5) / 0.44).powi(2) + ((p.1 - 0.5) / 0.28).powi(2)).sqrt();
            let eye = ((n - 1.0) * 0.28).abs() - 0.04 <= 0.0 || circle(p, (0.5, 0.5), 0.10) <= 0.0;
            eye || line(p, (0.14, 0.84), (0.86, 0.16), 0.085) <= 0.0
        }
        Sym::Speaker | Sym::SpeakerWave | Sym::SpeakerLow | Sym::SpeakerMid | Sym::SpeakerMute => {
            let body = (p.0 > 0.10 && p.0 < 0.30 && (p.1 - 0.5).abs() < 0.13)
                || (p.0 >= 0.30 && p.0 < 0.52 && (p.1 - 0.5).abs() < 0.13 + (p.0 - 0.30) / 0.22 * 0.20);
            if sym == Sym::Speaker {
                return body;
            }
            if sym == Sym::SpeakerMute {
                return body || line(p, (0.64, 0.38), (0.88, 0.62), 0.085) <= 0.0 || line(p, (0.88, 0.38), (0.64, 0.62), 0.085) <= 0.0;
            }
            let waves: &[f32] = match sym {
                Sym::SpeakerLow => &[0.24],
                Sym::SpeakerMid => &[0.24, 0.36],
                _ => &[0.24, 0.36, 0.48],
            };
            let (dx, dy) = (p.0 - 0.5, p.1 - 0.5);
            let ang = dy.atan2(dx);
            let d = (dx * dx + dy * dy).sqrt();
            body || (ang.abs() < 0.78 && waves.iter().any(|r| (d - r).abs() < 0.04) && p.0 > 0.52)
        }
        Sym::Play => poly(p, &[(0.34, 0.26), (0.34, 0.74), (0.74, 0.5)]) - 0.075 <= 0.0,
        Sym::Pause => rrect(p, (0.35, 0.5), (0.085, 0.30), 0.05) <= 0.0 || rrect(p, (0.65, 0.5), (0.085, 0.30), 0.05) <= 0.0,
        Sym::Backward => tri_left(p, 0.06, 0.48) || tri_left(p, 0.52, 0.94),
        Sym::Forward => {
            let q = (1.0 - p.0, p.1);
            tri_left(q, 0.06, 0.48) || tri_left(q, 0.52, 0.94)
        }
        Sym::Plus => line(p, (0.2, 0.5), (0.8, 0.5), 0.10) <= 0.0 || line(p, (0.5, 0.2), (0.5, 0.8), 0.10) <= 0.0,
        Sym::Minus => line(p, (0.2, 0.5), (0.8, 0.5), 0.10) <= 0.0,
        Sym::Calendar => {
            let frame = rrect(p, (0.5, 0.57), (0.40, 0.33), 0.11).abs() - 0.035 <= 0.0;
            let header = p.1 > 0.30 && p.1 < 0.40 && p.0 > 0.12 && p.0 < 0.88;
            let rings = line(p, (0.32, 0.12), (0.32, 0.26), 0.085) <= 0.0 || line(p, (0.68, 0.12), (0.68, 0.26), 0.085) <= 0.0;
            frame || header || rings
        }
        Sym::Note => {
            let head = circle(p, (0.33, 0.74), 0.15) <= 0.0;
            let stem = line(p, (0.46, 0.72), (0.46, 0.18), 0.085) <= 0.0;
            let flag = line(p, (0.46, 0.18), (0.80, 0.34), 0.09) <= 0.0 || line(p, (0.80, 0.34), (0.80, 0.46), 0.09) <= 0.0;
            head || stem || flag
        }
        Sym::Bluetooth => {
            let w = 0.085;
            let pts: [P; 6] = [(0.22, 0.34), (0.74, 0.68), (0.48, 0.88), (0.48, 0.12), (0.74, 0.32), (0.22, 0.66)];
            (0..5).any(|i| line(p, pts[i], pts[i + 1], w) <= 0.0)
        }
        Sym::Sun | Sym::SunSmall => {
            let small = sym == Sym::SunSmall;
            let (r, r0, r1, w) = if small { (0.15, 0.28, 0.38, 0.07) } else { (0.20, 0.32, 0.46, 0.075) };
            let c = (0.5, 0.5);
            circle(p, c, r) <= 0.0
                || (0..8).any(|i| {
                    let a = i as f32 * std::f32::consts::FRAC_PI_4;
                    line(p, (c.0 + r0 * a.cos(), c.1 + r0 * a.sin()), (c.0 + r1 * a.cos(), c.1 + r1 * a.sin()), w) <= 0.0
                })
        }
        Sym::Stage => {
            let panel = rrect(p, (0.64, 0.5), (0.30, 0.31), 0.09).abs() - 0.036 <= 0.0;
            let card_a = rrect(p, (0.11, 0.5), (0.055, 0.19), 0.04) <= 0.0;
            panel || card_a
        }
    }
}

/// Simbol `px`×`px` berwarna `rgb` (BGRA ter-premultiply).
pub fn render(sym: Sym, px: u32, rgb: [u8; 3]) -> Vec<u8> {
    let n = px.max(1);
    let mut out = vec![0u8; (n * n * 4) as usize];
    const SS: u32 = 4;
    // sisakan tepi 1 px agar bentuk tidak terpotong
    let inner = n as f32 - 2.0;
    for y in 0..n {
        for x in 0..n {
            let mut hit = 0u32;
            for sy in 0..SS {
                for sx in 0..SS {
                    let fx = (x as f32 + (sx as f32 + 0.5) / SS as f32 - 1.0) / inner;
                    let fy = (y as f32 + (sy as f32 + 0.5) / SS as f32 - 1.0) / inner;
                    if shape(sym, (fx, fy)) {
                        hit += 1;
                    }
                }
            }
            if hit == 0 {
                continue;
            }
            let a = (hit * 255 / (SS * SS)) as u32;
            let i = ((y * n + x) * 4) as usize;
            out[i] = (rgb[2] as u32 * a / 255) as u8;
            out[i + 1] = (rgb[1] as u32 * a / 255) as u8;
            out[i + 2] = (rgb[0] as u32 * a / 255) as u8;
            out[i + 3] = a as u8;
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
    fn menu_symbols_draw_and_command_map() {
        assert_eq!(for_command(111), Some(Sym::Restart));
        assert_eq!(for_command(999), None);
        for s in [Sym::Info, Sym::Gear, Sym::Bag, Sym::Grid, Sym::Windows, Sym::XCircle, Sym::Moon, Sym::Restart, Sym::Power, Sym::Lock, Sym::EyeSlash, Sym::Speaker, Sym::SpeakerWave, Sym::SpeakerLow, Sym::SpeakerMid, Sym::SpeakerMute, Sym::Sun, Sym::SunSmall, Sym::Bluetooth, Sym::Play, Sym::Pause, Sym::Backward, Sym::Forward, Sym::Plus, Sym::Minus, Sym::Calendar, Sym::Note] {
            let b = render(s, 40, [255, 255, 255]);
            let filled = b.chunks_exact(4).filter(|p| p[3] > 128).count();
            assert!(filled > 40 && filled < 40 * 40 * 3 / 4, "{s:?} terisi {filled}");
        }
    }

    #[test]
    fn media_symbols_have_expected_shape() {
        let n = 64;
        let play = render(Sym::Play, n, [255, 255, 255]);
        assert!(alpha(&play, n, 28, 32) > 200, "badan segitiga terisi");
        assert_eq!(alpha(&play, n, 56, 6), 0, "pojok kanan atas kosong");
        let pause = render(Sym::Pause, n, [255, 255, 255]);
        assert!(alpha(&pause, n, 22, 32) > 200 && alpha(&pause, n, 42, 32) > 200);
        assert_eq!(alpha(&pause, n, 32, 32), 0, "celah di antara dua batang");
        let fwd = render(Sym::Forward, n, [255, 255, 255]);
        let back = render(Sym::Backward, n, [255, 255, 255]);
        // maju adalah cermin mundur: piksel kiri-kanan bertukar
        assert_eq!(alpha(&fwd, n, 10, 32), alpha(&back, n, 53, 32));
        let minus = render(Sym::Minus, n, [255, 255, 255]);
        assert!(alpha(&minus, n, 32, 32) > 200 && alpha(&minus, n, 32, 12) == 0);
    }

    #[test]
    fn crop_ink_finds_the_ink_box() {
        let (w, h) = (10u32, 8u32);
        let mut b = vec![0u8; (w * h * 4) as usize];
        for y in 2..5 {
            for x in 3..8 {
                let i = ((y * w + x) * 4) as usize;
                b[i + 3] = 255;
            }
        }
        let (c, cw, ch) = crop_ink(&b, w, h).unwrap();
        assert_eq!((cw, ch), (5, 3));
        assert!(c.chunks_exact(4).all(|p| p[3] == 255));
        assert!(crop_ink(&vec![0u8; 16], 2, 2).is_none());
    }

    #[test]
    fn glyph_mapping() {
        assert_eq!(for_glyph("\u{E701}"), Some(Sym::Wifi));
        assert_eq!(for_glyph("x"), None);
    }

    #[test]
    fn search_has_ring_with_empty_center_and_handle() {
        let n = 64;
        let b = render(Sym::Search, n, [255, 255, 255]);
        assert_eq!(alpha(&b, n, 26, 26), 0, "pusat lensa kosong");
        assert!(alpha(&b, n, 55, 55) > 200, "gagang terisi");
        assert!(alpha(&b, n, 26, 10) > 200, "cincin atas terisi");
    }

    #[test]
    fn wifi_has_dot_and_arcs() {
        let n = 64;
        let b = render(Sym::Wifi, n, [255, 255, 255]);
        assert!(alpha(&b, n, 32, 50) > 200, "titik bawah");
        assert!(alpha(&b, n, 32, 50 - 17) > 200, "busur pertama di atas titik");
        assert_eq!(alpha(&b, n, 32, 5), 0, "tidak ada isian di atas busur terluar (tepi)");
    }

    #[test]
    fn control_and_stage_draw_and_premultiply() {
        for s in [Sym::Control, Sym::Stage] {
            let b = render(s, 48, [200, 100, 50]);
            assert!(b.chunks_exact(4).any(|p| p[3] > 200));
            for p in b.chunks_exact(4) {
                assert!(p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3]);
            }
        }
    }
}
