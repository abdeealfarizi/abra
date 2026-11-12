//! Kalender bulanan (tanpa Win32): jumlah hari, kolom awal, jumlah baris, nama dalam bahasa Indonesia.

pub fn is_leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

pub fn days_in_month(y: i32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(y) => 29,
        2 => 28,
        _ => 30,
    }
}

/// Hari dalam pekan untuk tanggal `d` (0 = Minggu), algoritma Sakamoto.
pub fn weekday(y: i32, m: u32, d: u32) -> u32 {
    const T: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let y = if m < 3 { y - 1 } else { y };
    ((y + y / 4 - y / 100 + y / 400 + T[(m - 1) as usize] + d as i32).rem_euclid(7)) as u32
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Grid {
    /// Kolom (0-based) tempat tanggal 1 jatuh.
    pub first_col: u32,
    pub days: u32,
    pub rows: u32,
}

pub fn grid(y: i32, m: u32, monday_first: bool) -> Grid {
    let wd = weekday(y, m, 1);
    let first_col = if monday_first { (wd + 6) % 7 } else { wd };
    let days = days_in_month(y, m);
    Grid { first_col, days, rows: (first_col + days).div_ceil(7) }
}

/// Geser bulan sebesar `delta`, menjaga tahun.
pub fn shift_month(y: i32, m: u32, delta: i32) -> (i32, u32) {
    let idx = y * 12 + (m as i32 - 1) + delta;
    (idx.div_euclid(12), (idx.rem_euclid(12) + 1) as u32)
}

pub fn month_name(m: u32) -> &'static str {
    const N: [&str; 12] = [
        "Januari", "Februari", "Maret", "April", "Mei", "Juni", "Juli", "Agustus", "September", "Oktober", "November",
        "Desember",
    ];
    N[(m.clamp(1, 12) - 1) as usize]
}

pub fn weekday_labels(monday_first: bool) -> [&'static str; 7] {
    if monday_first {
        ["Sen", "Sel", "Rab", "Kam", "Jum", "Sab", "Min"]
    } else {
        ["Min", "Sen", "Sel", "Rab", "Kam", "Jum", "Sab"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_weekdays() {
        assert_eq!(weekday(2026, 11, 4), 3); // Rabu
        assert_eq!(weekday(2000, 1, 1), 6); // Sabtu
        assert_eq!(weekday(2024, 2, 29), 4); // Kamis
    }

    #[test]
    fn leap_years_and_month_lengths() {
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2100, 2), 28);
        assert_eq!(days_in_month(2000, 2), 29);
        assert_eq!(days_in_month(2026, 11), 30);
        assert_eq!(days_in_month(2026, 12), 31);
    }

    #[test]
    fn grid_rows_depend_on_week_start() {
        // Nov 2026 dimulai hari Minggu: 30 hari
        assert_eq!(grid(2026, 11, false), Grid { first_col: 0, days: 30, rows: 5 });
        assert_eq!(grid(2026, 11, true), Grid { first_col: 6, days: 30, rows: 6 });
        // Feb 2026 dimulai Minggu, 28 hari: tepat 4 baris bila pekan mulai Minggu
        assert_eq!(grid(2026, 2, false).rows, 4);
    }

    #[test]
    fn month_shift_wraps_years() {
        assert_eq!(shift_month(2026, 12, 1), (2027, 1));
        assert_eq!(shift_month(2026, 1, -1), (2025, 12));
        assert_eq!(shift_month(2026, 6, 0), (2026, 6));
        assert_eq!(shift_month(2026, 3, -15), (2024, 12));
    }

    #[test]
    fn labels_follow_week_start() {
        assert_eq!(weekday_labels(false)[0], "Min");
        assert_eq!(weekday_labels(true)[0], "Sen");
        assert_eq!(month_name(11), "November");
    }
}
