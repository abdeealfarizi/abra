//! Encoder kode QR (ISO/IEC 18004) tanpa dependensi, untuk "Kirim ke HP": mode byte, koreksi galat M, versi 1–10.
//! Mengikuti algoritma baku: aliran bit → Reed-Solomon GF(256) per blok → sisip-silang → penempatan zig-zag →
//! masker dengan skor penalti terendah → info format/versi.

/// Matriks QR persegi; `true` = modul gelap.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Matrix {
    pub size: usize,
    pub version: u8,
    pub mask: u8,
    pub modules: Vec<bool>,
}

impl Matrix {
    pub fn get(&self, x: usize, y: usize) -> bool {
        self.modules[y * self.size + x]
    }
}

/// Struktur blok koreksi galat M per versi: (codeword EC per blok, [(jumlah blok, codeword data per blok)]).
fn blocks(version: u8) -> (usize, &'static [(usize, usize)]) {
    match version {
        1 => (10, &[(1, 16)]),
        2 => (16, &[(1, 28)]),
        3 => (26, &[(1, 44)]),
        4 => (18, &[(2, 32)]),
        5 => (24, &[(2, 43)]),
        6 => (16, &[(4, 27)]),
        7 => (18, &[(4, 31)]),
        8 => (22, &[(2, 38), (2, 39)]),
        9 => (22, &[(3, 36), (2, 37)]),
        _ => (26, &[(4, 43), (1, 44)]),
    }
}

fn data_capacity(version: u8) -> usize {
    blocks(version).1.iter().map(|(n, d)| n * d).sum()
}

fn alignment_positions(version: u8) -> &'static [usize] {
    match version {
        1 => &[],
        2 => &[6, 18],
        3 => &[6, 22],
        4 => &[6, 26],
        5 => &[6, 30],
        6 => &[6, 34],
        7 => &[6, 22, 38],
        8 => &[6, 24, 42],
        9 => &[6, 26, 46],
        _ => &[6, 28, 50],
    }
}

// ---------- Reed-Solomon GF(256), polinomial 0x11D ----------

fn gf_mul(a: u8, b: u8) -> u8 {
    let (mut a, mut b, mut r) = (a as u16, b, 0u16);
    while b != 0 {
        if b & 1 != 0 {
            r ^= a;
        }
        a <<= 1;
        if a & 0x100 != 0 {
            a ^= 0x11D;
        }
        b >>= 1;
    }
    r as u8
}

/// Codeword koreksi galat sebanyak `n` untuk `data`.
pub fn rs_ecc(data: &[u8], n: usize) -> Vec<u8> {
    // generator: hasil kali (x - α^i), i = 0..n-1; koefisien dari derajat tertinggi (tanpa koefisien utama 1)
    let mut gen = vec![0u8; n];
    gen[n - 1] = 1;
    let mut root = 1u8;
    for _ in 0..n {
        for j in 0..n {
            gen[j] = gf_mul(gen[j], root);
            if j + 1 < n {
                gen[j] ^= gen[j + 1];
            }
        }
        root = gf_mul(root, 2);
    }
    let mut rem = vec![0u8; n];
    for &b in data {
        let factor = b ^ rem[0];
        rem.remove(0);
        rem.push(0);
        for (r, g) in rem.iter_mut().zip(&gen) {
            *r ^= gf_mul(*g, factor);
        }
    }
    rem
}

// ---------- info format & versi ----------

/// 15 bit info format untuk tingkat M (bit 00) dan masker `mask`.
pub fn format_bits(mask: u8) -> u16 {
    let data = mask as u32; // tingkat M = 00
    let mut rem = data;
    for _ in 0..10 {
        rem = (rem << 1) ^ ((rem >> 9) * 0x537);
    }
    (((data << 10) | rem) ^ 0x5412) as u16
}

/// 18 bit info versi (versi ≥ 7).
pub fn version_bits(version: u8) -> u32 {
    let mut rem = version as u32;
    for _ in 0..12 {
        rem = (rem << 1) ^ ((rem >> 11) * 0x1F25);
    }
    ((version as u32) << 12) | rem
}

// ---------- matriks ----------

struct Grid {
    size: usize,
    dark: Vec<bool>,
    func: Vec<bool>,
}

impl Grid {
    fn set(&mut self, x: usize, y: usize, dark: bool) {
        let i = y * self.size + x;
        self.dark[i] = dark;
        self.func[i] = true;
    }

    fn finder(&mut self, cx: i32, cy: i32) {
        for dy in -4i32..=4 {
            for dx in -4i32..=4 {
                let (x, y) = (cx + dx, cy + dy);
                if x < 0 || y < 0 || x >= self.size as i32 || y >= self.size as i32 {
                    continue;
                }
                let d = dx.abs().max(dy.abs());
                self.set(x as usize, y as usize, d != 2 && d != 4);
            }
        }
    }

    fn alignment(&mut self, cx: usize, cy: usize) {
        for dy in -2i32..=2 {
            for dx in -2i32..=2 {
                self.set((cx as i32 + dx) as usize, (cy as i32 + dy) as usize, dx.abs().max(dy.abs()) != 1);
            }
        }
    }

    fn format(&mut self, mask: u8) {
        let bits = format_bits(mask);
        let bit = |i: u32| (bits >> i) & 1 != 0;
        let s = self.size;
        for i in 0..=5 {
            self.set(8, i, bit(i as u32));
        }
        self.set(8, 7, bit(6));
        self.set(8, 8, bit(7));
        self.set(7, 8, bit(8));
        for i in 9..15 {
            self.set(14 - i, 8, bit(i as u32));
        }
        for i in 0..8 {
            self.set(s - 1 - i, 8, bit(i as u32));
        }
        for i in 8..15 {
            self.set(8, s - 15 + i, bit(i as u32));
        }
        self.set(8, s - 8, true); // modul gelap tetap
    }

    fn version(&mut self, version: u8) {
        if version < 7 {
            return;
        }
        let bits = version_bits(version);
        let s = self.size;
        for i in 0..18 {
            let dark = (bits >> i) & 1 != 0;
            let (a, b) = (s - 11 + i % 3, i / 3);
            self.set(a, b, dark);
            self.set(b, a, dark);
        }
    }
}

fn mask_bit(mask: u8, x: usize, y: usize) -> bool {
    match mask {
        0 => (x + y) % 2 == 0,
        1 => y % 2 == 0,
        2 => x % 3 == 0,
        3 => (x + y) % 3 == 0,
        4 => (x / 3 + y / 2) % 2 == 0,
        5 => x * y % 2 + x * y % 3 == 0,
        6 => (x * y % 2 + x * y % 3) % 2 == 0,
        _ => ((x + y) % 2 + x * y % 3) % 2 == 0,
    }
}

/// Skor penalti baku (N1 deret, N2 kotak 2×2, N3 pola mirip finder, N4 keseimbangan gelap/terang).
fn penalty(size: usize, m: &[bool]) -> u32 {
    let at = |x: usize, y: usize| m[y * size + x];
    let mut score = 0u32;
    for horizontal in [true, false] {
        for a in 0..size {
            let line: Vec<bool> = (0..size).map(|b| if horizontal { at(b, a) } else { at(a, b) }).collect();
            let mut run = 1;
            for i in 1..=size {
                if i < size && line[i] == line[i - 1] {
                    run += 1;
                } else {
                    if run >= 5 {
                        score += 3 + (run - 5) as u32;
                    }
                    run = 1;
                }
            }
            // 1011101 dengan 4 modul terang di salah satu sisi
            let pat = [true, false, true, true, true, false, true];
            for i in 0..size.saturating_sub(6) {
                if (0..7).all(|k| line[i + k] == pat[k]) {
                    let light = |from: i32, to: i32| (from..to).all(|k| k < 0 || k >= size as i32 || !line[k as usize]);
                    if light(i as i32 - 4, i as i32) || light(i as i32 + 7, i as i32 + 11) {
                        score += 40;
                    }
                }
            }
        }
    }
    for y in 0..size - 1 {
        for x in 0..size - 1 {
            let c = at(x, y);
            if c == at(x + 1, y) && c == at(x, y + 1) && c == at(x + 1, y + 1) {
                score += 3;
            }
        }
    }
    let dark = m.iter().filter(|d| **d).count() as i64;
    let total = (size * size) as i64;
    let k = ((dark * 20 - total * 10).abs() + total - 1) / total - 1;
    score + (k.max(0) as u32) * 10
}

/// Kode QR untuk `text` (mode byte, tingkat M), versi terkecil yang muat sampai versi 10; `None` bila terlalu panjang.
pub fn encode(text: &str) -> Option<Matrix> {
    let bytes = text.as_bytes();
    let version = (1u8..=10).find(|v| {
        let count_bits = if *v < 10 { 8 } else { 16 };
        4 + count_bits + bytes.len() * 8 <= data_capacity(*v) * 8
    })?;
    let cap = data_capacity(version);
    // aliran bit
    let mut bits: Vec<bool> = Vec::with_capacity(cap * 8);
    let mut push = |v: u32, n: usize, bits: &mut Vec<bool>| {
        for i in (0..n).rev() {
            bits.push((v >> i) & 1 != 0);
        }
    };
    push(0b0100, 4, &mut bits);
    push(bytes.len() as u32, if version < 10 { 8 } else { 16 }, &mut bits);
    for b in bytes {
        push(*b as u32, 8, &mut bits);
    }
    let room = cap * 8 - bits.len();
    push(0, room.min(4), &mut bits);
    while bits.len() % 8 != 0 {
        bits.push(false);
    }
    let mut data: Vec<u8> = bits.chunks(8).map(|c| c.iter().fold(0u8, |a, b| (a << 1) | *b as u8)).collect();
    let mut pad = [0xECu8, 0x11].into_iter().cycle();
    while data.len() < cap {
        data.push(pad.next().unwrap());
    }
    // blok + koreksi galat, lalu sisip-silang
    let (ec_n, layout) = blocks(version);
    let mut dblocks: Vec<&[u8]> = Vec::new();
    let mut off = 0;
    for (n, len) in layout {
        for _ in 0..*n {
            dblocks.push(&data[off..off + len]);
            off += len;
        }
    }
    let eblocks: Vec<Vec<u8>> = dblocks.iter().map(|b| rs_ecc(b, ec_n)).collect();
    let mut stream = Vec::new();
    let max_d = dblocks.iter().map(|b| b.len()).max().unwrap_or(0);
    for i in 0..max_d {
        for b in &dblocks {
            if i < b.len() {
                stream.push(b[i]);
            }
        }
    }
    for i in 0..ec_n {
        for b in &eblocks {
            stream.push(b[i]);
        }
    }

    // pola fungsi
    let size = 17 + 4 * version as usize;
    let mut g = Grid { size, dark: vec![false; size * size], func: vec![false; size * size] };
    for i in 0..size {
        g.set(6, i, i % 2 == 0);
        g.set(i, 6, i % 2 == 0);
    }
    g.finder(3, 3);
    g.finder(size as i32 - 4, 3);
    g.finder(3, size as i32 - 4);
    let al = alignment_positions(version);
    for (i, &ay) in al.iter().enumerate() {
        for (j, &ax) in al.iter().enumerate() {
            let last = al.len() - 1;
            if (i == 0 && j == 0) || (i == 0 && j == last) || (i == last && j == 0) {
                continue;
            }
            g.alignment(ax, ay);
        }
    }
    g.format(0);
    g.version(version);

    // codeword: zig-zag dua kolom dari kanan bawah, kolom timing dilewati
    let mut idx = 0usize;
    let mut right = size as i32 - 1;
    while right >= 1 {
        if right == 6 {
            right = 5;
        }
        for vert in 0..size {
            for j in 0..2 {
                let x = (right - j) as usize;
                let upward = (right + 1) & 2 == 0;
                let y = if upward { size - 1 - vert } else { vert };
                if !g.func[y * size + x] && idx < stream.len() * 8 {
                    g.dark[y * size + x] = (stream[idx >> 3] >> (7 - (idx & 7))) & 1 != 0;
                    idx += 1;
                }
            }
        }
        right -= 2;
    }

    // masker terbaik
    let mut best: Option<(u32, u8, Vec<bool>)> = None;
    for mask in 0..8u8 {
        let mut t = Grid { size, dark: g.dark.clone(), func: g.func.clone() };
        for y in 0..size {
            for x in 0..size {
                if !t.func[y * size + x] && mask_bit(mask, x, y) {
                    t.dark[y * size + x] ^= true;
                }
            }
        }
        t.format(mask);
        let p = penalty(size, &t.dark);
        if best.as_ref().is_none_or(|(bp, _, _)| p < *bp) {
            best = Some((p, mask, t.dark));
        }
    }
    let (_, mask, modules) = best?;
    Some(Matrix { size, version, mask, modules })
}

/// Bitmap BGRA (ter-premultiply) `n`×`n` px: modul hitam di atas putih dengan zona tenang `quiet` modul. Ukuran modul
/// dibulatkan ke piksel utuh agar tajam dan mudah dipindai; sisa ruang dibagi rata di tepi.
pub fn render(m: &Matrix, n: u32, quiet: usize) -> Vec<u8> {
    let cells = m.size + 2 * quiet;
    let unit = (n as usize / cells).max(1);
    let off = (n as usize).saturating_sub(unit * cells) / 2 + quiet * unit;
    let mut out = vec![255u8; (n * n * 4) as usize];
    for y in 0..m.size {
        for x in 0..m.size {
            if !m.get(x, y) {
                continue;
            }
            for py in 0..unit {
                for px in 0..unit {
                    let (ix, iy) = (off + x * unit + px, off + y * unit + py);
                    if ix < n as usize && iy < n as usize {
                        let i = (iy * n as usize + ix) * 4;
                        out[i] = 0;
                        out[i + 1] = 0;
                        out[i + 2] = 0;
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reed_solomon_known_vector() {
        // contoh baku "HELLO WORLD" 1-M (mode alfanumerik): 16 codeword data → 10 codeword koreksi
        let data = [32, 91, 11, 120, 209, 114, 220, 77, 67, 64, 236, 17, 236, 17, 236, 17];
        assert_eq!(rs_ecc(&data, 10), vec![196, 35, 39, 119, 235, 215, 231, 226, 93, 23]);
    }

    #[test]
    fn format_and_version_info() {
        assert_eq!(format_bits(0), 0b101010000010010);
        assert_eq!(format_bits(2), 0b101111001111100);
        assert_eq!(version_bits(7), 0x07C94);
        assert_eq!(version_bits(10), 0x0A4D3);
    }

    #[test]
    fn picks_smallest_version_and_draws_fixed_patterns() {
        let m = encode("http://192.168.1.23:54321/0123456789abcdef0123/").unwrap();
        assert_eq!(m.version, 4, "URL kirim (±48 karakter) muat di versi 4");
        assert_eq!(m.size, 33);
        // finder kiri atas: cincin gelap luar, terang, inti gelap 3×3
        for i in 0..7 {
            assert!(m.get(i, 0) && m.get(0, i) && m.get(6, i) && m.get(i, 6));
        }
        assert!(!m.get(1, 1) && m.get(3, 3) && !m.get(7, 7));
        // timing berselang-seling dan modul gelap tetap
        for i in 8..m.size - 8 {
            assert_eq!(m.get(i, 6), i % 2 == 0);
            assert_eq!(m.get(6, i), i % 2 == 0);
        }
        assert!(m.get(8, m.size - 8));
        // info format terbaca kembali sebagai masker yang dipakai
        let mut f = 0u16;
        for (k, (x, y)) in [(8, 0), (8, 1), (8, 2), (8, 3), (8, 4), (8, 5), (8, 7), (8, 8), (7, 8), (5, 8), (4, 8), (3, 8), (2, 8), (1, 8), (0, 8)].iter().enumerate() {
            if m.get(*x, *y) {
                f |= 1 << k;
            }
        }
        assert_eq!(f, format_bits(m.mask));
        assert!(encode("").is_some());
        assert!(encode(&"x".repeat(300)).is_none(), "di atas kapasitas versi 10");
    }

    /// Uji manual dengan pemindai sungguhan: `NACRE_QR_DUMP=<folder> cargo test -p nacre-core dump -- --ignored`
    /// menulis beberapa QR sebagai PGM untuk didekode (mis. OpenCV); isi teks disimpan di nama berkas .txt pendamping.
    #[test]
    #[ignore]
    fn dump_for_decoder() {
        let Ok(dir) = std::env::var("NACRE_QR_DUMP") else { return };
        let samples = ["http://192.168.1.23:54321/0123456789abcdef0123/", "ABRA", "http://10.0.0.5:61234/ffffffffffffffffffff/", &"x".repeat(200)];
        for (i, s) in samples.iter().enumerate() {
            let m = encode(s).unwrap();
            let n = 400u32;
            let b = render(&m, n, 4);
            let mut pgm = format!("P5\n{n} {n}\n255\n").into_bytes();
            pgm.extend(b.chunks(4).map(|p| p[0]));
            std::fs::write(format!("{dir}/qr{i}.pgm"), pgm).unwrap();
            std::fs::write(format!("{dir}/qr{i}.txt"), s).unwrap();
        }
    }

    #[test]
    fn render_is_sharp_and_square() {
        let m = encode("ABRA").unwrap();
        let n = 100;
        let b = render(&m, n, 2);
        assert_eq!(b.len(), (n * n * 4) as usize);
        assert!(b.chunks(4).all(|p| p[0] == 0 || p[0] == 255), "hanya hitam/putih");
        // pojok kiri atas = zona tenang putih
        assert_eq!(&b[0..4], &[255, 255, 255, 255]);
    }
}
