//! Urutan ulang pin lewat drag.

/// Posisi sisip untuk kursor di `x`: banyaknya pusat slot yang berada di kirinya.
pub fn target_index(centers: &[f32], x: f32) -> usize {
    centers.iter().filter(|c| **c < x).count()
}

/// Pindahkan elemen `from` sehingga berada pada indeks `to` (indeks tujuan dihitung sebelum pencabutan,
/// seperti hasil `target_index`). Mengembalikan `true` bila urutan berubah.
pub fn move_to<T>(v: &mut Vec<T>, from: usize, to: usize) -> bool {
    if from >= v.len() {
        return false;
    }
    let to = to.min(v.len());
    let dest = if to > from { to - 1 } else { to };
    if dest == from {
        return false;
    }
    let item = v.remove(from);
    v.insert(dest, item);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_counts_centers_to_the_left() {
        let c = [10.0, 30.0, 50.0];
        assert_eq!(target_index(&c, 0.0), 0);
        assert_eq!(target_index(&c, 35.0), 2);
        assert_eq!(target_index(&c, 999.0), 3);
    }

    #[test]
    fn moving_right_and_left_works() {
        let mut v = vec!['a', 'b', 'c', 'd'];
        assert!(move_to(&mut v, 0, 3));
        assert_eq!(v, ['b', 'c', 'a', 'd']);
        assert!(move_to(&mut v, 3, 0));
        assert_eq!(v, ['d', 'b', 'c', 'a']);
    }

    #[test]
    fn dropping_in_place_changes_nothing() {
        let mut v = vec![1, 2, 3];
        assert!(!move_to(&mut v, 1, 1));
        assert!(!move_to(&mut v, 1, 2));
        assert_eq!(v, [1, 2, 3]);
        assert!(!move_to(&mut v, 9, 0));
    }

    #[test]
    fn to_beyond_the_end_appends() {
        let mut v = vec![1, 2, 3];
        assert!(move_to(&mut v, 0, 99));
        assert_eq!(v, [2, 3, 1]);
    }
}
