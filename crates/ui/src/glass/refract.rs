//! Peta distorsi untuk refraksi tepi ala Liquid Glass. Piksel BGRA: R/G menyandi arah dan kekuatan geser
//! (0,5 = tanpa geser); di tengah panel netral, di pita tepi bergeser ke luar kuat di tepi dan memudar ke dalam.

/// Bangun peta `w`×`h` untuk persegi membulat berradius `radius` px; pita setebal `band` px.
pub fn displacement_map(w: u32, h: u32, radius: f32, band: f32) -> Vec<u8> {
    let (wf, hf) = (w as f32, h as f32);
    let (cx, cy) = (wf / 2.0, hf / 2.0);
    let r = radius.min(wf / 2.0).min(hf / 2.0);
    let mut out = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let (dx, dy) = (px - cx, py - cy);
            let (qx, qy) = (dx.abs() - (wf / 2.0 - r), dy.abs() - (hf / 2.0 - r));
            let (ox, oy) = (qx.max(0.0), qy.max(0.0));
            let dist = (ox * ox + oy * oy).sqrt() + qx.max(qy).min(0.0) - r; // < 0 di dalam
            let depth = -dist;
            let (mut nx, mut ny, mut s) = (0.0f32, 0.0f32, 0.0f32);
            if depth < band && depth >= 0.0 {
                s = (1.0 - depth / band).powf(2.2);
                let (sx, sy) = (if dx < 0.0 { -1.0 } else { 1.0 }, if dy < 0.0 { -1.0 } else { 1.0 });
                if qx > 0.0 && qy > 0.0 {
                    let l = (qx * qx + qy * qy).sqrt().max(1e-3);
                    nx = sx * qx / l;
                    ny = sy * qy / l;
                } else if qx > qy {
                    nx = sx;
                } else {
                    ny = sy;
                }
            }
            let i = ((y * w + x) * 4) as usize;
            out[i] = 128; // B
            out[i + 1] = (127.5 + 127.0 * ny * s).round() as u8; // G
            out[i + 2] = (127.5 + 127.0 * nx * s).round() as u8; // R
            out[i + 3] = 255;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn center_is_neutral_and_edge_displaces_outward() {
        let (w, h) = (200u32, 80u32);
        let m = displacement_map(w, h, 20.0, 14.0);
        let px = |x: u32, y: u32| {
            let i = ((y * w + x) * 4) as usize;
            (m[i + 2] as i32 - 128, m[i + 1] as i32 - 128)
        };
        assert_eq!(px(100, 40), (0, 0));
        let (rx, _) = px(w - 1, 40); // tepi kanan: geser +x
        let (lx, _) = px(0, 40); // tepi kiri: geser -x
        let (_, by) = px(100, h - 1); // tepi bawah: geser +y
        assert!(rx > 60 && lx < -60 && by > 60, "{rx} {lx} {by}");
    }
}
