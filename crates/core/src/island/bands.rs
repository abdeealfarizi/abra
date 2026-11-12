//! Analisis spektrum untuk visualizer 5 pita: FFT 512 titik (radix-2 tulisan sendiri, tanpa dependensi),
//! jendela Hann, normalisasi adaptif per pita, dan penghalusan asimetris (naik cepat, turun pelan).

pub const FFT: usize = 512;
pub const BANDS: usize = 5;
/// Rentang bin per pita (bawah → atas) pada 48 kHz ≈ 94 Hz per bin.
const RANGES: [(usize, usize); BANDS] = [(1, 2), (2, 6), (6, 18), (18, 60), (60, 200)];
/// Pita tinggi berenergi kecil; bobot mengimbangi kemiringan spektrum musik.
const WEIGHT: [f32; BANDS] = [1.2, 1.2, 1.5, 2.8, 5.0];
pub const FLOOR: f32 = 0.18;

fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    // pembalikan bit
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * std::f32::consts::PI / len as f32;
        let (wr, wi) = (ang.cos(), ang.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let (a, b) = (start + k, start + k + len / 2);
                let (tr, ti) = (re[b] * cr - im[b] * ci, re[b] * ci + im[b] * cr);
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let nr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = nr;
            }
        }
        len <<= 1;
    }
}

pub struct Analyzer {
    buf: Vec<f32>,
    fill: usize,
    peaks: [f32; BANDS],
    prev: [f32; BANDS],
    re: Vec<f32>,
    im: Vec<f32>,
}

impl Default for Analyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl Analyzer {
    pub fn new() -> Analyzer {
        Analyzer {
            buf: vec![0.0; FFT],
            fill: 0,
            peaks: [0.01; BANDS],
            prev: [0.1; BANDS],
            re: vec![0.0; FFT],
            im: vec![0.0; FFT],
        }
    }

    /// Masukkan sampel mono [-1, 1]. `emit` dipanggil tiap 512 sampel terkumpul.
    pub fn push(&mut self, mono: &[f32], mut emit: impl FnMut([f32; BANDS])) {
        for &s in mono {
            let w = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * self.fill as f32 / FFT as f32).cos());
            self.buf[self.fill] = s * w;
            self.fill += 1;
            if self.fill == FFT {
                self.fill = 0;
                emit(self.analyze());
            }
        }
    }

    fn analyze(&mut self) -> [f32; BANDS] {
        self.re.copy_from_slice(&self.buf);
        self.im.iter_mut().for_each(|v| *v = 0.0);
        fft(&mut self.re, &mut self.im);
        let mut out = [0.0; BANDS];
        for (i, &(lo, hi)) in RANGES.iter().enumerate() {
            let mut sum = 0.0;
            let mut n = 0u32;
            for b in lo..hi.min(FFT / 2) {
                sum += (self.re[b] * self.re[b] + self.im[b] * self.im[b]).sqrt();
                n += 1;
            }
            let scaled = sum / n.max(1) as f32 * WEIGHT[i];
            // pelacak puncak yang meluruh perlahan menormalkan loudness lagu
            if scaled > self.peaks[i] {
                self.peaks[i] = scaled;
            } else {
                self.peaks[i] *= 0.99;
            }
            let target = (scaled / self.peaks[i].max(0.12)).min(1.0).powf(0.75);
            let smooth = if target > self.prev[i] { 0.10 } else { 0.20 };
            let v = (self.prev[i] * smooth + target * (1.0 - smooth)).clamp(FLOOR, 1.0);
            self.prev[i] = v;
            out[i] = v;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, rate: f32, n: usize) -> Vec<f32> {
        (0..n).map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / rate).sin() * 0.8).collect()
    }

    fn run(samples: &[f32]) -> [f32; BANDS] {
        let mut a = Analyzer::new();
        let mut last = [0.0; BANDS];
        a.push(samples, |b| last = b);
        last
    }

    #[test]
    fn silence_stays_at_the_floor() {
        let out = run(&vec![0.0; FFT * 8]);
        assert!(out.iter().all(|v| (*v - FLOOR).abs() < 1e-6), "{out:?}");
    }

    #[test]
    fn fft_places_a_pure_tone_in_the_right_bin() {
        // 48 kHz, bin 10 = 937,5 Hz → pita ke-3 (indeks 2: bin 6..18)
        let tone = sine(937.5, 48_000.0, FFT * 30);
        let out = run(&tone);
        let top = out.iter().enumerate().max_by(|a, b| a.1.partial_cmp(b.1).unwrap()).unwrap().0;
        assert_eq!(top, 2, "{out:?}");
        assert!(out[2] > 0.9, "{out:?}");
    }

    #[test]
    fn low_tone_lands_in_the_low_band_and_not_in_the_highest() {
        let out = run(&sine(280.0, 48_000.0, FFT * 30)); // bin ≈ 3 → indeks 1
        assert!(out[1] > out[4], "{out:?}");
        assert_eq!(out.iter().enumerate().max_by(|a, b| a.1.partial_cmp(b.1).unwrap()).unwrap().0, 1);
    }

    #[test]
    fn output_is_always_within_bounds() {
        let noisy: Vec<f32> = (0..FFT * 10).map(|i| (((i * 7919) % 2000) as f32 / 1000.0) - 1.0).collect();
        let mut a = Analyzer::new();
        a.push(&noisy, |b| assert!(b.iter().all(|v| (FLOOR..=1.0).contains(v)), "{b:?}"));
    }

    #[test]
    fn emits_one_frame_per_512_samples() {
        let mut a = Analyzer::new();
        let mut n = 0;
        a.push(&vec![0.1; FFT * 3 + 100], |_| n += 1);
        assert_eq!(n, 3);
        a.push(&vec![0.1; FFT - 100], |_| n += 1);
        assert_eq!(n, 4, "sisa sampel dibawa ke panggilan berikutnya");
    }
}
