//! Notch ABRA generasi baru, dirancang sendiri (clean room). Satu jendela tanpa aktivasi di tengah atas layar ditambah
//! jendela bayangan tembus klik di bawahnya. Keputusan keadaan, ukuran, dan prioritas aktivitas ada di
//! `nacre_core::notch`; modul ini hanya menerjemahkan kejadian Win32 ke sana lalu menggambar hasilnya.

mod body;
mod draw;
mod drop;
mod ears;
mod shadow;
mod views;

use crate::dock::menu::{Item as MItem, Menu, MENU_CLOSE, MENU_PICK};
use crate::strip::Strip;
use body::Body;
use draw::Painter;
use nacre_core::island::calendar;
use nacre_core::island::timer::{Phase, Timer};
use nacre_core::notch::glance;
use nacre_core::notch::state::{self, motion, Event, Live, Look, Mode, OpenOn, Size, Tab, View};
use nacre_core::visibility::policy::{should_hide, Inputs, Mode as Vis};
use nacre_platform::loopback::{self, Loopback};
use nacre_platform::media::{self, ArtUpdate, Command, Media};
use nacre_platform::wins::{overlap, tracker};
use nacre_platform::{log, sys};
use nacre_ui::gfx::Gfx;
use nacre_ui::glass::Tier;
use nacre_ui::host::{Host, Surface};
use shadow::Shadow;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::time::Instant;
use views::{Action, Data, Frame, LyrState, LyricsView, MediaView, Region};
use windows::core::w;
use windows::UI::Composition::{AnimationDelayBehavior, CompositionDrawingSurface, CompositionObject, Compositor, Visual};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{CreateRectRgn, SetWindowRgn};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows_core::{Interface, HSTRING};
use windows_numerics::{Vector2, Vector3};

const WM_MOUSELEAVE: u32 = 0x02A3;
const WM_DISPLAYCHANGE: u32 = 0x007E;
const WM_DPICHANGED: u32 = 0x02E0;
const WM_POWERBROADCAST: u32 = 0x0218;
const WM_SETTINGCHANGE: u32 = 0x001A;
const STRIP_ENTER: u32 = WM_APP + 14;
const STRIP_LEAVE: u32 = WM_APP + 15;
/// Dari menu bar: naikkan kembali ke paling atas.
const WM_TOPMOST: u32 = WM_APP + 72;
/// Buka panel (mis. dari pintasan).
const WM_OPEN: u32 = WM_APP + 73;
/// Sesi media berubah / pita visualizer baru.
const WM_MEDIA: u32 = WM_APP + 10;
const WM_BANDS: u32 = WM_APP + 11;
/// Jawaban pekerja lirik (WM_APP+12/13 dipakai menu popup).
const WM_LYRICS: u32 = WM_APP + 18;
/// Seret-taruh berkas (wParam = drop::ENTER/OVER/LEAVE/DROP, lParam = titik layar).
const WM_DND: u32 = WM_APP + 19;
/// Setelah dilepas di zona "Salin ke…" / "Buka dengan…" (wParam = zona): dikerjakan di luar callback OLE.
const WM_AFTERDROP: u32 = WM_APP + 20;
/// Pemantau unduhan punya data baru.
const WM_DOWNLOAD: u32 = WM_APP + 21;
/// Acara kalender untuk kartu kalender sudah dibaca.
const WM_AGENDA: u32 = WM_APP + 23;
/// Kemajuan sesi "Kirim ke HP".
const WM_SEND: u32 = WM_APP + 24;

const T_DWELL: usize = 1;
const T_LEAVE: usize = 2;
const T_GHOST: usize = 3;
const T_REGION: usize = 4;
const T_TICK: usize = 5;
const T_CHARGE: usize = 6;
const T_WINCHG: usize = 7;
const T_RECONF: usize = 8;
const T_EDGE: usize = 9;
const T_GRACE: usize = 10;
const T_START: usize = 11;
const T_HIDEWIN: usize = 12;
const T_GLANCE: usize = 13;
const T_MINUTE: usize = 14;
const T_PROGRESS: usize = 15;
const T_LYRIC: usize = 16;
const T_PRIVACY: usize = 17;
/// Unduhan selesai sudah cukup lama tampil.
const T_DLDONE: usize = 18;
/// Pemeriksa sesi Kirim yang menganggur (berhenti setelah 15 menit), selesai-kirim yang sudah cukup lama tampil, dan
/// pemicu uji `NACRE_TEST_SEND`.
const T_SEND: usize = 19;
const T_SENDDONE: usize = 20;
const T_TESTSEND: usize = 21;
const SEND_IDLE: std::time::Duration = std::time::Duration::from_secs(15 * 60);

/// Sesi "Kirim ke HP" yang sedang berjalan.
struct SendSess {
    server: nacre_platform::sendsrv::Server,
    qr: Option<CompositionDrawingSurface>,
    qr_px: f32,
    names: Vec<String>,
    prog: nacre_platform::sendsrv::Progress,
    last: Instant,
    done_at: Option<Instant>,
}

/// Jendela notch (dp): muat panel terbuka terbesar beserta telinga.
const WIN_W: f32 = 640.0;
const WIN_H: f32 = 222.0;
/// Ruang bayangan di sekitar jendela notch (dp).
const SHADOW_PAD: f32 = 36.0;

static NOTCH_HWND: AtomicIsize = AtomicIsize::new(0);
static BUSY: AtomicBool = AtomicBool::new(false);

pub struct Options {
    pub glass: Option<Tier>,
    pub mode: Vis,
    /// Cadangkan strip di atas layar (notch tetap).
    pub reserve: bool,
}

/// Animasikan satu properti skalar dengan pegas compositor.
pub(crate) fn spring(comp: &Compositor, obj: &CompositionObject, prop: &str, v: f32, ratio: f32, period_ms: u32) {
    let go = || -> windows_core::Result<()> {
        let a = comp.CreateSpringScalarAnimation()?;
        a.SetDampingRatio(ratio)?;
        a.SetPeriod(windows::Foundation::TimeSpan { Duration: period_ms as i64 * 10_000 })?;
        a.SetFinalValue(&windows::Foundation::PropertyValue::CreateSingle(v)?.cast::<windows::Foundation::IReference<f32>>()?)?;
        obj.StartAnimation(&HSTRING::from(prop), &a)
    };
    let _ = go();
}

fn ts(ms: u32) -> windows::Foundation::TimeSpan {
    windows::Foundation::TimeSpan { Duration: ms as i64 * 10_000 }
}

/// Animasi skalar dengan jeda; nilai awal dipasang selama jeda.
fn keyframes(comp: &Compositor, obj: &CompositionObject, prop: &str, from: f32, to: f32, ms: u32, delay: u32) -> windows_core::Result<()> {
    let e = crate::motion::ease(comp)?;
    let a = comp.CreateScalarKeyFrameAnimation()?;
    a.InsertKeyFrame(0.0, from)?;
    a.InsertKeyFrameWithEasingFunction(1.0, to, &e)?;
    a.SetDuration(ts(ms))?;
    if delay > 0 {
        a.SetDelayTime(ts(delay))?;
        a.SetDelayBehavior(AnimationDelayBehavior::SetInitialValueBeforeDelay)?;
    }
    obj.StartAnimation(&HSTRING::from(prop), &a)
}

fn with<R>(hwnd: HWND, f: impl FnOnce(&mut Notch) -> R) -> Option<R> {
    let p = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut Notch;
    if p.is_null() || BUSY.swap(true, Ordering::Acquire) {
        return None;
    }
    let r = f(unsafe { &mut *p });
    BUSY.store(false, Ordering::Release);
    Some(r)
}

fn timer(hwnd: HWND, id: usize, ms: u32) {
    unsafe {
        SetTimer(Some(hwnd), id, ms, None);
    }
}

fn kill(hwnd: HWND, id: usize) {
    unsafe {
        let _ = KillTimer(Some(hwnd), id);
    }
}

fn post(hwnd: isize, msg: u32) {
    unsafe {
        let _ = PostMessageW(Some(HWND(hwnd as *mut _)), msg, WPARAM(0), LPARAM(0));
    }
}

fn post_w(hwnd: isize, msg: u32, w: usize) {
    unsafe {
        let _ = PostMessageW(Some(HWND(hwnd as *mut _)), msg, WPARAM(w), LPARAM(0));
    }
}

fn on_windows_changed() {
    let h = NOTCH_HWND.load(Ordering::Relaxed);
    if h != 0 {
        timer(HWND(h as *mut _), T_WINCHG, 150);
    }
}

fn auto_tier(over: Option<Tier>) -> Tier {
    over.unwrap_or_else(|| {
        let c = sys::conditions();
        if !c.transparency || c.battery_saver || c.remote_session {
            Tier::Solid
        } else {
            Tier::Full
        }
    })
}

fn win_size(dpi: f32, screen_w: i32) -> (i32, i32) {
    (((WIN_W * dpi) as i32).min(screen_w), (WIN_H * dpi).ceil() as i32)
}

pub struct Notch {
    hwnd: HWND,
    paint: Painter,
    _surface: Surface,
    body: Body,
    shadow: Option<Shadow>,
    dpi: f32,
    win: (i32, i32),
    screen_w: i32,

    mode: Mode,
    look: Look,
    size: Size,
    first: bool,
    tracking: bool,
    regions: Vec<Region>,

    timer: Timer,
    t0: Instant,
    last_sec: i64,
    battery: Option<(u8, bool)>,
    charging_fresh: bool,
    /// Indeks item sapaan & info yang sedang tampil.
    glance_idx: usize,

    media: Media,
    has_media: bool,
    title: String,
    artist: String,
    playing: bool,
    pos_ms: i64,
    dur_ms: i64,
    pos_at: Instant,
    can_seek: bool,
    art: Option<CompositionDrawingSurface>,
    tint: Option<[u8; 3]>,
    loopback: Option<Loopback>,
    live_vis: views::Live,
    /// Penggeser posisi sedang diseret: (x awal, lebar, posisi 0..1).
    seek_drag: Option<(f32, f32, f32)>,

    album: String,
    lyr_worker: nacre_platform::lyrics::Lyrics,
    lyr: Option<nacre_core::notch::lyrics::Lyrics>,
    lyr_state: LyrState,
    lyr_lrclib: bool,
    /// Kunci cache lagu yang lirik-nya sedang ditunggu/ditampilkan, dan lagu (judul+artis) terakhir.
    lyr_key: String,
    lyr_track: String,
    /// Baris yang terakhir ditampilkan (untuk animasi pergantian).
    lyr_idx: Option<usize>,

    shelf: nacre_core::notch::shelf::Shelf,
    shelf_icons: std::collections::HashMap<String, CompositionDrawingSurface>,
    dnd: drop::Shared,
    _drop_target: Option<windows::Win32::System::Ole::IDropTarget>,
    drop_zone: Option<usize>,
    /// Berkas yang menunggu "Salin ke…" / "Buka dengan…".
    pending: Vec<String>,
    menu: Option<Menu>,
    /// Berkas Rak yang menu klik-kanannya sedang terbuka.
    menu_item: Option<usize>,
    /// (mikrofon, kamera) sedang dipakai.
    privacy: (bool, bool),
    /// Unduhan: pemantau (hidup hanya bila pengaturannya aktif), yang ditampilkan, yang baru selesai, ikon app.
    dl_watch: Option<nacre_platform::dlwatch::Watch>,
    dl: Option<nacre_core::notch::download::Download>,
    dl_done: Option<nacre_core::notch::download::Download>,
    dl_icons: std::collections::HashMap<String, Option<CompositionDrawingSurface>>,
    /// Kartu kalender: pilihan (hari dari hari ini), pembaca acara (hidup saat pertama dibutuhkan), acara terakhir.
    cal_sel: i32,
    agenda: Option<nacre_platform::agenda::Agenda>,
    cal_events: Option<(views::Date, Option<Vec<nacre_platform::agenda::Event>>)>,
    /// "Kirim ke HP": sesi aktif, atau gagal dimulai (ditampilkan di tab Rak sampai Selesai).
    send: Option<SendSess>,
    send_err: bool,
    /// App sumber lagu (AppUserModelID) dan ikonnya, dipakai hanya sebagai pengganti sampul bila lagu tanpa sampul.
    media_app: String,
    app_icons: std::collections::HashMap<String, Option<CompositionDrawingSurface>>,
    vis: Vis,
    reserved: bool,
    off: bool,
    shown: bool,
    starting: bool,
    overlap: overlap::Overlap,
    grace: bool,
    edge_armed: bool,
    strip: Option<Strip>,
    tier: Tier,
    glass_override: Option<Tier>,
}

impl Notch {
    pub fn create(host: &Host, opts: Options) -> windows_core::Result<HWND> {
        unsafe {
            let hi: HINSTANCE = GetModuleHandleW(None)?.into();
            let class = w!("AbraNotch");
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(proc),
                hInstance: hi,
                lpszClassName: class,
                hCursor: LoadCursorW(None, IDC_ARROW)?,
                ..Default::default()
            });
            let hwnd = CreateWindowExW(
                WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class,
                w!("abra-notch"),
                WS_POPUP,
                0, 0, 400, 200,
                None, None, Some(hi), None,
            )?;
            let dpi = GetDpiForWindow(hwnd).max(96) as f32 / 96.0;
            let screen_w = sys::screen_size().0;
            let win = win_size(dpi, screen_w);
            let _ = SetWindowPos(hwnd, None, (screen_w - win.0) / 2, 0, win.0, win.1, SWP_NOACTIVATE | SWP_NOZORDER);
            let comp = host.compositor.clone();
            let surface = host.attach(hwnd, (win.0 as f32, win.1 as f32))?;
            let tier = auto_tier(opts.glass);
            let gfx = Gfx::new(&comp)?;
            let body = Body::new(&comp, &gfx, &surface.root, win, dpi, tier)?;
            let pad = (SHADOW_PAD * dpi).round();
            let shadow = match Shadow::new(&comp, (win.0 + 2 * pad as i32, win.1 + pad as i32), pad) {
                Ok(s) => Some(s),
                Err(e) => {
                    log::line("notch", &format!("bayangan gagal: {e}"));
                    None
                }
            };
            let rest = state::look(Mode::Rest, None);
            let mut n = Box::new(Notch {
                hwnd,
                paint: Painter::new(comp, gfx, dpi),
                _surface: surface,
                body,
                shadow,
                dpi,
                win,
                screen_w,
                mode: Mode::Rest,
                look: rest,
                size: state::size(rest, dpi),
                first: true,
                tracking: false,
                regions: Vec::new(),
                timer: Timer::new(std::env::var("NACRE_TIMER_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(25 * 60)),
                t0: Instant::now(),
                last_sec: -1,
                battery: sys::battery(),
                charging_fresh: false,
                glance_idx: 0,
                media: {
                    let hw = hwnd.0 as isize;
                    if std::env::var_os("NACRE_NO_MEDIA").is_some() {
                        // kait uji: abaikan sesi media sistem (mis. untuk menguji notch diam)
                        let (commands, _) = std::sync::mpsc::channel();
                        let (_, snapshots) = std::sync::mpsc::channel();
                        Media { commands, snapshots }
                    } else if std::env::var_os("NACRE_FAKE_MEDIA").is_some() {
                        nacre_platform::media_fake::spawn(move || post(hw, WM_MEDIA))
                    } else {
                        media::spawn(move || post(hw, WM_MEDIA))
                    }
                },
                has_media: false,
                title: String::new(),
                artist: String::new(),
                playing: false,
                pos_ms: 0,
                dur_ms: 0,
                pos_at: Instant::now(),
                can_seek: false,
                art: None,
                tint: None,
                loopback: None,
                live_vis: views::Live::default(),
                seek_drag: None,
                album: String::new(),
                lyr_worker: {
                    let hw = hwnd.0 as isize;
                    nacre_platform::lyrics::spawn(crate::paths::config_dir().join("lyrics"), move || post(hw, WM_LYRICS))
                },
                lyr: None,
                lyr_state: LyrState::Idle,
                lyr_lrclib: false,
                lyr_key: String::new(),
                lyr_track: String::new(),
                lyr_idx: None,
                shelf: nacre_core::notch::shelf::Shelf::new(24),
                shelf_icons: std::collections::HashMap::new(),
                dnd: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
                _drop_target: None,
                drop_zone: None,
                pending: Vec::new(),
                menu: None,
                menu_item: None,
                privacy: (false, false),
                dl_watch: None,
                dl: None,
                dl_done: None,
                dl_icons: std::collections::HashMap::new(),
                cal_sel: 0,
                agenda: None,
                send: None,
                send_err: false,
                cal_events: None,
                media_app: String::new(),
                app_icons: std::collections::HashMap::new(),
                vis: opts.mode,
                reserved: false,
                off: !crate::prefs::get().notch_enabled,
                shown: true,
                starting: true,
                overlap: overlap::Overlap::default(),
                grace: false,
                edge_armed: false,
                strip: None,
                tier,
                glass_override: opts.glass,
            });
            // hitung tampilan awal dari data (mis. sapaan & info), tanpa animasi
            n.refresh();
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(n) as isize);
            NOTCH_HWND.store(hwnd.0 as isize, Ordering::Relaxed);
            crate::services::register_island(hwnd);
            tracker::install(on_windows_changed);
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            timer(hwnd, T_START, 1600);
            timer(hwnd, T_MINUTE, nacre_platform::time::ms_to_next_minute());
            timer(hwnd, T_PRIVACY, 1500);
            with(hwnd, |d| {
                d._drop_target = drop::register(hwnd, WM_DND, d.dnd.clone());
                if d._drop_target.is_none() {
                    log::line("notch", "seret-taruh tidak tersedia");
                }
                d.place_shadow_window();
                if d.vis != Vis::Always {
                    d.strip = Strip::new(hwnd, STRIP_ENTER, STRIP_LEAVE).ok();
                    d.place_strip();
                }
                if opts.reserve && !d.off && !crate::prefs::get().menubar {
                    d.set_reserved(true);
                }
                d.publish_notch();
                d.update_visibility();
                d.sync_download_watch();
            });
            crate::prefs::listen(hwnd);
            if std::env::var_os("NACRE_TEST_SEND").is_some() {
                timer(hwnd, T_TESTSEND, 2500);
            }
            log::line("notch", &format!("dibuat tier={tier:?} dpi={dpi} jendela={win:?} layar_w={screen_w}"));
            Ok(hwnd)
        }
    }

    fn now_ms(&self) -> i64 {
        self.t0.elapsed().as_millis() as i64
    }

    fn x0(&self) -> f32 {
        ((self.win.0 as f32 - self.size.w) / 2.0).floor()
    }

    fn live(&self) -> Live {
        let running = self.timer.phase == Phase::Running;
        Live {
            media: self.has_media,
            playing: self.playing,
            lyric_line: self.lyr.as_ref().is_some_and(|l| l.synced),
            lyrics_on: crate::prefs::get().lyrics_compact,
            timer_left_s: running.then(|| self.timer.remaining_secs(self.now_ms())),
            charging_fresh: self.charging_fresh,
            shelf: self.shelf.len(),
            glance: crate::prefs::get().notch_idle == 0,
            download: (crate::prefs::get().notch_downloads && (self.dl.is_some() || self.dl_done.is_some())) || self.send_compact().is_some(),
            wide_music: crate::prefs::get().notch_idle == 2,
        }
    }

    // ---------- kirim ke HP ----------

    /// Mulai sesi Kirim untuk `paths` dan buka tab Rak yang menampilkan QR-nya.
    fn start_send(&mut self, paths: Vec<String>) {
        if paths.is_empty() {
            return;
        }
        self.send = None;
        let en = crate::prefs::get().lang == nacre_core::i18n::Lang::En;
        let hw = self.hwnd.0 as isize;
        match nacre_platform::sendsrv::start(&paths, en, move || post(hw, WM_SEND)) {
            Ok(server) => {
                let px = (86.0 * self.dpi).floor();
                let qr = nacre_core::qr::encode(&server.url).and_then(|m| {
                    let bmp = nacre_core::qr::render(&m, px as u32, 1);
                    self.paint.gfx.surface_from_bgra(px as u32, px as u32, &bmp).ok()
                });
                let names = paths.iter().filter_map(|p| std::path::Path::new(p).file_name().map(|n| n.to_string_lossy().to_string())).collect();
                let total = paths.iter().filter_map(|p| std::fs::metadata(p).ok()).map(|m| m.len()).sum();
                self.send = Some(SendSess {
                    server,
                    qr,
                    qr_px: px,
                    names,
                    prog: nacre_platform::sendsrv::Progress { total, ..Default::default() },
                    last: Instant::now(),
                    done_at: None,
                });
                self.send_err = false;
                timer(self.hwnd, T_SEND, 30_000);
            }
            Err(e) => {
                log::line("kirim", &format!("gagal memulai: {e}"));
                self.send_err = true;
            }
        }
        match self.mode {
            Mode::Drop => self.dispatch(Event::Dropped),
            Mode::Open(Tab::Shelf) => self.refresh(),
            Mode::Open(_) => self.dispatch(Event::SelectTab(Tab::Shelf)),
            _ => {
                self.dispatch(Event::Click);
                self.dispatch(Event::SelectTab(Tab::Shelf));
            }
        }
    }

    fn stop_send(&mut self) {
        if self.send.take().is_some() {
            log::line("kirim", "sesi diakhiri");
        }
        self.send_err = false;
        kill(self.hwnd, T_SEND);
        kill(self.hwnd, T_SENDDONE);
        self.refresh();
        // panel tadi ditahan oleh sesi: tutup bila kursor sudah tidak di notch
        if !self.tracking && matches!(self.mode, Mode::Open(_)) {
            self.dispatch(Event::Leave);
        }
    }

    /// Panel QR tetap terbuka selama sesi Kirim (atau pesan gagalnya) tampil, walau kursor keluar dari notch; ditutup
    /// lewat tombol Selesai, batas waktu sesi, atau pindah tab.
    fn send_pinned(&self) -> bool {
        self.mode == Mode::Open(Tab::Shelf) && (self.send.is_some() || self.send_err)
    }

    fn on_send(&mut self) {
        let Some(s) = self.send.as_mut() else { return };
        let mut changed = false;
        while let Ok(p) = s.server.progress.try_recv() {
            s.prog = p;
            s.last = Instant::now();
            changed = true;
        }
        let n = s.names.len();
        if s.prog.files_done >= n && !s.prog.active && s.done_at.is_none() && s.prog.sent > 0 {
            s.done_at = Some(Instant::now());
            log::line("kirim", &format!("{n} berkas terkirim"));
            timer(self.hwnd, T_SENDDONE, nacre_core::notch::download::DONE_LINGER_MS);
        } else if s.prog.active {
            s.done_at = None;
        }
        if changed {
            self.refresh();
        }
    }

    /// Kemajuan kirim untuk notch tertutup: (pecahan, selesai) selama mengirim atau sesaat setelah selesai.
    fn send_compact(&self) -> Option<(f32, bool)> {
        let s = self.send.as_ref()?;
        let done = s.done_at.is_some_and(|t| t.elapsed().as_millis() < nacre_core::notch::download::DONE_LINGER_MS as u128);
        (s.prog.active || done).then(|| (nacre_core::notch::send::fraction(s.prog.sent, s.prog.total), done && !s.prog.active))
    }

    // ---------- kalender & badge app ----------

    fn today() -> views::Date {
        let t = nacre_platform::time::local_now();
        (t.year, t.month, t.day)
    }

    fn cal_date(&self) -> views::Date {
        let (y, m, d) = Self::today();
        nacre_core::island::calendar::add_days(y, m, d, self.cal_sel)
    }

    /// Minta acara tanggal terpilih bila belum ada (pembaca kalender dinyalakan saat pertama dibutuhkan).
    fn ensure_events(&mut self) {
        let date = self.cal_date();
        if self.cal_events.as_ref().is_some_and(|(d, _)| *d == date) {
            return;
        }
        if self.agenda.is_none() {
            let hw = self.hwnd.0 as isize;
            self.agenda = Some(nacre_platform::agenda::spawn(move || post(hw, WM_AGENDA)));
        }
        if let Some(a) = &self.agenda {
            let _ = a.requests.send(date);
        }
    }

    fn on_agenda(&mut self) {
        let Some(a) = &self.agenda else { return };
        let mut got = false;
        while let Ok(ans) = a.answers.try_recv() {
            self.cal_events = Some(ans);
            got = true;
        }
        if got && self.mode == Mode::Open(Tab::Timer) {
            self.render();
        }
    }

    /// Ikon app sumber lagu (pengganti sampul bila lagu tidak punya sampul): lewat AppUserModelID (shell:AppsFolder), atau exe proses bila id-nya nama berkas .exe.
    fn ensure_app_icon(&mut self) {
        let id = self.media_app.clone();
        if id.is_empty() || self.app_icons.contains_key(&id) {
            return;
        }
        let px = (48.0 * self.dpi).round() as i32;
        let bmp = nacre_platform::icons::load(&format!("shell:AppsFolder\\{id}"), px).or_else(|| {
            id.to_ascii_lowercase().ends_with(".exe").then(|| nacre_platform::wins::enumerate::exe_path_of_running(&id)).flatten().and_then(|p| nacre_platform::icons::load(&p, px))
        });
        let s = bmp.and_then(|b| self.paint.gfx.surface_from_bgra(b.w, b.h, &b.bgra).ok());
        self.app_icons.insert(id, s);
    }

    // ---------- unduhan ----------

    /// Nyalakan/matikan pemantau unduhan sesuai pengaturan.
    fn sync_download_watch(&mut self) {
        let on = crate::prefs::get().notch_downloads;
        if on && self.dl_watch.is_none() {
            let hw = self.hwnd.0 as isize;
            self.dl_watch = Some(nacre_platform::dlwatch::spawn(move || post(hw, WM_DOWNLOAD)));
        } else if !on && self.dl_watch.is_some() {
            // menutup penerima menghentikan thread pada kiriman berikutnya
            self.dl_watch = None;
            self.dl = None;
            self.dl_done = None;
        }
    }

    fn on_download(&mut self) {
        let Some(w) = &self.dl_watch else { return };
        let mut changed = false;
        while let Ok(s) = w.snapshots.try_recv() {
            self.dl = nacre_core::notch::download::primary(&s.active).cloned();
            if self.dl.is_some() {
                self.dl_done = None;
            } else if let Some(f) = s.finished {
                log::line("unduhan", &format!("selesai ({:?})", f.source));
                self.dl_done = Some(f);
                timer(self.hwnd, T_DLDONE, nacre_core::notch::download::DONE_LINGER_MS);
            }
            changed = true;
        }
        if changed {
            let app = self.dl.as_ref().or(self.dl_done.as_ref()).map(|d| d.app.clone()).unwrap_or_default();
            if !app.is_empty() && !self.dl_icons.contains_key(&app) {
                let px = (44.0 * self.dpi).round() as i32;
                let s = nacre_platform::icons::load(&app, px).and_then(|b| self.paint.gfx.surface_from_bgra(b.w, b.h, &b.bgra).ok());
                self.dl_icons.insert(app, s);
            }
            self.refresh();
        }
    }

    /// Data tampilan unduhan (fungsi terkait, bukan metode, agar tidak meminjam seluruh `self` saat menggambar).
    fn download_view<'a>(
        dl: &'a Option<nacre_core::notch::download::Download>,
        dl_done: &'a Option<nacre_core::notch::download::Download>,
        icons: &'a std::collections::HashMap<String, Option<CompositionDrawingSurface>>,
        send: Option<(f32, bool)>,
    ) -> Option<views::DownloadView<'a>> {
        let (d, done) = match (dl, dl_done) {
            (Some(d), _) => (d, false),
            (None, Some(d)) => (d, true),
            _ => {
                let (fr, done) = send?;
                let pct = (fr * 100.0).floor() as u32;
                return Some(views::DownloadView { icon: None, label: format!("{}", if done { 100 } else { pct.min(99) }), frac: Some(fr), done, up: true });
            }
        };
        Some(views::DownloadView {
            icon: icons.get(&d.app).and_then(|s| s.as_ref()),
            label: nacre_core::notch::download::label(d),
            frac: d.percent.map(|p| p / 100.0),
            done,
            up: false,
        })
    }

    // ---------- sapaan & info ----------

    fn glance_items(&self) -> Vec<glance::Item> {
        glance::items(self.battery.is_some())
    }

    fn glance_text(&self, item: glance::Item) -> String {
        let s = crate::prefs::get();
        let now = nacre_platform::time::local_now();
        let lang = s.lang;
        match item {
            glance::Item::Greeting => {
                let name = if s.greet_name.is_empty() { std::env::var("USERNAME").unwrap_or_default() } else { s.greet_name.as_str().to_string() };
                glance::greeting_text(s.greet_text.as_str(), &name, now.hour, lang)
            }
            glance::Item::Clock => glance::clock(now.hour, now.minute, s.clock_24h, lang),
            glance::Item::Date => {
                let wd = calendar::weekday(now.year, now.month, now.day) as usize;
                let days = nacre_core::i18n::weekday_labels(lang, false, calendar::weekday_labels(false));
                let month: String = nacre_core::i18n::month_name(lang, now.month, calendar::month_name(now.month)).chars().take(3).collect();
                format!("{}, {} {}", days[wd], now.day, month)
            }
            glance::Item::Battery => self.battery.map_or(String::new(), |(p, _)| format!("{p}%")),
        }
    }

    /// Pergantian sapaan & info hanya berjalan saat tampil dan tidak sedang diintip/terbuka.
    fn sync_glance(&self) {
        if self.look == state::look(Mode::Rest, Some(state::Activity::Glance)) {
            let ms = std::env::var("NACRE_GLANCE_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(glance::ROTATE_MS);
            timer(self.hwnd, T_GLANCE, ms);
        } else {
            kill(self.hwnd, T_GLANCE);
        }
    }

    fn rotate_glance(&mut self) {
        let n = self.glance_items().len();
        self.glance_idx = glance::next(self.glance_idx, n);
        // isi lama naik dan memudar, isi baru naik dari bawah; ukuran notch tetap
        self.retire_content(120);
        let rise = 6.0 * self.dpi;
        if let Ok(g) = self.body.ghost.cast::<CompositionObject>() {
            let _ = keyframes(&self.paint.comp, &g, "Offset.Y", 0.0, -rise, 160, 0);
        }
        self.render();
        if let Ok(c) = self.body.content.cast::<CompositionObject>() {
            let comp = &self.paint.comp;
            let _ = self.body.content.SetScale(Vector3 { X: 1.0, Y: 1.0, Z: 1.0 });
            let _ = keyframes(comp, &c, "Opacity", 0.0, 1.0, 180, 60);
            let _ = keyframes(comp, &c, "Offset.Y", rise, 0.0, 220, 40);
        }
    }

    // ---------- keadaan ----------

    fn dispatch(&mut self, ev: Event) {
        let open_on = if crate::prefs::get().notch_hover_open { OpenOn::Hover } else { OpenOn::Click };
        let next = state::step(self.mode, ev, open_on, self.tracking);
        if next != self.mode {
            log::line("notch", &format!("{:?} --{ev:?}--> {next:?}", self.mode));
            self.mode = next;
            self.refresh();
            self.update_visibility();
        }
    }

    /// Hitung ulang tampilan dari mode + data; beranimasi bila berubah, selain itu cukup gambar ulang isinya.
    fn refresh(&mut self) {
        let look = state::look(self.mode, state::activity(&self.live()));
        if look != self.look || self.first {
            self.transition(look);
        } else {
            self.render();
        }
    }

    fn springs(old: Look, new: Look) -> (f32, u32) {
        let big = |v: View| matches!(v, View::Open(_) | View::Drop);
        if big(new.view) && !big(old.view) {
            (motion::OPEN_RATIO, motion::OPEN_PERIOD_MS)
        } else if big(old.view) && !big(new.view) {
            (motion::CLOSE_RATIO, motion::CLOSE_PERIOD_MS)
        } else if old.view == new.view {
            (motion::PEEK_RATIO, motion::PEEK_PERIOD_MS)
        } else {
            (motion::SWAP_RATIO, motion::SWAP_PERIOD_MS)
        }
    }

    fn transition(&mut self, new: Look) {
        let old = self.look;
        let first = self.first;
        self.first = false;
        self.look = new;
        let view_changed = old.view != new.view || first;
        // kalender kembali ke hari ini setiap panel ditutup
        if !matches!(new.view, View::Open(_)) {
            self.cal_sel = 0;
        }
        let springy = (!first).then(|| Self::springs(old, new));
        if view_changed && !first {
            let closing = matches!(old.view, View::Open(_) | View::Drop) && !matches!(new.view, View::Open(_) | View::Drop);
            self.retire_content(if closing { motion::CLOSE_FADE_MS } else { motion::SWAP_FADE_MS });
        }
        self.apply_size(springy);
        self.render();
        self.sync_glance();
        if view_changed && !first {
            self.intro(matches!(new.view, View::Open(_) | View::Drop) && !matches!(old.view, View::Open(_) | View::Drop));
        }
    }

    /// Ukuran target untuk tampilan sekarang, badan + bayangan ikut beranimasi.
    fn apply_size(&mut self, springy: Option<(f32, u32)>) {
        let (px0, pw, ph) = (self.x0(), self.size.w, self.size.h);
        let mut z = state::size(self.look, self.dpi);
        z.radius *= crate::prefs::round(crate::prefs::Role::Notch);
        z.w = z.w.round();
        z.h = z.h.round();
        self.size = z;
        let x0 = self.x0();
        self.body.set_geometry(self.win, x0, z.w, z.h, z.radius, springy);
        let (blur, dy, op) = state::shadow(self.look);
        let over = self.body.top_over;
        if let Some(sh) = self.shadow.as_mut() {
            sh.follow(&self.paint.gfx, (x0, -over, z.w, z.h + over), z.radius, (blur * self.dpi, dy * self.dpi, op), springy);
        }
        let e = self.body.ear;
        let (l, r) = (px0.min(x0) - e, (px0 + pw).max(x0 + z.w) + e);
        self.set_region((l.max(0.0), 0.0, r.min(self.win.0 as f32), ph.max(z.h)));
        timer(self.hwnd, T_REGION, 700);
    }

    fn set_region(&self, r: (f32, f32, f32, f32)) {
        unsafe {
            let rgn = CreateRectRgn(r.0 as i32, r.1 as i32, r.2.ceil() as i32, r.3.ceil() as i32);
            SetWindowRgn(self.hwnd, Some(rgn), true);
        }
    }

    fn settle_region(&self) {
        let (x0, e) = (self.x0(), self.body.ear);
        self.set_region(((x0 - e).max(0.0), 0.0, (x0 + self.size.w + e).min(self.win.0 as f32), self.size.h));
    }

    fn render(&mut self) {
        let c = self.body.content.clone();
        if let Ok(k) = c.Children() {
            let _ = k.RemoveAll();
        }
        self.regions.clear();
        let f = Frame { x0: self.x0(), w: self.size.w, h: self.size.h };
        let now_ms = self.now_ms();
        let items = self.glance_items();
        let item = items[self.glance_idx.min(items.len() - 1)];
        let glance = (self.look.view == View::Compact(state::Activity::Glance)).then(|| (item, self.glance_text(item)));
        self.live_vis = views::Live::default();
        if self.mode == Mode::Open(Tab::Shelf) {
            self.load_shelf_icons();
        }
        if self.mode == Mode::Open(Tab::Timer) {
            self.ensure_events();
        }
        if self.has_media && self.art.is_none() {
            self.ensure_app_icon();
        }
        let cal_date = self.cal_date();
        let events = match &self.cal_events {
            Some((d, Some(list))) if *d == cal_date => views::Events::List(list),
            Some((d, None)) if *d == cal_date => views::Events::Unavailable,
            _ => views::Events::Loading,
        };
        let send_c = self.send_compact();
        let cal = views::CalView { today: Self::today(), sel: cal_date, events };
        let send = match &self.send {
            Some(ss) => {
                let n = ss.names.len();
                let title = if n == 1 { ss.names[0].clone() } else { format!("{} {}", crate::prefs::tr("Kirim"), views::count_text(n)) };
                let frac = nacre_core::notch::send::fraction(ss.prog.sent, ss.prog.total);
                let done = ss.done_at.is_some() && !ss.prog.active;
                let status = if done {
                    format!("{} \u{2713}", crate::prefs::tr("Terkirim"))
                } else if ss.prog.active || ss.prog.sent > 0 {
                    format!("{} {}%", crate::prefs::tr("Mengirim"), (frac * 100.0).floor() as u32)
                } else {
                    crate::prefs::tr("Menunggu HP\u{2026}").to_string()
                };
                Some(views::SendView { qr: ss.qr.as_ref(), qr_px: ss.qr_px, title, status, frac, done, error: false })
            }
            None if self.send_err => Some(views::SendView { qr: None, qr_px: 0.0, title: String::new(), status: String::new(), frac: 0.0, done: false, error: true }),
            None => None,
        };
        // tanpa sampul: ikon app pemutar menggantikan sampulnya (tidak pernah ditampilkan sebagai badge)
        let app_icon = if self.art.is_none() { self.app_icons.get(&self.media_app).and_then(|s| s.as_ref()) } else { None };
        let pos_ms = self.position_ms();
        let media = self.has_media.then(|| MediaView {
            title: &self.title,
            artist: &self.artist,
            playing: self.playing,
            pos_ms,
            dur_ms: self.dur_ms,
            can_seek: self.can_seek,
            art: self.art.as_ref().or(app_icon),
            tint: self.tint,
            drag: self.seek_drag.map(|(_, _, fr)| fr),
        });
        let lyrics = LyricsView { lines: self.lyr.as_ref(), state: self.lyr_state, pos_ms, lrclib: self.lyr_lrclib };
        let shelf = self.shelf.items().iter().map(|i| (i.name(), self.shelf_icons.get(&i.path))).collect();
        let drop_count = if self.mode == Mode::Drop { self.dnd.borrow().len() } else { 0 };
        let d = Data {
            battery: self.battery,
            timer: &self.timer,
            now_ms,
            glance,
            media,
            lyrics,
            shelf,
            drop_zone: self.drop_zone,
            drop_count,
            privacy: self.privacy,
            download: (self.look.view == View::Compact(state::Activity::Download)).then(|| Self::download_view(&self.dl, &self.dl_done, &self.dl_icons, send_c)).flatten(),
            cal,
            send,
            send_zone: crate::prefs::get().notch_send,
        };
        views::draw(&mut self.paint, &c, self.look, f, &d, &mut self.regions, &mut self.live_vis);
        self.sync_visualizer();
        self.sync_progress();
    }

    /// Isi lama pindah ke lapisan ghost lalu memudar.
    fn retire_content(&mut self, ms: u32) {
        let go = || -> windows_core::Result<()> {
            let kids = self.body.content.Children()?;
            let old: Vec<Visual> = kids.clone().into_iter().collect();
            let ghost = self.body.ghost.Children()?;
            ghost.RemoveAll()?;
            self.body.ghost.SetOffset(Vector3 { X: 0.0, Y: 0.0, Z: 0.0 })?;
            for v in &old {
                kids.Remove(v)?;
                ghost.InsertAtTop(v)?;
            }
            let g: CompositionObject = self.body.ghost.cast()?;
            keyframes(&self.paint.comp, &g, "Opacity", 1.0, 0.0, ms, 0)
        };
        let _ = go();
        timer(self.hwnd, T_GHOST, 260);
    }

    /// Isi baru masuk. Membuka panel: tertunda, memudar masuk sambil sedikit membesar dan turun ke tempatnya.
    fn intro(&self, opening: bool) {
        let go = || -> windows_core::Result<()> {
            let c: CompositionObject = self.body.content.cast()?;
            let comp = &self.paint.comp;
            if opening {
                let cx = self.x0() + self.size.w / 2.0;
                self.body.content.SetCenterPoint(Vector3 { X: cx, Y: 0.0, Z: 0.0 })?;
                let (d, ms) = (motion::CONTENT_DELAY_MS, motion::CONTENT_FADE_MS);
                keyframes(comp, &c, "Opacity", 0.0, 1.0, ms, d)?;
                keyframes(comp, &c, "Scale.X", motion::CONTENT_SCALE_FROM, 1.0, ms + 60, d)?;
                keyframes(comp, &c, "Scale.Y", motion::CONTENT_SCALE_FROM, 1.0, ms + 60, d)?;
                keyframes(comp, &c, "Offset.Y", -motion::CONTENT_RISE_DP * self.dpi, 0.0, ms + 60, d)
            } else {
                self.body.content.SetScale(Vector3 { X: 1.0, Y: 1.0, Z: 1.0 })?;
                self.body.content.SetOffset(Vector3 { X: 0.0, Y: 0.0, Z: 0.0 })?;
                keyframes(comp, &c, "Opacity", 0.0, 1.0, 150, 50)
            }
        };
        let _ = go();
    }

    // ---------- musik ----------

    fn position_ms(&self) -> i64 {
        let extra = if self.playing { self.pos_at.elapsed().as_millis() as i64 } else { 0 };
        let p = self.pos_ms + extra;
        if self.dur_ms > 0 {
            p.clamp(0, self.dur_ms)
        } else {
            p.max(0)
        }
    }

    fn on_media(&mut self) {
        let mut changed = false;
        while let Ok(s) = self.media.snapshots.try_recv() {
            changed = true;
            self.has_media = s.has_media;
            self.title = s.title;
            self.artist = s.artist;
            self.album = s.album;
            self.media_app = s.app;
            self.playing = s.playing;
            self.pos_ms = s.position_ms;
            self.dur_ms = s.duration_ms;
            self.pos_at = Instant::now();
            self.can_seek = s.can_seek;
            match s.art {
                ArtUpdate::Unchanged => {}
                ArtUpdate::None => {
                    self.art = None;
                    self.tint = None;
                }
                ArtUpdate::New(b) => {
                    self.tint = nacre_core::notch::tint::from_art(&b.bgra, b.w, b.h);
                    self.art = self.paint.gfx.surface_from_bgra(b.w, b.h, &b.bgra).ok();
                }
            }
        }
        if changed {
            let track = if self.has_media { format!("{}\u{1}{}", self.title, self.artist) } else { String::new() };
            if track != self.lyr_track {
                self.lyr_track = track;
                self.request_lyrics();
            }
            self.refresh();
            self.schedule_lyric();
        }
    }

    // ---------- lirik ----------

    fn request_lyrics(&mut self) {
        self.lyr = None;
        self.lyr_idx = None;
        self.lyr_lrclib = false;
        if !self.has_media {
            self.lyr_state = LyrState::Idle;
            self.lyr_key.clear();
            return;
        }
        let (title, artist) = nacre_core::notch::lrclib::clean(&self.title, &self.artist);
        let track = nacre_core::notch::lrclib::Track {
            title,
            artist,
            album: self.album.clone(),
            duration_s: (self.dur_ms > 0).then(|| ((self.dur_ms + 500) / 1000) as u32),
        };
        self.lyr_key = nacre_core::notch::lrclib::cache_key(&track);
        self.lyr_state = LyrState::Loading;
        let online = crate::prefs::get().lyrics_online;
        let _ = self.lyr_worker.requests.send(nacre_platform::lyrics::Request { track, online });
    }

    fn on_lyrics(&mut self) {
        let mut got = false;
        while let Ok(a) = self.lyr_worker.answers.try_recv() {
            if a.key != self.lyr_key {
                continue;
            }
            got = true;
            let parsed = a.text.map(|t| nacre_core::notch::lyrics::parse(&t)).filter(|l| !l.is_empty());
            self.lyr_state = if parsed.is_some() { LyrState::Ready } else { LyrState::Missing };
            self.lyr_lrclib = a.source == nacre_platform::lyrics::Source::Lrclib;
            log::line("lirik", &format!("{:?} sumber={:?} baris={}", self.lyr_state, a.source, parsed.as_ref().map_or(0, |l| l.lines.len())));
            self.lyr = parsed;
            self.lyr_idx = None;
        }
        if got {
            self.refresh();
            self.schedule_lyric();
        }
    }

    /// Bangun tepat saat baris berikutnya dimulai (tanpa polling); hanya selama lagu diputar.
    fn schedule_lyric(&self) {
        kill(self.hwnd, T_LYRIC);
        let Some(l) = &self.lyr else { return };
        if !l.synced || !self.playing {
            return;
        }
        let pos = self.position_ms();
        if let Some(next) = l.next_change_ms(pos) {
            timer(self.hwnd, T_LYRIC, ((next - pos).max(10) + 15).min(60_000) as u32);
        }
    }

    fn on_lyric_tick(&mut self) {
        let pos = self.position_ms();
        let idx = self.lyr.as_ref().and_then(|l| l.index_at(pos));
        if idx != self.lyr_idx {
            let forward = matches!((self.lyr_idx, idx), (Some(a), Some(b)) if b == a + 1) || (self.lyr_idx.is_none() && idx == Some(0));
            self.lyr_idx = idx;
            if self.look.view == View::Compact(state::Activity::Lyrics) {
                self.swap_lyric();
            } else if self.mode == Mode::Open(Tab::Play) || self.look.view == View::Compact(state::Activity::Player) {
                self.render();
                // maju satu baris: isi lirik bergulir naik ke posisi barunya
                if forward {
                    if let Some(sc) = &self.live_vis.lyric_scroll {
                        if let Ok(o) = sc.cast::<CompositionObject>() {
                            let _ = keyframes(&self.paint.comp, &o, "Offset.Y", self.live_vis.lyric_shift, 0.0, 420, 0);
                        }
                    }
                }
            }
        }
        self.schedule_lyric();
    }

    /// Baris lirik ringkas berganti: baris lama naik dan memudar, baris baru naik dari bawah.
    fn swap_lyric(&mut self) {
        let rise = state::motion::LYRIC_RISE_DP * self.dpi;
        let ms = state::motion::LYRIC_SWAP_MS;
        if let Some(old) = self.live_vis.lyric.take() {
            let go = || -> windows_core::Result<()> {
                let ghost = self.body.ghost.Children()?;
                ghost.RemoveAll()?;
                self.body.ghost.SetOffset(Vector3 { X: 0.0, Y: 0.0, Z: 0.0 })?;
                self.body.content.Children()?.Remove(&old)?;
                ghost.InsertAtTop(&old)?;
                let g: CompositionObject = self.body.ghost.cast()?;
                keyframes(&self.paint.comp, &g, "Opacity", 1.0, 0.0, ms, 0)?;
                keyframes(&self.paint.comp, &g, "Offset.Y", 0.0, -rise, ms, 0)
            };
            let _ = go();
            timer(self.hwnd, T_GHOST, ms + 60);
        }
        self.render();
        if let Some(v) = &self.live_vis.lyric {
            if let (Ok(o), Ok(at)) = (v.cast::<CompositionObject>(), v.Offset()) {
                let comp = &self.paint.comp;
                let _ = keyframes(comp, &o, "Opacity", 0.0, 1.0, ms, 40);
                let _ = keyframes(comp, &o, "Offset.Y", at.Y + rise, at.Y, ms + 40, 0);
            }
        }
    }

    /// Visualizer hanya hidup saat musik ringkas tampil dan lagu diputar (loopback berhenti total selain itu).
    fn sync_visualizer(&mut self) {
        // tetap hidup selama lagu diputar dan notch terlihat (tidak dinyalakan-matikan tiap ganti tampilan)
        let want = self.has_media && self.playing && self.shown && !self.overlap.fullscreen && !self.off;
        if want && self.loopback.is_none() {
            let hw = self.hwnd.0 as isize;
            self.loopback = Some(loopback::start(move || post(hw, WM_BANDS)));
        } else if !want && self.loopback.is_some() {
            self.loopback = None;
        }
    }

    fn on_bands(&mut self) {
        let b = loopback::latest();
        for (i, bar) in self.live_vis.bars.iter().enumerate() {
            let v = b.get(i).copied().unwrap_or(0.0).clamp(0.0, 1.0);
            let _ = bar.SetScale(Vector3 { X: 1.0, Y: 0.25 + 0.75 * v, Z: 1.0 });
        }
    }

    /// Waktu lagu di tab Putar diperbarui tiap detik selama terlihat dan diputar.
    fn sync_progress(&self) {
        if self.mode == Mode::Open(Tab::Play) && self.has_media && self.playing && self.seek_drag.is_none() {
            timer(self.hwnd, T_PROGRESS, 1000);
        } else {
            kill(self.hwnd, T_PROGRESS);
        }
    }

    fn on_down(&mut self, x: f32, y: f32) -> bool {
        if !matches!(self.mode, Mode::Open(_)) {
            return false;
        }
        if let Some(Action::Seek(sx, sw)) = self.regions.iter().find(|r| r.hit(x, y)).map(|r| r.action) {
            self.seek_drag = Some((sx, sw, ((x - sx) / sw).clamp(0.0, 1.0)));
            self.render();
            return true;
        }
        false
    }

    fn on_drag(&mut self, x: f32) {
        if let Some((sx, sw, _)) = self.seek_drag {
            self.seek_drag = Some((sx, sw, ((x - sx) / sw).clamp(0.0, 1.0)));
            self.render();
        }
    }

    /// Lepas penggeser: lompat ke posisi itu. `true` bila memang sedang menyeret.
    fn on_up(&mut self) -> bool {
        let Some((_, _, fr)) = self.seek_drag.take() else { return false };
        if self.dur_ms > 0 {
            let ms = (self.dur_ms as f32 * fr) as i64;
            let _ = self.media.commands.send(Command::SeekMs(ms));
            self.pos_ms = ms;
            self.pos_at = Instant::now();
        }
        self.render();
        self.schedule_lyric();
        true
    }

    // ---------- Rak & seret-taruh ----------

    fn load_shelf_icons(&mut self) {
        let px = (40.0 * self.dpi).round() as i32;
        for it in self.shelf.items().iter().take(8) {
            if self.shelf_icons.contains_key(&it.path) {
                continue;
            }
            let bmp = nacre_platform::icons::thumbnail(&it.path, px).or_else(|| nacre_platform::icons::load(&it.path, px));
            if let Some(b) = bmp {
                if let Ok(s) = self.paint.gfx.surface_from_bgra(b.w, b.h, &b.bgra) {
                    self.shelf_icons.insert(it.path.clone(), s);
                }
            }
        }
    }

    fn zone_at(&self, x: f32, y: f32) -> Option<usize> {
        self.regions.iter().find(|r| matches!(r.action, Action::Zone(_)) && r.hit(x, y)).and_then(|r| match r.action {
            Action::Zone(i) => Some(i),
            _ => None,
        })
    }

    fn on_dnd(&mut self, kind: usize, lp: LPARAM) {
        let (sx, sy) = ((lp.0 as u32 & 0xFFFF) as u16 as i16 as f32, ((lp.0 as u32 >> 16) & 0xFFFF) as u16 as i16 as f32);
        let (cx, cy) = (sx - ((self.screen_w - self.win.0) / 2) as f32, sy);
        match kind {
            drop::ENTER => {
                self.drop_zone = None;
                self.dispatch(Event::DragEnter);
                self.drop_zone = self.zone_at(cx, cy);
                self.render();
            }
            drop::OVER => {
                let z = self.zone_at(cx, cy);
                if z != self.drop_zone {
                    self.drop_zone = z;
                    self.render();
                }
            }
            drop::LEAVE => {
                self.drop_zone = None;
                self.dispatch(Event::DragLeave);
            }
            drop::DROP => {
                let zone = self.zone_at(cx, cy).unwrap_or(0);
                let paths = self.dnd.borrow().clone();
                self.drop_zone = None;
                log::line("notch", &format!("ditaruh {} berkas di zona {zone}", paths.len()));
                if zone == 0 {
                    for p in &paths {
                        self.shelf.add(p);
                    }
                    self.dispatch(Event::Dropped);
                } else if zone == 1 {
                    self.start_send(paths);
                } else {
                    self.pending = paths;
                    self.dispatch(Event::DragLeave);
                    post_w(self.hwnd.0 as isize, WM_AFTERDROP, zone);
                }
            }
            _ => {}
        }
    }

    fn shelf_menu(&mut self, i: usize, x: f32, y: f32) {
        if self.menu.is_none() {
            match Menu::new(&self.paint.comp, self.hwnd, self.dpi, sys::screen_size(), Tier::Full) {
                Ok(m) => self.menu = Some(m),
                Err(_) => return,
            }
        }
        let tr = crate::prefs::tr;
        let items = vec![
            MItem::action(1, tr("Buka"), true),
            MItem::action(2, tr("Tampilkan di folder"), true),
            MItem::action(3, tr("Salin jalur"), true),
            MItem::action(5, tr("Kirim ke HP"), crate::prefs::get().notch_send),
            MItem::separator(),
            MItem::action(4, tr("Hapus dari Rak"), true),
        ];
        self.menu_item = Some(i);
        let wx = (self.screen_w - self.win.0) / 2;
        if let Some(m) = self.menu.as_mut() {
            let _ = m.show(&self.paint.comp, &self.paint.gfx, items, wx + x as i32, y as i32);
        }
    }

    fn menu_pick(&mut self, id: usize) {
        if let Some(m) = &self.menu {
            m.hide();
        }
        let Some(i) = self.menu_item.take() else { return };
        let Some(path) = self.shelf.items().get(i).map(|it| it.path.clone()) else { return };
        match id {
            1 => nacre_platform::launch::open(&path),
            2 => nacre_platform::launch::run("explorer.exe", &format!("/select,\"{path}\"")),
            3 => {
                let _ = nacre_platform::clipboard::copy_text(&path);
            }
            4 => {
                self.shelf.remove(&path);
                self.shelf_icons.remove(&path);
                self.refresh();
            }
            5 => self.start_send(vec![path]),
            _ => {}
        }
    }

    fn on_right_click(&mut self, x: f32, y: f32) {
        if self.mode != Mode::Open(Tab::Shelf) {
            return;
        }
        if let Some(Action::ShelfItem(i)) = self.regions.iter().find(|r| r.hit(x, y)).map(|r| r.action) {
            self.shelf_menu(i, x, y);
        }
    }

    fn poll_privacy(&mut self) {
        if self.shown && !self.off {
            let a = nacre_platform::privacy::active();
            let v = (a.iter().any(|u| !u.camera), a.iter().any(|u| u.camera));
            if v != self.privacy {
                self.privacy = v;
                self.render();
            }
        }
        timer(self.hwnd, T_PRIVACY, 3000);
    }

    // ---------- input ----------

    fn on_move(&mut self) {
        if !self.tracking {
            let mut t = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: self.hwnd, dwHoverTime: 0 };
            if unsafe { TrackMouseEvent(&mut t) }.is_ok() {
                self.tracking = true;
            }
        }
        kill(self.hwnd, T_LEAVE);
        if self.grace {
            self.grace = false;
            kill(self.hwnd, T_GRACE);
        }
        self.dispatch(Event::Enter);
        if self.mode == Mode::Peek && crate::prefs::get().notch_hover_open {
            if self.cursor_on_media_button() {
                // di atas tombol mundur/putar/maju: biarkan diklik tanpa membuka panel
                kill(self.hwnd, T_DWELL);
            } else {
                // dimulai ulang tiap gerakan: panel terbuka hanya bila kursor benar-benar berhenti di notch
                timer(self.hwnd, T_DWELL, motion::DWELL_MS);
            }
        }
    }

    fn on_leave(&mut self) {
        self.tracking = false;
        kill(self.hwnd, T_DWELL);
        if self.seek_drag.is_some() || self.menu.as_ref().is_some_and(|m| m.is_open()) {
            // penggeser masih diseret atau menu klik-kanan terbuka: jangan tutup panel
            return;
        }
        if matches!(self.mode, Mode::Open(_)) {
            timer(self.hwnd, T_LEAVE, motion::LEAVE_GRACE_MS);
        } else {
            self.dispatch(Event::Leave);
        }
        self.grace = true;
        timer(self.hwnd, T_GRACE, 800);
        self.update_visibility();
    }

    /// Titik (px jendela) berada di tombol media notch diam "musik & lirik".
    fn on_media_button(&self, x: f32, y: f32) -> bool {
        self.look.view == View::Compact(state::Activity::Player) && self.regions.iter().any(|r| r.hit(x, y) && matches!(r.action, Action::PlayPause | Action::Next | Action::Prev))
    }

    /// Kursor sekarang di tombol media notch diam (panel tidak dibuka oleh diam kursor di sana).
    fn cursor_on_media_button(&self) -> bool {
        let mut pt = windows::Win32::Foundation::POINT::default();
        unsafe {
            if GetCursorPos(&mut pt).is_err() {
                return false;
            }
            let _ = windows::Win32::Graphics::Gdi::ScreenToClient(self.hwnd, &mut pt);
        }
        self.on_media_button(pt.x as f32, pt.y as f32)
    }

    fn on_click(&mut self, x: f32, y: f32) {
        if !matches!(self.mode, Mode::Open(_)) && !self.on_media_button(x, y) {
            kill(self.hwnd, T_DWELL);
            self.dispatch(Event::Click);
            return;
        }
        let Some(a) = self.regions.iter().find(|r| r.hit(x, y)).map(|r| r.action) else { return };
        let now = self.now_ms();
        match a {
            Action::PlayPause => {
                let _ = self.media.commands.send(Command::PlayPause);
            }
            Action::Next => {
                let _ = self.media.commands.send(Command::Next);
            }
            Action::Prev => {
                let _ = self.media.commands.send(Command::Prev);
            }
            Action::Seek(..) | Action::Zone(_) => {}
            Action::ShelfItem(i) => {
                if let Some(it) = self.shelf.items().get(i) {
                    nacre_platform::launch::open(&it.path);
                }
            }
            Action::ShelfClear => {
                self.shelf.clear();
                self.shelf_icons.clear();
                self.refresh();
            }
            Action::Tab(t) => self.dispatch(Event::SelectTab(t)),
            Action::Settings => crate::settings_win::open(&self.paint.comp),
            Action::TimerToggle => {
                if self.timer.phase == Phase::Running {
                    self.timer.pause(now);
                } else {
                    self.timer.start(now);
                }
                self.sync_tick();
                self.refresh();
            }
            Action::TimerReset => {
                self.timer.reset();
                self.sync_tick();
                self.refresh();
            }
            Action::TimerPlus => {
                self.timer.adjust(300);
                self.refresh();
            }
            Action::TimerMinus => {
                self.timer.adjust(-300);
                self.refresh();
            }
            Action::SendDone => self.stop_send(),
            Action::CalDay(n) => {
                self.cal_sel = (self.cal_sel + n).clamp(-3650, 3650);
                self.render();
            }
        }
    }

    fn on_wheel(&mut self, delta: i16) {
        if let Mode::Open(cur) = self.mode {
            let i = Tab::ALL.iter().position(|t| *t == cur).unwrap_or(0) as i32;
            let n = Tab::ALL.len() as i32;
            let next = Tab::ALL[(i + if delta > 0 { -1 } else { 1 }).rem_euclid(n) as usize];
            self.dispatch(Event::SelectTab(next));
        }
    }

    // ---------- data yang berjalan ----------

    fn sync_tick(&self) {
        if self.timer.phase == Phase::Running {
            timer(self.hwnd, T_TICK, 250);
        } else {
            kill(self.hwnd, T_TICK);
        }
    }

    fn on_tick(&mut self) {
        let now = self.now_ms();
        let finished = self.timer.tick(now);
        let s = self.timer.remaining_secs(now);
        if finished || s != self.last_sec {
            self.last_sec = s;
            self.refresh();
        }
        if finished {
            log::line("notch", "timer selesai");
            self.sync_tick();
        }
    }

    fn on_power(&mut self) {
        let b = sys::battery();
        let was = self.battery.is_some_and(|x| x.1);
        let now = b.is_some_and(|x| x.1);
        self.battery = b;
        if now && !was {
            self.charging_fresh = true;
            timer(self.hwnd, T_CHARGE, 4000);
        }
        self.refresh();
    }

    // ---------- tampil/sembunyi, ruang, pengaturan ----------

    /// Kotak notch diam (px layar) untuk uji tumpang-tindih dan celah menu bar.
    fn pill_screen_rect(&self) -> (i32, i32, i32, i32) {
        let base = state::size(state::look(Mode::Rest, None), self.dpi);
        let l = ((sys::screen_size().0 as f32 - base.w) / 2.0) as i32;
        (l, 0, l + base.w as i32, (base.h + 4.0 * self.dpi) as i32)
    }

    fn place_strip(&self) {
        if let Some(s) = &self.strip {
            let (l, _, r, _) = self.pill_screen_rect();
            let pad = (60.0 * self.dpi) as i32;
            s.place(l - pad, 0, r - l + 2 * pad, 2);
            s.set_visible(!self.overlap.fullscreen && !self.off);
        }
    }

    fn place_shadow_window(&self) {
        if let Some(sh) = &self.shadow {
            let x = (self.screen_w - self.win.0) / 2 - sh.pad as i32;
            sh.place_window(self.hwnd, x, 0, self.win.0 + 2 * sh.pad as i32, self.win.1 + sh.pad as i32);
            sh.set_visible(self.shown && !self.off);
        }
    }

    fn interacting(&self) -> bool {
        self.tracking || matches!(self.mode, Mode::Open(_) | Mode::Drop) || self.grace
    }

    fn update_visibility(&mut self) {
        let inputs = Inputs {
            overlapped: self.overlap.overlapped,
            interacting: self.interacting(),
            edge_armed: self.edge_armed,
            starting: self.starting,
            fullscreen: self.overlap.fullscreen,
        };
        let hide = self.off || should_hide(self.vis, &inputs);
        if let Some(s) = &self.strip {
            s.set_visible(!self.overlap.fullscreen && !self.off);
        }
        if hide && self.shown {
            self.shown = false;
            let y = -(self.size.h + self.body.top_over + 8.0 * self.dpi);
            self.body.stage_y(y, 0.95, 90);
            if let Some(sh) = &self.shadow {
                sh.stage_y(y, 0.95, 90);
            }
            timer(self.hwnd, T_HIDEWIN, 550);
            log::line("notch", "sembunyi");
        } else if !hide && !self.shown {
            self.shown = true;
            kill(self.hwnd, T_HIDEWIN);
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
            }
            if let Some(sh) = &self.shadow {
                sh.set_visible(true);
                sh.stage_y(0.0, 0.8, 90);
            }
            self.body.stage_y(0.0, 0.8, 90);
            log::line("notch", "tampil");
        }
    }

    fn publish_notch(&self) {
        if self.off {
            crate::services::set_notch(0, 0, 0);
            return;
        }
        let (l, _, r, h) = self.pill_screen_rect();
        crate::services::set_notch(l, r, h - (4.0 * self.dpi) as i32);
    }

    fn set_reserved(&mut self, on: bool) {
        self.reserved = on;
        if on {
            let strip = (34.0 * self.dpi).ceil() as i32;
            let r = nacre_platform::shell::appbar::reserve_top(self.hwnd, self.screen_w, strip);
            log::line("appbar", &format!("atas strip={strip} hasil={r:?}"));
        } else {
            nacre_platform::shell::appbar::release_top();
        }
    }

    fn apply_prefs(&mut self) {
        let s = crate::prefs::get();
        self.off = !s.notch_enabled;
        // menu bar aktif: dialah satu-satunya yang mencadangkan ruang atas
        let (vis, reserve) = (s.notch.mode(), s.notch.reserves() && s.notch_enabled && !s.menubar);
        if vis != self.vis {
            self.vis = vis;
            if vis == Vis::Always {
                self.strip = None;
                self.edge_armed = false;
            } else if self.strip.is_none() {
                self.strip = Strip::new(self.hwnd, STRIP_ENTER, STRIP_LEAVE).ok();
                self.place_strip();
            }
        }
        if reserve != self.reserved {
            self.set_reserved(reserve);
        }
        self.publish_notch();
        let _ = self.body.panel.set_params(body::glass_params(self.size.radius));
        self.body.paint_ears(self.tier);
        if let Some(sh) = &self.shadow {
            sh.set_visible(self.shown && !self.off);
        }
        self.update_visibility();
        self.sync_download_watch();
        // tampilan bisa berubah (mis. "Saat diam" diganti): hitung ulang tanpa animasi
        self.first = true;
        self.refresh();
        log::line("notch", &format!("pengaturan: perilaku={vis:?} cadangan={reserve} mati={}", self.off));
    }

    fn refresh_tier(&mut self) {
        let t = auto_tier(self.glass_override);
        if t != self.tier {
            self.tier = t;
            let _ = self.body.panel.set_tier(t);
            self.body.paint_ears(t);
        }
    }

    fn on_reconfigure(&mut self) {
        let screen_w = sys::screen_size().0;
        let dpi = unsafe { GetDpiForWindow(self.hwnd) }.max(96) as f32 / 96.0;
        let win = win_size(dpi, screen_w);
        if win == self.win && screen_w == self.screen_w && (dpi - self.dpi).abs() < 0.01 {
            return;
        }
        log::line("notch", &format!("konfigurasi ulang layar_w={screen_w} dpi={dpi}"));
        let dpi_changed = (dpi - self.dpi).abs() >= 0.01;
        self.screen_w = screen_w;
        self.dpi = dpi;
        self.win = win;
        unsafe {
            let _ = SetWindowPos(self.hwnd, None, (screen_w - win.0) / 2, 0, win.0, win.1, SWP_NOACTIVATE | SWP_NOZORDER);
        }
        let _ = self._surface.root.SetSize(Vector2 { X: win.0 as f32, Y: win.1 as f32 });
        self.body.resize(win);
        self.paint.reset(dpi);
        if dpi_changed {
            self.body.remake_ears(&self.paint.gfx, dpi, self.tier);
            if let Some(sh) = self.shadow.as_mut() {
                sh.pad = (SHADOW_PAD * dpi).round();
            }
        }
        self.place_shadow_window();
        self.place_strip();
        self.publish_notch();
        self.first = true;
        let look = self.look;
        self.transition(look);
    }

    fn on_timer(&mut self, id: usize) {
        if id != T_TICK {
            kill(self.hwnd, id);
        }
        match id {
            T_DWELL => {
                if self.tracking && !self.cursor_on_media_button() {
                    self.dispatch(Event::Dwell);
                }
            }
            T_LEAVE => {
                if !self.tracking && !self.send_pinned() {
                    self.dispatch(Event::Leave);
                }
            }
            T_GHOST => {
                if let Ok(k) = self.body.ghost.Children() {
                    let _ = k.RemoveAll();
                }
            }
            T_REGION => self.settle_region(),
            T_TICK => self.on_tick(),
            T_CHARGE => {
                self.charging_fresh = false;
                self.refresh();
            }
            T_WINCHG => {
                let mut ov = overlap::evaluate(tracker::external_foreground(), std::process::id(), self.pill_screen_rect(), sys::screen_size());
                if ov.fullscreen && crate::prefs::get().above_fullscreen {
                    ov = overlap::Overlap { overlapped: false, fullscreen: false };
                }
                if ov != self.overlap {
                    self.overlap = ov;
                    self.update_visibility();
                    self.sync_visualizer();
                }
            }
            T_RECONF => self.on_reconfigure(),
            T_EDGE => {
                self.edge_armed = true;
                self.update_visibility();
            }
            T_GRACE => {
                self.grace = false;
                self.update_visibility();
            }
            T_START => {
                self.starting = false;
                self.update_visibility();
            }
            T_GLANCE => {
                self.rotate_glance();
                self.sync_glance();
            }
            T_LYRIC => self.on_lyric_tick(),
            T_PRIVACY => self.poll_privacy(),
            T_SEND => {
                let idle = self.send.as_ref().is_some_and(|s| !s.prog.active && s.last.elapsed() > SEND_IDLE);
                if idle {
                    log::line("kirim", "sesi menganggur 15 menit");
                    self.stop_send();
                } else if self.send.is_some() {
                    timer(self.hwnd, T_SEND, 30_000);
                }
            }
            T_SENDDONE => {
                kill(self.hwnd, T_SENDDONE);
                self.refresh();
            }
            T_TESTSEND => {
                kill(self.hwnd, T_TESTSEND);
                if let Ok(p) = std::env::var("NACRE_TEST_SEND") {
                    self.start_send(p.split(';').map(|s| s.to_string()).collect());
                }
            }
            T_DLDONE => {
                unsafe {
                    let _ = KillTimer(Some(self.hwnd), T_DLDONE);
                }
                self.dl_done = None;
                self.refresh();
            }
            T_PROGRESS => {
                if self.mode == Mode::Open(Tab::Play) {
                    self.render();
                }
            }
            T_MINUTE => {
                if self.look.view == View::Compact(state::Activity::Glance) {
                    self.render();
                }
                timer(self.hwnd, T_MINUTE, nacre_platform::time::ms_to_next_minute());
            }
            T_HIDEWIN => {
                if !self.shown {
                    unsafe {
                        let _ = ShowWindow(self.hwnd, SW_HIDE);
                    }
                    if let Some(sh) = &self.shadow {
                        sh.set_visible(false);
                    }
                }
            }
            _ => {}
        }
    }
}

/// "Salin ke…" (pilih folder lalu salin dengan dialog salin Windows) atau "Buka dengan…" (berkas pertama).
fn after_drop(paths: &[String], zone: usize) {
    if paths.is_empty() {
        return;
    }
    if zone == 2 {
        let Some(dir) = nacre_platform::pick::folder(crate::prefs::tr("Salin ke folder")) else { return };
        let from: Vec<u16> = paths.iter().flat_map(|p| p.encode_utf16().chain(std::iter::once(0))).chain(std::iter::once(0)).collect();
        let to: Vec<u16> = dir.encode_utf16().chain([0, 0]).collect();
        let mut op = windows::Win32::UI::Shell::SHFILEOPSTRUCTW {
            wFunc: windows::Win32::UI::Shell::FO_COPY,
            pFrom: windows::core::PCWSTR(from.as_ptr()),
            pTo: windows::core::PCWSTR(to.as_ptr()),
            fFlags: windows::Win32::UI::Shell::FOF_ALLOWUNDO.0 as u16,
            ..Default::default()
        };
        let r = unsafe { windows::Win32::UI::Shell::SHFileOperationW(&mut op) };
        log::line("notch", &format!("salin {} berkas ke folder: hasil={r}", paths.len()));
    } else {
        let file: Vec<u16> = paths[0].encode_utf16().chain(std::iter::once(0)).collect();
        let info = windows::Win32::UI::Shell::OPENASINFO {
            pcszFile: windows::core::PCWSTR(file.as_ptr()),
            pcszClass: windows::core::PCWSTR::null(),
            oaifInFlags: windows::Win32::UI::Shell::OAIF_ALLOW_REGISTRATION | windows::Win32::UI::Shell::OAIF_EXEC,
        };
        let _ = unsafe { windows::Win32::UI::Shell::SHOpenWithDialog(None, &info) };
    }
}


fn xy(lp: LPARAM) -> (f32, f32) {
    ((lp.0 as i16) as f32, ((lp.0 >> 16) as i16) as f32)
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_LBUTTONDOWN => {
            let (x, y) = xy(lp);
            if with(hwnd, |d| d.on_down(x, y)).unwrap_or(false) {
                let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetCapture(hwnd);
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let (x, _) = xy(lp);
            with(hwnd, |d| {
                d.on_drag(x);
                d.on_move();
            });
            LRESULT(0)
        }
        WM_MEDIA => {
            with(hwnd, |d| d.on_media());
            LRESULT(0)
        }
        WM_BANDS => {
            with(hwnd, |d| d.on_bands());
            LRESULT(0)
        }
        WM_LYRICS => {
            with(hwnd, |d| d.on_lyrics());
            LRESULT(0)
        }
        WM_DOWNLOAD => {
            with(hwnd, |d| d.on_download());
            LRESULT(0)
        }
        WM_AGENDA => {
            with(hwnd, |d| d.on_agenda());
            LRESULT(0)
        }
        WM_SEND => {
            with(hwnd, |d| d.on_send());
            LRESULT(0)
        }
        WM_DND => {
            with(hwnd, |d| d.on_dnd(wp.0, lp));
            LRESULT(0)
        }
        WM_AFTERDROP => {
            // dialog bersifat modal: jalankan di luar penjaga BUSY agar pesan lain tetap terlayani
            let pending = with(hwnd, |d| std::mem::take(&mut d.pending)).unwrap_or_default();
            after_drop(&pending, wp.0);
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            let (x, y) = xy(lp);
            with(hwnd, |d| d.on_right_click(x, y));
            LRESULT(0)
        }
        m if m == MENU_PICK => {
            let id = wp.0;
            with(hwnd, |d| d.menu_pick(id));
            LRESULT(0)
        }
        m if m == MENU_CLOSE => {
            with(hwnd, |d| {
                if let Some(mn) = &d.menu {
                    mn.hide();
                }
                if !d.tracking && matches!(d.mode, Mode::Open(_)) {
                    timer(hwnd, T_LEAVE, motion::LEAVE_GRACE_MS);
                }
            });
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            with(hwnd, |d| d.on_leave());
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let (x, y) = xy(lp);
            let _ = windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
            if with(hwnd, |d| d.on_up()).unwrap_or(false) {
                return LRESULT(0);
            }
            with(hwnd, |d| d.on_click(x, y));
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = (wp.0 >> 16) as i16;
            with(hwnd, |d| d.on_wheel(delta));
            LRESULT(0)
        }
        STRIP_ENTER => {
            with(hwnd, |d| {
                if d.vis != Vis::Always {
                    timer(hwnd, T_EDGE, 200);
                }
            });
            LRESULT(0)
        }
        STRIP_LEAVE => {
            with(hwnd, |d| {
                kill(hwnd, T_EDGE);
                if d.edge_armed {
                    d.edge_armed = false;
                    d.grace = true;
                    timer(hwnd, T_GRACE, 800);
                    d.update_visibility();
                }
            });
            LRESULT(0)
        }
        WM_TIMER => {
            with(hwnd, |d| d.on_timer(wp.0));
            LRESULT(0)
        }
        crate::prefs::WM_PREFS => {
            with(hwnd, |d| d.apply_prefs());
            LRESULT(0)
        }
        WM_TOPMOST => {
            let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            with(hwnd, |d| d.place_shadow_window());
            LRESULT(0)
        }
        WM_OPEN => {
            with(hwnd, |d| {
                if !d.off && !matches!(d.mode, Mode::Open(_)) {
                    d.dispatch(Event::Click);
                }
            });
            LRESULT(0)
        }
        WM_DISPLAYCHANGE | WM_DPICHANGED => {
            timer(hwnd, T_RECONF, 500);
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_POWERBROADCAST => {
            with(hwnd, |d| {
                d.refresh_tier();
                d.on_power();
            });
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_SETTINGCHANGE => {
            with(hwnd, |d| d.refresh_tier());
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_DESTROY => {
            NOTCH_HWND.store(0, Ordering::Relaxed);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
