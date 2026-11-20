use crate::prefs;
use nacre_platform::wins::park::{self, Parked};
use nacre_platform::wins::{enumerate, focus, snapshot, tracker};
use nacre_platform::log;
use nacre_ui::gfx::Gfx;
use nacre_ui::host::{attach, Host, Surface};
use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Mutex;
use windows::core::w;
use windows::Foundation::TimeSpan;
use windows::UI::Composition::{
    AnimationDelayBehavior, CompositionDrawingSurface, CompositionEasingFunction, CompositionObject, CompositionStretch, Compositor, ContainerVisual, SpriteVisual,
};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows_core::{Interface, HSTRING};
use windows_numerics::{Matrix3x2, Vector2, Vector3};

const WM_FX: u32 = WM_APP + 60;
const T_CAPTURE: usize = 1;
const T_REFRESH: usize = 2;
const T_HIDE: usize = 3;
const T_RESTORE: usize = 4;

static FX_HWND: AtomicIsize = AtomicIsize::new(0);
static FLAGGED: Mutex<Vec<isize>> = Mutex::new(Vec::new());
static OPEN_PARKED: Mutex<Option<Parked>> = Mutex::new(None);
const T_OPEN_POLL: usize = 7;
const T_OPEN_END: usize = 8;
const OPEN_WAIT_MS: u128 = 320;

struct Opening {
    hwnd: isize,
    target: (i32, i32, i32, i32),
    since: std::time::Instant,
    playing: bool,
}

fn open_file() -> std::path::PathBuf {
    crate::paths::config_dir().join("opening.txt")
}

fn hold_parked(p: Option<Parked>) {
    let mut g = OPEN_PARKED.lock().unwrap_or_else(|e| e.into_inner());
    *g = p;
    match p {
        Some(p) => park::save(&open_file(), &[p]),
        None => {
            let _ = std::fs::remove_file(open_file());
        }
    }
}

fn release_parked() {
    let p = OPEN_PARKED.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some(p) = p {
        if park::is_parked(p.hwnd) {
            park::unpark(&p);
        }
    }
    let _ = std::fs::remove_file(open_file());
}

#[derive(Clone)]
struct Snap {
    hwnd: isize,
    rect: (i32, i32, i32, i32),
    surface: CompositionDrawingSurface,
}

struct Fx {
    hwnd: HWND,
    comp: Compositor,
    gfx: Gfx,
    surface: Surface,
    screen_origin: (i32, i32),
    screen: (i32, i32),
    snap: Option<Snap>,
    parked: Vec<Snap>,
    current: isize,
    sprite: Option<SpriteVisual>,
    genie: Option<ContainerVisual>,
    pending_restore: Option<(isize, isize)>,
    seen: HashSet<isize>,
    opened: HashSet<isize>,
    opening: Option<Opening>,
    shadow: Option<ShadowWin>,
    shadow_target: isize,
    shadow_key: (i32, i32, i32),
    shadow_rect: (i32, i32, i32, i32),
}

struct ShadowWin {
    hwnd: HWND,
    surface: Surface,
    sprite: SpriteVisual,
}

const T_SHADOW: usize = 5;
const T_SHADOW_WATCH: usize = 6;
const SHADOW_MARGIN: i32 = 44;

thread_local! {
    static FX: RefCell<Option<Fx>> = const { RefCell::new(None) };
}

fn with_fx<R>(f: impl FnOnce(&mut Fx) -> R) -> Option<R> {
    FX.with(|c| c.try_borrow_mut().ok().and_then(|mut b| b.as_mut().map(f)))
}

fn enabled() -> bool {
    prefs::get().anim
}

fn genie_on() -> bool {
    prefs::get().min_effect == 0
}

const START_DELAY: u32 = 40;

fn noop() {}

fn hook(event: u32, hwnd: isize) {
    let h = FX_HWND.load(Ordering::Relaxed);
    if h != 0 {
        unsafe {
            let _ = PostMessageW(Some(HWND(h as *mut _)), WM_FX, WPARAM(event as usize), LPARAM(hwnd));
        }
    }
}

pub fn init(host: &Host) -> windows_core::Result<()> {
    let n = park::restore_file(&open_file());
    if n > 0 {
        log::line("fx", &format!("{n} jendela yang sedang dibuka sesi lalu dikembalikan"));
    }
    unsafe {
        let hi: HINSTANCE = GetModuleHandleW(None)?.into();
        let class = w!("AbraFx");
        RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(proc), hInstance: hi, lpszClassName: class, ..Default::default() });
        let bounds = crate::display::virtual_bounds().unwrap_or_else(|| {
            let (w, h) = nacre_platform::sys::screen_size();
            windows::Win32::Foundation::RECT { left: 0, top: 0, right: w, bottom: h }
        });
        let screen_origin = (bounds.left, bounds.top);
        let screen = (bounds.right - bounds.left, bounds.bottom - bounds.top);
        let hwnd = CreateWindowExW(
            WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_LAYERED,
            class,
            w!("abra-fx"),
            WS_POPUP,
            screen_origin.0, screen_origin.1, screen.0, screen.1,
            None, None, Some(hi), None,
        )?;
            let _ = SetLayeredWindowAttributes(hwnd, windows::Win32::Foundation::COLORREF(0), 255, LWA_ALPHA);
        let surface = attach(&host.compositor, hwnd, (screen.0 as f32, screen.1 as f32))?;
        let gfx = Gfx::shared(&host.compositor)?;
        FX_HWND.store(hwnd.0 as isize, Ordering::Relaxed);
        FX.with(|c| {
            *c.borrow_mut() = Some(Fx {
                hwnd,
                comp: host.compositor.clone(),
                gfx,
                surface,
                screen_origin,
                screen,
                snap: None,
                parked: Vec::new(),
                current: 0,
                sprite: None,
                genie: None,
                pending_restore: None,
                seen: HashSet::new(),
                opened: HashSet::new(),
                opening: None,
                shadow: None,
                shadow_target: 0,
                shadow_key: (0, 0, 0),
                shadow_rect: (0, 0, 0, 0),
            })
        });
        prefs::listen(hwnd);
        tracker::install(noop);
        tracker::install_fx(hook);
        if enabled() {
            SetTimer(Some(hwnd), T_REFRESH, 4000, None);
            let fg = focus::foreground();
            if eligible(fg) {
                flag(fg);
                with_fx(|fx| fx.current = fg);
                SetTimer(Some(hwnd), T_CAPTURE, 600, None);
            }
        }
        {
            let fg = focus::foreground();
            if eligible(fg) {
                with_fx(|fx| fx.update_shadow(fg, true));
            }
        }
        log::line("fx", "siap");
    }
    Ok(())
}

pub fn restore_all() {
    release_parked();
    let list: Vec<isize> = std::mem::take(&mut *FLAGGED.lock().unwrap_or_else(|e| e.into_inner()));
    for h in list {
        snapshot::set_transitions_disabled(h, false);
    }
}

fn shadow_bitmap(w: i32, h: i32, m: i32, dy: f32, radius: f32, k: i32) -> (u32, u32, Vec<u8>) {
    let (fw, fh) = (w + 2 * m, h + 2 * m);
    let (bw, bh) = (((fw / k).max(2)) as u32, ((fh / k).max(2)) as u32);
    let (hx, hy) = (w as f32 / 2.0, h as f32 / 2.0);
    let (cx, cy) = (fw as f32 / 2.0, fh as f32 / 2.0 + dy);
    let r = radius.min(hx).min(hy);
    let blur = 30.0f32;
    let peak = 0.26f32;
    let mut out = vec![0u8; (bw * bh * 4) as usize];
    for y in 0..bh {
        for x in 0..bw {
            let (px, py) = ((x as f32 + 0.5) * k as f32, (y as f32 + 0.5) * k as f32);
            let (qx, qy) = ((px - cx).abs() - (hx - r), (py - cy).abs() - (hy - r));
            let d = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r;
            let t = (d / blur).clamp(0.0, 1.0);
            let a = peak * (1.0 - t) * (1.0 - t) * (1.0 - t);
            let i = ((y * bw + x) * 4) as usize;
            out[i + 3] = (a * 255.0).round() as u8;
        }
    }
    (bw, bh, out)
}

fn flag(h: isize) {
    let mut g = FLAGGED.lock().unwrap_or_else(|e| e.into_inner());
    if !g.contains(&h) {
        if snapshot::set_transitions_disabled(h, true) {
            g.push(h);
            log::line("fx", &format!("transisi DWM dimatikan hwnd={h}"));
        }
        if g.len() > 400 {
            g.retain(|x| focus::is_alive(*x));
        }
    }
}

fn openable(h: isize) -> bool {
    if !eligible(h) || enumerate::cloaked(h) || park::is_parked(h) {
        return false;
    }
    unsafe {
        let hw = HWND(h as *mut _);
        if GetAncestor(hw, GA_ROOT) != hw || GetWindow(hw, GW_OWNER).is_ok_and(|o| !o.0.is_null()) {
            return false;
        }
        if IsZoomed(hw).as_bool() || IsIconic(hw).as_bool() {
            return false;
        }
    }
    match snapshot::frame_rect(h) {
        Some((x, y, w, hh)) => {
            let monitor = crate::display::monitor_bounds_for_window(h);
            let fills_monitor = monitor.is_some_and(|r| {
                x <= r.left + 2 && y <= r.top + 2 && w >= r.right - r.left - 4 && hh >= r.bottom - r.top - 4
            });
            w >= 120 && hh >= 80 && !fills_monitor && x > -2000 && y > -2000
        }
        None => false,
    }
}

fn eligible(h: isize) -> bool {
    unsafe {
        let hw = HWND(h as *mut _);
        if !IsWindow(Some(hw)).as_bool() || !IsWindowVisible(hw).as_bool() {
            return false;
        }
        let style = GetWindowLongPtrW(hw, GWL_STYLE) as u32;
        let ex = GetWindowLongPtrW(hw, GWL_EXSTYLE) as u32;
        if style & WS_CAPTION.0 != WS_CAPTION.0 || ex & WS_EX_TOOLWINDOW.0 != 0 {
            return false;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hw, Some(&mut pid));
        pid != std::process::id()
    }
}

pub fn restore(target: isize, icon: (i32, i32, i32, i32), dock: isize) -> bool {
    if !enabled() {
        return false;
    }
    with_fx(|fx| {
        if fx.pending_restore.is_some() {
            return false;
        }
        let Some(pos) = fx.parked.iter().position(|p| p.hwnd == target) else { return false };
        let s = fx.parked.remove(pos);
        let (x, y, w, h) = s.rect;
        fx.play(s.surface.clone(), icon, (x, y, w, h), Kind::Restore);
        fx.snap = Some(s);
        fx.pending_restore = Some((target, dock));
        unsafe {
            SetTimer(Some(fx.hwnd), T_RESTORE, START_DELAY + Kind::Restore.ms() - 30, None);
        }
        true
    })
    .unwrap_or(false)
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Minimize,
    Restore,
    Close,
    Open,
}

impl Kind {
    fn ms(self) -> u32 {
        let g = genie_on();
        match self {
            Kind::Minimize => if g { 540 } else { 380 },
            Kind::Restore => if g { 470 } else { 340 },
            Kind::Close => 270,
            Kind::Open => 300,
        }
    }
}

fn span(ms: u32) -> TimeSpan {
    TimeSpan { Duration: ms as i64 * 10_000 }
}

impl Fx {
    fn ease(&self, a: (f32, f32), b: (f32, f32)) -> windows_core::Result<CompositionEasingFunction> {
        self.comp.CreateCubicBezierEasingFunction(Vector2 { X: a.0, Y: a.1 }, Vector2 { X: b.0, Y: b.1 })?.cast()
    }

    fn vec3(&self, obj: &CompositionObject, prop: &str, from: Vector3, frames: &[(f32, Vector3, (f32, f32), (f32, f32))], ms: u32) -> windows_core::Result<()> {
        let a = self.comp.CreateVector3KeyFrameAnimation()?;
        a.InsertKeyFrame(0.0, from)?;
        for (t, v, c1, c2) in frames {
            a.InsertKeyFrameWithEasingFunction(*t, *v, &self.ease(*c1, *c2)?)?;
        }
        a.SetDuration(span(ms))?;
        a.SetDelayTime(span(START_DELAY))?;
        a.SetDelayBehavior(AnimationDelayBehavior::SetInitialValueBeforeDelay)?;
        obj.StartAnimation(&HSTRING::from(prop), &a)
    }

    fn scalar(&self, obj: &CompositionObject, prop: &str, frames: &[(f32, f32)], ms: u32) -> windows_core::Result<()> {
        let a = self.comp.CreateScalarKeyFrameAnimation()?;
        for (t, v) in frames {
            a.InsertKeyFrame(*t, *v)?;
        }
        a.SetDuration(span(ms))?;
        a.SetDelayTime(span(START_DELAY))?;
        a.SetDelayBehavior(AnimationDelayBehavior::SetInitialValueBeforeDelay)?;
        obj.StartAnimation(&HSTRING::from(prop), &a)
    }

    fn play(&mut self, surface: CompositionDrawingSurface, a: (i32, i32, i32, i32), b: (i32, i32, i32, i32), kind: Kind) {
        let localize = |(x, y, w, h): (i32, i32, i32, i32)| {
            (x - self.screen_origin.0, y - self.screen_origin.1, w, h)
        };
        let (a, b) = (localize(a), localize(b));
        if matches!(kind, Kind::Minimize | Kind::Restore) && genie_on() {
            let (win, icon) = if kind == Kind::Restore { (b, a) } else { (a, b) };
            match self.play_genie(surface, win, icon, kind == Kind::Restore) {
                Ok(()) => {
                    log::line("fx", &format!("animasi {} genie ({} ms) jendela {win:?} ikon {icon:?}", if kind == Kind::Restore { "restore" } else { "minimize" }, kind.ms()));
                    unsafe {
                        let _ = SetWindowPos(self.hwnd, Some(HWND_TOPMOST), self.screen_origin.0, self.screen_origin.1, self.screen.0, self.screen.1, SWP_NOACTIVATE);
                        let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                        SetTimer(Some(self.hwnd), T_HIDE, START_DELAY + kind.ms() + 80, None);
                    }
                }
                Err(e) => log::line("fx", &format!("genie gagal: {e}")),
            }
            return;
        }
        let mut go = || -> windows_core::Result<()> {
            let (win, icon) = match kind {
                Kind::Restore => (b, a),
                _ => (a, b),
            };
            let (wx, wy, ww, wh) = (win.0 as f32, win.1 as f32, win.2.max(1) as f32, win.3.max(1) as f32);
            let (ix, iy, iw, ih) = (icon.0 as f32, icon.1 as f32, icon.2.max(1) as f32, icon.3.max(1) as f32);
            if let Some(old) = self.sprite.take() {
                let _ = self.surface.root.Children()?.Remove(&old);
            }
            let sprite = self.comp.CreateSpriteVisual()?;
            let brush = self.comp.CreateSurfaceBrushWithSurface(&surface)?;
            brush.SetStretch(CompositionStretch::Fill)?;
            sprite.SetBrush(&brush)?;
            sprite.SetSize(Vector2 { X: ww, Y: wh })?;
            sprite.SetCenterPoint(Vector3 { X: ww / 2.0, Y: wh / 2.0, Z: 0.0 })?;
            self.surface.root.Children()?.InsertAtTop(&sprite)?;
            let obj: CompositionObject = sprite.cast()?;
            let at_win = Vector3 { X: wx, Y: wy, Z: 0.0 };
            let at_icon = Vector3 { X: ix + iw / 2.0 - ww / 2.0, Y: iy + ih / 2.0 - wh / 2.0, Z: 0.0 };
            let (sx, sy) = (iw / ww, ih / wh);
            let one = Vector3 { X: 1.0, Y: 1.0, Z: 1.0 };
            let ms = kind.ms();
            match kind {
                Kind::Minimize => {
                    sprite.SetOffset(at_win)?;
                    sprite.SetScale(one)?;
                    self.vec3(&obj, "Offset", at_win, &[(1.0, at_icon, (0.45, 0.0), (0.2, 1.0))], ms)?;
                    self.vec3(
                        &obj,
                        "Scale",
                        one,
                        &[
                            (0.55, Vector3 { X: 0.82, Y: 0.35, Z: 1.0 }, (0.3, 0.0), (0.5, 1.0)),
                            (1.0, Vector3 { X: sx, Y: sy, Z: 1.0 }, (0.5, 0.0), (0.2, 1.0)),
                        ],
                        ms,
                    )?;
                    self.scalar(&obj, "Opacity", &[(0.0, 1.0), (0.82, 1.0), (1.0, 0.0)], ms)?;
                }
                Kind::Restore => {
                    let from = Vector3 { X: sx, Y: sy, Z: 1.0 };
                    sprite.SetOffset(at_icon)?;
                    sprite.SetScale(from)?;
                    self.vec3(&obj, "Offset", at_icon, &[(1.0, at_win, (0.2, 0.8), (0.2, 1.0))], ms)?;
                    self.vec3(
                        &obj,
                        "Scale",
                        from,
                        &[
                            (0.45, Vector3 { X: 0.82, Y: 0.35, Z: 1.0 }, (0.2, 0.7), (0.3, 1.0)),
                            (1.0, one, (0.2, 0.8), (0.2, 1.0)),
                        ],
                        ms,
                    )?;
                    self.scalar(&obj, "Opacity", &[(0.0, 0.0), (0.12, 1.0), (1.0, 1.0)], ms)?;
                }
                Kind::Close => {
                    sprite.SetOffset(at_win)?;
                    sprite.SetScale(one)?;
                    self.vec3(&obj, "Scale", one, &[(1.0, Vector3 { X: 0.06, Y: 0.06, Z: 1.0 }, (0.33, 0.55), (0.45, 1.0))], ms)?;
                    self.scalar(&obj, "Opacity", &[(0.0, 1.0), (0.55, 0.92), (1.0, 0.0)], ms)?;
                }
                Kind::Open => {
                    let from = Vector3 { X: 0.86, Y: 0.86, Z: 1.0 };
                    sprite.SetOffset(at_win)?;
                    sprite.SetScale(from)?;
                    sprite.SetOpacity(0.0)?;
                    self.vec3(&obj, "Scale", from, &[(1.0, one, (0.16, 1.0), (0.3, 1.0))], ms)?;
                    self.scalar(&obj, "Opacity", &[(0.0, 0.0), (0.45, 1.0), (1.0, 1.0)], ms)?;
                }
            }
            self.sprite = Some(sprite);
            log::line("fx", &format!("animasi {} ({ms} ms) dari {a:?} ke {b:?}", match kind { Kind::Minimize => "minimize", Kind::Restore => "restore", Kind::Close => "tutup", Kind::Open => "buka" }));
            unsafe {
                let _ = SetWindowPos(self.hwnd, Some(HWND_TOPMOST), self.screen_origin.0, self.screen_origin.1, self.screen.0, self.screen.1, SWP_NOACTIVATE);
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                SetTimer(Some(self.hwnd), T_HIDE, START_DELAY + ms + 80, None);
            }
            Ok(())
        };
        if let Err(e) = go() {
            log::line("fx", &format!("animasi gagal: {e}"));
        }
    }

    fn play_genie(&mut self, surface: CompositionDrawingSurface, win: (i32, i32, i32, i32), icon: (i32, i32, i32, i32), reverse: bool) -> windows_core::Result<()> {
        use nacre_core::genie::{self, Rect as GRect};
        if let Some(old) = self.genie.take() {
            let _ = self.surface.root.Children()?.Remove(&old);
        }
        if let Some(old) = self.sprite.take() {
            let _ = self.surface.root.Children()?.Remove(&old);
        }
        let w = GRect { x: win.0 as f32, y: win.1 as f32, w: win.2.max(1) as f32, h: win.3.max(1) as f32 };
        let ic = GRect { x: icon.0 as f32, y: icon.1 as f32, w: icon.2.max(1) as f32, h: icon.3.max(1) as f32 };
        let n = ((w.h / 5.0) as usize).clamp(48, 120);
        let sh = w.h / n as f32;
        let ms = if reverse { Kind::Restore.ms() } else { Kind::Minimize.ms() };
        let samples = ((ms / 12) as usize).clamp(24, 48);
        let root = self.comp.CreateContainerVisual()?;
        self.surface.root.Children()?.InsertAtTop(&root)?;
        let (dur, delay) = (span(ms), span(START_DELAY));
        let v3 = |p: (f32, f32), z: f32| Vector3 { X: p.0, Y: p.1, Z: z };
        for i in 0..n {
            let s = self.comp.CreateSpriteVisual()?;
            let b = self.comp.CreateSurfaceBrushWithSurface(&surface)?;
            b.SetStretch(CompositionStretch::Fill)?;
            b.SetTransformMatrix(Matrix3x2 { M11: 1.0, M12: 0.0, M21: 0.0, M22: n as f32, M31: 0.0, M32: -(i as f32) * sh })?;
            s.SetBrush(&b)?;
            s.SetSize(Vector2 { X: w.w, Y: sh })?;
            s.SetCenterPoint(Vector3 { X: 0.0, Y: 0.0, Z: 0.0 })?;
            let (o0, s0) = genie::slice(i, n, genie::tau_at(0.0, reverse), w, ic);
            s.SetOffset(v3(o0, 0.0))?;
            s.SetScale(v3(s0, 1.0))?;
            root.Children()?.InsertAtTop(&s)?;

            let ao = self.comp.CreateVector3KeyFrameAnimation()?;
            let asc = self.comp.CreateVector3KeyFrameAnimation()?;
            for k in 0..samples {
                let t = k as f32 / (samples - 1) as f32;
                let (o, sc) = genie::slice(i, n, genie::tau_at(t, reverse), w, ic);
                ao.InsertKeyFrame(t, v3(o, 0.0))?;
                asc.InsertKeyFrame(t, v3(sc, 1.0))?;
            }
            for a in [&ao, &asc] {
                a.SetDuration(dur)?;
                a.SetDelayTime(delay)?;
                a.SetDelayBehavior(AnimationDelayBehavior::SetInitialValueBeforeDelay)?;
            }
            let obj: CompositionObject = s.cast()?;
            obj.StartAnimation(&HSTRING::from("Offset"), &ao)?;
            obj.StartAnimation(&HSTRING::from("Scale"), &asc)?;
        }
        let fade = self.comp.CreateScalarKeyFrameAnimation()?;
        if reverse {
            fade.InsertKeyFrame(0.0, 0.0)?;
            fade.InsertKeyFrame(0.08, 1.0)?;
            fade.InsertKeyFrame(1.0, 1.0)?;
        } else {
            fade.InsertKeyFrame(0.0, 1.0)?;
            fade.InsertKeyFrame(0.88, 1.0)?;
            fade.InsertKeyFrame(1.0, 0.0)?;
        }
        fade.SetDuration(dur)?;
        fade.SetDelayTime(delay)?;
        fade.SetDelayBehavior(AnimationDelayBehavior::SetInitialValueBeforeDelay)?;
        root.SetOpacity(if reverse { 0.0 } else { 1.0 })?;
        root.cast::<CompositionObject>()?.StartAnimation(&HSTRING::from("Opacity"), &fade)?;
        self.genie = Some(root);
        Ok(())
    }

    fn begin_open(&mut self, h: isize) {
        if self.opened.contains(&h) || self.opening.as_ref().is_some_and(|o| o.hwnd == h) {
            return;
        }
        if !openable(h) {
            return;
        }
        self.opened.insert(h);
        if self.opened.len() > 600 {
            self.opened.retain(|x| focus::is_alive(*x));
        }
        self.end_open();
        let Some(target) = snapshot::frame_rect(h) else { return };
        flag(h);
        let Some(p) = park::park(h) else { return };
        if !park::is_parked(h) {
            park::unpark(&p);
            return;
        }
        hold_parked(Some(p));
        self.opening = Some(Opening { hwnd: h, target, since: std::time::Instant::now(), playing: false });
        unsafe {
            SetTimer(Some(self.hwnd), T_OPEN_POLL, 16, None);
        }
    }

    fn poll_open(&mut self) {
        let Some((h, target, waited, playing)) = self.opening.as_ref().map(|o| (o.hwnd, o.target, o.since.elapsed().as_millis(), o.playing)) else {
            unsafe {
                let _ = KillTimer(Some(self.hwnd), T_OPEN_POLL);
            }
            return;
        };
        if playing {
            return;
        }
        if !focus::is_alive(h) || !park::is_parked(h) {
            self.opening = None;
            hold_parked(None);
            unsafe {
                let _ = KillTimer(Some(self.hwnd), T_OPEN_POLL);
            }
            return;
        }
        let shot = snapshot::capture(h, 1).filter(|s| !crate::stage::blank(&s.bmp));
        match shot {
            Some(shot) => {
                unsafe {
                    let _ = KillTimer(Some(self.hwnd), T_OPEN_POLL);
                }
                match self.gfx.surface_from_bgra(shot.bmp.w, shot.bmp.h, &shot.bmp.bgra) {
                    Ok(surface) => {
                        if let Some(o) = self.opening.as_mut() {
                            o.playing = true;
                        }
                        self.play(surface.clone(), target, target, Kind::Open);
                        self.snap = Some(Snap { hwnd: h, rect: target, surface });
                        unsafe {
                            SetTimer(Some(self.hwnd), T_OPEN_END, START_DELAY + Kind::Open.ms() - 16, None);
                        }
                    }
                    Err(_) => self.end_open(),
                }
            }
            None if waited > OPEN_WAIT_MS => self.end_open(),
            None => {}
        }
    }

    fn end_open(&mut self) {
        unsafe {
            let _ = KillTimer(Some(self.hwnd), T_OPEN_POLL);
            let _ = KillTimer(Some(self.hwnd), T_OPEN_END);
        }
        if self.opening.take().is_some() {
            release_parked();
        }
    }

    fn capture_current(&mut self) {
        let h = self.current;
        if h == 0 || !enabled() || !eligible(h) || focus::is_minimized(h) || park::is_parked(h) {
            return;
        }
        let t0 = std::time::Instant::now();
        let Some(shot) = snapshot::capture(h, 1) else { return };
        let t_cap = t0.elapsed();
        if let Ok(surface) = self.gfx.surface_from_bgra(shot.bmp.w, shot.bmp.h, &shot.bmp.bgra) {
            log::line("fx", &format!("tangkap hwnd={h} {}x{} (px {}x{}) ambil={}ms unggah={}ms", shot.rect.2, shot.rect.3, shot.bmp.w, shot.bmp.h, t_cap.as_millis(), (t0.elapsed() - t_cap).as_millis()));
            self.snap = Some(Snap { hwnd: h, rect: shot.rect, surface });
        }
    }

    fn icon_rect(&self, hwnd: isize) -> (i32, i32, i32, i32) {
        crate::dock::icon_rect_for(hwnd).unwrap_or_else(|| {
            let bounds = crate::display::monitor_bounds_for_window(hwnd).unwrap_or(windows::Win32::Foundation::RECT {
                left: self.screen_origin.0,
                top: self.screen_origin.1,
                right: self.screen_origin.0 + self.screen.0,
                bottom: self.screen_origin.1 + self.screen.1,
            });
            ((bounds.left + bounds.right) / 2 - 24, bounds.bottom - 70, 48, 48)
        })
    }

    fn on_event(&mut self, event: u32, h: isize) {
        self.on_look_event(event, h);
        if !enabled() {
            return;
        }
        match event {
            tracker::EV_SHOW | tracker::EV_UNCLOAKED => self.begin_open(h),
            tracker::EV_FOREGROUND => {
                if eligible(h) {
                    flag(h);
                    self.seen.insert(h);
                    self.current = h;
                    unsafe {
                        SetTimer(Some(self.hwnd), T_CAPTURE, 350, None);
                    }
                }
            }
            tracker::EV_MOVESIZE_END if h == self.current => unsafe {
                SetTimer(Some(self.hwnd), T_CAPTURE, 350, None);
            },
            tracker::EV_MINIMIZE_START => {
                if let Some(s) = &self.snap {
                    if s.hwnd == h && self.pending_restore.is_none() {
                        let (surface, rect) = (s.surface.clone(), s.rect);
                        crate::dock::reveal_for(h, START_DELAY + Kind::Minimize.ms() + 450);
                        let icon = self.icon_rect(h);
                        let keep = s.clone();
                        self.play(surface, rect, icon, Kind::Minimize);
                        self.parked.retain(|p| p.hwnd != h);
                        self.parked.push(keep);
                        if self.parked.len() > 2 {
                            self.parked.remove(0);
                        }
                    }
                }
            }
            tracker::EV_DESTROY => {
                self.parked.retain(|p| p.hwnd != h);
                self.opened.remove(&h);
                if self.opening.as_ref().is_some_and(|o| o.hwnd == h) {
                    self.opening = None;
                    hold_parked(None);
                }
                if let Some(s) = &self.snap {
                    if s.hwnd == h && !focus::is_alive(h) {
                        let (surface, rect) = (s.surface.clone(), s.rect);
                        self.play(surface, rect, rect, Kind::Close);
                        self.snap = None;
                        if self.current == h {
                            self.current = 0;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn hide_shadow(&mut self) {
        self.shadow_target = 0;
        unsafe {
            let _ = KillTimer(Some(self.hwnd), T_SHADOW_WATCH);
        }
        if let Some(s) = &self.shadow {
            unsafe {
                let _ = ShowWindow(s.hwnd, SW_HIDE);
            }
        }
    }

    fn watch_shadow(&mut self) {
        let t = self.shadow_target;
        if t == 0 {
            self.hide_shadow();
            return;
        }
        let fg = focus::foreground();
        if fg != t {
            if eligible(fg) {
                self.update_shadow(fg, true);
            } else {
                self.hide_shadow();
            }
            return;
        }
        let gone = unsafe { IsIconic(HWND(t as *mut _)).as_bool() } || enumerate::cloaked(t);
        if gone {
            self.hide_shadow();
            return;
        }
        if snapshot::frame_rect(t) != Some(self.shadow_rect) {
            self.update_shadow(t, true);
        }
    }

    fn on_look_event(&mut self, event: u32, h: isize) {
        match event {
            tracker::EV_FOREGROUND => {
                if eligible(h) {
                    self.update_shadow(h, true);
                } else {
                    self.hide_shadow();
                }
            }
            tracker::EV_LOCATION if h == self.shadow_target => {
                self.update_shadow(h, false);
                unsafe {
                    SetTimer(Some(self.hwnd), T_SHADOW, 120, None);
                }
            }
            tracker::EV_MOVESIZE_END if h == self.shadow_target => {
                self.update_shadow(h, true);
            }
            tracker::EV_MINIMIZE_START | tracker::EV_DESTROY if h == self.shadow_target => self.hide_shadow(),
            _ => {}
        }
    }

    fn ensure_shadow_window(&mut self) -> windows_core::Result<()> {
        if self.shadow.is_some() {
            return Ok(());
        }
        unsafe {
            let hi: HINSTANCE = GetModuleHandleW(None)?.into();
            let class = w!("AbraShadow");
            RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(shadow_proc), hInstance: hi, lpszClassName: class, ..Default::default() });
            let hwnd = CreateWindowExW(
                WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_LAYERED,
                class,
                w!("abra-shadow"),
                WS_POPUP,
                0, 0, 100, 100,
                None, None, Some(hi), None,
            )?;
            let _ = SetLayeredWindowAttributes(hwnd, windows::Win32::Foundation::COLORREF(0), 255, LWA_ALPHA);
            let surface = attach(&self.comp, hwnd, (100.0, 100.0))?;
            let sprite = self.comp.CreateSpriteVisual()?;
            surface.root.Children()?.InsertAtTop(&sprite)?;
            self.shadow = Some(ShadowWin { hwnd, surface, sprite });
        }
        Ok(())
    }

    fn update_shadow(&mut self, h: isize, full: bool) {
        let p = prefs::get();
        if !p.win_shadow || h == 0 || !eligible(h) {
            self.hide_shadow();
            return;
        }
        unsafe {
            let hw = HWND(h as *mut _);
            if IsZoomed(hw).as_bool() || IsIconic(hw).as_bool() {
                self.hide_shadow();
                return;
            }
        }
        let Some((x, y, w, ht)) = snapshot::frame_rect(h) else { return };
        if w < 80 || ht < 60 {
            self.hide_shadow();
            return;
        }
        if self.ensure_shadow_window().is_err() {
            return;
        }
        let scale = unsafe { windows::Win32::UI::HiDpi::GetDpiForSystem() }.max(96) as f32 / 96.0;
        let (m, dy, radius) = ((SHADOW_MARGIN as f32 * scale) as i32, 10.0 * scale, 8.0 * scale);
        let (fw, fh) = (w + 2 * m, ht + 2 * m);
        let key = (fw, fh, radius as i32);
        let go = |this: &mut Fx| -> windows_core::Result<()> {
            let Some(sh) = &this.shadow else { return Ok(()) };
            if full && key != this.shadow_key {
                let (bw, bh, data) = shadow_bitmap(w, ht, m, dy, radius, 4);
                let surface = this.gfx.surface_from_bgra(bw, bh, &data)?;
                let brush = this.comp.CreateSurfaceBrushWithSurface(&surface)?;
                brush.SetStretch(CompositionStretch::Fill)?;
                sh.sprite.SetBrush(&brush)?;
                sh.sprite.SetSize(Vector2 { X: fw as f32, Y: fh as f32 })?;
                sh.surface.root.SetSize(Vector2 { X: fw as f32, Y: fh as f32 })?;
                this.shadow_key = key;
            }
            unsafe {
                let below = GetWindow(HWND(h as *mut _), GW_HWNDNEXT).unwrap_or(HWND_BOTTOM);
                let _ = SetWindowPos(sh.hwnd, Some(below), x - m, y - m, fw, fh, SWP_NOACTIVATE | SWP_SHOWWINDOW);
            }
            Ok(())
        };
        if go(self).is_ok() {
            self.shadow_target = h;
            self.shadow_rect = (x, y, w, ht);
            unsafe {
                SetTimer(Some(self.hwnd), T_SHADOW_WATCH, 250, None);
            }
        }
    }

    fn hide_overlay(&mut self) {
        if let Some(old) = self.genie.take() {
            if let Ok(k) = self.surface.root.Children() {
                let _ = k.Remove(&old);
            }
        }
        if let Some(old) = self.sprite.take() {
            if let Ok(k) = self.surface.root.Children() {
                let _ = k.Remove(&old);
            }
        }
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
    }

    fn apply_prefs(&mut self) {
        let fg = focus::foreground();
        if eligible(fg) {
            self.update_shadow(fg, true);
        } else if !prefs::get().win_shadow {
            self.hide_shadow();
        }
        unsafe {
            if enabled() {
                SetTimer(Some(self.hwnd), T_REFRESH, 4000, None);
                if self.current == 0 {
                    let fg = focus::foreground();
                    if eligible(fg) {
                        flag(fg);
                        self.current = fg;
                        SetTimer(Some(self.hwnd), T_CAPTURE, 200, None);
                    }
                }
            } else {
                let _ = KillTimer(Some(self.hwnd), T_REFRESH);
                self.end_open();
                self.snap = None;
                self.hide_overlay();
                restore_all();
            }
        }
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_FX => {
            with_fx(|fx| fx.on_event(wp.0 as u32, lp.0));
            LRESULT(0)
        }
        prefs::WM_PREFS => {
            with_fx(|fx| fx.apply_prefs());
            LRESULT(0)
        }
        WM_TIMER => {
            let id = wp.0;
            match id {
                T_HIDE | T_CAPTURE | T_RESTORE | T_SHADOW | T_OPEN_END => {
                    let _ = KillTimer(Some(hwnd), id);
                }
                _ => {}
            }
            with_fx(|fx| match id {
                T_SHADOW => {
                    let t = fx.shadow_target;
                    if t != 0 {
                        fx.update_shadow(t, true);
                    }
                }
                T_SHADOW_WATCH => fx.watch_shadow(),
                T_OPEN_POLL => fx.poll_open(),
                T_OPEN_END => fx.end_open(),
                T_CAPTURE | T_REFRESH => fx.capture_current(),
                T_HIDE => {
                    fx.hide_overlay();
                    fx.pending_restore = None;
                }
                T_RESTORE => {
                    if let Some((t, dock)) = fx.pending_restore {
                        let ok = focus::activate(t, dock);
                        log::line("fx", &format!("restore hwnd={t} ok={ok}"));
                    }
                }
                _ => {}
            });
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe extern "system" fn shadow_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_NCHITTEST => LRESULT(-1),
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
