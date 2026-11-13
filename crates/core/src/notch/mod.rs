//! Notch ABRA generasi baru, dirancang sendiri (clean room): hanya fungsi yang ditiru dari aplikasi notch lain, bentuk,
//! ukuran, warna, dan animasinya memakai bahasa visual ABRA. Modul ini murni dan diuji; jendela dan gambarnya ada di app.
//!
//! - [`state`]: mesin keadaan (diam, intip, aktivitas ringkas, panel terbuka, zona taruh), ukuran, dan konstanta animasi.
//! - [`lyrics`]: pengurai lirik LRC dan baris yang aktif pada posisi lagu.
//! - [`lrclib`]: alamat kueri LRCLIB, pembaca jawabannya, dan kunci cache.
//! - [`json`]: pembaca JSON kecil untuk jawaban LRCLIB.
//! - [`shelf`]: Rak berkas yang ditaruh ke notch.
//! - [`tint`]: warna utama dari sampul album untuk visualizer.
//! - [`shade`]: bitmap bayangan tipis (nine-grid) di bawah notch.
//! - [`download`]: aktivitas unduhan (busur pengukur, angka, manifes Steam).
//! - [`glance`]: notch diam "sapaan & info" (sapaan menurut jam, jam, tanggal, baterai).

pub mod download;
pub mod send;
pub mod glance;
pub mod json;
pub mod lrclib;
pub mod lyrics;
pub mod shade;
pub mod shelf;
pub mod state;
pub mod tint;
pub mod typeset;
