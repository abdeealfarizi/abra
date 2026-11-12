//! Urutan fokus bergilir untuk app multi-window (SPEC §4.5): klik berulang mengunjungi
//! semua window, bukan hanya bolak-balik antara dua yang terbaru.

/// `recent_first`: window item ini, paling baru difokus lebih dulu. `foreground`: window di depan.
/// Bila `foreground` ada dalam daftar → window berikutnya menurut urutan tetap (terurut handle);
/// bila tidak → yang paling baru difokus.
pub fn next_in_ring(recent_first: &[isize], foreground: isize) -> Option<isize> {
    let most_recent = *recent_first.first()?;
    let mut ring = recent_first.to_vec();
    ring.sort_unstable();
    Some(match ring.iter().position(|&h| h == foreground) {
        Some(i) => ring[(i + 1) % ring.len()],
        None => most_recent,
    })
}

/// Kebalikan `next_in_ring`: jendela sebelumnya menurut urutan tetap (terurut handle).
pub fn prev_in_ring(recent_first: &[isize], foreground: isize) -> Option<isize> {
    let most_recent = *recent_first.first()?;
    let mut ring = recent_first.to_vec();
    ring.sort_unstable();
    Some(match ring.iter().position(|&h| h == foreground) {
        Some(i) => ring[(i + ring.len() - 1) % ring.len()],
        None => most_recent,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outside_the_group_picks_the_most_recent() {
        assert_eq!(next_in_ring(&[30, 10, 20], 99), Some(30));
    }

    #[test]
    fn inside_the_group_visits_every_window_in_stable_order() {
        let group = [30, 10, 20];
        let mut fg = 10;
        let mut seen = vec![];
        for _ in 0..3 {
            fg = next_in_ring(&group, fg).unwrap();
            seen.push(fg);
        }
        assert_eq!(seen, vec![20, 30, 10]);
    }

    #[test]
    fn previous_walks_the_ring_backwards() {
        let group = [30, 10, 20];
        assert_eq!(prev_in_ring(&group, 10), Some(30));
        assert_eq!(prev_in_ring(&group, 20), Some(10));
        assert_eq!(prev_in_ring(&group, 99), Some(30));
    }

    #[test]
    fn single_window_group_returns_itself() {
        assert_eq!(next_in_ring(&[7], 7), Some(7));
    }

    #[test]
    fn empty_group_has_no_target() {
        assert_eq!(next_in_ring(&[], 1), None);
    }
}
