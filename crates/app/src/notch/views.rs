//! Isi notch untuk tiap tampilan. Semua koordinat px jendela notch; `Frame` = badan notch yang terlihat.
//! Gaya ABRA: kartu kaca tipis berradius besar di dalam panel, kontrol segmen kapsul, simbol ABRA.

use super::draw::{white, Anchor, Painter, WHITE};
use crate::prefs::tr;
use nacre_core::island::timer::{format_mmss, Phase, Timer};
use nacre_core::notch::glance::Item;
use nacre_core::notch::state::{Activity, Look, Tab, View};
use nacre_core::symbols::Sym;
use windows::UI::Color;
use windows::UI::Composition::{CompositionDrawingSurface, ContainerVisual, SpriteVisual};
use windows_numerics::Vector3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    PlayPause,
    Next,
    Prev,
    /// Penggeser posisi lagu: x awal dan lebar jalur (px).
    Seek(f32, f32),
    Tab(Tab),
    /// Berkas ke-i di Rak (klik = buka, klik kanan = menu).
    ShelfItem(usize),
    ShelfClear,
    /// Zona taruh ke-i (hanya untuk menentukan zona di bawah kursor).
    Zone(usize),
    Settings,
    TimerToggle,
    TimerReset,
    TimerPlus,
    TimerMinus,
    /// Strip tanggal di kartu kalender: geser pilihan sebanyak n hari.
    CalDay(i32),
    /// Akhiri sesi "Kirim ke HP".
    SendDone,
}

pub struct Region {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub action: Action,
}

impl Region {
    pub fn hit(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

#[derive(Clone, Copy)]
pub struct Frame {
    pub x0: f32,
    pub w: f32,
    pub h: f32,
}

pub struct Data<'a> {
    pub battery: Option<(u8, bool)>,
    pub timer: &'a Timer,
    pub now_ms: i64,
    /// Item sapaan & info yang sedang tampil beserta teksnya.
    pub glance: Option<(Item, String)>,
    pub media: Option<MediaView<'a>>,
    pub lyrics: LyricsView<'a>,
    /// Berkas di Rak: (nama, ikon).
    pub shelf: Vec<(&'a str, Option<&'a CompositionDrawingSurface>)>,
    /// Zona taruh yang sedang di bawah kursor dan jumlah berkas yang diseret.
    pub drop_zone: Option<usize>,
    pub drop_count: usize,
    /// Mikrofon / kamera sedang dipakai app lain.
    pub privacy: (bool, bool),
    /// Unduhan yang ditampilkan (berjalan atau baru selesai).
    pub download: Option<DownloadView<'a>>,
    /// Kartu kalender di tab Timer.
    pub cal: CalView<'a>,
    /// Sesi "Kirim ke HP" (menggantikan isi tab Rak selama aktif) dan apakah zona Kirim ditampilkan saat menyeret.
    pub send: Option<SendView<'a>>,
    pub send_zone: bool,
}

/// Kartu Kirim: QR di kiri, nama berkas, petunjuk, status + bilah kemajuan, tombol Selesai.
pub struct SendView<'a> {
    pub qr: Option<&'a CompositionDrawingSurface>,
    pub qr_px: f32,
    pub title: String,
    pub status: String,
    pub frac: f32,
    pub done: bool,
    /// Sesi tidak bisa dimulai (mis. tidak di jaringan lokal).
    pub error: bool,
}

pub type Date = (i32, u32, u32);

/// Kalender di samping timer: strip lima tanggal di sekitar tanggal terpilih dan acara tanggal itu.
pub struct CalView<'a> {
    pub today: Date,
    pub sel: Date,
    pub events: Events<'a>,
}

pub enum Events<'a> {
    Loading,
    /// Kalender Windows tidak bisa dibaca.
    Unavailable,
    List(&'a [nacre_platform::agenda::Event]),
}

/// Unduhan di notch ringkas: ikon app pengunduh di kiri, busur + angka di kanan.
pub struct DownloadView<'a> {
    pub icon: Option<&'a CompositionDrawingSurface>,
    /// Angka di bawah busur (persen tanpa "%", atau ukuran untuk peramban).
    pub label: String,
    /// 0..1, atau `None` = total tidak diketahui (busur berputar).
    pub frac: Option<f32>,
    pub done: bool,
    /// Mengirim ke HP (panah atas), bukan mengunduh.
    pub up: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LyrState {
    /// Tidak ada lagu.
    Idle,
    Loading,
    Ready,
    Missing,
}

/// Lirik lagu yang sedang diputar.
pub struct LyricsView<'a> {
    pub lines: Option<&'a nacre_core::notch::lyrics::Lyrics>,
    pub state: LyrState,
    pub pos_ms: i64,
    /// Sumbernya LRCLIB (dicantumkan kecil di kartu lirik).
    pub lrclib: bool,
}

/// Lagu yang sedang ada di sesi media Windows.
pub struct MediaView<'a> {
    pub title: &'a str,
    pub artist: &'a str,
    pub playing: bool,
    pub pos_ms: i64,
    pub dur_ms: i64,
    pub can_seek: bool,
    pub art: Option<&'a CompositionDrawingSurface>,
    /// Warna dari sampul (None = pakai aksen).
    pub tint: Option<[u8; 3]>,
    /// Sedang diseret: posisi 0..1 di penggeser.
    pub drag: Option<f32>,
}

/// Visual yang dibuat `draw` dan masih perlu digerakkan kontroler (bar visualizer).
#[derive(Default)]
pub struct Live {
    pub bars: Vec<SpriteVisual>,
    /// Baris lirik di notch ringkas (dianimasikan saat berganti).
    pub lyric: Option<SpriteVisual>,
    /// Wadah lirik besar di tab Putar dan jarak gulirnya saat maju satu baris.
    pub lyric_scroll: Option<windows::UI::Composition::ContainerVisual>,
    pub lyric_shift: f32,
}

const ORANGE: [u8; 3] = [255, 159, 10];
const GREEN: [u8; 3] = [48, 209, 88];
const INK: [u8; 3] = [22, 22, 24];

/// Kartu kaca tipis di dalam panel.
fn card(p: &Painter, c: &ContainerVisual, x: f32, y: f32, w: f32, h: f32) {
    p.rect(c, x, y, w, h, p.px(18.0), white(18));
}

pub fn draw(p: &mut Painter, c: &ContainerVisual, look: Look, f: Frame, d: &Data, regions: &mut Vec<Region>, live: &mut Live) {
    match look.view {
        View::Rest => {}
        View::Compact(Activity::Timer) | View::Compact(Activity::TimerEnding) => {
            let urgent = look.view == View::Compact(Activity::TimerEnding);
            compact_timer(p, c, f, d, urgent)
        }
        View::Compact(Activity::Charging) => compact_charging(p, c, f, d),
        View::Compact(Activity::Glance) => compact_glance(p, c, f, d),
        View::Compact(Activity::Music) => compact_music(p, c, f, d, live, f.h),
        View::Compact(Activity::Lyrics) => compact_lyrics(p, c, f, d, live),
        View::Compact(Activity::Shelf) => compact_shelf(p, c, f, d),
        View::Compact(Activity::Download) => compact_download(p, c, f, d),
        View::Compact(Activity::Player) => compact_player(p, c, f, d, regions, live),
        View::Open(tab) => {
            header(p, c, f, tab, d, regions);
            match tab {
                Tab::Play => play(p, c, f, d, regions, live),
                Tab::Shelf if d.send.is_some() => send_card(p, c, f, d, regions),
                Tab::Shelf if d.shelf.is_empty() => shelf_empty(p, c, f),
                Tab::Shelf => shelf_tab(p, c, f, d, regions),
                Tab::Timer => timer(p, c, f, d, regions),
            }
        }
        View::Drop => drop_zones(p, c, f, d, regions),
    }
    if matches!(look.view, View::Rest | View::Compact(_)) {
        privacy_dots(p, c, f, d.privacy);
    }
}

/// Titik oranye (mikrofon) / hijau (kamera) kecil di tepi kanan notch tertutup.
fn privacy_dots(p: &mut Painter, c: &ContainerVisual, f: Frame, (mic, cam): (bool, bool)) {
    let (r, cy) = (p.px(3.0), f.h / 2.0);
    let mut x = f.x0 + f.w - p.px(7.0);
    for (on, rgb) in [(mic, ORANGE), (cam, GREEN)] {
        if on {
            p.rect(c, x - r, cy - r, 2.0 * r, 2.0 * r, r, Color { A: 255, R: rgb[0], G: rgb[1], B: rgb[2] });
            x -= p.px(9.0);
        }
    }
}

pub(super) fn count_text(n: usize) -> String {
    if crate::prefs::get().lang == nacre_core::i18n::Lang::En {
        format!("{n} {}", if n == 1 { "file" } else { "files" })
    } else {
        format!("{n} berkas")
    }
}

/// Rak ringkas: ikon baki + jumlah berkas.
fn compact_shelf(p: &mut Painter, c: &ContainerVisual, f: Frame, d: &Data) {
    let cy = f.h / 2.0;
    p.sym(c, Sym::Tray, f.x0 + p.px(22.0), cy, 15.0, WHITE, 0.9);
    let th = p.px(12.5) * 1.3;
    p.text(c, &count_text(d.shelf.len()), f.x0 + f.w - p.px(16.0), cy - th / 2.0, 12.5, 600, WHITE, 0.95, Anchor::Right, f.w / 2.0);
}

/// Zona taruh saat berkas diseret ke notch: Simpan ke Rak · Salin ke… · Buka dengan….
fn drop_zones(p: &mut Painter, c: &ContainerVisual, f: Frame, d: &Data, regions: &mut Vec<Region>) {
    let (pad, gap) = (p.px(14.0), p.px(10.0));
    let all = [(0usize, Sym::Tray, "Simpan ke Rak"), (1, Sym::ArrowUp, "Kirim"), (2, Sym::Folder, "Salin ke\u{2026}"), (3, Sym::Grid, "Buka dengan\u{2026}")];
    let zones: Vec<_> = all.into_iter().filter(|z| z.0 != 1 || d.send_zone).collect();
    let n = zones.len() as f32;
    let zw = (f.w - 2.0 * pad - (n - 1.0) * gap) / n;
    let (top, zh) = (pad, f.h - 2.0 * pad);
    let accent = crate::prefs::accent();
    for (k, (i, s, label)) in zones.iter().enumerate() {
        let i = *i;
        let x = f.x0 + pad + k as f32 * (zw + gap);
        let on = d.drop_zone == Some(i);
        let fill = if on { Color { A: 230, R: accent[0], G: accent[1], B: accent[2] } } else { white(22) };
        p.rect(c, x, top, zw, zh, p.px(18.0), fill);
        p.sym(c, *s, x + zw / 2.0, top + zh / 2.0 - p.px(12.0), 22.0, WHITE, if on { 1.0 } else { 0.85 });
        let th = p.px(12.5) * 1.3;
        p.text(c, tr(label), x + zw / 2.0, top + zh / 2.0 + p.px(10.0) - th / 2.0 + p.px(6.0), 12.5, 600, WHITE, if on { 1.0 } else { 0.85 }, Anchor::Center, zw - p.px(12.0));
        regions.push(Region { x, y: top, w: zw, h: zh, action: Action::Zone(i) });
    }
    if d.drop_count > 0 {
        let th = p.px(10.5) * 1.3;
        p.text(c, &count_text(d.drop_count), f.x0 + f.w / 2.0, f.h - pad / 2.0 - th / 2.0 - p.px(1.0), 10.5, 500, WHITE, 0.55, Anchor::Center, f.w);
    }
}

/// Kartu "Kirim ke HP": QR (di atas pelat putih agar mudah dipindai) di kiri; kanan: nama berkas, petunjuk, status
/// dengan bilah kemajuan hijau, catatan firewall, dan tombol Selesai.
fn send_card(p: &mut Painter, c: &ContainerVisual, f: Frame, d: &Data, regions: &mut Vec<Region>) {
    let Some(sv) = &d.send else { return };
    let pad = p.px(14.0);
    let (top, h) = (p.px(48.0), f.h - p.px(48.0) - pad);
    let (x0, w) = (f.x0 + pad, f.w - 2.0 * pad);
    card(p, c, x0, top, w, h);
    let s = h - p.px(16.0);
    let (qx, qy) = (x0 + p.px(8.0), top + p.px(8.0));
    if sv.error {
        p.rect(c, qx, qy, s, s, p.px(14.0), white(20));
        p.sym(c, Sym::Wifi, qx + s / 2.0, qy + s / 2.0, 30.0, WHITE, 0.45);
    } else {
        p.rect(c, qx, qy, s, s, p.px(14.0), Color { A: 255, R: 255, G: 255, B: 255 });
        if let Some(q) = sv.qr {
            let inset = ((s - sv.qr_px) / 2.0).round();
            p.place(c, q, qx + inset, qy + inset, sv.qr_px, sv.qr_px, 1.0);
        }
    }
    let tx = qx + s + p.px(18.0);
    let tw = x0 + w - p.px(16.0) - tx;
    let accent = crate::prefs::accent();
    if sv.error {
        p.text(c, tr("Tidak tersambung ke jaringan lokal"), tx, top + p.px(16.0), 14.0, 700, WHITE, 1.0, Anchor::Left, tw);
        p.text(c, tr("Sambungkan laptop ke Wi-Fi yang sama dengan HP."), tx, top + p.px(40.0), 12.0, 500, WHITE, 0.6, Anchor::Left, tw);
    } else {
        p.text(c, &p.fit(&sv.title, 14.0, 700, tw), tx, top + p.px(12.0), 14.0, 700, WHITE, 1.0, Anchor::Left, tw + p.px(4.0));
        p.text(c, &p.fit(tr("Pindai QR dengan kamera HP (Wi-Fi yang sama)"), 11.5, 500, tw), tx, top + p.px(33.0), 11.5, 500, WHITE, 0.6, Anchor::Left, tw + p.px(4.0));
        let green = Color { A: 255, R: GREEN[0], G: GREEN[1], B: GREEN[2] };
        p.text(c, &sv.status, tx, top + p.px(55.0), 12.0, 600, if sv.done { GREEN } else { WHITE }, 0.95, Anchor::Left, tw);
        // bilah kemajuan berhenti sebelum tombol Selesai
        let (by, bh) = (top + p.px(76.0), p.px(4.0));
        let bar = tw - p.measure(tr("Selesai"), 12.0, 600) - p.px(28.0 + 16.0);
        p.rect(c, tx, by, bar, bh, bh / 2.0, white(30));
        if sv.frac > 0.0 {
            p.rect(c, tx, by, (bar * sv.frac.clamp(0.0, 1.0)).max(bh), bh, bh / 2.0, green);
        }
        p.text(c, tr("Izinkan ABRA di Windows Firewall bila diminta."), tx, top + h - p.px(22.0), 10.0, 500, WHITE, 0.4, Anchor::Left, tw - p.px(100.0));
    }
    // tombol Selesai
    let label = tr("Selesai");
    let (bw, bh) = (p.measure(label, 12.0, 600) + p.px(28.0), p.px(28.0));
    let (bx, by) = (x0 + w - p.px(14.0) - bw, top + h - p.px(12.0) - bh);
    p.rect(c, bx, by, bw, bh, bh / 2.0, Color { A: 255, R: accent[0], G: accent[1], B: accent[2] });
    let th = p.px(12.0) * 1.3;
    p.text(c, label, bx + bw / 2.0, by + bh / 2.0 - th / 2.0, 12.0, 600, WHITE, 1.0, Anchor::Center, bw);
    regions.push(Region { x: bx, y: by, w: bw, h: bh, action: Action::SendDone });
}

/// Tab Rak berisi berkas: ubin ikon + nama. Klik = buka, klik kanan = menu.
fn shelf_tab(p: &mut Painter, c: &ContainerVisual, f: Frame, d: &Data, regions: &mut Vec<Region>) {
    let pad = p.px(14.0);
    let (top, h) = (p.px(48.0), f.h - p.px(48.0) - pad);
    let (x0, w) = (f.x0 + pad, f.w - 2.0 * pad);
    card(p, c, x0, top, w, h);
    p.text(c, &count_text(d.shelf.len()), x0 + p.px(14.0), top + p.px(9.0), 11.5, 600, WHITE, 0.5, Anchor::Left, w / 2.0);
    let clear = tr("Kosongkan");
    let cw = p.measure(clear, 11.5, 600) + p.px(4.0);
    let accent = crate::prefs::accent();
    p.text(c, clear, x0 + w - p.px(14.0), top + p.px(9.0), 11.5, 600, accent, 0.95, Anchor::Right, cw + 4.0);
    regions.push(Region { x: x0 + w - p.px(14.0) - cw - p.px(6.0), y: top + p.px(2.0), w: cw + p.px(12.0), h: p.px(24.0), action: Action::ShelfClear });
    let (tw, th, gap) = (p.px(96.0), p.px(78.0), p.px(8.0));
    let ty = top + p.px(28.0);
    let max = ((w - p.px(20.0) + gap) / (tw + gap)).floor().max(1.0) as usize;
    let extra = d.shelf.len().saturating_sub(max);
    for (i, (name, icon)) in d.shelf.iter().take(max).enumerate() {
        let x = x0 + p.px(10.0) + i as f32 * (tw + gap);
        if extra > 0 && i == max - 1 {
            p.rect(c, x, ty, tw, th, p.px(14.0), white(14));
            let t = p.px(13.0) * 1.3;
            p.text(c, &format!("+{}", extra + 1), x + tw / 2.0, ty + th / 2.0 - t / 2.0, 13.0, 600, WHITE, 0.7, Anchor::Center, tw);
            break;
        }
        let isz = p.px(40.0);
        match icon {
            Some(s) => {
                p.image(c, s, x + (tw - isz) / 2.0, ty + p.px(6.0), isz, isz, p.px(6.0));
            }
            None => p.sym(c, Sym::Folder, x + tw / 2.0, ty + p.px(26.0), 26.0, WHITE, 0.7),
        }
        let shown = p.fit(name, 11.0, 500, tw - p.px(8.0));
        p.text(c, &shown, x + tw / 2.0, ty + p.px(52.0), 11.0, 500, WHITE, 0.9, Anchor::Center, tw - p.px(6.0));
        regions.push(Region { x, y: ty, w: tw, h: th, action: Action::ShelfItem(i) });
    }
}

// ---------- ringkas ----------

fn compact_timer(p: &mut Painter, c: &ContainerVisual, f: Frame, d: &Data, urgent: bool) {
    let cy = f.h / 2.0;
    let rgb = if urgent { ORANGE } else { WHITE };
    p.sym(c, Sym::Stopwatch, f.x0 + p.px(20.0), cy, 15.0, rgb, 0.95);
    let t = format_mmss(d.timer.remaining_secs(d.now_ms));
    let th = p.px(13.0) * 1.3;
    p.text(c, &t, f.x0 + f.w - p.px(16.0), cy - th / 2.0, 13.0, 600, rgb, 1.0, Anchor::Right, f.w);
}

fn accent_or(t: Option<[u8; 3]>) -> [u8; 3] {
    t.unwrap_or_else(crate::prefs::accent)
}

/// Teks baris lirik yang sedang dinyanyikan; "♪" sebelum baris pertama dan saat jeda instrumental.
fn current_line(d: &Data) -> String {
    match d.lyrics.lines {
        Some(l) => l.current(d.lyrics.pos_ms).unwrap_or("\u{266A}").to_string(),
        None => String::new(),
    }
}

/// Lirik ringkas: baris atas = musik ringkas, baris bawah = baris lirik yang sedang dinyanyikan, di tengah.
fn compact_lyrics(p: &mut Painter, c: &ContainerVisual, f: Frame, d: &Data, live: &mut Live) {
    compact_music(p, c, f, d, live, f.h);
    // lirik di tengah, di antara sampul (kiri) dan visualizer (kanan)
    let left = f.x0 + p.px(12.0 + 20.0 + 10.0);
    let right = f.x0 + f.w - p.px(14.0 + 27.0 + 10.0);
    let max = (right - left).max(p.px(20.0));
    let line = current_line(d);
    let shown = p.fit(&line, 13.0, 600, max);
    let th = p.px(13.0) * 1.3;
    let (_, v) = p.text_v(c, &shown, (left + right) / 2.0, f.h / 2.0 - th / 2.0, 13.0, 600, WHITE, 1.0, Anchor::Center, max + p.px(2.0));
    live.lyric = v;
}

/// Notch diam "musik & lirik": sampul kecil (ikon app pemutar hanya bila lagu tanpa sampul), tombol mundur · putar/jeda · maju
/// di bawahnya (bisa diklik tanpa membuka panel), dan lirik bergaya Apple Music di kanan (baris aktif tebal terang, baris
/// lain redup dan buram, bergulir naik). Tanpa lirik tersinkron: judul dan artis.
fn compact_player(p: &mut Painter, c: &ContainerVisual, f: Frame, d: &Data, regions: &mut Vec<Region>, live: &mut Live) {
    let Some(m) = &d.media else { return };
    let a = p.px(52.0);
    let (ax, ay) = (f.x0 + p.px(22.0), p.px(10.0));
    match m.art {
        Some(s) => {
            p.image(c, s, ax, ay, a, a, p.px(12.0));
        }
        None => {
            p.rect(c, ax, ay, a, a, p.px(12.0), white(36));
            p.sym(c, Sym::Note, ax + a / 2.0, ay + a / 2.0, 20.0, WHITE, 0.7);
        }
    }
    // tombol di bawah sampul
    let (cx0, cy, step, hit) = (ax + a / 2.0, ay + a + p.px(17.0), p.px(24.0), p.px(24.0));
    let buttons = [(Action::Prev, Sym::Backward, 12.0), (Action::PlayPause, if m.playing { Sym::Pause } else { Sym::Play }, 14.0), (Action::Next, Sym::Forward, 12.0)];
    for (i, (act, sym, size)) in buttons.into_iter().enumerate() {
        let cx = cx0 + (i as f32 - 1.0) * step;
        p.sym(c, sym, cx, cy, size, WHITE, 0.95);
        regions.push(Region { x: cx - hit / 2.0, y: cy - hit / 2.0, w: hit, h: hit, action: act });
    }
    let lx = ax + a + p.px(28.0);
    let lw = f.x0 + f.w - p.px(22.0) - lx;
    let synced = d.lyrics.lines.is_some_and(|l| l.synced);
    if synced {
        lyrics_flow(p, c, lx, p.px(6.0), lw, f.h - p.px(12.0), d, live, Flow { size: 16.0, pad: 0.0, anchor: 0.36, card: false });
    } else {
        // judul + artis di tengah tinggi notch
        let th = p.px(16.0) * 1.3;
        let cy = f.h / 2.0;
        p.text(c, &p.fit(m.title, 16.0, 700, lw), lx, cy - th, 16.0, 700, WHITE, 1.0, Anchor::Left, lw + p.px(4.0));
        p.text(c, &p.fit(m.artist, 13.0, 500, lw), lx, cy + p.px(2.0), 13.0, 500, WHITE, 0.6, Anchor::Left, lw + p.px(4.0));
    }
}

/// Musik ringkas: sampul kecil di kiri, lima bar visualizer berwarna sampul di kanan; `row_h` = tinggi baris.
fn compact_music(p: &mut Painter, c: &ContainerVisual, f: Frame, d: &Data, live: &mut Live, row_h: f32) {
    let Some(m) = &d.media else { return };
    let cy = row_h / 2.0;
    let a = p.px(20.0);
    match m.art {
        Some(s) => {
            p.image(c, s, f.x0 + p.px(12.0), cy - a / 2.0, a, a, p.px(6.0));
        }
        None => {
            p.rect(c, f.x0 + p.px(12.0), cy - a / 2.0, a, a, p.px(6.0), white(40));
            p.sym(c, Sym::Note, f.x0 + p.px(12.0) + a / 2.0, cy, 12.0, WHITE, 0.8);
        }
    }
    let rgb = accent_or(m.tint);
    let (bw, gap, bh) = (p.px(3.0), p.px(3.0), p.px(14.0));
    let right = f.x0 + f.w - p.px(14.0);
    let x0 = right - 5.0 * bw - 4.0 * gap;
    for i in 0..5 {
        if let Some(v) = p.rect(c, x0 + i as f32 * (bw + gap), cy - bh / 2.0, bw, bh, bw / 2.0, Color { A: 240, R: rgb[0], G: rgb[1], B: rgb[2] }) {
            let _ = v.SetCenterPoint(Vector3 { X: bw / 2.0, Y: bh / 2.0, Z: 0.0 });
            let _ = v.SetScale(Vector3 { X: 1.0, Y: 0.3, Z: 1.0 });
            live.bars.push(v);
        }
    }
}

fn mmss(ms: i64) -> String {
    let s = (ms.max(0) + 500) / 1000;
    format!("{}:{:02}", s / 60, s % 60)
}

/// Kartu musik di tab Putar: sampul bercahaya warna sampul, judul, artis, penggeser posisi, kontrol.
#[allow(clippy::too_many_arguments)]
fn music_card(p: &mut Painter, c: &ContainerVisual, x: f32, y: f32, w: f32, h: f32, m: &MediaView, regions: &mut Vec<Region>) {
    let tint = m.tint;
    let bg = match tint {
        Some(t) => Color { A: 48, R: t[0], G: t[1], B: t[2] },
        None => white(18),
    };
    p.rect(c, x, y, w, h, p.px(18.0), bg);
    let pad = p.px(14.0);
    let art = p.px(64.0);
    let (ax, ay) = (x + pad, y + pad);
    // cahaya lembut di belakang sampul
    let glow = accent_or(tint);
    p.rect(c, ax - p.px(4.0), ay - p.px(2.0), art + p.px(8.0), art + p.px(8.0), p.px(18.0), Color { A: 60, R: glow[0], G: glow[1], B: glow[2] });
    match m.art {
        Some(s) => {
            p.image(c, s, ax, ay, art, art, p.px(14.0));
        }
        None => {
            p.rect(c, ax, ay, art, art, p.px(14.0), white(40));
            p.sym(c, Sym::Note, ax + art / 2.0, ay + art / 2.0, 24.0, WHITE, 0.8);
        }
    }
    let tx = ax + art + p.px(12.0);
    let tw = x + w - pad - tx;
    let title = p.fit(m.title, 13.5, 600, tw);
    p.text(c, &title, tx, ay + p.px(10.0), 13.5, 600, WHITE, 1.0, Anchor::Left, tw + 2.0);
    let artist = p.fit(m.artist, 12.0, 400, tw);
    p.text(c, &artist, tx, ay + p.px(30.0), 12.0, 400, WHITE, 0.62, Anchor::Left, tw + 2.0);

    // penggeser posisi: jalur tipis, kenop pil putih; diseret = kenop kaca lebih besar
    let (sx, sw) = (x + pad, w - 2.0 * pad);
    let sy = ay + art + p.px(14.0);
    let th = p.px(5.0);
    let frac = m.drag.unwrap_or(if m.dur_ms > 0 { m.pos_ms as f32 / m.dur_ms as f32 } else { 0.0 }).clamp(0.0, 1.0);
    p.rect(c, sx, sy - th / 2.0, sw, th, th / 2.0, white(52));
    p.rect(c, sx, sy - th / 2.0, (sw * frac).max(th), th, th / 2.0, white(236));
    let kx = sx + sw * frac;
    if m.drag.is_some() {
        let (kw, kh) = (p.px(28.0), p.px(18.0));
        p.rect(c, kx - kw / 2.0, sy - kh / 2.0, kw, kh, kh / 2.0, white(125));
        p.rect(c, kx - kw / 2.0 + p.px(1.5), sy - kh / 2.0 + p.px(1.5), kw - p.px(3.0), kh - p.px(3.0), (kh - p.px(3.0)) / 2.0, white(40));
    } else {
        let (kw, kh) = (p.px(18.0), p.px(12.0));
        p.rect(c, kx - kw / 2.0, sy - kh / 2.0, kw, kh, kh / 2.0, white(250));
    }
    if m.can_seek && m.dur_ms > 0 {
        regions.push(Region { x: sx - p.px(6.0), y: sy - p.px(12.0), w: sw + p.px(12.0), h: p.px(24.0), action: Action::Seek(sx, sw) });
    }
    let pos = if let Some(fr) = m.drag { (m.dur_ms as f32 * fr) as i64 } else { m.pos_ms };
    let ty = sy + p.px(8.0);
    p.text(c, &mmss(pos), sx, ty, 10.5, 500, WHITE, 0.55, Anchor::Left, sw / 2.0);
    p.text(c, &format!("-{}", mmss(m.dur_ms - pos)), sx + sw, ty, 10.5, 500, WHITE, 0.55, Anchor::Right, sw / 2.0);

    // kontrol
    let cy = y + h - p.px(20.0);
    let cx = x + w / 2.0;
    let gap = p.px(48.0);
    let items = [(Sym::Backward, cx - gap, 17.0, Action::Prev), (if m.playing { Sym::Pause } else { Sym::Play }, cx, 22.0, Action::PlayPause), (Sym::Forward, cx + gap, 17.0, Action::Next)];
    for (s, bx, size, a) in items {
        p.sym(c, s, bx, cy, size, WHITE, 0.95);
        regions.push(Region { x: bx - p.px(20.0), y: cy - p.px(16.0), w: p.px(40.0), h: p.px(32.0), action: a });
    }
}

/// Sapaan & info: ikon di kiri, nilai di kanan (rata kanan).
fn compact_glance(p: &mut Painter, c: &ContainerVisual, f: Frame, d: &Data) {
    let Some((item, text)) = &d.glance else { return };
    let cy = f.h / 2.0;
    let th = p.px(12.5) * 1.3;
    if *item == Item::Greeting {
        // sapaan: hanya tulisan, di tengah
        let max = f.w - p.px(32.0);
        let shown = p.fit(text, 12.5, 600, max);
        p.text(c, &shown, f.x0 + f.w / 2.0, cy - th / 2.0, 12.5, 600, WHITE, 0.95, Anchor::Center, max + p.px(2.0));
        return;
    }
    let left = f.x0 + p.px(14.0);
    let icon_w = match item {
        Item::Battery => {
            let (pct, ch) = d.battery.unwrap_or((0, false));
            p.battery(c, left, cy, pct, ch)
        }
        _ => {
            let sym = if *item == Item::Clock { Sym::Clock } else { Sym::Calendar };
            p.sym(c, sym, left + p.px(8.0), cy, 15.0, WHITE, 0.9);
            p.px(16.0)
        }
    };
    let right = f.x0 + f.w - p.px(16.0);
    let max = right - (left + icon_w + p.px(10.0));
    let shown = p.fit(text, 12.5, 600, max);
    p.text(c, &shown, right, cy - th / 2.0, 12.5, 600, WHITE, 0.95, Anchor::Right, max + p.px(2.0));
}

/// Unduhan: ikon app pengunduh di kiri; di kanan busur hijau dari ±jam 8 searah jarum jam ke ±jam 4 dengan panah unduh
/// membulat di tengahnya dan angka di celah bawah (tanpa "%"). Peramban (total tidak diketahui): potongan busur berputar.
fn compact_download(p: &mut Painter, c: &ContainerVisual, f: Frame, d: &Data) {
    let Some(dl) = &d.download else { return };
    let cy = f.h / 2.0;
    let a = p.px(22.0);
    let ix = f.x0 + p.px(14.0);
    match dl.icon {
        Some(s) => {
            p.image(c, s, ix, cy - a / 2.0, a, a, p.px(6.0));
        }
        None => {
            p.rect(c, ix, cy - a / 2.0, a, a, a / 2.0, white(36));
            p.sym(c, if dl.up { Sym::ArrowUp } else { Sym::ArrowDown }, ix + a / 2.0, cy, 11.0, WHITE, 0.9);
        }
    }
    // busur: jari-jari 11 dp, tebal 2,6 dp; pusatnya sedikit di atas tengah agar angka muat di celah bawah
    let (r, w) = (11.0, 2.6);
    let gx = f.x0 + f.w - p.px(16.0) - p.px(r + w / 2.0);
    let gy = p.px(15.0);
    p.arc(c, gx, gy, r, w, 0.0, 1.0, WHITE, 56);
    match (dl.frac, dl.done) {
        (_, true) => {
            p.arc(c, gx, gy, r, w, 0.0, 1.0, GREEN, 255);
        }
        (Some(fr), false) => {
            if fr > 0.002 {
                p.arc(c, gx, gy, r, w, 0.0, fr.clamp(0.0, 1.0), GREEN, 255);
            }
        }
        (None, false) => {
            if let Some((v, n)) = p.arc(c, gx, gy, r, w, 0.0, 0.24, GREEN, 255) {
                spin(p, &v, n, d.now_ms);
            }
        }
    }
    p.sym(c, if dl.done { Sym::Check } else if dl.up { Sym::ArrowUp } else { Sym::ArrowDown }, gx, gy, 12.0, WHITE, 0.95);
    let th = p.px(9.5) * 1.3;
    p.text(c, &dl.label, gx, f.h - p.px(2.0) - th, 9.5, 600, WHITE, 0.95, Anchor::Center, p.px(40.0));
}

/// Putar sprite persegi pada pusatnya tanpa henti; fasenya dari jam sehingga menggambar ulang tidak membuatnya melompat.
fn spin(p: &Painter, v: &SpriteVisual, n: f32, now_ms: i64) {
    use windows::Foundation::TimeSpan;
    use windows::UI::Composition::{AnimationIterationBehavior, CompositionEasingFunction};
    use windows_core::{Interface, HSTRING};
    const PERIOD: i64 = 1100;
    let go = || -> windows_core::Result<()> {
        let phase = (now_ms.rem_euclid(PERIOD)) as f32 / PERIOD as f32 * 360.0;
        v.SetCenterPoint(Vector3 { X: n / 2.0, Y: n / 2.0, Z: 0.0 })?;
        let lin: CompositionEasingFunction = p.comp.CreateLinearEasingFunction()?.cast()?;
        let a = p.comp.CreateScalarKeyFrameAnimation()?;
        a.InsertKeyFrame(0.0, phase)?;
        a.InsertKeyFrameWithEasingFunction(1.0, phase + 360.0, &lin)?;
        a.SetDuration(TimeSpan { Duration: PERIOD * 10_000 })?;
        a.SetIterationBehavior(AnimationIterationBehavior::Forever)?;
        v.StartAnimation(&HSTRING::from("RotationAngleInDegrees"), &a)
    };
    let _ = go();
}

fn compact_charging(p: &mut Painter, c: &ContainerVisual, f: Frame, d: &Data) {
    let cy = f.h / 2.0;
    let th = p.px(13.0) * 1.3;
    p.text(c, tr("Mengisi daya"), f.x0 + p.px(16.0), cy - th / 2.0, 12.5, 500, WHITE, 0.85, Anchor::Left, f.w / 2.0);
    let pct = d.battery.map_or(String::new(), |(b, _)| format!("\u{26A1} {b}%"));
    p.text(c, &pct, f.x0 + f.w - p.px(16.0), cy - th / 2.0, 13.0, 600, GREEN, 1.0, Anchor::Right, f.w / 2.0);
}

// ---------- panel terbuka ----------

/// Kepala panel: kontrol segmen kapsul (Putar · Rak · Timer) di kiri, baterai dan Pengaturan di kanan.
fn header(p: &mut Painter, c: &ContainerVisual, f: Frame, cur: Tab, d: &Data, regions: &mut Vec<Region>) {
    let (cy, bh, pad) = (p.px(25.0), p.px(26.0), p.px(3.0));
    let widths: Vec<f32> = Tab::ALL.iter().map(|t| (p.measure(tr(t.title()), 12.5, 600) + p.px(28.0)).max(p.px(60.0))).collect();
    let total: f32 = widths.iter().sum::<f32>() + 2.0 * pad;
    let x = f.x0 + p.px(16.0);
    p.rect(c, x, cy - bh / 2.0, total, bh, bh / 2.0, white(26));
    let mut sx = x + pad;
    let th = p.px(12.5) * 1.3;
    for (t, w) in Tab::ALL.iter().zip(&widths) {
        let on = *t == cur;
        if on {
            p.rect(c, sx, cy - bh / 2.0 + pad, *w, bh - 2.0 * pad, (bh - 2.0 * pad) / 2.0, white(236));
        }
        p.text(c, tr(t.title()), sx + w / 2.0, cy - th / 2.0, 12.5, 600, if on { INK } else { WHITE }, if on { 1.0 } else { 0.78 }, Anchor::Center, *w);
        regions.push(Region { x: sx, y: cy - bh / 2.0 - pad, w: *w, h: bh + 2.0 * pad, action: Action::Tab(*t) });
        sx += w;
    }
    // kanan: roda gigi, lalu baterai di kirinya
    let gx = f.x0 + f.w - p.px(28.0);
    p.sym(c, Sym::Gear, gx, cy, 16.0, WHITE, 0.7);
    regions.push(Region { x: gx - p.px(16.0), y: cy - p.px(15.0), w: p.px(32.0), h: p.px(30.0), action: Action::Settings });
    if let Some((pct, charging)) = d.battery {
        let label = if charging { format!("\u{26A1}{pct}%") } else { format!("{pct}%") };
        p.text(c, &label, gx - p.px(18.0), cy - th / 2.0, 12.5, 600, if charging { GREEN } else { WHITE }, 0.82, Anchor::Right, p.px(80.0));
    }
}

/// Tab Putar: kartu musik (kiri), kartu lirik (tengah), kartu "hari ini" + timer (kanan).
fn play(p: &mut Painter, c: &ContainerVisual, f: Frame, d: &Data, regions: &mut Vec<Region>, live: &mut Live) {
    let (pad, gap) = (p.px(14.0), p.px(10.0));
    let (top, bottom) = (p.px(48.0), f.h - pad);
    let h = bottom - top;
    let mw = p.px(230.0);
    let lw = f.w - 2.0 * pad - mw - gap;
    let (mx, lx) = (f.x0 + pad, f.x0 + pad + mw + gap);
    // musik
    match &d.media {
        Some(m) => music_card(p, c, mx, top, mw, h, m, regions),
        None => {
            card(p, c, mx, top, mw, h);
            p.sym(c, Sym::Note, mx + mw / 2.0, top + h / 2.0 - p.px(14.0), 26.0, WHITE, 0.35);
            p.text(c, tr("Tidak diputar"), mx + mw / 2.0, top + h / 2.0 + p.px(8.0), 12.5, 500, WHITE, 0.6, Anchor::Center, mw - p.px(20.0));
        }
    }
    // lirik, gaya besar: baris aktif terang, baris lain redup dan buram
    lyrics_flow(p, c, lx, top, lw, h, d, live, Flow { size: 17.0, pad: 16.0, anchor: 0.30, card: true });
}

/// Gaya aliran lirik: ukuran huruf (dp), tepi kiri/kanan (dp), letak baris aktif (bagian tinggi), dengan kartu kaca.
#[derive(Clone, Copy)]
struct Flow {
    size: f32,
    pad: f32,
    anchor: f32,
    card: bool,
}

/// Lirik bergaya Apple Music: teks tebal rata kiri, baris aktif putih penuh, baris sebelum/sesudahnya redup dan sedikit
/// buram; baris panjang dipenggal. Saat maju satu baris, isinya bergulir naik (lihat `Live::lyric_scroll`). Dipakai kartu
/// lirik tab Putar (dengan kartu) dan notch diam lebar (tanpa kartu).
#[allow(clippy::too_many_arguments)]
fn lyrics_flow(p: &mut Painter, c: &ContainerVisual, x: f32, y: f32, w: f32, h: f32, d: &Data, live: &mut Live, fl: Flow) {
    if fl.card {
        card(p, c, x, y, w, h);
    }
    let ly = &d.lyrics;
    let note = match ly.lines {
        Some(l) if !l.synced => tr("Lirik tanpa waktu"),
        Some(_) if ly.lrclib => "LRCLIB",
        _ => "",
    };
    let center_msg = |p: &mut Painter, s: &str| {
        p.text(c, s, x + w / 2.0, y + h / 2.0 - p.px(8.0), 13.0, 600, WHITE, 0.45, Anchor::Center, w - p.px(20.0));
    };
    let Some(l) = ly.lines else {
        match (ly.state, d.media.is_some()) {
            (LyrState::Loading, _) => center_msg(p, tr("Mencari lirik\u{2026}")),
            (_, true) => center_msg(p, tr("Lirik tidak tersedia")),
            _ => center_msg(p, tr("Lirik tampil di sini saat lagu diputar.")),
        }
        return;
    };
    // wadah terpotong seluas kartu; isinya di wadah gulir
    let go = || -> windows_core::Result<(ContainerVisual, ContainerVisual)> {
        let frame = p.comp.CreateContainerVisual()?;
        frame.SetOffset(Vector3 { X: x, Y: y, Z: 0.0 })?;
        frame.SetSize(windows_numerics::Vector2 { X: w, Y: h })?;
        frame.SetClip(&p.comp.CreateInsetClip()?)?;
        c.Children()?.InsertAtTop(&frame)?;
        let scroll = p.comp.CreateContainerVisual()?;
        scroll.SetSize(windows_numerics::Vector2 { X: w, Y: h })?;
        frame.Children()?.InsertAtTop(&scroll)?;
        Ok((frame, scroll))
    };
    let Ok((frame, sc)) = go() else { return };
    let (pad, size) = (p.px(fl.pad), fl.size);
    let line_h = p.px(size) * 1.28;
    let gap = p.px(size * 0.53);
    let maxw = w - 2.0 * pad;
    let blocks = |p: &Painter, text: &str| -> Vec<String> {
        let t = if text.is_empty() { "\u{266A}" } else { text };
        nacre_core::notch::typeset::wrap(t, maxw, 3, |s| p.measure(s, size, 700))
    };
    if l.synced {
        let active = l.index_at(ly.pos_ms);
        let center = active.unwrap_or(0);
        // blok aktif di sepertiga atas; satu blok sebelumnya di atasnya
        let ya = h * fl.anchor;
        let mut drawn: Vec<(usize, f32, Vec<String>)> = Vec::new();
        if center > 0 {
            let prev = blocks(p, &l.lines[center - 1].text);
            let hp = prev.len() as f32 * line_h;
            live.lyric_shift = hp + gap;
            drawn.push((center - 1, ya - gap - hp, prev));
        }
        let mut yy = ya;
        for i in center..l.lines.len() {
            if yy > h {
                break;
            }
            let b = blocks(p, &l.lines[i].text);
            let hb = b.len() as f32 * line_h;
            drawn.push((i, yy, b));
            yy += hb + gap;
        }
        for (i, top, lines) in drawn {
            let on = active == Some(i);
            for (k, t) in lines.iter().enumerate() {
                let ty = top + k as f32 * line_h;
                if on {
                    p.text(&sc, t, pad, ty, size, 700, WHITE, 1.0, Anchor::Left, maxw + p.px(4.0));
                } else {
                    p.text_soft(&sc, t, pad, ty, size, 700, WHITE, 0.32, (2.2 * p.dpi).round());
                }
            }
        }
        live.lyric_scroll = Some(sc);
    } else {
        let mut yy = pad;
        for line in l.lines.iter().filter(|x| !x.text.is_empty()) {
            if yy > h {
                break;
            }
            for t in nacre_core::notch::typeset::wrap(&line.text, maxw, 3, |s| p.measure(s, 15.0, 600)) {
                p.text(&sc, &t, pad, yy, 15.0, 600, WHITE, 0.8, Anchor::Left, maxw + p.px(4.0));
                yy += p.px(15.0) * 1.28;
            }
            yy += gap * 0.6;
        }
    }
    // keterangan sumber kecil di pojok kanan bawah, di atas lirik (hanya di kartu)
    if fl.card {
        p.text(&frame, note, w - p.px(12.0), h - p.px(18.0), 9.5, 500, WHITE, 0.35, Anchor::Right, w / 2.0);
    }
}

fn shelf_empty(p: &mut Painter, c: &ContainerVisual, f: Frame) {
    let pad = p.px(14.0);
    let (top, h) = (p.px(48.0), f.h - p.px(48.0) - pad);
    card(p, c, f.x0 + pad, top, f.w - 2.0 * pad, h);
    let cx = f.x0 + f.w / 2.0;
    p.sym(c, Sym::Plus, cx, top + h / 2.0 - p.px(16.0), 18.0, WHITE, 0.4);
    p.text(c, tr("Rak kosong"), cx, top + h / 2.0 + p.px(2.0), 13.0, 600, WHITE, 0.8, Anchor::Center, f.w);
    p.text(c, tr("Seret berkas ke notch untuk menaruhnya di sini."), cx, top + h / 2.0 + p.px(22.0), 12.0, 400, WHITE, 0.5, Anchor::Center, f.w);
}

fn round_button(p: &mut Painter, c: &ContainerVisual, cx: f32, cy: f32, d: f32, fill: Color) {
    p.rect(c, cx - d / 2.0, cy - d / 2.0, d, d, d / 2.0, fill);
}

/// Tab Timer: dua kartu. Kiri: timer (waktu besar, status, tombol −5 · mulai/jeda · ulang · +5). Kanan: kalender
/// horizontal (bulan, strip lima tanggal, acara tanggal terpilih).
fn timer(p: &mut Painter, c: &ContainerVisual, f: Frame, d: &Data, regions: &mut Vec<Region>) {
    let (pad, gap) = (p.px(14.0), p.px(10.0));
    let (top, h) = (p.px(48.0), f.h - p.px(48.0) - pad);
    let cw = (f.w - 2.0 * pad - gap) / 2.0;
    timer_card(p, c, f.x0 + pad, top, cw, h, d, regions);
    calendar_card(p, c, f.x0 + pad + cw + gap, top, cw, h, d, regions);
}

#[allow(clippy::too_many_arguments)]
fn timer_card(p: &mut Painter, c: &ContainerVisual, x: f32, y: f32, w: f32, h: f32, d: &Data, regions: &mut Vec<Region>) {
    card(p, c, x, y, w, h);
    let t = d.timer;
    let secs = t.remaining_secs(d.now_ms);
    let (rgb, status) = match t.phase {
        Phase::Running if secs <= 60 => (ORANGE, tr("Berjalan")),
        Phase::Running => (WHITE, tr("Berjalan")),
        Phase::Paused => (WHITE, tr("Dijeda")),
        Phase::Finished => (ORANGE, tr("Selesai")),
        Phase::Idle => (WHITE, tr("Siap")),
    };
    p.text(c, &format_mmss(secs), x + p.px(18.0), y + p.px(8.0), 30.0, 600, rgb, 1.0, Anchor::Left, w * 0.7);
    p.text(c, status, x + w - p.px(16.0), y + p.px(18.0), 11.5, 500, WHITE, 0.55, Anchor::Right, w * 0.3);
    // tombol bulat di baris bawah, tersebar rata
    let (bd, bgap) = (p.px(34.0), p.px(14.0));
    let total = 4.0 * bd + 3.0 * bgap;
    let cy = y + h - p.px(12.0) - bd / 2.0;
    let mut cx = x + (w - total) / 2.0 + bd / 2.0;
    let idle = matches!(t.phase, Phase::Idle | Phase::Finished);
    let accent = crate::prefs::accent();
    let buttons: [(Action, bool); 4] = [(Action::TimerMinus, idle), (Action::TimerToggle, true), (Action::TimerReset, true), (Action::TimerPlus, idle)];
    for (a, enabled) in buttons {
        let primary = a == Action::TimerToggle;
        let fill = if primary { Color { A: 255, R: accent[0], G: accent[1], B: accent[2] } } else { white(if enabled { 30 } else { 14 }) };
        round_button(p, c, cx, cy, bd, fill);
        let op = if enabled { 1.0 } else { 0.35 };
        match a {
            Action::TimerToggle => p.sym(c, if t.phase == Phase::Running { Sym::Pause } else { Sym::Play }, cx, cy, 16.0, WHITE, 1.0),
            Action::TimerReset => p.sym(c, Sym::Restart, cx, cy, 15.0, WHITE, 0.9),
            Action::TimerMinus | Action::TimerPlus => {
                let th = p.px(12.0) * 1.3;
                let s = if a == Action::TimerMinus { "\u{2212}5" } else { "+5" };
                p.text(c, s, cx, cy - th / 2.0, 12.0, 600, WHITE, op, Anchor::Center, bd);
            }
            _ => {}
        }
        if enabled {
            regions.push(Region { x: cx - bd / 2.0, y: cy - bd / 2.0, w: bd, h: bd, action: a });
        }
        cx += bd + bgap;
    }
}

/// Kalender horizontal: nama bulan di kiri, strip lima tanggal (terpilih di tengah, lingkaran aksen; hari ini berwarna
/// aksen), dan baris acara di bawah. Klik tanggal untuk memilihnya.
#[allow(clippy::too_many_arguments)]
fn calendar_card(p: &mut Painter, c: &ContainerVisual, x: f32, y: f32, w: f32, h: f32, d: &Data, regions: &mut Vec<Region>) {
    use nacre_core::island::calendar::{add_days, month_short, weekday, weekday_short};
    card(p, c, x, y, w, h);
    let en = crate::prefs::get().lang == nacre_core::i18n::Lang::En;
    let cal = &d.cal;
    let accent = crate::prefs::accent();
    let (sy, sm, sd) = cal.sel;
    p.text(c, month_short(sm, en), x + p.px(16.0), y + p.px(12.0), 18.0, 700, WHITE, 1.0, Anchor::Left, p.px(60.0));
    p.text(c, &sy.to_string(), x + p.px(16.0), y + p.px(36.0), 11.0, 500, WHITE, 0.45, Anchor::Left, p.px(60.0));
    // strip tanggal
    let (left, right) = (x + p.px(70.0), x + w - p.px(10.0));
    let cell = (right - left) / 5.0;
    let (wy, ny, dia) = (y + p.px(10.0), y + p.px(34.0), p.px(28.0));
    for i in -2i32..=2 {
        let (yy, mm, dd) = add_days(sy, sm, sd, i);
        let cx = left + cell * (i + 2) as f32 + cell / 2.0;
        let selected = i == 0;
        let today = (yy, mm, dd) == cal.today;
        let wd = weekday_short(weekday(yy, mm, dd), en);
        p.text(c, wd, cx, wy, 9.0, 600, if selected { accent } else { WHITE }, if selected { 1.0 } else { 0.4 }, Anchor::Center, cell);
        if selected {
            p.rect(c, cx - dia / 2.0, ny, dia, dia, dia / 2.0, Color { A: 255, R: accent[0], G: accent[1], B: accent[2] });
        }
        let th = p.px(14.0) * 1.3;
        let (rgb, op) = if selected { (WHITE, 1.0) } else if today { (accent, 1.0) } else { (WHITE, 0.8) };
        p.text(c, &format!("{dd:02}"), cx, ny + dia / 2.0 - th / 2.0, 14.0, if selected || today { 700 } else { 500 }, rgb, op, Anchor::Center, cell);
        if !selected {
            regions.push(Region { x: cx - cell / 2.0, y: wy, w: cell, h: ny + dia - wy, action: Action::CalDay(i) });
        }
    }
    // acara tanggal terpilih
    let is_today = cal.sel == cal.today;
    let line = match &cal.events {
        Events::Loading => "\u{2026}".to_string(),
        Events::Unavailable => String::new(),
        Events::List([]) => tr(if is_today { "Tidak ada acara hari ini" } else { "Tidak ada acara" }).to_string(),
        Events::List(list) => {
            let e = &list[0];
            let when = match e.minute {
                Some(m) if crate::prefs::get().clock_24h => format!("{:02}.{:02}  ", m / 60, m % 60),
                Some(m) => format!("{}.{:02} {}  ", (m / 60 + 11) % 12 + 1, m % 60, if m < 720 { "AM" } else { "PM" }),
                None => String::new(),
            };
            let more = if list.len() > 1 { format!("  +{}", list.len() - 1) } else { String::new() };
            format!("{when}{}{more}", e.title)
        }
    };
    if !line.is_empty() {
        let ly = y + h - p.px(26.0);
        p.sym(c, Sym::Calendar, x + p.px(22.0), ly + p.px(7.5), 12.0, WHITE, 0.55);
        let maxw = w - p.px(44.0);
        let shown = p.fit(&line, 11.5, 500, maxw);
        p.text(c, &shown, x + p.px(34.0), ly, 11.5, 500, WHITE, 0.75, Anchor::Left, maxw + p.px(4.0));
    }
}
