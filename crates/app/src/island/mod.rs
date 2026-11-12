//! Kontroler island (notch): pil di tengah atas layar yang melebar menjadi panel Now Playing atau Kalender+Timer.
//! Satu thread (UI). Keputusan ukuran/panel/kalender/timer/FFT ada di `nacre-core`.

mod ears;

use nacre_core::symbols::Sym;
use nacre_core::island::bands::BANDS;
use nacre_core::island::calendar;
use nacre_core::island::layout::{self, Panel};
use nacre_core::island::timer::{format_mmss, Phase, Timer};
use nacre_core::visibility::policy::{should_hide, Inputs, Mode};
use crate::strip::Strip;
use nacre_platform::icons::Bitmap;
use nacre_platform::loopback::{self, Loopback};
use nacre_platform::media::{self, ArtUpdate, Command, Media};
use nacre_platform::text::{self, Style, ICON_FACE};
use nacre_platform::wins::{overlap, tracker};
use nacre_platform::{log, sys, time};
use nacre_ui::gfx::Gfx;
use nacre_ui::glass::{GlassPanel, GlassParams, Tier};
use nacre_ui::host::{Host, Surface};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::time::Instant;
use windows::core::w;
use windows::UI::Color;
use windows::UI::Composition::{
    CompositionDrawingSurface, CompositionObject, CompositionStretch, ContainerVisual, Compositor, SpriteVisual,
};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{CreateRectRgn, SetWindowRgn};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows_core::{Interface, HSTRING};
use windows_numerics::{Vector2, Vector3};

const WM_MEDIA: u32 = WM_APP + 10;
const WM_BANDS: u32 = WM_APP + 11;
const WM_MOUSELEAVE: u32 = 0x02A3;

const T_CLOSE: usize = 1;
const T_CLOCK: usize = 2;
const T_TICK: usize = 3;
const T_REGION: usize = 4;
const T_WINCHG: usize = 5;
const T_RECONF: usize = 6;
const T_DWELL: usize = 7;
const T_GRACE: usize = 8;
const T_START: usize = 9;
const T_HIDEWIN: usize = 10;
const T_GHOST: usize = 11;
const T_HOVEROPEN: usize = 12;
const STRIP_ENTER_I: u32 = WM_APP + 14;
const STRIP_LEAVE_I: u32 = WM_APP + 15;
const WM_DISPLAYCHANGE: u32 = 0x007E;
const WM_DPICHANGED: u32 = 0x02E0;

static ISLAND_HWND: AtomicIsize = AtomicIsize::new(0);
static BUSY: AtomicBool = AtomicBool::new(false);

pub struct Options {
    pub glass: Option<Tier>,
    pub mode: Mode,
    /// Cadangkan strip di atas layar (notch tetap).
    pub reserve: bool,
}

#[derive(Clone, Copy, Debug)]
enum Action {
    PlayPause,
    Next,
    Prev,
    /// Klik pada lintasan progres: x awal dan lebar lintasan (px klien).
    Seek(f32, f32),
    CalPrev,
    CalNext,
    TimerToggle,
    TimerReset,
    TimerPlus,
    TimerMinus,
    /// Slider kontrol: 0 volume, 1 kecerahan; awal x dan lebar lintasan.
    Slider(u8, f32, f32),
    Mute,
    Radio(nacre_platform::radio::Kind),
    /// Pindah ke panel lain (pager di dasar panel).
    Goto(Panel),
    /// Buka Pengaturan ABRA.
    OpenSettings,
}

#[derive(Clone, Copy)]
enum Anchor {
    Left,
    Center,
    Right,
}

struct Region {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    action: Action,
}

pub struct Island {
    hwnd: HWND,
    comp: Compositor,
    gfx: Gfx,
    _surface: Surface,
    panel: GlassPanel,
    ear_l: SpriteVisual,
    ear_r: SpriteVisual,
    /// Lapisan tint di atas telinga (untuk material kaca/transparan) dan bentuk topengnya.
    ear_tint: [SpriteVisual; 2],
    ear_mask: [CompositionDrawingSurface; 2],
    content: ContainerVisual,
    /// Isi lama yang memudar saat panel berganti (crossfade).
    ghost: ContainerVisual,
    /// Klip isi: mengikuti geometri kaca dengan spring yang sama, sehingga isi tak bisa menyembul keluar badan.
    clip: windows::UI::Composition::InsetClip,
    bars_root: ContainerVisual,
    bars: Vec<SpriteVisual>,

    dpi: f32,
    win: (i32, i32),
    screen_w: i32,
    ear: f32,
    top_over: f32,
    state: layout::State,
    size: layout::Size,
    first_apply: bool,

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
    loopback: Option<Loopback>,

    timer: Timer,
    t0: Instant,
    cal: (i32, u32),
    monday_first: bool,

    regions: Vec<Region>,
    text_cache: HashMap<String, (CompositionDrawingSurface, i32, i32)>,
    tracking: bool,
    fullscreen: bool,
    mode: Mode,
    reserved: bool,
    off: bool,
    stage: ContainerVisual,
    shown: bool,
    starting: bool,
    overlap: overlap::Overlap,
    grace: bool,
    edge_armed: bool,
    strip: Option<Strip>,
    tier: Tier,
    glass_override: Option<Tier>,
    /// Diagnostik (env NACRE_VIS=1): paksa visualizer hidup dan catat nilai pita.
    diag_bands: bool,
    band_msgs: u32,
    dragging: Option<(u8, f32, f32)>,
    radios: Option<nacre_platform::radio::Radios>,
    radio_state: nacre_platform::radio::Snapshot,
}

fn with_island<R>(hwnd: HWND, f: impl FnOnce(&mut Island) -> R) -> Option<R> {
    let p = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut Island;
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

fn on_windows_changed() {
    let h = ISLAND_HWND.load(Ordering::Relaxed);
    if h != 0 {
        timer(HWND(h as *mut _), T_WINCHG, 150);
    }
}

/// Animasikan satu properti skalar objek Composition dengan spring (jalan di compositor).
fn spring_to(comp: &Compositor, obj: &CompositionObject, prop: &str, v: f32, ratio: f32, period_ms: u32) {
    let go = || -> windows_core::Result<()> {
        let a = comp.CreateSpringScalarAnimation()?;
        a.SetDampingRatio(ratio)?;
        a.SetPeriod(windows::Foundation::TimeSpan { Duration: period_ms as i64 * 10_000 })?;
        a.SetFinalValue(&windows::Foundation::PropertyValue::CreateSingle(v)?.cast::<windows::Foundation::IReference<f32>>()?)?;
        obj.StartAnimation(&HSTRING::from(prop), &a)
    };
    let _ = go();
}

const ONYX: (u8, u8, u8) = (10, 10, 12);
const ONYX_ALPHA: u8 = 234;

/// Ukuran target dengan radius sudut mengikuti pengaturan kebulatan notch.
fn target_size(s: &layout::State, dpi: f32) -> layout::Size {
    let mut z = layout::target(s, dpi);
    z.radius *= crate::prefs::round(crate::prefs::Role::Notch);
    z.w = z.w.round();
    z.h = z.h.round();
    z
}

/// Material notch Hitam buram: badan pekat seperti notch MacBook; selain itu Liquid Glass seperti dock.
fn black_material() -> bool {
    crate::prefs::get().notch_material == nacre_core::settings::Material::Black
}

fn island_glass(radius: f32) -> GlassParams {
    let look = crate::prefs::look(crate::prefs::Role::Notch);
    let mut g = crate::prefs::apply(GlassParams::dark((10.0, 10.0), radius), &look);
    if black_material() {
        // Notch hitam: badan 100% pekat, tanpa garis tepi/kilau, menyatu dengan telinga dan layar.
        g.tint = Color { A: 255, R: ONYX.0, G: ONYX.1, B: ONYX.2 };
        g.solid = Color { A: 240, R: ONYX.0, G: ONYX.1, B: ONYX.2 };
        g.rim = Color { A: 0, R: 255, G: 255, B: 255 };
        g.highlight = 0.0;
    } else {
        // Liquid Glass: sama dengan dock (blur, kilau tepi atas, tepi yang menangkap cahaya), varian gelap agar teks
        // putih tetap terbaca; tint diberi alas minimum supaya kontras tidak hilang di atas wallpaper terang.
        g.tint.A = g.tint.A.max(84);
        // Satu kesatuan dengan telinga: tanpa garis tepi dan tanpa kilau (keduanya hanya ada di badan, bukan telinga).
        g.rim = Color { A: 0, R: 255, G: 255, B: 255 };
        g.highlight = 0.0;
    }
    let _ = ONYX_ALPHA;
    g
}

impl Island {
    pub fn create(host: &Host, opts: Options) -> windows_core::Result<HWND> {
        unsafe {
            let hi: HINSTANCE = GetModuleHandleW(None)?.into();
            let class = w!("NacreIsland");
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
                w!("nacre-island"),
                WS_POPUP,
                0, 0, 400, 200,
                None, None, Some(hi), None,
            )?;
            let dpi = GetDpiForWindow(hwnd).max(96) as f32 / 96.0;
            let screen_w = sys::screen_size().0;
            let win = (((470.0 * dpi) as i32).min(screen_w), (390.0 * dpi) as i32);
            let _ = SetWindowPos(hwnd, None, (screen_w - win.0) / 2, 0, win.0, win.1, SWP_NOACTIVATE | SWP_NOZORDER);

            let surface = host.attach(hwnd, (win.0 as f32, win.1 as f32))?;
            let state = layout::State { open: None, hover: false, media: false, calendar_rows: 5 };
            let size = target_size(&state, dpi);
            let top_over = 32.0 * dpi;
            let stage = host.compositor.CreateContainerVisual()?;
            surface.root.Children()?.InsertAtTop(&stage)?;
            let mut panel = GlassPanel::new(&host.compositor, &stage, island_glass(size.radius))?;
            let tier = opts.glass.unwrap_or_else(|| {
                let c = sys::conditions();
                if !c.transparency || c.battery_saver || c.remote_session { Tier::Solid } else { Tier::Full }
            });
            panel.set_tier(tier)?;

            let gfx = Gfx::new(&host.compositor)?;
            let e = (13.0 * dpi).ceil() as u32;
            let make_ear = |left: bool| -> windows_core::Result<(SpriteVisual, SpriteVisual, CompositionDrawingSurface)> {
                let b = ears::bitmap(e, ONYX, 255, left);
                let s = gfx.surface_from_bgra(b.w, b.h, &b.bgra)?;
                let v = host.compositor.CreateSpriteVisual()?;
                v.SetSize(Vector2 { X: e as f32, Y: e as f32 })?;
                v.SetBrush(&host.compositor.CreateSurfaceBrushWithSurface(&s)?)?;
                let tint = host.compositor.CreateSpriteVisual()?;
                tint.SetSize(Vector2 { X: e as f32, Y: e as f32 })?;
                v.Children()?.InsertAtTop(&tint)?;
                stage.Children()?.InsertAtTop(&v)?;
                Ok((v, tint, s))
            };
            let ((ear_l, tint_l, mask_l), (ear_r, tint_r, mask_r)) = (make_ear(true)?, make_ear(false)?);

            let content = host.compositor.CreateContainerVisual()?;
            stage.Children()?.InsertAtTop(&content)?;
            let ghost = host.compositor.CreateContainerVisual()?;
            stage.Children()?.InsertAtTop(&ghost)?;
            let clip = host.compositor.CreateInsetClip()?;
            let full = Vector2 { X: win.0 as f32, Y: win.1 as f32 };
            content.SetSize(full)?;
            ghost.SetSize(full)?;
            content.SetClip(&clip)?;
            ghost.SetClip(&clip)?;
            let bars_root = host.compositor.CreateContainerVisual()?;
            stage.Children()?.InsertAtTop(&bars_root)?;
            let mut bars = Vec::new();
            let (bw, bh, gap) = (3.0 * dpi, 16.0 * dpi, 3.0 * dpi);
            for i in 0..BANDS {
                let b = host.compositor.CreateSpriteVisual()?;
                b.SetSize(Vector2 { X: bw, Y: bh })?;
                b.SetOffset(Vector3 { X: i as f32 * (bw + gap), Y: 0.0, Z: 0.0 })?;
                b.SetCenterPoint(Vector3 { X: bw / 2.0, Y: bh / 2.0, Z: 0.0 })?;
                b.SetBrush(&host.compositor.CreateColorBrushWithColor(Color { A: 235, R: 255, G: 255, B: 255 })?)?;
                let clip = nacre_ui_clip(&host.compositor, bw, bh, bw / 2.0)?;
                b.SetClip(&clip)?;
                b.SetScale(Vector3 { X: 1.0, Y: 0.2, Z: 1.0 })?;
                bars_root.Children()?.InsertAtTop(&b)?;
                bars.push(b);
            }

            let hw = hwnd.0 as isize;
            let media = if std::env::var_os("NACRE_FAKE_MEDIA").is_some() {
                nacre_platform::media_fake::spawn(move || post(hw, WM_MEDIA))
            } else {
                media::spawn(move || post(hw, WM_MEDIA))
            };
            let now = time::local_now();
            let mut isl = Box::new(Island {
                hwnd,
                comp: host.compositor.clone(),
                gfx,
                _surface: surface,
                panel,
                ear_l,
                ear_r,
                ear_tint: [tint_l, tint_r],
                ear_mask: [mask_l, mask_r],
                content,
                ghost,
                clip,
                bars_root,
                bars,
                dpi,
                win,
                screen_w,
                ear: e as f32,
                top_over,
                state,
                size,
                first_apply: true,
                media,
                has_media: false,
                title: String::new(),
                artist: String::new(),
                playing: false,
                pos_ms: 0,
                dur_ms: 0,
                pos_at: Instant::now(),
                can_seek: false,
                art: None,
                loopback: None,
                timer: Timer::new(
                    std::env::var("NACRE_TIMER_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(25 * 60),
                ),
                t0: Instant::now(),
                cal: (now.year, now.month),
                monday_first: true,
                regions: vec![],
                text_cache: HashMap::new(),
                tracking: false,
                fullscreen: false,
                mode: opts.mode,
                reserved: false,
                off: !crate::prefs::get().notch_enabled,
                stage,
                shown: true,
                starting: true,
                overlap: overlap::Overlap::default(),
                grace: false,
                edge_armed: false,
                strip: None,
                tier,
                glass_override: opts.glass,
                diag_bands: std::env::var_os("NACRE_VIS").is_some(),
                band_msgs: 0,
                dragging: None,
                radios: None,
                radio_state: Default::default(),
            });
            isl.apply_target();
            isl.render();
            isl.rebuild_ears();
            isl.sync_visualizer();
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(isl) as isize);
            ISLAND_HWND.store(hwnd.0 as isize, Ordering::Relaxed);
            crate::services::register_island(hwnd);
            tracker::install(on_windows_changed);
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            timer(hwnd, T_CLOCK, time::ms_to_next_minute());
            timer(hwnd, T_START, 1600);
            with_island(hwnd, |d| {
                if d.mode != Mode::Always {
                    d.strip = Strip::new(hwnd, STRIP_ENTER_I, STRIP_LEAVE_I).ok();
                    d.place_strip();
                }
                if opts.reserve && !d.off && !crate::prefs::get().menubar {
                    d.set_reserved(true);
                }
                d.publish_notch();
            });
            crate::prefs::listen(hwnd);
            {
                let mut r = windows::Win32::Foundation::RECT::default();
                let _ = GetWindowRect(hwnd, &mut r);
                log::line("island", &format!("dibuat tier={tier:?} dpi={dpi} jendela=({},{})-({},{}) layar_w={screen_w}", r.left, r.top, r.right, r.bottom));
            }
            Ok(hwnd)
        }
    }

    // ---------- geometri ----------

    fn x0(&self) -> f32 {
        // Bulat ke piksel utuh: tepi badan dan telinga bertemu tepat di batas piksel (tanpa garis rambut di sambungan).
        ((self.win.0 as f32 - self.size.w) / 2.0).floor()
    }

    fn set_region(&self, rect: (f32, f32, f32, f32)) {
        unsafe {
            let rgn = CreateRectRgn(rect.0 as i32, rect.1 as i32, rect.2.ceil() as i32, rect.3.ceil() as i32);
            SetWindowRgn(self.hwnd, Some(rgn), true);
        }
    }

    /// Hitung ukuran target dan animasikan panel + telinga ke sana. Region diperluas ke gabungan awal+target
    /// selama animasi, lalu dipotong ke target (agar tidak menelan klik di sekitar pil).
    fn apply_target(&mut self) {
        let prev = (self.x0(), self.size.w, self.size.h);
        self.size = target_size(&self.state, self.dpi);
        // Membesar: sedikit membal seperti cairan; mengecil: cepat dan tenang (tanpa goyang badan kosong).
        let grow = self.size.w * self.size.h >= prev.1 * prev.2;
        let (ratio, period) = if grow { (0.76, 105) } else { (0.95, 80) };
        let (x0, e, r) = (self.x0(), self.ear, self.top_over);
        if self.first_apply {
            self.first_apply = false;
            let _ = self.panel.set_rect(x0, -r, self.size.w, self.size.h + r);
            let _ = self.panel.set_radius(self.size.radius);
            self.set_clip(x0, false, ratio, period);
            let _ = self.ear_l.SetOffset(Vector3 { X: x0 - e, Y: 0.0, Z: 0.0 });
            let _ = self.ear_r.SetOffset(Vector3 { X: x0 + self.size.w, Y: 0.0, Z: 0.0 });
        } else {
            let _ = self.panel.animate_rect(x0, -r, self.size.w, self.size.h + r, self.size.radius, ratio, period);
            self.set_clip(x0, true, ratio, period);
            let l: CompositionObject = self.ear_l.cast().unwrap();
            let rr: CompositionObject = self.ear_r.cast().unwrap();
            spring_to(&self.comp, &l, "Offset.X", x0 - e, ratio, period);
            spring_to(&self.comp, &rr, "Offset.X", x0 + self.size.w, ratio, period);
        }
        let (px0, pw, ph) = prev;
        let left = px0.min(x0) - e;
        let right = (px0 + pw).max(x0 + self.size.w) + e;
        self.set_region((left.max(0.0), 0.0, right.min(self.win.0 as f32), ph.max(self.size.h)));
        timer(self.hwnd, T_REGION, 700);
    }

    /// Telinga notch mengikuti material notch agar siluetnya sama pada semua material: hitam pekat untuk
    /// Hitam; untuk Kaca/Transparan telinga memakai blur backdrop + tint yang sama, dipotong oleh bentuk telinga.
    fn rebuild_ears(&self) {
        let go = || -> windows_core::Result<()> {
            let g = island_glass(self.size.radius);
            let black = black_material();
            for (i, ear) in [&self.ear_l, &self.ear_r].into_iter().enumerate() {
                let mask = self.comp.CreateSurfaceBrushWithSurface(&self.ear_mask[i])?;
                let tint = &self.ear_tint[i];
                if black {
                    ear.SetBrush(&mask)?;
                    tint.SetOpacity(0.0)?;
                    continue;
                }
                let base: windows::UI::Composition::CompositionBrush = if self.tier == Tier::Solid {
                    self.comp.CreateColorBrushWithColor(g.solid)?.cast()?
                } else {
                    nacre_ui::glass::backdrop_blur_brush(&self.comp, g.blur_sigma, g.saturation)?.cast()?
                };
                let mb = self.comp.CreateMaskBrush()?;
                mb.SetSource(&base)?;
                mb.SetMask(&mask)?;
                ear.SetBrush(&mb)?;
                let mask2 = self.comp.CreateSurfaceBrushWithSurface(&self.ear_mask[i])?;
                let tb = self.comp.CreateMaskBrush()?;
                tb.SetSource(&self.comp.CreateColorBrushWithColor(g.tint)?)?;
                tb.SetMask(&mask2)?;
                tint.SetBrush(&tb)?;
                tint.SetOpacity(1.0)?;
            }
            Ok(())
        };
        if let Err(e) = go() {
            log::line("island", &format!("telinga gagal: {e}"));
        }
    }

    /// Baris pager di dasar panel terbuka: ikon Musik/Kalender/Kontrol (klik untuk pindah) + tombol Pengaturan ABRA.
    fn render_pager(&mut self, c: &ContainerVisual, x0: f32, w: f32, h: f32) {
        let d = self.dpi;
        let cy = h - (layout::PAGER_H * d) / 2.0 - 2.0 * d;
        let panels = layout::panels(self.has_media);
        let (bw, bh, gap) = (44.0 * d, 24.0 * d, 6.0 * d);
        let total = panels.len() as f32 * bw + (panels.len() as f32 - 1.0) * gap;
        let mut x = x0 + w / 2.0 - total / 2.0;
        for p in panels {
            let glyph = match p {
                Panel::NowPlaying => Sym::Note,
                Panel::Calendar => Sym::Calendar,
                Panel::Controls => Sym::Control,
            };
            let active = self.state.open == Some(p);
            if active {
                self.rect(c, x, cy - bh / 2.0, bw, bh, bh / 2.0, Color { A: 52, R: 255, G: 255, B: 255 });
            }
            self.sym(c, glyph, x + bw / 2.0, cy, 18.0, if active { 1.0 } else { 0.55 }, [255, 255, 255]);
            self.region(x, cy - bh / 2.0 - 3.0 * d, bw, bh + 6.0 * d, Action::Goto(p));
            x += bw + gap;
        }
        // pengaturan ABRA di kanan
        let gx = x0 + w - 26.0 * d;
        self.sym(c, Sym::Gear, gx, cy, 18.0, 0.6, [255, 255, 255]);
        self.region(gx - 16.0 * d, cy - 14.0 * d, 32.0 * d, 28.0 * d, Action::OpenSettings);
    }

    /// Klip isi = persegi kaca yang terlihat. `animate`: spring identik dengan kaca.
    fn set_clip(&self, x0: f32, animate: bool, ratio: f32, period: u32) {
        let (l, t, r, b) = (x0, 0.0, self.win.0 as f32 - (x0 + self.size.w), self.win.1 as f32 - self.size.h);
        let c = &self.clip;
        if !animate {
            let _ = c.SetLeftInset(l);
            let _ = c.SetTopInset(t);
            let _ = c.SetRightInset(r.max(0.0));
            let _ = c.SetBottomInset(b.max(0.0));
            return;
        }
        let o: CompositionObject = c.cast().unwrap();
        spring_to(&self.comp, &o, "LeftInset", l, ratio, period);
        spring_to(&self.comp, &o, "RightInset", r.max(0.0), ratio, period);
        spring_to(&self.comp, &o, "BottomInset", b.max(0.0), ratio, period);
    }

    fn settle_region(&self) {
        let (x0, e) = (self.x0(), self.ear);
        self.set_region(((x0 - e).max(0.0), 0.0, (x0 + self.size.w + e).min(self.win.0 as f32), self.size.h));
    }

    // ---------- teks & elemen ----------

    fn text_surface(&mut self, s: &str, px: i32, weight: i32, face: &str, w: i32, h: i32) -> Option<(CompositionDrawingSurface, i32, i32)> {
        let key = format!("{face}|{px}|{weight}|{w}x{h}|{s}");
        if let Some(v) = self.text_cache.get(&key) {
            return Some(v.clone());
        }
        if self.text_cache.len() > 120 {
            self.text_cache.clear();
        }
        let bmp = text::render_face(s, &Style { px, weight }, face, w, h)?;
        let surface = self.gfx.surface_from_bgra(bmp.w, bmp.h, &bmp.bgra).ok()?;
        self.text_cache.insert(key, (surface.clone(), w, h));
        Some((surface, w, h))
    }

    /// Teks di `parent`. `x` adalah titik jangkar (sesuai `anchor`), `y` tepi atas. Mengembalikan lebar yang dipakai.
    #[allow(clippy::too_many_arguments)]
    fn text(&mut self, parent: &ContainerVisual, s: &str, x: f32, y: f32, size: f32, weight: i32, opacity: f32, anchor: Anchor, max_w: f32) -> f32 {
        let px = (size * self.dpi).round() as i32;
        let st = Style { px, weight };
        let (tw, th) = text::measure(s, &st);
        let w = (tw + 2).min(max_w as i32).max(1);
        let Some((surface, w, h)) = self.text_surface(s, px, weight, text::text_face(), w, th) else { return 0.0 };
        let left = match anchor {
            Anchor::Left => x,
            Anchor::Center => x - w as f32 / 2.0,
            Anchor::Right => x - w as f32,
        };
        self.place_surface(parent, &surface, left, y, w as f32, h as f32, opacity);
        w as f32
    }

    /// Simbol vektor bergaya SF Symbols (set yang sama dengan menu bar), berpusat pada tinta di (`cx`,`cy`).
    #[allow(clippy::too_many_arguments)]
    fn sym(&mut self, parent: &ContainerVisual, s: nacre_core::symbols::Sym, cx: f32, cy: f32, size: f32, opacity: f32, rgb: [u8; 3]) {
        let n = (size * self.dpi).round().max(8.0) as u32;
        let key = format!("sym|{s:?}|{n}|{rgb:?}");
        let hit = self.text_cache.get(&key).cloned();
        let (surf, w, h) = match hit {
            Some(v) => v,
            None => {
                let data = nacre_core::symbols::render(s, n, rgb);
                let Some((cd, cw, ch)) = nacre_core::symbols::crop_ink(&data, n, n) else { return };
                let Ok(sf) = self.gfx.surface_from_bgra(cw, ch, &cd) else { return };
                let v = (sf, cw as i32, ch as i32);
                self.text_cache.insert(key, v.clone());
                v
            }
        };
        self.place_surface(parent, &surf, cx - w as f32 / 2.0, cy - h as f32 / 2.0, w as f32, h as f32, opacity);
    }

    /// Glyph font ikon di dalam kotak persegi `box_px` berpusat di (`cx`,`cy`).
    fn icon(&mut self, parent: &ContainerVisual, glyph: &str, cx: f32, cy: f32, size: f32, opacity: f32) {
        let px = (size * self.dpi).round() as i32;
        let b = (size * 1.5 * self.dpi).round() as i32;
        if let Some((surface, w, h)) = self.text_surface_centered(glyph, px, b) {
            self.place_surface(parent, &surface, cx - w as f32 / 2.0, cy - h as f32 / 2.0, w as f32, h as f32, opacity);
        }
    }

    fn text_surface_centered(&mut self, glyph: &str, px: i32, b: i32) -> Option<(CompositionDrawingSurface, i32, i32)> {
        let key = format!("icon|{px}|{b}|{glyph}");
        if let Some(v) = self.text_cache.get(&key) {
            return Some(v.clone());
        }
        // Rata tengah: gambar di kotak bÃ—b dengan DT_CENTER lewat bitmap yang sudah dipusatkan.
        let g = text::render_face(glyph, &Style { px, weight: 400 }, ICON_FACE, b, b)?;
        let centered = center_glyph(&g)?;
        let surface = self.gfx.surface_from_bgra(centered.w, centered.h, &centered.bgra).ok()?;
        let v = (surface, centered.w as i32, centered.h as i32);
        self.text_cache.insert(key, v.clone());
        Some(v)
    }

    #[allow(clippy::too_many_arguments)]
    fn place_surface(&self, parent: &ContainerVisual, s: &CompositionDrawingSurface, x: f32, y: f32, w: f32, h: f32, opacity: f32) {
        let make = || -> windows_core::Result<()> {
            let brush = self.comp.CreateSurfaceBrushWithSurface(s)?;
            brush.SetStretch(CompositionStretch::None)?;
            let v = self.comp.CreateSpriteVisual()?;
            v.SetBrush(&brush)?;
            v.SetSize(Vector2 { X: w, Y: h })?;
            v.SetOffset(Vector3 { X: x, Y: y, Z: 0.0 })?;
            v.SetOpacity(opacity)?;
            parent.Children()?.InsertAtTop(&v)
        };
        let _ = make();
    }

    fn rect(&self, parent: &ContainerVisual, x: f32, y: f32, w: f32, h: f32, radius: f32, c: Color) {
        let make = || -> windows_core::Result<()> {
            let v = self.comp.CreateSpriteVisual()?;
            v.SetSize(Vector2 { X: w, Y: h })?;
            v.SetOffset(Vector3 { X: x, Y: y, Z: 0.0 })?;
            v.SetBrush(&self.comp.CreateColorBrushWithColor(c)?)?;
            if radius > 0.0 {
                v.SetClip(&nacre_ui_clip(&self.comp, w, h, radius)?)?;
            }
            parent.Children()?.InsertAtTop(&v)
        };
        let _ = make();
    }

    fn region(&mut self, x: f32, y: f32, w: f32, h: f32, action: Action) {
        self.regions.push(Region { x, y, w, h, action });
    }

    // ---------- render isi ----------

    fn now_ms(&self) -> i64 {
        self.t0.elapsed().as_millis() as i64
    }

    fn position_ms(&self) -> i64 {
        let extra = if self.playing { self.pos_at.elapsed().as_millis() as i64 } else { 0 };
        let p = self.pos_ms + extra;
        if self.dur_ms > 0 {
            p.min(self.dur_ms)
        } else {
            p
        }
    }

    fn render(&mut self) {
        if let Ok(kids) = self.content.Children() {
            let _ = kids.RemoveAll();
        }
        self.regions.clear();
        let content = self.content.clone();
        let d = self.dpi;
        let x0 = self.x0();
        let (w, h) = (self.size.w, self.size.h);
        let white = Color { A: 255, R: 255, G: 255, B: 255 };
        let bars_hidden = !self.has_media || !matches!(self.state.open, None | Some(Panel::NowPlaying));
        let _ = self.bars_root.SetOpacity(if bars_hidden { 0.0 } else { 1.0 });
        let bars_w = (BANDS as f32) * 3.0 * d + (BANDS as f32 - 1.0) * 3.0 * d;

        match self.state.open {
            None => {
                if self.has_media {
                    // sayap kiri: artwork mini; sayap kanan: bar visualizer; tengah dikosongkan seperti notch
                    let a = 20.0 * d;
                    let ay = (h - a) / 2.0;
                    self.artwork(&content, x0 + 12.0 * d, ay, a, 6.0 * d);
                    let _ = self.bars_root.SetOffset(Vector3 { X: x0 + w - 12.0 * d - bars_w, Y: (h - 16.0 * d) / 2.0, Z: 0.0 });
                } else if !crate::prefs::get().menubar {
                    // dengan menu bar aktif jam sudah ada di kanan bar: notch dibiarkan hitam polos seperti MacBook
                    let t = time::local_now();
                    let clock = format!("{:02}:{:02}", t.hour, t.minute);
                    self.text(&content, &clock, x0 + w / 2.0, (h - 15.0 * d) / 2.0 - 1.0, 12.0, 600, 0.92, Anchor::Center, w);
                }
                if !self.has_media {
                    let _ = self.bars_root.SetOffset(Vector3 { X: -100.0, Y: 0.0, Z: 0.0 });
                }
                let _ = white;
            }
            Some(Panel::NowPlaying) => self.render_now_playing(&content, x0, w),
            Some(Panel::Calendar) => self.render_calendar(&content, x0, w),
            Some(Panel::Controls) => self.render_controls(&content, x0, w),
        }
        if self.state.open.is_some() {
            self.render_pager(&content, x0, w, h);
        }
    }

    /// Ikon berwarna `rgb` (glyph dirender putih lalu diwarnai ulang; di-cache per warna).
    fn icon_rgb(&mut self, parent: &ContainerVisual, glyph: &str, cx: f32, cy: f32, size: f32, opacity: f32, rgb: [u8; 3]) {
        let px = (size * self.dpi).round() as i32;
        let b = (size * 1.5 * self.dpi).round() as i32;
        let key = format!("iconc|{px}|{b}|{glyph}|{rgb:?}");
        let cached = self.text_cache.get(&key).cloned();
        let (surface, w, h) = match cached {
            Some(v) => v,
            None => {
                let Some(g) = text::render_face(glyph, &Style { px, weight: 400 }, ICON_FACE, b, b) else { return };
                let Some(mut c) = center_glyph(&g) else { return };
                for p in c.bgra.chunks_exact_mut(4) {
                    let a = p[3] as u32;
                    p[0] = (rgb[2] as u32 * a / 255) as u8;
                    p[1] = (rgb[1] as u32 * a / 255) as u8;
                    p[2] = (rgb[0] as u32 * a / 255) as u8;
                }
                let Ok(surface) = self.gfx.surface_from_bgra(c.w, c.h, &c.bgra) else { return };
                let v = (surface, c.w as i32, c.h as i32);
                self.text_cache.insert(key, v.clone());
                v
            }
        };
        self.place_surface(parent, &surface, cx - w as f32 / 2.0, cy - h as f32 / 2.0, w as f32, h as f32, opacity);
    }

    /// Panel Kontrol bergaya Control Center macOS: dua ubin bundar (Wi-Fi, Bluetooth) dengan lingkaran ikon
    /// beraksen biru saat aktif, lalu slider kapsul tebal (Layar, Suara) dengan ikon di dalam isian putih.
    fn render_controls(&mut self, c: &ContainerVisual, x0: f32, w: f32) {
        use nacre_platform::radio::Kind as RK;
        let d = self.dpi;
        let _ = self.bars_root.SetOffset(Vector3 { X: -100.0, Y: 0.0, Z: 0.0 });
        let white = Color { A: 240, R: 255, G: 255, B: 255 };
        let accent = crate::prefs::accent_c(255);
        let m = 14.0 * d;

        // --- ubin Wi-Fi dan Bluetooth ---
        let rs = self.radio_state;
        let gap = 10.0 * d;
        let (tw, th) = ((w - 2.0 * m - gap) / 2.0, 56.0 * d);
        let ty = 14.0 * d;
        let tiles = [(Sym::Wifi, "Wi-Fi", rs.wifi, RK::Wifi), (Sym::Bluetooth, "Bluetooth", rs.bluetooth, RK::Bluetooth)];
        for (i, (glyph, label, st, kind)) in tiles.into_iter().enumerate() {
            let tx = x0 + m + i as f32 * (tw + gap);
            self.rect(c, tx, ty, tw, th, 16.0 * d, Color { A: 26, R: 255, G: 255, B: 255 });
            let on = st == Some(true);
            let r = 18.0 * d;
            let (ccx, ccy) = (tx + 12.0 * d + r, ty + th / 2.0);
            self.rect(c, ccx - r, ccy - r, 2.0 * r, 2.0 * r, r, if on { accent } else { Color { A: 56, R: 255, G: 255, B: 255 } });
            self.sym(c, glyph, ccx, ccy, 21.0, if st.is_some() { 1.0 } else { 0.4 }, [255, 255, 255]);
            let lx = ccx + r + 9.0 * d;
            let sub = match st {
                Some(true) => crate::prefs::tr("Aktif"),
                Some(false) => crate::prefs::tr("Mati"),
                None => crate::prefs::tr("Tidak tersedia"),
            };
            self.text(c, label, lx, ty + 11.0 * d, 13.0, 600, 1.0, Anchor::Left, tw - (lx - tx) - 6.0 * d);
            self.text(c, sub, lx, ty + 30.0 * d, 11.0, 400, 0.6, Anchor::Left, tw - (lx - tx) - 6.0 * d);
            if st.is_some() {
                self.region(tx, ty, tw, th, Action::Radio(kind));
            }
        }

        // --- slider gaya Control Center Tahoe: judul, jalur tipis dengan ikon kecil dan besar di ujungnya,
        // dan kenop kaca bening di ujung isian selama diseret ---
        let vol = nacre_platform::audio::latest();
        let rows: [(u8, &str, Option<f32>, Sym, Sym); 2] = [
            (1, "Layar", nacre_platform::brightness::latest().map(|b| b as f32 / 100.0), Sym::SunSmall, Sym::Sun),
            (0, "Suara", vol.available.then_some(if vol.muted { 0.0 } else { vol.level }), Sym::Speaker, Sym::SpeakerWave),
        ];
        let (sx, sw) = (x0 + m, w - 2.0 * m);
        let mut y = ty + th + 14.0 * d;
        for (kind, label, val, lo, hi) in rows {
            self.text(c, crate::prefs::tr(label), sx + 2.0 * d, y, 12.5, 600, 0.9, Anchor::Left, sw);
            y += 20.0 * d;
            let row_h = 30.0 * d;
            let cy = y + row_h / 2.0;
            let (tx, tw, tth) = (sx + 32.0 * d, sw - 64.0 * d, 8.0 * d);
            let (lo_size, hi_size) = (if lo == Sym::SunSmall { 14.0 } else { 19.0 }, if hi == Sym::Sun { 18.0 } else { 24.0 });
            let ink = if val.is_some() { 0.8 } else { 0.35 };
            self.rect(c, tx, cy - tth / 2.0, tw, tth, tth / 2.0, Color { A: 70, R: 255, G: 255, B: 255 });
            self.sym(c, lo, sx + 14.0 * d, cy, lo_size, ink, [255, 255, 255]);
            self.sym(c, hi, sx + sw - 15.0 * d, cy, hi_size, ink, [255, 255, 255]);
            if let Some(v) = val {
                self.rect(c, tx, cy - tth / 2.0, (tw * v).max(tth), tth, tth / 2.0, white);
                if self.dragging.map(|(k, _, _)| k) == Some(kind) {
                    // kenop kaca: kapsul putih tembus pandang dengan inti lebih redup, seperti Control Center
                    let (kw, kh) = (34.0 * d, 22.0 * d);
                    let kx = tx + tw * v - kw / 2.0;
                    self.rect(c, kx, cy - kh / 2.0, kw, kh, kh / 2.0, Color { A: 175, R: 255, G: 255, B: 255 });
                    self.rect(c, kx + 1.5 * d, cy - kh / 2.0 + 1.5 * d, kw - 3.0 * d, kh - 3.0 * d, (kh - 3.0 * d) / 2.0, Color { A: 70, R: 255, G: 255, B: 255 });
                }
                self.region(tx - 10.0 * d, cy - row_h / 2.0, tw + 20.0 * d, row_h, Action::Slider(kind, tx, tw));
                if kind == 0 {
                    self.region(sx, cy - row_h / 2.0, 28.0 * d, row_h, Action::Mute);
                }
            }
            y += row_h + 12.0 * d;
        }
    }


    fn artwork(&mut self, parent: &ContainerVisual, x: f32, y: f32, size: f32, radius: f32) {
        let make = || -> windows_core::Result<()> {
            let v = self.comp.CreateSpriteVisual()?;
            v.SetSize(Vector2 { X: size, Y: size })?;
            v.SetOffset(Vector3 { X: x, Y: y, Z: 0.0 })?;
            match &self.art {
                Some(s) => {
                    let b = self.comp.CreateSurfaceBrushWithSurface(s)?;
                    b.SetStretch(CompositionStretch::UniformToFill)?;
                    v.SetBrush(&b)?;
                }
                None => v.SetBrush(&self.comp.CreateColorBrushWithColor(Color { A: 60, R: 255, G: 255, B: 255 })?)?,
            }
            v.SetClip(&nacre_ui_clip(&self.comp, size, size, radius)?)?;
            parent.Children()?.InsertAtTop(&v)
        };
        let _ = make();
    }

    fn render_now_playing(&mut self, c: &ContainerVisual, x0: f32, w: f32) {
        let d = self.dpi;
        self.artwork(c, x0 + 16.0 * d, 16.0 * d, 88.0 * d, 14.0 * d);
        let cx = x0 + 120.0 * d; // kolom kanan
        let col_w = w - 120.0 * d - 16.0 * d;
        let bars_w = BANDS as f32 * 3.0 * d + (BANDS as f32 - 1.0) * 3.0 * d;
        let _ = self.bars_root.SetOffset(Vector3 { X: x0 + w - 16.0 * d - bars_w, Y: 20.0 * d, Z: 0.0 });
        let title = if self.title.is_empty() { crate::prefs::tr("Tidak ada media").to_string() } else { self.title.clone() };
        let (title, artist) = (title, self.artist.clone());
        self.text(c, &title, cx, 16.0 * d, 15.0, 600, 1.0, Anchor::Left, col_w - bars_w - 10.0 * d);
        self.text(c, &artist, cx, 38.0 * d, 12.0, 400, 0.62, Anchor::Left, col_w);

        // progres
        let (ty, th) = (70.0 * d, 4.0 * d);
        self.rect(c, cx, ty, col_w, th, th / 2.0, Color { A: 55, R: 255, G: 255, B: 255 });
        let pos = self.position_ms();
        if self.dur_ms > 0 {
            let frac = (pos as f32 / self.dur_ms as f32).clamp(0.0, 1.0);
            self.rect(c, cx, ty, (col_w * frac).max(th), th, th / 2.0, Color { A: 235, R: 255, G: 255, B: 255 });
        }
        if self.can_seek && self.dur_ms > 0 {
            self.region(cx, ty - 8.0 * d, col_w, th + 16.0 * d, Action::Seek(cx, col_w));
        }
        let fmt = |ms: i64| format_mmss(ms / 1000);
        self.text(c, &fmt(pos), cx, 78.0 * d, 11.0, 400, 0.55, Anchor::Left, 80.0 * d);
        if self.dur_ms > 0 {
            self.text(c, &fmt(self.dur_ms), cx + col_w, 78.0 * d, 11.0, 400, 0.55, Anchor::Right, 80.0 * d);
        }

        // kontrol: sebelumnya / putar-jeda / berikutnya
        let by = 114.0 * d;
        let mid = cx + col_w / 2.0;
        let step = 56.0 * d;
        let play = if self.playing { Sym::Pause } else { Sym::Play };
        self.sym(c, Sym::Backward, mid - step, by, 21.0, 0.9, [255, 255, 255]);
        self.sym(c, play, mid, by, 27.0, 1.0, [255, 255, 255]);
        self.sym(c, Sym::Forward, mid + step, by, 21.0, 0.9, [255, 255, 255]);
        let (bw, bh) = (44.0 * d, 34.0 * d);
        self.region(mid - step - bw / 2.0, by - bh / 2.0, bw, bh, Action::Prev);
        self.region(mid - bw / 2.0, by - bh / 2.0, bw, bh, Action::PlayPause);
        self.region(mid + step - bw / 2.0, by - bh / 2.0, bw, bh, Action::Next);
    }

    fn render_calendar(&mut self, c: &ContainerVisual, x0: f32, w: f32) {
        let d = self.dpi;
        let _ = self.bars_root.SetOffset(Vector3 { X: -100.0, Y: 0.0, Z: 0.0 });
        let (y, m) = self.cal;
        let g = calendar::grid(y, m, self.monday_first);

        // judul + navigasi bulan
        let title = format!("{} {}", nacre_core::i18n::month_name(crate::prefs::get().lang, m, calendar::month_name(m)), y);
        self.text(c, &title, x0 + w / 2.0, 12.0 * d, 14.0, 600, 1.0, Anchor::Center, w - 100.0 * d);

        let cw = (w - 32.0 * d) / 7.0;
        let colx = |col: u32| x0 + 16.0 * d + cw * (col as f32 + 0.5);
        for (i, lab) in nacre_core::i18n::weekday_labels(crate::prefs::get().lang, self.monday_first, calendar::weekday_labels(self.monday_first)).iter().enumerate() {
            self.text(c, lab, colx(i as u32), 40.0 * d, 11.0, 400, 0.5, Anchor::Center, cw);
        }
        let today = time::local_now();
        let (row_h, grid_y) = (26.0 * d, 60.0 * d);
        for day in 1..=g.days {
            let idx = g.first_col + day - 1;
            let (col, row) = (idx % 7, idx / 7);
            let (cx, ry) = (colx(col), grid_y + row as f32 * row_h);
            let is_today = (today.year, today.month, today.day) == (y, m, day);
            if is_today {
                let dia = 24.0 * d;
                self.rect(c, cx - dia / 2.0, ry + (row_h - dia) / 2.0, dia, dia, dia / 2.0, Color { A: 255, R: 255, G: 69, B: 58 });
            }
            self.text(c, &day.to_string(), cx, ry + 4.0 * d, 12.0, if is_today { 600 } else { 400 }, 0.95, Anchor::Center, cw);
        }

        // timer
        let ty = grid_y + g.rows as f32 * row_h + 10.0 * d;
        self.rect(c, x0 + 16.0 * d, ty - 6.0 * d, w - 32.0 * d, 1.0, 0.0, Color { A: 30, R: 255, G: 255, B: 255 });
        let now = self.now_ms();
        let shown = self.timer.remaining_secs(now);
        let label = format_mmss(shown);
        let finished = self.timer.phase == Phase::Finished;
        self.text(c, &label, x0 + 20.0 * d, ty + 14.0 * d, 28.0, 600, if finished { 0.55 } else { 1.0 }, Anchor::Left, 150.0 * d);
        let by = ty + 32.0 * d;
        let running = self.timer.phase == Phase::Running;
        let btn = 38.0 * d;
        let right = x0 + w - 20.0 * d - btn / 2.0;
        let slots = [
            (Sym::Restart, right, Action::TimerReset, 17.0),
            (if running { Sym::Pause } else { Sym::Play }, right - btn, Action::TimerToggle, 20.0),
            (Sym::Plus, right - 2.0 * btn, Action::TimerPlus, 16.0),
            (Sym::Minus, right - 3.0 * btn, Action::TimerMinus, 16.0),
        ];
        for (glyph, bx, action, size) in slots {
            self.sym(c, glyph, bx, by, size, 0.95, [255, 255, 255]);
            self.region(bx - btn / 2.0, by - btn / 2.0, btn, btn, action);
        }
    }

    // ---------- data ----------

    fn on_media(&mut self) {
        let mut last = None;
        while let Ok(s) = self.media.snapshots.try_recv() {
            last = Some(s);
        }
        let Some(s) = last else { return };
        self.has_media = s.has_media;
        self.title = s.title;
        self.artist = s.artist;
        self.playing = s.playing;
        self.pos_ms = s.position_ms;
        self.dur_ms = s.duration_ms;
        self.pos_at = Instant::now();
        self.can_seek = s.can_seek;
        match s.art {
            ArtUpdate::Unchanged => {}
            ArtUpdate::None => self.art = None,
            ArtUpdate::New(b) => {
                self.art = self.gfx.surface_from_bgra(b.w, b.h, &b.bgra).ok();
            }
        }
        self.state.media = self.has_media;
        if !self.has_media && self.state.open == Some(Panel::NowPlaying) {
            self.state.open = Some(Panel::Calendar);
            // tinggi kalender bergantung jumlah baris bulan yang ditampilkan
            self.state.calendar_rows = calendar::grid(self.cal.0, self.cal.1, self.monday_first).rows;
        }
        self.sync_visualizer();
        self.apply_target();
        self.render();
        self.sync_tick();
    }

    /// Penangkap audio hanya hidup saat ada musik yang main dan island terlihat.
    fn sync_visualizer(&mut self) {
        let want = (self.playing || self.diag_bands) && !self.fullscreen;
        if want && self.loopback.is_none() {
            let hw = self.hwnd.0 as isize;
            self.loopback = Some(loopback::start(move || post(hw, WM_BANDS)));
        } else if !want && self.loopback.is_some() {
            self.loopback = None;
            self.set_bars_idle();
        }
    }

    fn set_bars_idle(&self) {
        for b in &self.bars {
            let o: CompositionObject = b.cast().unwrap();
            spring_to(&self.comp, &o, "Scale.Y", 0.2, 0.9, 60);
        }
    }

    fn on_bands(&mut self) {
        let v = loopback::latest();
        if self.diag_bands {
            self.band_msgs += 1;
            if self.band_msgs % 30 == 1 {
                log::line("visualizer", &format!("pita={:.2?} (pesan ke-{})", v, self.band_msgs));
            }
        }
        for (b, val) in self.bars.iter().zip(v.iter()) {
            let o: CompositionObject = b.cast().unwrap();
            spring_to(&self.comp, &o, "Scale.Y", val.clamp(0.18, 1.0), 0.8, 40);
        }
    }

    /// Timer 1 detik hanya selama ada yang perlu diperbarui tiap detik.
    fn sync_tick(&self) {
        let need = self.timer.phase == Phase::Running
            || (self.playing && self.state.open == Some(Panel::NowPlaying));
        if need {
            timer(self.hwnd, T_TICK, 1000);
        } else {
            kill(self.hwnd, T_TICK);
        }
    }

    fn on_tick(&mut self) {
        let now = self.now_ms();
        if self.timer.tick(now) {
            time::chime();
            log::line("island", "timer selesai");
        }
        if self.state.open.is_some() {
            self.render();
        }
        self.sync_tick();
    }

    // ---------- input ----------

    fn ensure_radios(&mut self) {
        if let Some(r) = &self.radios {
            let _ = r.commands.send(nacre_platform::radio::Cmd::Refresh);
            return;
        }
        let hw = self.hwnd.0 as isize;
        self.radios = Some(nacre_platform::radio::spawn(move || post(hw, crate::services::WM_RADIO)));
    }

    fn on_radio(&mut self) {
        if let Some(r) = &self.radios {
            while let Ok(s) = r.snapshots.try_recv() {
                self.radio_state = s;
            }
        }
        if self.state.open == Some(Panel::Controls) && self.dragging.is_none() {
            self.render();
        }
    }

    fn on_system_value(&mut self) {
        if self.state.open == Some(Panel::Controls) {
            self.render();
        }
    }

    fn slider_set(&self, kind: u8, x: f32, sx: f32, sw: f32) {
        let v = nacre_core::hud::slider::value_at(x, sx, sw, false);
        if kind == 0 {
            crate::services::set_volume(v);
        } else {
            crate::services::set_brightness(v);
        }
    }

    fn on_down(&mut self, x: f32, y: f32) -> bool {
        if self.state.open != Some(Panel::Controls) {
            return false;
        }
        let hit = self.regions.iter().find(|r| x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h).map(|r| r.action);
        log::line("island", &format!("down ({x},{y}) hit={hit:?}"));
        if let Some(Action::Slider(k, sx, sw)) = hit {
            self.dragging = Some((k, sx, sw));
            self.slider_set(k, x, sx, sw);
            return true;
        }
        false
    }

    fn on_drag(&mut self, x: f32) {
        if let Some((k, sx, sw)) = self.dragging {
            self.slider_set(k, x, sx, sw);
        }
    }

    fn on_up(&mut self) -> bool {
        let was = self.dragging.take().is_some();
        if was {
            self.render();
        }
        was
    }

    /// Pindahkan isi lama ke lapisan ghost yang memudar cepat, lalu siapkan isi baru untuk fade-in tertunda
    /// (menunggu kaca membesar/mengecil), sehingga isi tidak muncul penuh di luar badan notch.
    fn begin_transition(&mut self) {
        let go = || -> windows_core::Result<()> {
            let kids = self.content.Children()?;
            let old: Vec<windows::UI::Composition::Visual> = kids.clone().into_iter().collect();
            for v in &old {
                kids.Remove(v)?;
                self.ghost.Children()?.InsertAtTop(v)?;
            }
            self.ghost.SetOpacity(1.0)?;
            let a = self.comp.CreateScalarKeyFrameAnimation()?;
            a.InsertKeyFrame(0.0, 1.0)?;
            a.InsertKeyFrame(1.0, 0.0)?;
            a.SetDuration(windows::Foundation::TimeSpan { Duration: 110 * 10_000 })?;
            self.ghost.cast::<CompositionObject>()?.StartAnimation(&HSTRING::from("Opacity"), &a)?;
            timer(self.hwnd, T_GHOST, 160);
            Ok(())
        };
        let _ = go();
    }

    /// Isi baru: transparan dulu, lalu muncul setelah badan hampir selesai bergerak.
    fn fade_in_content(&self) {
        let go = || -> windows_core::Result<()> {
            self.content.SetOpacity(0.0)?;
            let a = self.comp.CreateScalarKeyFrameAnimation()?;
            a.InsertKeyFrame(0.0, 0.0)?;
            a.InsertKeyFrame(0.18, 0.0)?;
            let ease = self.comp.CreateCubicBezierEasingFunction(Vector2 { X: 0.2, Y: 0.0 }, Vector2 { X: 0.2, Y: 1.0 })?;
            a.InsertKeyFrameWithEasingFunction(1.0, 1.0, &ease.cast::<windows::UI::Composition::CompositionEasingFunction>()?)?;
            a.SetDuration(windows::Foundation::TimeSpan { Duration: 280 * 10_000 })?;
            self.content.cast::<CompositionObject>()?.StartAnimation(&HSTRING::from("Opacity"), &a)
        };
        let _ = go();
    }

    fn open(&mut self, p: Panel) {
        self.begin_transition();
        self.state.open = Some(p);
        if p == Panel::Controls {
            self.ensure_radios();
        }
        if p == Panel::Calendar {
            let now = time::local_now();
            self.cal = (now.year, now.month);
            let g = calendar::grid(self.cal.0, self.cal.1, self.monday_first);
            self.state.calendar_rows = g.rows;
        }
        self.apply_target();
        self.render();
        self.fade_in_content();
        self.sync_tick();
    }

    fn close(&mut self) {
        log::line("island", "tutup");
        if self.state.open.is_some() || self.state.hover {
            let was_open = self.state.open.is_some();
            if was_open {
                self.begin_transition();
            }
            self.state.open = None;
            self.state.hover = false;
            self.apply_target();
            self.render();
            if was_open {
                self.fade_in_content();
            }
            self.sync_tick();
        }
    }

    fn on_move(&mut self) {
        kill(self.hwnd, T_CLOSE);
        if self.grace {
            self.grace = false;
            kill(self.hwnd, T_GRACE);
        }
        if !self.state.hover {
            self.state.hover = true;
            self.apply_target();
            self.render();
        }
        // Buka otomatis: kursor menetap sebentar di pil (bukan sekadar lewat).
        if self.state.open.is_none() && crate::prefs::get().notch_hover_open {
            timer(self.hwnd, T_HOVEROPEN, 170);
        }
    }

    fn on_click(&mut self, x: f32, y: f32) {
        log::line("island", &format!("klik ({x},{y}) buka={:?}", self.state.open));
        if self.state.open.is_none() {
            self.open(layout::initial_panel(self.has_media));
            return;
        }
        let hit = self.regions.iter().find(|r| x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h).map(|r| r.action);
        let Some(a) = hit else { return };
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
            Action::Seek(sx, sw) => {
                if self.dur_ms > 0 {
                    let frac = ((x - sx) / sw).clamp(0.0, 1.0);
                    let _ = self.media.commands.send(Command::SeekMs((self.dur_ms as f32 * frac) as i64));
                }
            }
            Action::CalPrev | Action::CalNext => {
                let delta = if matches!(a, Action::CalPrev) { -1 } else { 1 };
                self.cal = calendar::shift_month(self.cal.0, self.cal.1, delta);
                let g = calendar::grid(self.cal.0, self.cal.1, self.monday_first);
                if g.rows != self.state.calendar_rows {
                    self.state.calendar_rows = g.rows;
                    self.apply_target();
                }
            }
            Action::TimerToggle => {
                if self.timer.phase == Phase::Running {
                    self.timer.pause(now);
                } else {
                    self.timer.start(now);
                }
                self.sync_tick();
            }
            Action::TimerReset => {
                self.timer.reset();
                self.sync_tick();
            }
            Action::TimerPlus => self.timer.adjust(300),
            Action::TimerMinus => self.timer.adjust(-300),
            Action::Slider(..) => {}
            Action::Mute => crate::services::toggle_mute(),
            Action::Goto(p) => {
                if self.state.open != Some(p) {
                    self.open(p);
                }
                return;
            }
            Action::OpenSettings => {
                crate::settings_win::open(&self.comp);
                return;
            }
            Action::Radio(k) => {
                let cur = match k {
                    nacre_platform::radio::Kind::Wifi => self.radio_state.wifi,
                    nacre_platform::radio::Kind::Bluetooth => self.radio_state.bluetooth,
                };
                if let Some(r) = &self.radios {
                    let _ = r.commands.send(nacre_platform::radio::Cmd::Set(k, !cur.unwrap_or(false)));
                }
            }
        }
        self.render();
    }

    fn on_wheel(&mut self, delta: i16) {
        // Pil tertutup tidak membuka panel lewat scroll (hanya klik/hover); scroll hanya berpindah antar panel.
        if self.state.open.is_none() {
            return;
        }
        let list = layout::panels(self.has_media);
        let dir = if delta > 0 { -1 } else { 1 };
        let cur = self.state.open.unwrap_or_else(|| layout::initial_panel(self.has_media));
        let next = if self.state.open.is_some() { layout::cycle(&list, cur, dir) } else { cur };
        self.open(next);
    }

    fn on_timer(&mut self, id: usize) {
        match id {
            T_HOVEROPEN => {
                kill(self.hwnd, id);
                if self.tracking && self.state.open.is_none() && !self.off && crate::prefs::get().notch_hover_open {
                    let p = layout::initial_panel(self.has_media);
                    self.open(p);
                }
            }
            T_GHOST => {
                kill(self.hwnd, id);
                if let Ok(k) = self.ghost.Children() {
                    let _ = k.RemoveAll();
                }
            }
            T_CLOSE => {
                kill(self.hwnd, id);
                if !self.tracking {
                    self.close();
                }
            }
            T_CLOCK => {
                if self.state.open.is_none() && !self.has_media {
                    self.render();
                }
                timer(self.hwnd, T_CLOCK, time::ms_to_next_minute());
            }
            T_TICK => self.on_tick(),
            T_RECONF => {
                kill(self.hwnd, id);
                self.on_reconfigure();
            }
            T_REGION => {
                kill(self.hwnd, id);
                self.settle_region();
            }
            T_WINCHG => {
                kill(self.hwnd, id);
                let screen = sys::screen_size();
                let mut ov = overlap::evaluate(tracker::external_foreground(), std::process::id(), self.pill_screen_rect(), screen);
                if ov.fullscreen && crate::prefs::get().above_fullscreen {
                    ov = overlap::Overlap { overlapped: false, fullscreen: false };
                }
                if ov != self.overlap {
                    self.overlap = ov;
                    self.fullscreen = ov.fullscreen;
                    self.sync_visualizer();
                    self.update_visibility();
                }
            }
            T_DWELL => {
                kill(self.hwnd, id);
                self.edge_armed = true;
                self.update_visibility();
            }
            T_GRACE => {
                kill(self.hwnd, id);
                self.grace = false;
                self.update_visibility();
            }
            T_START => {
                kill(self.hwnd, id);
                self.starting = false;
                self.update_visibility();
            }
            T_HIDEWIN => {
                kill(self.hwnd, id);
                if !self.shown {
                    unsafe {
                        let _ = ShowWindow(self.hwnd, SW_HIDE);
                    }
                }
            }
            _ => {}
        }
    }

    /// Kotak pil (tertutup) dalam piksel layar, untuk menguji tumpang-tindih dengan window lain.
    fn pill_screen_rect(&self) -> (i32, i32, i32, i32) {
        let base = layout::target(&layout::State { open: None, hover: false, media: self.has_media, calendar_rows: 5 }, self.dpi);
        let (sx, _) = (sys::screen_size().0, 0);
        let l = ((sx as f32 - base.w) / 2.0) as i32;
        (l, 0, l + base.w as i32, (base.h + 4.0 * self.dpi) as i32)
    }

    fn place_strip(&self) {
        if let Some(s) = &self.strip {
            let (l, _, r, _) = self.pill_screen_rect();
            let pad = (60.0 * self.dpi) as i32;
            s.place(l - pad, 0, r - l + 2 * pad, 2);
            s.set_visible(!self.fullscreen);
        }
    }

    fn interacting(&self) -> bool {
        self.tracking || self.state.open.is_some() || self.grace
    }

    fn animate_stage_y(&self, y: f32) {
        spring_to(&self.comp, &self.stage.cast::<CompositionObject>().unwrap(), "Offset.Y", y, 0.78, 150);
    }

    fn update_visibility(&mut self) {
        let inputs = Inputs {
            overlapped: self.overlap.overlapped,
            interacting: self.interacting(),
            edge_armed: self.edge_armed,
            starting: self.starting,
            fullscreen: self.overlap.fullscreen,
        };
        let hide = self.off || should_hide(self.mode, &inputs);
        if let Some(s) = &self.strip {
            s.set_visible(!self.overlap.fullscreen && !self.off);
        }
        if hide && self.shown {
            self.shown = false;
            self.animate_stage_y(-(self.size.h + self.top_over + 8.0));
            timer(self.hwnd, T_HIDEWIN, 550);
            log::line("island", "sembunyi");
        } else if !hide && !self.shown {
            self.shown = true;
            kill(self.hwnd, T_HIDEWIN);
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
            }
            self.animate_stage_y(0.0);
            log::line("island", "tampil");
        }
    }

    fn start_grace(&mut self) {
        self.grace = true;
        timer(self.hwnd, T_GRACE, 800);
    }

    fn make_ear(&self, e: u32, left: bool) -> Option<CompositionDrawingSurface> {
        // alfa penuh seperti saat dibuat: telinga setengah tembus akan meninggalkan garis di sambungan dengan badan
        let b = ears::bitmap(e, ONYX, 255, left);
        self.gfx.surface_from_bgra(b.w, b.h, &b.bgra).ok()
    }

    /// Resolusi/DPI berubah saat berjalan: posisikan ulang window, ukur ulang telinga dan bar, lalu render ulang.
    fn on_reconfigure(&mut self) {
        let screen_w = sys::screen_size().0;
        let dpi = unsafe { GetDpiForWindow(self.hwnd) }.max(96) as f32 / 96.0;
        let win = (((470.0 * dpi) as i32).min(screen_w), (390.0 * dpi) as i32);
        if win == self.win && screen_w == self.screen_w && (dpi - self.dpi).abs() < 0.01 {
            return;
        }
        self.screen_w = screen_w;
        log::line("island", &format!("reconfigurasi layar_w={screen_w} dpi={dpi}"));
        self.text_cache.clear();
        let dpi_changed = (dpi - self.dpi).abs() >= 0.01;
        self.dpi = dpi;
        self.win = win;
        unsafe {
            let _ = SetWindowPos(self.hwnd, None, (screen_w - win.0) / 2, 0, win.0, win.1, SWP_NOACTIVATE | SWP_NOZORDER);
        }
        let _ = self._surface.root.SetSize(Vector2 { X: win.0 as f32, Y: win.1 as f32 });
        let full = Vector2 { X: win.0 as f32, Y: win.1 as f32 };
        let _ = self.content.SetSize(full);
        let _ = self.ghost.SetSize(full);
        if dpi_changed {
            let e = (13.0 * dpi).ceil() as u32;
            self.ear = e as f32;
            self.top_over = 32.0 * dpi;
            for (i, (sprite, left)) in [(self.ear_l.clone(), true), (self.ear_r.clone(), false)].into_iter().enumerate() {
                if let Some(s) = self.make_ear(e, left) {
                    let _ = sprite.SetSize(Vector2 { X: e as f32, Y: e as f32 });
                    let _ = self.ear_tint[i].SetSize(Vector2 { X: e as f32, Y: e as f32 });
                    self.ear_mask[i] = s;
                }
            }
            // brush hitam/kaca dibuat ulang dari topeng baru agar telinga tetap sama dengan badan notch
            self.rebuild_ears();
            let (bw, bh, gap) = (3.0 * dpi, 16.0 * dpi, 3.0 * dpi);
            for (i, b) in self.bars.iter().enumerate() {
                let _ = b.SetSize(Vector2 { X: bw, Y: bh });
                let _ = b.SetOffset(Vector3 { X: i as f32 * (bw + gap), Y: 0.0, Z: 0.0 });
                let _ = b.SetCenterPoint(Vector3 { X: bw / 2.0, Y: bh / 2.0, Z: 0.0 });
                if let Ok(c) = nacre_ui_clip(&self.comp, bw, bh, bw / 2.0) {
                    let _ = b.SetClip(&c);
                }
            }
        }
        self.first_apply = true;
        self.publish_notch();
        self.apply_target();
        self.render();
    }

    /// Umumkan pil notch (px layar) ke menu bar agar menyisakan celah; `(0,0,0)` bila notch mati.
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

    /// Terapkan pengaturan notch: perilaku (tetap/menimpa/auto-hide/peek) dan tampilan kaca.
    fn apply_prefs(&mut self) {
        let s = crate::prefs::get();
        if self.off == s.notch_enabled {
            self.off = !s.notch_enabled;
            log::line("island", if self.off { "dinonaktifkan" } else { "diaktifkan" });
        }
        let (mode, reserve) = (s.notch.mode(), s.notch.reserves());
        // Menu bar aktif: dialah satu-satunya yang mencadangkan ruang atas (tingginya sama dengan pil notch).
        let reserve = reserve && !self.off && !s.menubar;
        if mode != self.mode {
            self.mode = mode;
            if mode == Mode::Always {
                self.strip = None;
                self.edge_armed = false;
            } else if self.strip.is_none() {
                self.strip = Strip::new(self.hwnd, STRIP_ENTER_I, STRIP_LEAVE_I).ok();
                self.place_strip();
            }
        }
        if reserve != self.reserved {
            self.set_reserved(reserve);
        }
        self.publish_notch();
        self.update_visibility();
        let _ = self.panel.set_params(island_glass(self.size.radius));
        self.rebuild_ears();
        self.apply_target();
        self.render();
        log::line("island", &format!("pengaturan: perilaku={mode:?} cadangan={reserve}"));
    }

    fn refresh_tier(&mut self) {
        let t = self.glass_override.unwrap_or_else(|| {
            let c = sys::conditions();
            if !c.transparency || c.battery_saver || c.remote_session { Tier::Solid } else { Tier::Full }
        });
        if t != self.tier {
            self.tier = t;
            let _ = self.panel.set_tier(t);
            // telinga kaca memilih blur atau warna padat menurut tier
            self.rebuild_ears();
        }
    }
}

/// Klip rounded-rect untuk visual kecil.
fn nacre_ui_clip(comp: &Compositor, w: f32, h: f32, r: f32) -> windows_core::Result<windows::UI::Composition::CompositionClip> {
    let g = comp.CreateRoundedRectangleGeometry()?;
    g.SetSize(Vector2 { X: w, Y: h })?;
    g.SetCornerRadius(Vector2 { X: r, Y: r })?;
    let c = comp.CreateGeometricClip()?;
    c.SetGeometry(&g)?;
    c.cast()
}

/// Glyph font ikon digambar di pojok kiri-atas kotak oleh DrawText; pusatkan secara visual dengan
/// memotong ke kotak pembatas piksel berisi lalu menaruhnya di tengah kanvas semula.
fn center_glyph(b: &Bitmap) -> Option<Bitmap> {
    let (w, h) = (b.w as usize, b.h as usize);
    let (mut minx, mut miny, mut maxx, mut maxy) = (w, h, 0usize, 0usize);
    for y in 0..h {
        for x in 0..w {
            if b.bgra[(y * w + x) * 4 + 3] > 8 {
                minx = minx.min(x);
                maxx = maxx.max(x);
                miny = miny.min(y);
                maxy = maxy.max(y);
            }
        }
    }
    if minx > maxx || miny > maxy {
        return None;
    }
    let (gw, gh) = (maxx - minx + 1, maxy - miny + 1);
    let mut out = vec![0u8; w * h * 4];
    let (ox, oy) = ((w - gw) / 2, (h - gh) / 2);
    for y in 0..gh {
        for x in 0..gw {
            let s = ((miny + y) * w + minx + x) * 4;
            let d = ((oy + y) * w + ox + x) * 4;
            out[d..d + 4].copy_from_slice(&b.bgra[s..s + 4]);
        }
    }
    Some(Bitmap { w: b.w, h: b.h, bgra: out })
}

fn xy(lp: LPARAM) -> (f32, f32) {
    ((lp.0 as i16) as f32, ((lp.0 >> 16) as i16) as f32)
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_LBUTTONDOWN => {
            let (x, y) = xy(lp);
            let started = with_island(hwnd, |d| d.on_down(x, y)).unwrap_or(false);
            if started {
                let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetCapture(hwnd);
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let (mx, _) = xy(lp);
            with_island(hwnd, |d| d.on_drag(mx));
            with_island(hwnd, |d| {
                if !d.tracking {
                    let mut t = TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    if TrackMouseEvent(&mut t).is_ok() {
                        d.tracking = true;
                    }
                }
                d.on_move();
            });
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            log::line("island", "mouse keluar");
            with_island(hwnd, |d| {
                d.tracking = false;
                kill(hwnd, T_HOVEROPEN);
                timer(hwnd, T_CLOSE, 450);
                d.start_grace();
                d.update_visibility();
            });
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let (x, y) = xy(lp);
            let _ = windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
            if with_island(hwnd, |d| d.on_up()).unwrap_or(false) {
                return LRESULT(0);
            }
            let ran = with_island(hwnd, |d| d.on_click(x, y));
            if ran.is_none() {
                log::line("island", "klik diabaikan (sibuk atau belum siap)");
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = (wp.0 >> 16) as i16;
            with_island(hwnd, |d| d.on_wheel(delta));
            LRESULT(0)
        }
        STRIP_ENTER_I => {
            with_island(hwnd, |d| {
                if d.mode != Mode::Always {
                    timer(hwnd, T_DWELL, 200);
                }
            });
            LRESULT(0)
        }
        STRIP_LEAVE_I => {
            with_island(hwnd, |d| {
                kill(hwnd, T_DWELL);
                if d.edge_armed {
                    d.edge_armed = false;
                    d.start_grace();
                    d.update_visibility();
                }
            });
            LRESULT(0)
        }
        WM_TIMER => {
            with_island(hwnd, |d| d.on_timer(wp.0));
            LRESULT(0)
        }
        WM_MEDIA => {
            with_island(hwnd, |d| d.on_media());
            LRESULT(0)
        }
        crate::prefs::WM_PREFS => {
            with_island(hwnd, |d| d.apply_prefs());
            LRESULT(0)
        }
        m if m == WM_APP + 72 => {
            let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            LRESULT(0)
        }
        m if m == WM_APP + 73 => {
            with_island(hwnd, |d| {
                if !d.off {
                    d.open(Panel::Controls);
                }
            });
            LRESULT(0)
        }
        crate::services::WM_RADIO => {
            with_island(hwnd, |d| d.on_radio());
            LRESULT(0)
        }
        crate::services::WM_VOLUME | crate::services::WM_BRIGHT => {
            with_island(hwnd, |d| d.on_system_value());
            LRESULT(0)
        }
        WM_BANDS => {
            with_island(hwnd, |d| d.on_bands());
            LRESULT(0)
        }
        WM_DISPLAYCHANGE | WM_DPICHANGED => {
            timer(hwnd, T_RECONF, 500);
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        0x001A | 0x0218 => {
            with_island(hwnd, |d| d.refresh_tier());
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_DESTROY => {
            ISLAND_HWND.store(0, Ordering::Relaxed);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}







