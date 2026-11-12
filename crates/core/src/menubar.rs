//! Tata letak menu bar ala macOS: item kiri (logo, nama app, menu), item kanan (status), dan celah notch di
//! tengah. Murni (tanpa Win32): lebar item diberikan pemanggil, hasilnya posisi tiap item.

/// Satu item dengan lebar (px) yang sudah diukur.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub id: u32,
    pub width: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub id: u32,
    pub x: f32,
    pub w: f32,
}

/// Celah yang tidak boleh dipakai (notch): rentang x `[left, right]` dalam px layar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gap {
    pub left: f32,
    pub right: f32,
}

/// Susun item kiri dari `margin` ke kanan dan item kanan dari tepi kanan ke kiri.
/// Item kiri yang akan menembus celah (atau item kanan) disembunyikan, seperti macOS pada MacBook ber-notch;
/// item kanan yang menembus celah juga disembunyikan.
pub fn layout(screen_w: f32, margin: f32, left: &[Item], right: &[Item], gap: Option<Gap>) -> (Vec<Placed>, Vec<Placed>) {
    let left_limit = gap.map(|g| g.left - margin).unwrap_or(screen_w - margin);
    let mut l_out = Vec::new();
    let mut x = margin;
    for it in left {
        if x + it.width > left_limit {
            break; // item berikutnya pun tidak muat: berhenti agar urutan tetap utuh
        }
        l_out.push(Placed { id: it.id, x, w: it.width });
        x += it.width;
    }
    let right_limit = gap.map(|g| g.right + margin).unwrap_or(margin);
    let mut r_out = Vec::new();
    let mut rx = screen_w - margin;
    for it in right {
        if rx - it.width < right_limit {
            break;
        }
        rx -= it.width;
        r_out.push(Placed { id: it.id, x: rx, w: it.width });
    }
    (l_out, r_out)
}

/// Item yang berisi titik x (untuk hover/klik).
pub fn hit(placed: &[Placed], x: f32) -> Option<u32> {
    placed.iter().find(|p| x >= p.x && x < p.x + p.w).map(|p| p.id)
}

/// Tinggi menu bar (px): bila notch aktif, sama dengan tinggi pil notch agar rata; selain itu tinggi standar.
pub fn bar_height(dpi: f32, notch_height_dp: Option<f32>) -> f32 {
    (notch_height_dp.unwrap_or(26.0) * dpi).round()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(ws: &[f32]) -> Vec<Item> {
        ws.iter().enumerate().map(|(i, w)| Item { id: i as u32, width: *w }).collect()
    }

    #[test]
    fn no_gap_places_everything() {
        let (l, r) = layout(1000.0, 10.0, &items(&[40.0, 60.0]), &items(&[30.0, 30.0]), None);
        assert_eq!(l.iter().map(|p| p.id).collect::<Vec<_>>(), vec![0, 1]);
        assert_eq!(l[1].x, 50.0);
        assert_eq!(r[0].x, 960.0); // item kanan pertama paling kanan
        assert_eq!(r[1].x, 930.0);
    }

    #[test]
    fn left_items_hidden_before_notch() {
        let gap = Gap { left: 200.0, right: 400.0 };
        let (l, _) = layout(1000.0, 10.0, &items(&[60.0, 60.0, 60.0, 60.0]), &[], Some(gap));
        // batas kiri = 190; 10+60+60+60 = 190 muat, item keempat tidak
        assert_eq!(l.len(), 3);
    }

    #[test]
    fn right_items_stop_at_notch() {
        let gap = Gap { left: 100.0, right: 900.0 };
        let (_, r) = layout(1000.0, 10.0, &[], &items(&[40.0, 40.0, 40.0]), Some(gap));
        // batas kanan = 910; ruang 990-910 = 80 → hanya dua item
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn hit_finds_item() {
        let (l, _) = layout(1000.0, 10.0, &items(&[40.0, 60.0]), &[], None);
        assert_eq!(hit(&l, 15.0), Some(0));
        assert_eq!(hit(&l, 60.0), Some(1));
        assert_eq!(hit(&l, 500.0), None);
    }

    #[test]
    fn bar_height_matches_notch_when_present() {
        assert_eq!(bar_height(1.25, Some(32.0)), 40.0);
        assert_eq!(bar_height(1.0, None), 26.0);
    }
}
