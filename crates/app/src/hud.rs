use crate::motion;
use crate::popup::Popup;
use crate::services::{self, WM_BRIGHT, WM_VOLUME};
use nacre_core::hud::slider;
use nacre_core::hud::timeline::Timeline;
use nacre_platform::text::{self, Style};
use nacre_platform::{audio, brightness, log, osd, sys};
use nacre_ui::gfx::Gfx;
use nacre_ui::glass::Tier;
use nacre_ui::host::Host;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use windows::core::w;
use windows::UI::Color;
use windows::UI::Composition::{CompositionObject, CompositionStretch, Compositor, SpriteVisual};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows_core::{Interface, HSTRING};
use windows_numerics::{Vector2, Vector3};

const WM_MOUSELEAVE: u32 = 0x02A3;
const WM_DISPLAYCHANGE: u32 = 0x007E;
const HUD_MOUSE: u32 = WM_APP + 30;
const STRIP_L_ENTER: u32 = WM_APP + 31;
const STRIP_L_LEAVE: u32 = WM_APP + 32;
const STRIP_R_ENTER: u32 = WM_APP + 33;
const STRIP_R_LEAVE: u32 = WM_APP + 34;
const HUD_KEY: u32 = WM_APP + 35;

static KEYHOOK: std::sync::Mutex<Option<nacre_platform::hookthread::HookThread>> = std::sync::Mutex::new(None);
static KEY_TARGET: std::sync::Mutex<Option<(f32, Instant)>> = std::sync::Mutex::new(None);

unsafe extern "system" fn vol_proc(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code >= 0 {
        let k = &*(lp.0 as *const KBDLLHOOKSTRUCT);
        let ctl = CTL.load(Ordering::Relaxed);
        if ctl != 0 && ENABLED.load(Ordering::Relaxed) && nacre_core::hud::volkey::is_volume_key(k.vkCode) {
            let down = wp.0 as u32 == WM_KEYDOWN || wp.0 as u32 == WM_SYSKEYDOWN;
            if down {
                let _ = PostMessageW(Some(HWND(ctl as *mut _)), HUD_KEY, WPARAM(k.vkCode as usize), LPARAM(0));
            }
            return LRESULT(1);
        }
    }
    CallNextHookEx(None, code, wp, lp)
}

fn install_volume_keys() {
    if std::env::var_os("NACRE_NO_VOLKEYS").is_some() {
        return;
    }
    let mut g = KEYHOOK.lock().unwrap_or_else(|e| e.into_inner());
    if g.is_none() {
        *g = nacre_platform::hookthread::HookThread::start(nacre_platform::hookthread::Kind::Keyboard, Some(vol_proc));
        log::line("hud", if g.is_some() { "tombol volume diambil alih" } else { "hook tombol volume gagal" });
    }
}

fn on_volume_key(vk: u32) {
    use nacre_core::hud::volkey;
    if vk == volkey::MUTE {
        services::toggle_mute();
        return;
    }
    let up = vk == volkey::UP;
    let mut g = KEY_TARGET.lock().unwrap_or_else(|e| e.into_inner());
    let now = audio::latest();
    let base = match *g {
        Some((v, at)) if at.elapsed().as_millis() < volkey::CHAIN_MS => v,
        _ => now.level,
    };
    let next = volkey::step(base, up);
    *g = Some((next, Instant::now()));
    drop(g);
    if now.muted {
        services::set_muted(false);
    }
    services::set_volume(next);
}
const K_MOVE: usize = 0;
const K_DOWN: usize = 1;
const K_UP: usize = 2;
const K_ENTER: usize = 3;
const K_LEAVE: usize = 4;

const T_HIDE0: usize = 1;
const T_DWELL0: usize = 3;
const T_RECONF: usize = 5;
const T_FIN0: usize = 6;

static ENABLED: AtomicBool = AtomicBool::new(true);

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Volume,
    Brightness,
}

struct Bar {
    kind: Kind,
    popup: Popup,
    title: SpriteVisual,
    title_text: String,
    fill: SpriteVisual,
    time: Timeline,
    shown: bool,
    edge: bool,
    over: bool,
}

pub struct Hud {
    hwnd: HWND,
    comp: Compositor,
    gfx: Gfx,
    t0: Instant,
    dpi: f32,
    bars: Vec<Bar>,
    dragging: Option<usize>,
    last_glyph: [String; 2],
}

fn xy_y(lp: LPARAM) -> f32 {
    (lp.0 as i16) as f32
}

unsafe extern "system" fn bar_proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    let ctl = HWND(CTL.load(Ordering::Relaxed) as *mut _);
    let idx = GetWindowLongPtrW(h, GWLP_USERDATA) as usize;
    let post = |kind: usize, y: f32| {
        let _ = PostMessageW(Some(ctl), HUD_MOUSE, WPARAM(((idx.saturating_sub(1)) << 8) | kind), LPARAM(y as isize));
    };
    match m {
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_MOUSEMOVE if idx > 0 => {
            post(K_MOVE, xy_y(l));
            LRESULT(0)
        }
        WM_MOUSELEAVE if idx > 0 => {
            post(K_LEAVE, 0.0);
            LRESULT(0)
        }
        WM_LBUTTONDOWN if idx > 0 => {
            SetCapture(h);
            post(K_DOWN, xy_y(l));
            LRESULT(0)
        }
        WM_LBUTTONUP if idx > 0 => {
            let _ = ReleaseCapture();
            post(K_UP, xy_y(l));
            LRESULT(0)
        }
        _ => DefWindowProcW(h, m, w, l),
    }
}

static CTL: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

impl Hud {
    pub fn create(host: &Host, glass: Option<Tier>) -> windows_core::Result<HWND> {
        unsafe {
            let hi: HINSTANCE = GetModuleHandleW(None)?.into();
            let ctl_class = w!("NacreHudCtl");
            unsafe extern "system" fn ctl_proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
                match m {
                    HUD_KEY => {
                        on_volume_key(w.0 as u32);
                        LRESULT(0)
                    }
                    WM_VOLUME | WM_BRIGHT | HUD_MOUSE | WM_TIMER | STRIP_L_ENTER | STRIP_L_LEAVE | STRIP_R_ENTER
                    | STRIP_R_LEAVE | WM_DISPLAYCHANGE | crate::prefs::WM_PREFS => {
                        let p = GetWindowLongPtrW(h, GWLP_USERDATA) as *mut Hud;
                        if !p.is_null() {
                            (*p).on_message(m, w, l);
                        }
                        LRESULT(0)
                    }
                    _ => DefWindowProcW(h, m, w, l),
                }
            }
            RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(ctl_proc), hInstance: hi, lpszClassName: ctl_class, ..Default::default() });
            let ctl = CreateWindowExW(WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE, ctl_class, w!("nacre-hud"), WS_POPUP, 0, 0, 0, 0, None, None, Some(hi), None)?;
            CTL.store(ctl.0 as isize, Ordering::Relaxed);
            install_volume_keys();
            let dpi = GetDpiForWindow(ctl).max(96) as f32 / 96.0;
            let screen = sys::screen_size();
            let tier = glass.unwrap_or_else(|| {
                let c = sys::conditions();
                if !c.transparency || c.battery_saver || c.remote_session { Tier::Solid } else { Tier::Full }
            });
            let gfx = Gfx::shared(&host.compositor)?;
            let mut bars = Vec::new();
            for (i, kind) in [Kind::Volume, Kind::Brightness].into_iter().enumerate() {
                let mut popup = Popup::new(
                    &host.compositor,
                    w!("NacreHudBar"),
                    Some(bar_proc),
                    WINDOW_EX_STYLE(0),
                    crate::prefs::glass((10.0, 10.0), 22.0 * dpi, crate::prefs::Role::Dark),
                )?;
                let _ = popup.panel.set_tier(tier);
                SetWindowLongPtrW(popup.hwnd, GWLP_USERDATA, (i + 1) as isize);
                let (bw, bh) = (Self::box_w(dpi) as i32, Self::box_h(dpi) as i32);
                popup.set_rect(screen.0 - bw - (12.0 * dpi) as i32, (34.0 * dpi) as i32, bw, bh)?;
                popup.panel.set_radius(24.0 * dpi)?;
                let tv = host.compositor.CreateSpriteVisual()?;
                tv.SetOffset(Vector3 { X: 18.0 * dpi, Y: 10.0 * dpi, Z: 0.0 })?;
                tv.SetOpacity(0.95)?;
                popup.content.Children()?.InsertAtTop(&tv)?;
                let (track_w, track_h) = (Self::track_w(dpi), 5.0 * dpi);
                let (tx, ty) = (Self::track_x(dpi), Self::track_y(dpi));
                let track = host.compositor.CreateSpriteVisual()?;
                track.SetSize(Vector2 { X: track_w, Y: track_h })?;
                track.SetOffset(Vector3 { X: tx, Y: ty, Z: 0.0 })?;
                track.SetBrush(&host.compositor.CreateColorBrushWithColor(Color { A: 60, R: 255, G: 255, B: 255 })?)?;
                track.SetClip(&Self::clip(&host.compositor, track_w, track_h, track_h / 2.0)?)?;
                popup.content.Children()?.InsertAtTop(&track)?;
                let fill = host.compositor.CreateSpriteVisual()?;
                fill.SetSize(Vector2 { X: 0.0, Y: track_h })?;
                fill.SetOffset(Vector3 { X: tx, Y: ty, Z: 0.0 })?;
                fill.SetBrush(&host.compositor.CreateColorBrushWithColor(Color { A: 245, R: 255, G: 255, B: 255 })?)?;
                fill.SetClip(&Self::clip(&host.compositor, track_w, track_h, track_h / 2.0)?)?;
                popup.content.Children()?.InsertAtTop(&fill)?;
                let dot = (2.4 * dpi).max(2.0);
                for k in 0..16 {
                    let d = host.compositor.CreateSpriteVisual()?;
                    d.SetSize(Vector2 { X: dot, Y: dot })?;
                    d.SetOffset(Vector3 { X: tx + k as f32 * (track_w - dot) / 15.0, Y: ty + track_h + 7.0 * dpi, Z: 0.0 })?;
                    d.SetBrush(&host.compositor.CreateColorBrushWithColor(Color { A: 120, R: 255, G: 255, B: 255 })?)?;
                    d.SetClip(&Self::clip(&host.compositor, dot, dot, dot / 2.0)?)?;
                    popup.content.Children()?.InsertAtTop(&d)?;
                }
                let (sl, sr) = if i == 0 { (nacre_core::symbols::Sym::Speaker, nacre_core::symbols::Sym::SpeakerWave) } else { (nacre_core::symbols::Sym::SunSmall, nacre_core::symbols::Sym::Sun) };
                let isz = (16.0 * dpi).round() as u32;
                let mk = |sym, x: f32| -> windows_core::Result<SpriteVisual> {
                    let data = nacre_core::symbols::render(sym, isz, [255, 255, 255]);
                    let surf = gfx.surface_from_bgra(isz, isz, &data)?;
                    let br = host.compositor.CreateSurfaceBrushWithSurface(&surf)?;
                    br.SetStretch(CompositionStretch::None)?;
                    let v = host.compositor.CreateSpriteVisual()?;
                    v.SetBrush(&br)?;
                    v.SetSize(Vector2 { X: isz as f32, Y: isz as f32 })?;
                    v.SetOffset(Vector3 { X: x, Y: ty + track_h / 2.0 - isz as f32 / 2.0, Z: 0.0 })?;
                    v.SetOpacity(0.9)?;
                    Ok(v)
                };
                let icon = mk(sl, 16.0 * dpi)?;
                let icon2 = mk(sr, bw as f32 - 16.0 * dpi - isz as f32)?;
                popup.content.Children()?.InsertAtTop(&icon)?;
                popup.content.Children()?.InsertAtTop(&icon2)?;
                bars.push(Bar { kind, popup, title: tv, title_text: String::new(), fill, time: Timeline::default(), shown: false, edge: false, over: false });
            }
            let mut hud = Box::new(Hud {
                hwnd: ctl,
                comp: host.compositor.clone(),
                gfx,
                t0: Instant::now(),
                dpi,
                bars,
                dragging: None,
                last_glyph: [String::new(), String::new()],
            });
            hud.update_values(false);
            hud.update_title(0);
            hud.update_title(1);
            SetWindowLongPtrW(ctl, GWLP_USERDATA, Box::into_raw(hud) as isize);
            services::register_hud(ctl);
            crate::prefs::listen(ctl);
            log::line("hud", &format!("dibuat tier={tier:?} dpi={dpi}"));
            Ok(ctl)
        }
    }

    fn box_w(dpi: f32) -> f32 {
        250.0 * dpi
    }
    fn box_h(dpi: f32) -> f32 {
        64.0 * dpi
    }
    fn track_x(dpi: f32) -> f32 {
        44.0 * dpi
    }
    fn track_w(dpi: f32) -> f32 {
        Self::box_w(dpi) - 88.0 * dpi
    }
    fn track_y(dpi: f32) -> f32 {
        40.0 * dpi
    }
    fn clip(c: &Compositor, w: f32, h: f32, r: f32) -> windows_core::Result<windows::UI::Composition::CompositionClip> {
        let g = c.CreateRoundedRectangleGeometry()?;
        g.SetSize(Vector2 { X: w, Y: h })?;
        g.SetCornerRadius(Vector2 { X: r, Y: r })?;
        let cl = c.CreateGeometricClip()?;
        cl.SetGeometry(&g)?;
        cl.cast()
    }

    fn now(&self) -> i64 {
        self.t0.elapsed().as_millis() as i64
    }

    fn value_of(kind: Kind) -> Option<(f32, bool)> {
        match kind {
            Kind::Volume => {
                let v = audio::latest();
                v.available.then_some((v.level, v.muted))
            }
            Kind::Brightness => brightness::latest().map(|b| (b as f32 / 100.0, false)),
        }
    }

    fn spring(&self, obj: &CompositionObject, prop: &str, v: f32) {
        let go = || -> windows_core::Result<()> {
            let a = self.comp.CreateSpringScalarAnimation()?;
            a.SetDampingRatio(0.85)?;
            a.SetPeriod(windows::Foundation::TimeSpan { Duration: 500_000 })?;
            a.SetFinalValue(&windows::Foundation::PropertyValue::CreateSingle(v)?.cast::<windows::Foundation::IReference<f32>>()?)?;
            obj.StartAnimation(&HSTRING::from(prop), &a)
        };
        let _ = go();
    }

    fn update_values(&mut self, animate: bool) {
        for i in 0..self.bars.len() {
            let kind = self.bars[i].kind;
            let Some((level, muted)) = Self::value_of(kind) else { continue };
            let shown_level = if muted { 0.0 } else { level };
            let fill_w = Self::track_w(self.dpi) * shown_level;
            let bar = &self.bars[i];
            if animate {
                let f: CompositionObject = bar.fill.cast().unwrap();
                self.spring(&f, "Size.X", fill_w);
            } else {
                let _ = bar.fill.SetSize(Vector2 { X: fill_w, Y: 5.0 * self.dpi });
            }
            let _ = (level, muted, &self.last_glyph, kind);
        }
    }

    fn update_title(&mut self, i: usize) {
        let t = match self.bars[i].kind {
            Kind::Volume => {
                let n = audio::output_name();
                if n.is_empty() { crate::prefs::tr("Volume").to_string() } else { n }
            }
            Kind::Brightness => crate::prefs::tr("Layar").to_string(),
        };
        if t == self.bars[i].title_text {
            return;
        }
        let d = self.dpi;
        let style = Style { px: (13.0 * d).round() as i32, weight: 600 };
        let (tw, th) = text::measure(&t, &style);
        let tw = (tw + 2).min((Self::box_w(d) - 36.0 * d) as i32);
        let Some(bmp) = text::render(&t, &style, tw, th) else { return };
        let go = || -> windows_core::Result<()> {
            let surf = self.gfx.surface_from_bgra(bmp.w, bmp.h, &bmp.bgra)?;
            let br = self.comp.CreateSurfaceBrushWithSurface(&surf)?;
            br.SetStretch(CompositionStretch::None)?;
            self.bars[i].title.SetBrush(&br)?;
            self.bars[i].title.SetSize(Vector2 { X: bmp.w as f32, Y: bmp.h as f32 })
        };
        if go().is_ok() {
            self.bars[i].title_text = t;
        }
    }

    fn show(&mut self, i: usize) {
        if !ENABLED.load(Ordering::Relaxed) || Self::value_of(self.bars[i].kind).is_none() {
            return;
        }
        self.update_title(i);
        let now = self.now();
        for j in 0..self.bars.len() {
            if j != i && self.bars[j].shown {
                self.bars[j].shown = false;
                self.bars[j].popup.hide();
            }
        }
        self.bars[i].time.poke(now);
        if !self.bars[i].shown {
            self.bars[i].shown = true;
            self.bars[i].popup.show();
            unsafe {
                let _ = KillTimer(Some(self.hwnd), T_FIN0 + i);
            }
            let center = Vector3 { X: Self::box_w(self.dpi) - 28.0 * self.dpi, Y: 0.0, Z: 0.0 };
            let _ = motion::pop_in(&self.comp, self.bars[i].popup.root(), center, 0.72, 1.035, 0, 380);
        }
        self.schedule_hide(i);
    }

    fn schedule_hide(&self, i: usize) {
        if let Some(ms) = self.bars[i].time.remaining(self.now()) {
            unsafe {
                SetTimer(Some(self.hwnd), T_HIDE0 + i, (ms + 30) as u32, None);
            }
        }
    }

    fn hide_if_idle(&mut self, i: usize) {
        let now = self.now();
        if self.bars[i].shown && !self.bars[i].time.visible(now) {
            self.bars[i].shown = false;
            let center = Vector3 { X: Self::box_w(self.dpi) - 28.0 * self.dpi, Y: 0.0, Z: 0.0 };
            let _ = motion::pop_out(&self.comp, self.bars[i].popup.root(), center, 0.8, motion::CC_CLOSE_MS);
            unsafe {
                SetTimer(Some(self.hwnd), T_FIN0 + i, motion::CC_CLOSE_MS + 40, None);
            }
        } else {
            self.schedule_hide(i);
        }
    }

    fn hold(&mut self, i: usize) {
        let held = self.dragging == Some(i) || self.bars[i].over || self.bars[i].edge;
        let now = self.now();
        self.bars[i].time.hold(held, now);
        if !held {
            self.schedule_hide(i);
        }
    }

    fn value_from_y(&self, x: f32) -> f32 {
        slider::value_at(x, Self::track_x(self.dpi), Self::track_w(self.dpi), false)
    }

    fn apply_drag(&self, i: usize, y: f32) {
        let v = self.value_from_y(y);
        match self.bars[i].kind {
            Kind::Volume => services::set_volume(v),
            Kind::Brightness => services::set_brightness(v),
        }
    }

    fn on_message(&mut self, m: u32, w: WPARAM, l: LPARAM) {
        match m {
            WM_VOLUME | WM_BRIGHT => {
                if ENABLED.load(Ordering::Relaxed) {
                    osd::hide_native();
                }
                self.update_values(true);
                if crate::controlcenter::is_open() {
                    if crate::controlcenter::adjusting() {
                        return;
                    }
                    crate::controlcenter::close();
                }
                self.show(if m == WM_VOLUME { 0 } else { 1 });
            }
            HUD_MOUSE => {
                let (i, kind) = (w.0 >> 8, w.0 & 0xFF);
                let y = l.0 as f32;
                if i >= self.bars.len() {
                    return;
                }
                match kind {
                    K_MOVE => {
                        if !self.bars[i].over {
                            self.bars[i].over = true;
                            let mut t = TRACKMOUSEEVENT {
                                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                                dwFlags: TME_LEAVE,
                                hwndTrack: self.bars[i].popup.hwnd,
                                dwHoverTime: 0,
                            };
                            unsafe {
                                let _ = TrackMouseEvent(&mut t);
                            }
                            self.hold(i);
                        }
                        if self.dragging == Some(i) {
                            self.apply_drag(i, y);
                        }
                    }
                    K_DOWN => {
                        self.dragging = Some(i);
                        self.hold(i);
                        self.apply_drag(i, y);
                    }
                    K_UP => {
                        self.dragging = None;
                        self.hold(i);
                    }
                    K_ENTER => {}
                    K_LEAVE => {
                        self.bars[i].over = false;
                        self.hold(i);
                    }
                    _ => {}
                }
            }
            STRIP_L_ENTER | STRIP_R_ENTER => {
                let i = if m == STRIP_L_ENTER { 0 } else { 1 };
                unsafe {
                    SetTimer(Some(self.hwnd), T_DWELL0 + i, 200, None);
                }
            }
            STRIP_L_LEAVE | STRIP_R_LEAVE => {
                let i = if m == STRIP_L_LEAVE { 0 } else { 1 };
                unsafe {
                    let _ = KillTimer(Some(self.hwnd), T_DWELL0 + i);
                }
                self.bars[i].edge = false;
                self.hold(i);
            }
            WM_TIMER => {
                let id = w.0;
                unsafe {
                    let _ = KillTimer(Some(self.hwnd), id);
                }
                match id {
                    T_HIDE0 | 2 => self.hide_if_idle(id - T_HIDE0),
                    T_FIN0 | 7 => {
                        let i = id - T_FIN0;
                        if i < self.bars.len() && !self.bars[i].shown {
                            self.bars[i].popup.hide();
                        }
                    }
                    T_DWELL0 | 4 => {
                        let i = id - T_DWELL0;
                        self.bars[i].edge = true;
                        self.update_values(false);
                        self.show(i);
                        self.hold(i);
                    }
                    T_RECONF => {}
                    _ => {}
                }
            }
            WM_DISPLAYCHANGE => {}
            crate::prefs::WM_PREFS => {
                let p = crate::prefs::glass((10.0, 10.0), 22.0 * self.dpi, crate::prefs::Role::Dark);
                for b in self.bars.iter_mut() {
                    let _ = b.popup.panel.set_params(p);
                }
            }
            _ => {}
        }
    }
}
