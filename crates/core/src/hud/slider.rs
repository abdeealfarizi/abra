//! Pemetaan posisi pointer ↔ nilai slider (0..1), horizontal atau vertikal.

/// Nilai dari posisi `pos` pada lintasan yang mulai di `start` sepanjang `len`.
/// `inverted`: nilai besar di ujung `start` (slider vertikal: atas = penuh).
pub fn value_at(pos: f32, start: f32, len: f32, inverted: bool) -> f32 {
    if len <= 0.0 {
        return 0.0;
    }
    let f = ((pos - start) / len).clamp(0.0, 1.0);
    if inverted {
        1.0 - f
    } else {
        f
    }
}

/// Kebalikan: posisi untuk nilai `v`.
pub fn pos_of(v: f32, start: f32, len: f32, inverted: bool) -> f32 {
    let v = v.clamp(0.0, 1.0);
    start + len * if inverted { 1.0 - v } else { v }
}

/// Persen bulat untuk tampilan.
pub fn percent(v: f32) -> u32 {
    (v.clamp(0.0, 1.0) * 100.0).round() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn horizontal_maps_and_clamps() {
        assert_eq!(value_at(50.0, 0.0, 100.0, false), 0.5);
        assert_eq!(value_at(-30.0, 0.0, 100.0, false), 0.0);
        assert_eq!(value_at(400.0, 0.0, 100.0, false), 1.0);
    }

    #[test]
    fn vertical_is_inverted_so_top_is_full() {
        assert_eq!(value_at(0.0, 0.0, 200.0, true), 1.0);
        assert_eq!(value_at(200.0, 0.0, 200.0, true), 0.0);
        assert_eq!(value_at(50.0, 0.0, 200.0, true), 0.75);
    }

    #[test]
    fn position_and_value_round_trip() {
        for inv in [false, true] {
            for v in [0.0, 0.25, 0.6, 1.0] {
                let p = pos_of(v, 10.0, 120.0, inv);
                assert!((value_at(p, 10.0, 120.0, inv) - v).abs() < 1e-5);
            }
        }
    }

    #[test]
    fn degenerate_track_is_safe() {
        assert_eq!(value_at(5.0, 0.0, 0.0, false), 0.0);
        assert_eq!(percent(0.756), 76);
        assert_eq!(percent(2.0), 100);
    }
}
