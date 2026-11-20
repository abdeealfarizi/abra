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
use nacre_core::notch::pills::{Kind as PillKind, Queue as Pills};
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
const WM_TOPMOST: u32 = WM_APP + 72;
const WM_OPEN: u32 = WM_APP + 73;
const WM_MEDIA: u32 = WM_APP + 10;
const WM_BANDS: u32 = WM_APP + 11;
const WM_LYRICS: u32 = WM_APP + 18;
const WM_DND: u32 = WM_APP + 19;
const WM_AFTERDROP: u32 = WM_APP + 20;
const WM_DOWNLOAD: u32 = WM_APP + 21;
const WM_AGENDA: u32 = WM_APP + 23;
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
const T_DLDONE: usize = 18;
const T_SEND: usize = 19;
const T_SENDDONE: usize = 20;
const T_TESTSEND: usize = 21;
const T_PILL: usize = 22;
const T_TODO: usize = 23;
const T_CARET: usize = 24;
const WM_TODO_EDIT: u32 = WM_APP + 28;
const WM_PILLS: u32 = WM_APP + 26;
const WM_TASKS: u32 = WM_APP + 27;
const STRIP_DRAG: u32 = WM_APP + 29;
const WM_DEVICECHANGE_MSG: u32 = 0x0219;
const SEND_IDLE: std::time::Duration = std::time::Duration::from_secs(15 * 60);

struct SendSess {
    server: nacre_platform::sendsrv::Server,
    qr: Option<CompositionDrawingSurface>,
    qr_px: f32,
    names: Vec<String>,
    prog: nacre_platform::sendsrv::Progress,
    last: Instant,
    done_at: Option<Instant>,
}

const WIN_W: f32 = 640.0;
const WIN_H: f32 = 392.0;
const SHADOW_PAD: f32 = 36.0;

static NOTCHS: std::sync::Mutex<Vec<isize>> = std::sync::Mutex::new(Vec::new());
static NOTCH_HWND: AtomicIsize = AtomicIsize::new(0);
static BUSY: AtomicBool = AtomicBool::new(false);

pub struct Options {
    pub glass: Option<Tier>,
    pub monitor: crate::display::Monitor,
    pub monitor_index: u8,
    pub mode: Vis,
    pub reserve: bool,
}

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

enum PillMsg {
    Push { kind: PillKind, key: String, title: String, sub: String, linger: Option<i64> },
    Dismiss(String),
}

static PILL_INBOX: std::sync::Mutex<Vec<PillMsg>> = std::sync::Mutex::new(Vec::new());

fn send_pill_msg(m: PillMsg) {
    PILL_INBOX.lock().unwrap_or_else(|e| e.into_inner()).push(m);
    let h = NOTCH_HWND.load(Ordering::Relaxed);
    if h != 0 {
        post(h, WM_PILLS);
    }
}

pub fn push_pill(kind: PillKind, key: &str, title: &str, sub: &str, linger: Option<i64>) {
    send_pill_msg(PillMsg::Push { kind, key: key.to_string(), title: title.to_string(), sub: sub.to_string(), linger });
}

pub fn dismiss_pill(key: &str) {
    send_pill_msg(PillMsg::Dismiss(key.to_string()));
}

pub fn begin_task_entry() -> bool {
    let h = NOTCH_HWND.load(Ordering::Relaxed);
    if h == 0 || !crate::prefs::get().notch_enabled {
        return false;
    }
    post(h, WM_TODO_EDIT);
    true
}

pub fn tasks_changed() {
    let h = NOTCH_HWND.load(Ordering::Relaxed);
    if h != 0 {
        post(h, WM_TASKS);
    }
}

fn on_windows_changed() {
    crate::presence::refresh();
    let handles = NOTCHS.lock().unwrap_or_else(|e| e.into_inner()).clone();
    for raw in handles { timer(HWND(raw as *mut _), T_WINCHG, 150); }
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

static ISLAND_GAP: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn island_gap(dpi: f32) -> f32 {
    let s = crate::prefs::get();
    if s.notch_island {
        (s.island_gap as f32 * dpi).round().max(1.0)
    } else {
        0.0
    }
}

fn notch_dpi(hwnd: HWND) -> f32 {
    unsafe { GetDpiForWindow(hwnd) }.max(96) as f32 / 96.0 * crate::prefs::ui_scale() * crate::prefs::get().notch_scale as f32 / 100.0
}

fn win_size(dpi: f32, screen_w: i32) -> (i32, i32) {
    (((WIN_W * dpi) as i32).min(screen_w), (WIN_H * dpi + island_gap(dpi)).ceil() as i32)
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
    screen_origin: (i32, i32),
    monitor_index: u8,
    monitor: crate::display::Monitor,

    mode: Mode,
    look: Look,
    size: Size,
    first: bool,
    tracking: bool,
    hover_act: Option<Action>,
    regions: Vec<Region>,

    timer: Timer,
    t0: Instant,
    last_sec: i64,
    battery: Option<(u8, bool)>,
    charging_fresh: bool,
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
    seek_drag: Option<(f32, f32, f32)>,

    album: String,
    lyr_worker: nacre_platform::lyrics::Lyrics,
    lyr: Option<nacre_core::notch::lyrics::Lyrics>,
    lyr_state: LyrState,
    lyr_lrclib: bool,
    lyr_key: String,
    lyr_track: String,
    lyr_idx: Option<usize>,

    shelf: nacre_core::notch::shelf::Shelf,
    shelf_icons: std::collections::HashMap<String, CompositionDrawingSurface>,
    dnd: drop::Shared,
    _drop_target: Option<windows::Win32::System::Ole::IDropTarget>,
    drop_zone: Option<usize>,
    pending: Vec<String>,
    menu: Option<Menu>,
    menu_item: Option<usize>,
    privacy: (bool, bool),
    dl_watch: Option<nacre_platform::dlwatch::Watch>,
    dl: Option<nacre_core::notch::download::Download>,
    dl_done: Option<nacre_core::notch::download::Download>,
    dl_icons: std::collections::HashMap<String, Option<CompositionDrawingSurface>>,
    cal_sel: i32,
    agenda: Option<nacre_platform::agenda::Agenda>,
    cal_events: Option<(views::Date, Option<Vec<nacre_platform::agenda::Event>>)>,
    send: Option<SendSess>,
    send_err: bool,
    media_app: String,
    app_icons: std::collections::HashMap<String, Option<CompositionDrawingSurface>>,
    vis: Vis,
    reserved: bool,
    island_shape: bool,
    off: bool,
    shown: bool,
    starting: bool,
    overlap: overlap::Overlap,
    grace: bool,
    edge_armed: bool,
    strip: Option<Strip>,
    tier: Tier,
    glass_override: Option<Tier>,
    pills: Pills,
    glow_level: [f32; 3],
    todo_edit: Option<String>,
    caret: bool,
    edit_prev: isize,
    surrogate: Option<u16>,
    todo_draft: String,
    _bt: Option<nacre_platform::devices::Watchers>,
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
                opts.monitor.rect.left, opts.monitor.rect.top, 400, 200,
                None, None, Some(hi), None,
            )?;
            let dpi = notch_dpi(hwnd);
            let screen_w = opts.monitor.width();
            let screen_origin = (opts.monitor.rect.left, opts.monitor.rect.top);
            let monitor_index = opts.monitor_index;
            let win = win_size(dpi, screen_w);
            let _ = SetWindowPos(hwnd, None, screen_origin.0 + (screen_w - win.0) / 2, screen_origin.1, win.0, win.1, SWP_NOACTIVATE | SWP_NOZORDER);
            let comp = host.compositor.clone();
            let surface = host.attach(hwnd, (win.0 as f32, win.1 as f32))?;
            let tier = auto_tier(opts.glass);
            let gfx = Gfx::shared(&comp)?;
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
                screen_origin,
                monitor_index,
                monitor: opts.monitor,
                mode: Mode::Rest,
                look: rest,
                size: state::size(rest, dpi),
                first: true,
                tracking: false,
                hover_act: None,
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
                island_shape: crate::prefs::get().notch_island,
                off: { let s = crate::prefs::get(); !s.notch_enabled || (s.notch_monitor != u8::MAX && s.notch_monitor != monitor_index) },
                shown: true,
                starting: true,
                overlap: overlap::Overlap::default(),
                grace: false,
                edge_armed: false,
                strip: None,
                tier,
                glass_override: opts.glass,
                pills: Pills::default(),
                glow_level: [0.0; 3],
                todo_edit: None,
                caret: false,
                edit_prev: 0,
                surrogate: None,
                todo_draft: String::new(),
                _bt: None,
            });
            n.refresh();
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(n) as isize);
            NOTCHS.lock().unwrap_or_else(|e| e.into_inner()).push(hwnd.0 as isize);
            if opts.monitor_index == crate::prefs::get().notch_monitor || (opts.monitor_index == 0 && crate::prefs::get().notch_monitor == u8::MAX) { NOTCH_HWND.store(hwnd.0 as isize, Ordering::Relaxed); }
            crate::services::register_island(hwnd);
            tracker::install(on_windows_changed);
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            timer(hwnd, T_START, 1600);
            timer(hwnd, T_MINUTE, nacre_platform::time::ms_to_next_minute());
            timer(hwnd, T_PRIVACY, 1500);
            timer(hwnd, T_TODO, 2500);
            with(hwnd, |d| {
                if monitor_index == 0 {
                    d._bt = Some(nacre_platform::devices::watch_bluetooth(|name, on| {
                    let sub = match (on, crate::prefs::get().lang) {
                        (true, nacre_core::i18n::Lang::Id) => "Tersambung",
                        (false, nacre_core::i18n::Lang::Id) => "Terputus",
                        (true, nacre_core::i18n::Lang::En) => "Connected",
                        (false, nacre_core::i18n::Lang::En) => "Disconnected",
                        (true, nacre_core::i18n::Lang::Es) => "Conectado",
                        (false, nacre_core::i18n::Lang::Es) => "Desconectado",
                    };
                    push_pill(PillKind::Device, &format!("bt:{name}"), &name, sub, Some(nacre_core::notch::pills::DEVICE_MS));
                    }));
                }
                d._drop_target = drop::register(hwnd, WM_DND, d.dnd.clone());
                if d._drop_target.is_none() {
                    log::line("notch", "seret-taruh tidak tersedia");
                }
                d.place_shadow_window();
                if d.vis != Vis::Always {
                    d.strip = Strip::new(hwnd, STRIP_ENTER, STRIP_LEAVE).ok().map(|s| s.on_file_drag(STRIP_DRAG));
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
            if std::env::var_os("NACRE_TEST_SEND").is_some() || std::env::var_os("NACRE_TEST_OPEN").is_some() {
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
            pill: self.pills.current(self.now_ms()).is_some(),
        }
    }

    fn start_send(&mut self, paths: Vec<String>) {
        if paths.is_empty() {
            return;
        }
        self.send = None;
        let lang = crate::prefs::get().lang;
        let hw = self.hwnd.0 as isize;
        match nacre_platform::sendsrv::start(&paths, lang, move || post(hw, WM_SEND)) {
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
        if !self.tracking && matches!(self.mode, Mode::Open(_)) {
            self.dispatch(Event::Leave);
        }
    }

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

    fn send_compact(&self) -> Option<(f32, bool)> {
        let s = self.send.as_ref()?;
        let done = s.done_at.is_some_and(|t| t.elapsed().as_millis() < nacre_core::notch::download::DONE_LINGER_MS as u128);
        (s.prog.active || done).then(|| (nacre_core::notch::send::fraction(s.prog.sent, s.prog.total), done && !s.prog.active))
    }

    fn today() -> views::Date {
        let t = nacre_platform::time::local_now();
        (t.year, t.month, t.day)
    }

    fn cal_date(&self) -> views::Date {
        let (y, m, d) = Self::today();
        nacre_core::island::calendar::add_days(y, m, d, self.cal_sel)
    }

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

    fn sync_download_watch(&mut self) {
        let on = crate::prefs::get().notch_downloads;
        if on && self.dl_watch.is_none() {
            let hw = self.hwnd.0 as isize;
            self.dl_watch = Some(nacre_platform::dlwatch::spawn(move || post(hw, WM_DOWNLOAD)));
        } else if !on && self.dl_watch.is_some() {
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

    fn dispatch(&mut self, ev: Event) {
        let open_on = if crate::prefs::get().notch_hover_open { OpenOn::Hover } else { OpenOn::Click };
        let next = state::step(self.mode, ev, open_on, self.tracking);
        if next != self.mode && next != Mode::Open(Tab::Todo) && self.todo_edit.is_some() {
            self.suspend_edit();
        }
        if next != self.mode {
            log::line("notch", &format!("{:?} --{ev:?}--> {next:?}", self.mode));
            self.mode = next;
            self.refresh();
            self.update_visibility();
        }
    }

    fn refresh(&mut self) {
        let mut look = state::look(self.mode, state::activity(&self.live()));
        if self.mode == Mode::Open(Tab::Todo) {
            look.rows = crate::tasks::with(|l| l.tasks.len()).min(state::TODO_MAX_ROWS as usize) as u8 + u8::from(self.todo_edit.is_some());
        }
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

    fn apply_size(&mut self, springy: Option<(f32, u32)>) {
        let (px0, pw, ph) = (self.x0(), self.size.w, self.size.h);
        let mut z = state::size(self.look, self.dpi);
        z.radius *= crate::prefs::round(crate::prefs::Role::Notch);
        z.w = z.w.round();
        z.h = z.h.round();
        let gap = island_gap(self.dpi);
        if gap > 0.0 {
            z.radius = (z.h / 2.0).min(36.0 * self.dpi);
        }
        ISLAND_GAP.store(gap.to_bits(), std::sync::atomic::Ordering::Relaxed);
        self.body.set_island(gap, self.shown);
        if let Some(sh) = self.shadow.as_mut() {
            sh.set_island(gap, self.shown);
        }
        self.size = z;
        let x0 = self.x0();
        self.body.set_geometry(self.win, x0, z.w, z.h, z.radius, springy);
        let (blur, dy, op) = state::shadow(self.look);
        let over = if gap > 0.0 { 0.0 } else { self.body.top_over };
        if let Some(sh) = self.shadow.as_mut() {
            sh.follow(&self.paint.gfx, (x0, -over, z.w, z.h + over), z.radius, (blur * self.dpi, dy * self.dpi, op), springy);
        }
        self.paint_glow();
        let e = self.body.ear;
        let (l, r) = (px0.min(x0) - e, (px0 + pw).max(x0 + z.w) + e);
        self.set_region((l.max(0.0), 0.0, r.min(self.win.0 as f32), gap + ph.max(z.h)));
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
        self.set_region(((x0 - e).max(0.0), 0.0, (x0 + self.size.w + e).min(self.win.0 as f32), island_gap(self.dpi) + self.size.h));
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
            pill: self.pills.current(now_ms).map(|p| views::PillView { kind: p.kind, title: p.title.clone(), sub: p.sub.clone() }),
            todo: if self.mode == Mode::Open(Tab::Todo) { Self::todo_rows() } else { Vec::new() },
            todo_done: if self.mode == Mode::Open(Tab::Todo) { crate::tasks::with(|l| l.tasks.iter().filter(|t| t.done).count()) } else { 0 },
            todo_total: if self.mode == Mode::Open(Tab::Todo) { crate::tasks::with(|l| l.tasks.len()) } else { 0 },
            todo_more: if self.mode == Mode::Open(Tab::Todo) { crate::tasks::with(|l| l.tasks.len()).saturating_sub(state::TODO_MAX_ROWS as usize) } else { 0 },
            todo_edit: self.todo_edit.clone().map(|t| (t, self.caret)),
        };
        views::draw(&mut self.paint, &c, self.look, f, &d, &mut self.regions, &mut self.live_vis);
        if let Some(r) = self.hover_act.and_then(|a| self.regions.iter().find(|r| r.action == a)) {
            if let Some(v) = &r.vis {
                let k = hover_scale(r.action);
                let _ = v.SetCenterPoint(Vector3 { X: r.x + r.w / 2.0, Y: r.y + r.h / 2.0, Z: 0.0 });
                let _ = v.SetScale(Vector3 { X: k, Y: k, Z: 1.0 });
            }
        }
        self.sync_visualizer();
        self.sync_progress();
    }

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

    fn sync_visualizer(&mut self) {
        let want = self.has_media && self.playing && self.shown && !self.overlap.fullscreen && !self.off;
        if want && self.loopback.is_none() {
            let hw = self.hwnd.0 as isize;
            self.loopback = Some(loopback::start(move || post(hw, WM_BANDS)));
        } else if !want && self.loopback.is_some() {
            self.loopback = None;
        }
        if !want {
            self.glow_level = [0.0; 3];
        }
        self.paint_glow();
    }

    fn on_bands(&mut self) {
        let b = loopback::latest();
        for (i, bar) in self.live_vis.bars.iter().enumerate() {
            let v = b.get(i).copied().unwrap_or(0.0).clamp(0.0, 1.0);
            let _ = bar.SetScale(Vector3 { X: 1.0, Y: 0.25 + 0.75 * v, Z: 1.0 });
        }
        let pick = |i: usize| b.get(i).copied().unwrap_or(0.0).clamp(0.0, 1.0);
        let target = [pick(0).max(pick(1)), pick(2), pick(3).max(pick(4))];
        for (cur, t) in self.glow_level.iter_mut().zip(target) {
            *cur = if t > *cur { 0.6 * t + 0.4 * *cur } else { 0.2 * t + 0.8 * *cur };
        }
        self.body.react(self.glow_level);
    }

    fn glow_on(&self) -> bool {
        crate::prefs::get().notch_glow && self.has_media && self.playing && self.shown && !self.off && !self.overlap.fullscreen && crate::prefs::get().anim
    }

    fn paint_glow(&mut self) {
        let rgb = self.glow_on().then(|| self.tint.unwrap_or_else(crate::prefs::accent));
        self.body.set_aurora(rgb);
    }

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
            match Menu::new(&self.paint.comp, self.hwnd, self.dpi, (self.screen_w, self.monitor.height()), Tier::Full) {
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
            m.set_bounds(self.screen_origin, (self.screen_w, self.monitor.height()));
            let _ = m.show(&self.paint.comp, &self.paint.gfx, items, self.screen_origin.0 + wx + x as i32, self.screen_origin.1 + y as i32);
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
            2 => nacre_platform::launch::show("explorer.exe", &format!("/select,\"{path}\"")),
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

    fn on_pills(&mut self) {
        let msgs: Vec<PillMsg> = std::mem::take(&mut *PILL_INBOX.lock().unwrap_or_else(|e| e.into_inner()));
        let s = crate::prefs::get();
        let now = self.now_ms();
        let mut changed = false;
        for m in msgs {
            match m {
                PillMsg::Push { kind, key, title, sub, linger } => {
                    let on = match kind {
                        PillKind::Device => s.pill_devices,
                        PillKind::Notice => s.pill_notices,
                        PillKind::Reminder => s.pill_reminders,
                        PillKind::Update => true,
                    };
                    if on {
                        self.pills.push(kind, &key, &title, &sub, now, linger);
                        changed = true;
                    }
                }
                PillMsg::Dismiss(k) => changed |= self.pills.dismiss_key(&k),
            }
        }
        if changed {
            self.sync_pills();
        }
    }

    fn sync_pills(&mut self) {
        let now = self.now_ms();
        self.pills.prune(now);
        match self.pills.next_expiry() {
            Some(t) => timer(self.hwnd, T_PILL, (t - now).clamp(30, 600_000) as u32),
            None => kill(self.hwnd, T_PILL),
        }
        self.update_visibility();
        self.refresh();
    }

    fn pill_click(&mut self) {
        let Some(p) = self.pills.current(self.now_ms()).cloned() else { return };
        self.pills.dismiss(p.id);
        match p.kind {
            PillKind::Reminder => {
                self.sync_pills();
                self.dispatch(Event::Click);
                self.dispatch(Event::SelectTab(Tab::Todo));
                return;
            }
            PillKind::Update => {
                if let Ok(comp) = self.body.stage.Compositor() {
                    crate::settings_win::open(&comp);
                }
            }
            PillKind::Device => {
                if let Some(path) = p.key.strip_prefix("drive:") {
                    nacre_platform::launch::open(path);
                }
            }
            PillKind::Notice => {}
        }
        self.sync_pills();
    }

    fn schedule_tasks(&mut self) {
        if self.hwnd.0 as isize != NOTCH_HWND.load(Ordering::Relaxed) {
            timer(self.hwnd, T_TODO, 2500);
            return;
        }
        let now_m = crate::tasks::now_minutes();
        let (fired, next) = crate::tasks::with(|l| {
            let ids = l.due_now(now_m);
            for id in &ids {
                l.mark_fired(*id);
            }
            (ids.iter().filter_map(|id| l.get(*id).cloned()).collect::<Vec<_>>(), l.next_due())
        });
        if !fired.is_empty() {
            crate::tasks::save();
            let lang = crate::tasks::language();
            let now = self.now_ms();
            let s = crate::prefs::get();
            let title = match lang { nacre_core::i18n::Lang::Id => "Pengingat", nacre_core::i18n::Lang::En => "Reminder", nacre_core::i18n::Lang::Es => "Recordatorio" };
            for t in &fired {
                log::line("tugas", &format!("pengingat #{} jatuh tempo", t.id));
                let label = t.due.map(|d| nacre_core::todo::due_label_lang(d, crate::tasks::now(), lang)).unwrap_or_default();
                if s.pill_reminders {
                    self.pills.push(PillKind::Reminder, &format!("todo:{}", t.id), &t.text, &label, now, None);
                }
                if self.off || !self.shown || self.overlap.fullscreen || !s.pill_reminders {
                    crate::mainwin::balloon(title, &t.text);
                }
            }
            self.sync_pills();
        }
        match next {
            Some(m) => timer(self.hwnd, T_TODO, crate::tasks::ms_to_minute(m)),
            None => kill(self.hwnd, T_TODO),
        }
    }

    fn todo_rows() -> Vec<views::TodoRow> {
        let lang = crate::tasks::language();
        let now = crate::tasks::now();
        let now_m = now.minutes();
        crate::tasks::with(|l| {
            l.ordered()
                .into_iter()
                .take(state::TODO_MAX_ROWS as usize)
                .map(|t| views::TodoRow {
                    id: t.id,
                    text: t.text.clone(),
                    done: t.done,
                    due: t.due.map(|d| nacre_core::todo::due_label_lang(d, now, lang)).unwrap_or_default(),
                    late: !t.done && t.due.is_some_and(|d| d <= now_m),
                })
                .collect()
        })
    }

    fn on_right_click(&mut self, x: f32, y: f32) {
        if self.look.view == View::Compact(state::Activity::Pill) && !matches!(self.mode, Mode::Open(_)) {
            if let Some(id) = self.pills.current(self.now_ms()).map(|p| p.id) {
                self.pills.dismiss(id);
                self.sync_pills();
            }
            return;
        }
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
                kill(self.hwnd, T_DWELL);
            } else {
                timer(self.hwnd, T_DWELL, motion::DWELL_MS);
            }
        }
    }

    fn on_hover(&mut self, x: f32, y: f32) {
        let hit = self.regions.iter().find(|r| r.vis.is_some() && r.hit(x, y)).map(|r| r.action);
        if hit == self.hover_act {
            return;
        }
        for (a, on) in [(self.hover_act, false), (hit, true)] {
            let Some(r) = a.and_then(|a| self.regions.iter().find(|r| r.action == a)) else { continue };
            let Some(v) = &r.vis else { continue };
            let to = if on { hover_scale(r.action) } else { 1.0 };
            let Ok(comp) = v.Compositor() else { continue };
            let _ = crate::motion::hover_pop(&comp, v, Vector3 { X: r.x + r.w / 2.0, Y: r.y + r.h / 2.0, Z: 0.0 }, to);
        }
        self.hover_act = hit;
    }

    fn on_leave(&mut self) {
        self.on_hover(-1.0, -1.0);
        self.tracking = false;
        kill(self.hwnd, T_DWELL);
        if self.seek_drag.is_some() || self.menu.as_ref().is_some_and(|m| m.is_open()) {
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

    fn on_media_button(&self, x: f32, y: f32) -> bool {
        self.look.view == View::Compact(state::Activity::Player) && self.regions.iter().any(|r| r.hit(x, y) && matches!(r.action, Action::PlayPause | Action::Next | Action::Prev))
    }

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
        if self.look.view == View::Compact(state::Activity::Pill) && !matches!(self.mode, Mode::Open(_)) {
            self.pill_click();
            return;
        }
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
            Action::TodoToggle(id) => {
                crate::tasks::with(|l| l.toggle(id));
                self.pills.dismiss_key(&format!("todo:{id}"));
                crate::tasks::changed();
                self.sync_pills();
            }
            Action::TodoClear => {
                crate::tasks::with(|l| l.clear_done());
                crate::tasks::changed();
            }
            Action::NextTab => {
                if let Mode::Open(cur) = self.mode {
                    let i = Tab::ALL.iter().position(|t| *t == cur).unwrap_or(0);
                    if self.todo_edit.is_some() {
                        self.suspend_edit();
                    }
                    self.dispatch(Event::SelectTab(Tab::ALL[(i + 1) % Tab::ALL.len()]));
                }
            }
            Action::TodoAdd | Action::TodoField => {
                if self.todo_edit.is_none() {
                    self.begin_edit();
                }
            }
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

    fn pill_screen_rect(&self) -> (i32, i32, i32, i32) {
        let base = state::size(state::look(Mode::Rest, None), self.dpi);
        let l = self.screen_origin.0 + ((self.screen_w as f32 - base.w) / 2.0) as i32;
        (l, 0, l + base.w as i32, (base.h + 4.0 * self.dpi) as i32)
    }

    fn place_strip(&self) {
        if let Some(s) = &self.strip {
            let (l, _, r, _) = self.pill_screen_rect();
            let pad = (60.0 * self.dpi) as i32;
            s.place(l - pad, self.screen_origin.1, r - l + 2 * pad, 2);
            s.set_visible(!self.overlap.fullscreen && !self.off);
        }
    }

    fn place_shadow_window(&self) {
        if let Some(sh) = &self.shadow {
            let x = self.screen_origin.0 + (self.screen_w - self.win.0) / 2 - sh.pad as i32;
            sh.place_window(self.hwnd, x, self.screen_origin.1, self.win.0 + 2 * sh.pad as i32, self.win.1 + sh.pad as i32);
            sh.set_visible(self.shown && !self.off);
        }
    }

    fn interacting(&self) -> bool {
        self.tracking || matches!(self.mode, Mode::Open(_) | Mode::Drop) || self.grace || self.todo_edit.is_some()
    }

    fn begin_edit(&mut self) {
        if self.mode != Mode::Open(Tab::Todo) {
            if !matches!(self.mode, Mode::Open(_)) {
                self.dispatch(Event::Click);
            }
            self.dispatch(Event::SelectTab(Tab::Todo));
        }
        kill(self.hwnd, T_LEAVE);
        self.todo_edit = Some(std::mem::take(&mut self.todo_draft));
        self.caret = true;
        self.surrogate = None;
        unsafe {
            let fg = GetForegroundWindow();
            self.edit_prev = if fg == self.hwnd { 0 } else { fg.0 as isize };
            let ex = GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, ex & !(WS_EX_NOACTIVATE.0 as isize));
            let _ = SetForegroundWindow(self.hwnd);
            let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(Some(self.hwnd));
        }
        timer(self.hwnd, T_CARET, 530);
        self.update_visibility();
        self.refresh();
    }

    fn suspend_edit(&mut self) {
        if let Some(t) = self.todo_edit.as_ref() {
            self.todo_draft = t.clone();
        }
        self.end_edit();
    }

    fn end_edit(&mut self) {
        if self.todo_edit.take().is_none() {
            return;
        }
        kill(self.hwnd, T_CARET);
        unsafe {
            let ex = GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, ex | WS_EX_NOACTIVATE.0 as isize);
            let prev = std::mem::take(&mut self.edit_prev);
            if GetForegroundWindow() == self.hwnd && prev != 0 && IsWindow(Some(HWND(prev as *mut _))).as_bool() {
                let _ = SetForegroundWindow(HWND(prev as *mut _));
            }
        }
        if !self.tracking && matches!(self.mode, Mode::Open(_)) {
            timer(self.hwnd, T_LEAVE, motion::LEAVE_GRACE_MS);
        }
        self.refresh();
    }

    fn on_char(&mut self, unit: u16) {
        let Some(text) = self.todo_edit.as_mut() else { return };
        match unit {
            8 => {
                text.pop();
            }
            13 => {
                let t = std::mem::take(text);
                if t.trim().is_empty() {
                    self.end_edit();
                    return;
                }
                if let Some(id) = crate::tasks::with(|l| l.add(&t, crate::tasks::now())) {
                    log::line("tugas", &format!("tugas #{id} ditambahkan dari Visor"));
                    crate::tasks::changed();
                }
            }
            27 => {
                self.end_edit();
                return;
            }
            0xD800..=0xDBFF => {
                self.surrogate = Some(unit);
                return;
            }
            0xDC00..=0xDFFF => {
                if let Some(hi) = self.surrogate.take() {
                    if let Ok(s) = String::from_utf16(&[hi, unit]) {
                        if text.chars().count() < 160 {
                            text.push_str(&s);
                        }
                    }
                }
            }
            u if u >= 32 => {
                if let Some(ch) = char::from_u32(u as u32) {
                    if text.chars().count() < 160 {
                        text.push(ch);
                    }
                }
            }
            _ => return,
        }
        self.caret = true;
        timer(self.hwnd, T_CARET, 530);
        self.render();
    }

    fn update_visibility(&mut self) {
        let inputs = Inputs {
            overlapped: self.overlap.overlapped,
            interacting: self.interacting(),
            edge_armed: self.edge_armed,
            starting: self.starting,
            fullscreen: self.overlap.fullscreen,
            revealing: !self.overlap.fullscreen && self.pills.current(self.now_ms()).is_some(),
        };
        let hide = self.off
            || should_hide(self.vis, &inputs)
            || (self.vis == Vis::AutoHide
                && crate::presence::has_visible_app_on(windows::Win32::Foundation::RECT {
                    left: self.screen_origin.0,
                    top: self.screen_origin.1,
                    right: self.screen_origin.0 + self.screen_w,
                    bottom: self.screen_origin.1 + self.monitor.height(),
                }));
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
            crate::topmost::request_raise();
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
            crate::services::set_notch(self.monitor_index, 0, 0, 0);
            return;
        }
        let (l, _, r, h) = self.pill_screen_rect();
        crate::services::set_notch(self.monitor_index, l, r, h - (4.0 * self.dpi) as i32);
    }

    fn set_reserved(&mut self, on: bool) {
        self.reserved = on;
        if on {
            let strip = (34.0 * self.dpi).ceil() as i32;
            let r = nacre_platform::shell::appbar::reserve_top(self.hwnd, windows::Win32::Foundation::RECT { left: self.screen_origin.0, top: self.screen_origin.1, right: self.screen_origin.0 + self.screen_w, bottom: self.screen_origin.1 + strip }, strip);
            log::line("appbar", &format!("atas strip={strip} hasil={r:?}"));
        } else {
            nacre_platform::shell::appbar::release_top(Some(self.hwnd));
        }
    }

    fn apply_prefs(&mut self) {
        let s = crate::prefs::get();
        self.off = !s.notch_enabled || (s.notch_monitor != u8::MAX && s.notch_monitor != self.monitor_index);
        let selected_here = s.notch_monitor == u8::MAX || s.notch_monitor == self.monitor_index;
        if s.notch_monitor == self.monitor_index || (s.notch_monitor == u8::MAX && self.monitor_index == 0) { NOTCH_HWND.store(self.hwnd.0 as isize, Ordering::Relaxed); }
        let (vis, reserve) = (s.notch.mode(), selected_here && s.notch.reserves() && s.notch_enabled && !s.menubar);
        if vis != self.vis {
            self.vis = vis;
            if vis == Vis::Always {
                self.strip = None;
                self.edge_armed = false;
            } else if self.strip.is_none() {
                self.strip = Strip::new(self.hwnd, STRIP_ENTER, STRIP_LEAVE).ok().map(|s| s.on_file_drag(STRIP_DRAG));
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
        self.first = true;
        self.on_reconfigure();
        self.refresh();
        if s.notch_island != self.island_shape {
            self.island_shape = s.notch_island;
            self.morph(s.notch_island);
        }
        log::line("notch", &format!("pengaturan: perilaku={vis:?} cadangan={reserve} mati={}", self.off));
    }

    fn morph(&mut self, to_island: bool) {
        if !crate::prefs::get().anim || !self.shown {
            return;
        }
        let Ok(comp) = self.body.stage.Compositor() else { return };
        let (x0, w, h, gap) = (self.x0(), self.size.w, self.size.h, island_gap(self.dpi));
        let cx = x0 + w / 2.0;
        let center = windows_numerics::Vector3 { X: cx, Y: if to_island { gap + h / 2.0 } else { 0.0 }, Z: 0.0 };
        let _ = crate::motion::droplet(&comp, &self.body.stage, center, to_island);
        let rgb = if crate::prefs::get().notch_material == nacre_core::settings::Material::Black { [body::ONYX.0, body::ONYX.1, body::ONYX.2] } else { [235, 240, 255] };
        let land = if to_island { gap } else { (7.0 * self.dpi).round() };
        let _ = crate::motion::drip(&comp, &self.body.ghost, cx, land, (11.0 * self.dpi).round(), rgb, to_island);
        timer(self.hwnd, T_GHOST, crate::motion::MORPH_MS + 80);
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
        if let Some(monitor) = crate::display::monitors().get(self.monitor_index as usize) {
            self.monitor = monitor.clone();
        }
        let screen_w = self.monitor.width();
        let screen_origin = (self.monitor.rect.left, self.monitor.rect.top);
        let dpi = notch_dpi(self.hwnd);
        let win = win_size(dpi, screen_w);
        if win == self.win && screen_w == self.screen_w && screen_origin == self.screen_origin && (dpi - self.dpi).abs() < 0.01 {
            return;
        }
        log::line("notch", &format!("konfigurasi ulang layar_w={screen_w} dpi={dpi}"));
        let dpi_changed = (dpi - self.dpi).abs() >= 0.01;
        self.screen_w = screen_w;
        self.screen_origin = screen_origin;
        self.dpi = dpi;
        self.win = win;
        unsafe {
            let _ = SetWindowPos(self.hwnd, None, screen_origin.0 + (screen_w - win.0) / 2, screen_origin.1, win.0, win.1, SWP_NOACTIVATE | SWP_NOZORDER);
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
            T_CARET => {
                if self.todo_edit.is_some() {
                    self.caret = !self.caret;
                    timer(self.hwnd, T_CARET, 530);
                    if self.mode == Mode::Open(Tab::Todo) {
                        self.render();
                    }
                }
            }
            T_LEAVE => {
                if !self.tracking && !self.send_pinned() && self.todo_edit.is_none() {
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
                let mut ov = overlap::evaluate_in(tracker::external_foreground(), std::process::id(), self.pill_screen_rect(), windows::Win32::Foundation::RECT { left: self.screen_origin.0, top: self.screen_origin.1, right: self.screen_origin.0 + self.screen_w, bottom: self.screen_origin.1 + self.monitor.height() });
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
            T_PILL => self.sync_pills(),
            T_TODO => self.schedule_tasks(),
            T_TESTSEND => {
                kill(self.hwnd, T_TESTSEND);
                if let Ok(p) = std::env::var("NACRE_TEST_SEND") {
                    self.start_send(p.split(';').map(|s| s.to_string()).collect());
                }
                if let Ok(t) = std::env::var("NACRE_TEST_OPEN") {
                    let tab = match t.as_str() {
                        "shelf" => Tab::Shelf,
                        "timer" => Tab::Timer,
                        "todo" => Tab::Todo,
                        _ => Tab::Play,
                    };
                    self.dispatch(Event::Click);
                    self.dispatch(Event::SelectTab(tab));
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
                if sys::battery() != self.battery {
                    self.on_power();
                } else if matches!(self.look.view, View::Compact(state::Activity::Glance) | View::Open(_)) && self.todo_edit.is_none() {
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
    let gap = f32::from_bits(ISLAND_GAP.load(std::sync::atomic::Ordering::Relaxed));
    ((lp.0 as i16) as f32, ((lp.0 >> 16) as i16) as f32 - gap)
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => {
            if with(hwnd, |d| d.todo_edit.is_some()).unwrap_or(false) {
                LRESULT(MA_ACTIVATE as isize)
            } else {
                LRESULT(MA_NOACTIVATE as isize)
            }
        }
        WM_CHAR => {
            with(hwnd, |d| d.on_char(wp.0 as u16));
            LRESULT(0)
        }
        WM_KEYDOWN if wp.0 as u16 == 0x1B => {
            with(hwnd, |d| d.end_edit());
            LRESULT(0)
        }
        WM_ACTIVATE => {
            if (wp.0 & 0xFFFF) == 0 {
                with(hwnd, |d| {
                    if d.todo_edit.is_some() {
                        d.suspend_edit();
                    }
                });
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_TODO_EDIT => {
            with(hwnd, |d| {
                if d.todo_edit.is_none() && !d.off {
                    d.begin_edit();
                }
            });
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let (x, y) = xy(lp);
            if with(hwnd, |d| d.on_down(x, y)).unwrap_or(false) {
                let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetCapture(hwnd);
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let (x, y) = xy(lp);
            with(hwnd, |d| {
                d.on_drag(x);
                d.on_move();
                d.on_hover(x, y);
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
        WM_DEVICECHANGE_MSG => {
            if hwnd.0 as isize != NOTCH_HWND.load(Ordering::Relaxed) {
                return DefWindowProcW(hwnd, msg, wp, lp);
            }
            let ev = wp.0 as u32;
            if ev == 0x8000 || ev == 0x8004 {
                let lang = crate::prefs::get().lang;
                for l in nacre_platform::devices::volume_letters(lp.0) {
                    let label = nacre_platform::devices::drive_label(l);
                    let title = if label.is_empty() { match lang { nacre_core::i18n::Lang::Id => "Flashdisk", nacre_core::i18n::Lang::En => "USB drive", nacre_core::i18n::Lang::Es => "Unidad USB" }.to_string() } else { label };
                    let state = match (ev == 0x8000, lang) {
                        (true, nacre_core::i18n::Lang::Id) => "tersambung",
                        (false, nacre_core::i18n::Lang::Id) => "dilepas",
                        (true, nacre_core::i18n::Lang::En) => "connected",
                        (false, nacre_core::i18n::Lang::En) => "removed",
                        (true, nacre_core::i18n::Lang::Es) => "conectada",
                        (false, nacre_core::i18n::Lang::Es) => "retirada",
                    };
                    push_pill(PillKind::Device, &format!("drive:{l}:\\"), &title, &format!("({l}:) {state}"), Some(nacre_core::notch::pills::DEVICE_MS));
                }
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_PILLS => {
            with(hwnd, |d| d.on_pills());
            LRESULT(0)
        }
        WM_TASKS => {
            with(hwnd, |d| {
                d.schedule_tasks();
                if d.mode == Mode::Open(Tab::Todo) {
                    d.refresh();
                }
            });
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
        STRIP_DRAG => {
            with(hwnd, |d| {
                if d.vis != Vis::Always && !d.off {
                    kill(hwnd, T_EDGE);
                    d.grace = true;
                    timer(hwnd, T_GRACE, 3000);
                    log::line("notch", "file diseret ke tepi atas: Visor dimunculkan");
                    d.update_visibility();
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
            with(hwnd, |d| {
                d.apply_prefs();
                d.on_reconfigure();
            });
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
            crate::services::unregister_island(hwnd);
            let mut handles = NOTCHS.lock().unwrap_or_else(|e| e.into_inner());
            handles.retain(|h| *h != hwnd.0 as isize);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

fn hover_scale(a: Action) -> f32 {
    if matches!(a, Action::ShelfItem(_) | Action::TodoToggle(_)) {
        crate::motion::HOVER_ROW
    } else {
        crate::motion::HOVER_BUTTON
    }
}
