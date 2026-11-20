use nacre_core::appmenus::{Act, Line, Menu as AppMenu};
use crate::dock::menu::{Item, Menu, MENU_CLOSE, MENU_PICK};
use crate::popup::Popup;
use crate::strip::Strip;
use crate::{prefs, services};
use nacre_core::settings::Behavior;
use nacre_core::menubar::{self as lay, Gap, Item as LItem, Placed};
use nacre_core::visibility::policy::{self, Inputs};
use nacre_platform::icons::Bitmap;
use nacre_platform::text::{measure, render, render_face, Style, ICON_FACE};
use nacre_platform::wins::{enumerate, focus, tracker};
use nacre_platform::{keys, launch, log, radio, shell, sys, time};
use nacre_ui::glass::Tier;
use nacre_ui::gfx::Gfx;
use std::cell::RefCell;
use std::collections::HashMap;
use windows::core::w;
use windows::UI::Color;
use windows::UI::Composition::{CompositionDrawingSurface, CompositionStretch, Compositor, ContainerVisual, SpriteVisual};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows_numerics::{Vector2, Vector3};

const WM_MOUSELEAVE: u32 = 0x02A3;
const WM_APPCHG: u32 = WM_APP + 70;
const WM_CLICKWIN: u32 = WM_APP + 74;
static MOUSE_HOOK: std::sync::Mutex<Option<nacre_platform::hookthread::HookThread>> = std::sync::Mutex::new(None);
static BAR_CTL: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
const WM_NOTCH: u32 = WM_APP + 71;
const WM_TEST_NET: u32 = WM_APP + 75;
const STRIP_ENTER: u32 = WM_APP + 76;
const STRIP_LEAVE: u32 = WM_APP + 77;
const T_CLOCK: usize = 1;
const T_STATUS: usize = 2;
const T_APP: usize = 3;
const T_FS: usize = 4;
const T_SIG: usize = 5;
const T_EDGE: usize = 6;
const T_GRACE: usize = 7;
const T_HIDEWIN: usize = 8;
const BAR_W: i32 = 600;
const APP_W: i32 = 800;

const LOGO: u32 = 1;
const APP: u32 = 2;
const PROF0: u32 = 30;
const PROF_CMD: usize = 200_000;
const GUIDE_URL: &str = "https://github.com/abdeealfarizi/abra/blob/main/docs/PANDUAN.md";
const CLOCK: u32 = 10;
const CTRL: u32 = 11;
const SEARCH: u32 = 12;
const WIFI: u32 = 13;
const BATT: u32 = 14;
const STAGEI: u32 = 15;
const NATIVE0: u32 = 50;
const NATIVE_CMD: usize = 100_000;

struct EdgeStrip {
    strip: Strip,
    x: i32,
    width: i32,
}

struct Bar {
    popup: Popup,
    comp: Compositor,
    gfx: Gfx,
    dpi: f32,
    screen: (i32, i32),
    screen_origin: (i32, i32),
    monitor_index: u8,
    h: i32,
    layer: ContainerVisual,
    capsule: SpriteVisual,
    bg: SpriteVisual,
    hair: SpriteVisual,
    cache: HashMap<String, (CompositionDrawingSurface, i32, i32)>,
    left: Vec<Placed>,
    right: Vec<Placed>,
    app: String,
    app_path: String,
    hidden: Vec<isize>,
    menu: Menu,
    open: Option<u32>,
    hover: Option<u32>,
    tracking: bool,
    wifi: Option<bool>,
    wifi_bars: Option<u8>,
    bt: Option<bool>,
    radios: Option<radio::Radios>,
    dark_text: bool,
    shadow: bool,
    native: Vec<nacre_platform::menus::Top>,
    native_hwnd: isize,
    fullscreen: bool,
    strips: Vec<EdgeStrip>,
    edge_armed: bool,
    grace: bool,
    shown: bool,
}

struct Ctl {
    hwnd: HWND,
    comp: Compositor,
}

thread_local! {
    static BAR: RefCell<Vec<Box<Bar>>> = const { RefCell::new(Vec::new()) };
    static ACTIVE_BAR: std::cell::Cell<isize> = const { std::cell::Cell::new(0) };
    static CTL: RefCell<Option<Ctl>> = const { RefCell::new(None) };
}

fn with_bar<R>(f: impl FnOnce(&mut Bar) -> R) -> Option<R> {
    let active = ACTIVE_BAR.with(std::cell::Cell::get);
    BAR.with(|c| {
        let mut bars = c.try_borrow_mut().ok()?;
        let bar = if active != 0 {
            bars.iter_mut().find(|b| b.popup.hwnd.0 as isize == active)
        } else {
            bars.first_mut()
        }?;
        Some(f(bar))
    })
}

fn with_bars(mut f: impl FnMut(&mut Bar)) {
    BAR.with(|c| {
        if let Ok(mut bars) = c.try_borrow_mut() {
            for bar in bars.iter_mut() { f(bar); }
        }
    });
}

fn profile_text(id: &'static str, en: &'static str) -> String {
    match prefs::get().lang {
        nacre_core::i18n::Lang::Id => id.to_string(),
        nacre_core::i18n::Lang::En => en.to_string(),
        nacre_core::i18n::Lang::Es => nacre_core::i18n::menu_es(en).unwrap_or(id).to_string(),
    }
}

fn native_text(text: &str) -> String {
    if prefs::get().lang == nacre_core::i18n::Lang::Es {
        nacre_core::i18n::menu_es(text).unwrap_or(text).to_string()
    } else {
        text.to_string()
    }
}

pub fn refresh_radios() {
    with_bars(|b| {
        if let Some(r) = &b.radios { let _ = r.commands.send(radio::Cmd::Refresh); }
    });
}
fn edge_segments(screen_left: i32, screen_w: i32, dpi: f32, monitor_index: u8) -> Vec<(i32, i32)> {
    let screen_w = screen_w.max(0);
    let Some((left, right, _)) = services::notch_for(monitor_index).filter(|(left, right, _)| right > left) else {
        return vec![(0, screen_w)];
    };
    let pad = (60.0 * dpi).round() as i32;
    let (left, right) = ((left - screen_left - pad).clamp(0, screen_w), (right - screen_left + pad).clamp(0, screen_w));
    let mut segments = Vec::with_capacity(2);
    if left > 0 { segments.push((0, left)); }
    if right < screen_w { segments.push((right, screen_w - right)); }
    segments
}

fn create_edge_strips(hwnd: HWND, segments: &[(i32, i32)]) -> Vec<EdgeStrip> {
    segments
        .iter()
        .filter_map(|&(x, width)| {
            Strip::new(hwnd, STRIP_ENTER, STRIP_LEAVE).ok().map(|strip| EdgeStrip { strip, x, width })
        })
        .collect()
}

fn recolor(b: &Bitmap, rgb: [u8; 3]) -> Vec<u8> {
    let mut out = b.bgra.clone();
    for px in out.chunks_exact_mut(4) {
        let a = px[3] as u32;
        px[0] = (rgb[2] as u32 * a / 255) as u8;
        px[1] = (rgb[1] as u32 * a / 255) as u8;
        px[2] = (rgb[0] as u32 * a / 255) as u8;
    }
    out
}

fn noop_fg() {
    crate::presence::refresh();
    CTL.with(|c| {
        if let Some(ctl) = c.borrow().as_ref() {
            unsafe {
                let _ = PostMessageW(Some(ctl.hwnd), WM_APPCHG, WPARAM(0), LPARAM(0));
            }
        }
    });
}

unsafe extern "system" fn ll_mouse(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code >= 0 {
        let m = wp.0 as u32;
        if m == WM_LBUTTONDOWN || m == WM_RBUTTONDOWN || m == WM_MBUTTONDOWN {
            let ctl = BAR_CTL.load(std::sync::atomic::Ordering::Relaxed);
            if ctl != 0 {
                let pt = (*(lp.0 as *const MSLLHOOKSTRUCT)).pt;
                let h = WindowFromPoint(pt);
                if !h.0.is_null() {
                    let root = GetAncestor(h, GA_ROOT);
                    if !root.0.is_null() {
                        let _ = PostMessageW(Some(HWND(ctl as *mut _)), WM_CLICKWIN, WPARAM(root.0 as usize), LPARAM(0));
                    }
                }
            }
        }
    }
    CallNextHookEx(None, code, wp, lp)
}

fn loc_hook(ev: u32, hwnd: isize) {
    if hwnd != 0 && (ev == tracker::EV_FOREGROUND || (ev == tracker::EV_LOCATION && hwnd == focus::foreground())) {
        CTL.with(|c| {
            if let Some(ctl) = c.borrow().as_ref() {
                unsafe {
                    SetTimer(Some(ctl.hwnd), T_FS, 200, None);
                }
            }
        });
    }
}

pub fn init(comp: &Compositor) -> windows_core::Result<()> {
    unsafe {
        let hi: HINSTANCE = GetModuleHandleW(None)?.into();
        let class = w!("AbraBarCtl");
        RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(ctl_proc), hInstance: hi, lpszClassName: class, ..Default::default() });
        let hwnd = CreateWindowExW(WINDOW_EX_STYLE(0), class, w!("abra-bar-ctl"), WS_POPUP, 0, 0, 0, 0, Some(HWND_MESSAGE), None, Some(hi), None)?;
        CTL.with(|c| *c.borrow_mut() = Some(Ctl { hwnd, comp: comp.clone() }));
        prefs::listen(hwnd);
        tracker::install(noop_fg);
        tracker::install_fx(loc_hook);
        crate::presence::refresh();
        BAR_CTL.store(hwnd.0 as isize, std::sync::atomic::Ordering::Relaxed);
        *MOUSE_HOOK.lock().unwrap_or_else(|e| e.into_inner()) = nacre_platform::hookthread::HookThread::start(nacre_platform::hookthread::Kind::Mouse, Some(ll_mouse));
        apply_prefs();
    }
    Ok(())
}

fn apply_prefs() {
    let want = prefs::get().menubar;
    let have = BAR.with(|c| !c.borrow().is_empty());
    if want && !have {
        let comp = CTL.with(|c| c.borrow().as_ref().map(|c| c.comp.clone()));
        if let Some(comp) = comp {
            for (index, monitor) in crate::display::monitors().into_iter().enumerate() {
                match create(&comp, monitor, index.min(u8::MAX as usize) as u8) {
                    Ok(bar) => {
                        services::register_bar(bar.popup.hwnd);
                        BAR.with(|c| c.borrow_mut().push(bar));
                    }
                    Err(e) => log::line("menubar", &format!("monitor {}: gagal dibuat: {e}", index + 1)),
                }
            }
            with_bars(|b| { b.refresh_app(); b.relayout(); b.render(); });
            services::post_island(WM_APP + 72);
            CTL.with(|c| if let Some(ctl) = c.borrow().as_ref() { unsafe { SetTimer(Some(ctl.hwnd), T_APP, 900, None); } });
            log::line("menubar", "dibuat en las pantallas seleccionadas");
            crate::hotcorners::raise();
        }
    } else if !want && have {
        let bars = BAR.with(|c| std::mem::take(&mut *c.borrow_mut()));
        for bar in bars {
            bar.menu.hide();
            services::unregister_bar(bar.popup.hwnd);
            shell::appbar::release_top(Some(bar.popup.hwnd));
            unsafe { let _ = DestroyWindow(bar.popup.hwnd); }
        }
        log::line("menubar", "destruido");
        services::post_island(crate::prefs::WM_PREFS);
    } else if want {
        let rescale = BAR.with(|c| c.borrow().iter().any(|b| {
            let want_dpi = unsafe { GetDpiForWindow(b.popup.hwnd) }.max(96) as f32 / 96.0 * prefs::ui_scale();
            (want_dpi - b.dpi).abs() > 0.01
        }));
        if rescale {
            let bars = BAR.with(|c| std::mem::take(&mut *c.borrow_mut()));
            for bar in bars {
                bar.menu.hide();
                services::unregister_bar(bar.popup.hwnd);
                shell::appbar::release_top(Some(bar.popup.hwnd));
                unsafe { let _ = DestroyWindow(bar.popup.hwnd); }
            }
            apply_prefs();
            return;
        }
        with_bars(|b| { b.apply_behavior(); b.apply_look(); b.relayout(); b.render(); });
    }
}
fn create(comp: &Compositor, monitor: crate::display::Monitor, monitor_index: u8) -> windows_core::Result<Box<Bar>> {
    let screen = (monitor.width(), monitor.height());
    let screen_origin = (monitor.rect.left, monitor.rect.top);
    let glass = prefs::apply(nacre_ui::glass::GlassParams::dark((10.0, 10.0), 0.0), &prefs::bar_look());
    let mut popup = Popup::new(comp, w!("AbraMenuBar"), Some(bar_proc), WINDOW_EX_STYLE(0), glass)?;
    unsafe {
        let _ = SetWindowPos(popup.hwnd, None, screen_origin.0, screen_origin.1, screen.0, 26,
            SWP_NOACTIVATE | SWP_NOZORDER);
    }
    let dpi = unsafe { GetDpiForWindow(popup.hwnd) }.max(96) as f32 / 96.0 * prefs::ui_scale();
    let notch_h = services::notch_for(monitor_index).map(|(_, _, h)| h as f32 / dpi);
    let h = (notch_h.unwrap_or(26.0) * dpi).round() as i32;
    let tier = {
        let c = sys::conditions();
        if !c.transparency || c.battery_saver || c.remote_session { Tier::Solid } else { Tier::Full }
    };
    let _ = popup.panel.set_tier(tier);
    popup.set_rect(screen_origin.0, screen_origin.1, screen.0, h)?;
    popup.panel.set_radius(0.0)?;
    popup.panel.set_rect(0.0, 0.0, 0.0, 0.0)?;
    let bg = comp.CreateSpriteVisual()?;
    popup.content.Children()?.InsertAtTop(&bg)?;
    let hair = comp.CreateSpriteVisual()?;
    hair.SetOpacity(0.0)?;
    popup.content.Children()?.InsertAtTop(&hair)?;
    let capsule = comp.CreateSpriteVisual()?;
    capsule.SetOpacity(0.0)?;
    popup.content.Children()?.InsertAtTop(&capsule)?;
    let layer = comp.CreateContainerVisual()?;
    popup.content.Children()?.InsertAtTop(&layer)?;
    let gfx = Gfx::shared(comp)?;
    let mut menu = Menu::new(comp, popup.hwnd, dpi, screen, tier)?;
    menu.set_bounds(screen_origin, screen);
    unsafe {
        SetTimer(Some(popup.hwnd), T_CLOCK, time::ms_to_next_minute().max(200), None);
        SetTimer(Some(popup.hwnd), T_STATUS, 60_000, None);
        SetTimer(Some(popup.hwnd), T_SIG, 4_000, None);
    }
    let hw = popup.hwnd.0 as isize;
    let radios = radio::spawn(move || unsafe {
        let _ = PostMessageW(Some(HWND(hw as *mut _)), WM_APP + 22, WPARAM(0), LPARAM(0));
    });
    if let Ok(v) = std::env::var("NACRE_TEST_NETMENU") {
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(2500));
            let code = match v.as_str() {
                "bt" => 1,
                "wifi-others" => 2,
                "cc" => 4,
                "bt-click" => 6,
                "cc-edit" => 8,
                _ => 0,
            };
            unsafe {
                let _ = PostMessageW(Some(HWND(hw as *mut _)), WM_TEST_NET, WPARAM(code), LPARAM(0));
            }
            if code == 2 || code == 4 || code == 6 || code == 8 {
                std::thread::sleep(std::time::Duration::from_millis(2500));
                unsafe {
                    let _ = PostMessageW(Some(HWND(hw as *mut _)), WM_TEST_NET, WPARAM(code + 1), LPARAM(0));
                }
            }
        });
    }
    let behavior = prefs::get().rail;
    let target = prefs::get().menubar_monitor;
    let selected = target == u8::MAX || target == monitor_index;
    let segments = if behavior == Behavior::Smart { edge_segments(screen_origin.0, screen.0, dpi, monitor_index) } else { vec![] };
    let strips = create_edge_strips(popup.hwnd, &segments);
    let shown = selected && (behavior != Behavior::Smart || strips.is_empty());
    let mut bar = Box::new(Bar {
        popup,
        comp: comp.clone(),
        gfx,
        dpi,
        screen,
        screen_origin,
        monitor_index,
        h,
        layer,
        capsule,
        bg,
        hair,
        cache: HashMap::new(),
        left: vec![],
        right: vec![],
        app: String::new(),
        app_path: String::new(),
        hidden: vec![],
        menu,
        open: None,
        hover: None,
        tracking: false,
        wifi: None,
        wifi_bars: nacre_platform::wifi::link().map(|l| l.1),
        bt: None,
        radios: Some(radios),
        dark_text: false,
        shadow: false,
        native: vec![],
        native_hwnd: 0,
        fullscreen: false,
        strips,
        edge_armed: false,
        grace: false,
        shown,
    });
    bar.apply_bg();
    if !bar.shown {
        let _ = bar.popup.root().SetOffset(Vector3 { X: 0.0, Y: -h as f32, Z: 0.0 });
    }
    bar.place_strip();
    bar.update_appbar();
    if bar.shown {
        bar.popup.show();
    }
    if bar.strips.is_empty() && behavior == Behavior::Smart {
        log::line("menubar", "borde inteligente no disponible; se mantiene visible");
        let _ = bar.popup.root().SetOffset(Vector3 { X: 0.0, Y: 0.0, Z: 0.0 });
    }
    Ok(bar)
}

unsafe extern "system" fn ctl_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        prefs::WM_PREFS => {
            apply_prefs();
            LRESULT(0)
        }
        WM_APPCHG => {
            SetTimer(Some(hwnd), T_APP, 90, None);
            LRESULT(0)
        }
        WM_CLICKWIN => {
            let root = wp.0 as isize;
            let mut cls = [0u16; 64];
            let n = GetClassNameW(HWND(root as *mut _), &mut cls);
            let cls = String::from_utf16_lossy(&cls[..n.max(0) as usize]);
            let mut pid = 0u32;
            GetWindowThreadProcessId(HWND(root as *mut _), Some(&mut pid));
            let shell = matches!(cls.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "NotifyIconOverflowWindow" | "TopLevelWindowForOverflowXamlIsland");
            if pid != std::process::id() && !shell {
                with_bars(|b| {
                    if b.refresh_app_for(root) {
                        b.relayout();
                        b.render();
                    }
                });
            }
            LRESULT(0)
        }
        WM_TIMER if wp.0 == T_FS => {
            let _ = KillTimer(Some(hwnd), T_FS);
            with_bars(|b| {
                b.check_fullscreen();
                b.recheck_color();
            });
            LRESULT(0)
        }
        WM_TIMER if wp.0 == T_APP => {
            let _ = KillTimer(Some(hwnd), T_APP);
            with_bars(|b| {
                if b.refresh_app() {
                    b.relayout();
                    b.render();
                } else {
                    b.recheck_color();
                }
                b.update_visibility();
            });
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

impl Bar {
    fn screen_bounds(&self) -> windows::Win32::Foundation::RECT {
        windows::Win32::Foundation::RECT {
            left: self.screen_origin.0,
            top: self.screen_origin.1,
            right: self.screen_origin.0 + self.screen.0,
            bottom: self.screen_origin.1 + self.screen.1,
        }
    }

    fn fg(&self) -> [u8; 3] {
        if self.shadow {
            [0, 0, 0]
        } else if self.dark_text {
            [18, 18, 22]
        } else {
            [255, 255, 255]
        }
    }

    fn apply_bg(&mut self) {
        use nacre_core::settings::BarStyle;
        let p = prefs::get();
        let d = self.dpi;
        let flat = if p.menubar_glass && !p.menubar_material { Some(match p.menubar_style { BarStyle::Light => (248, 248, 250), BarStyle::Dark => (0, 0, 0) }) } else { None };
        if p.menubar_material {
            let look = prefs::look(prefs::Role::Themed);
            let base = nacre_ui::glass::GlassParams::dark((self.screen.0 as f32, self.h as f32), 0.0);
            let mut gp = prefs::apply(base, &look);
            gp.rim.A = 0;
            gp.highlight = 0.0;
            let _ = self.popup.panel.set_fringe(false);
            let _ = self.popup.panel.set_params(gp);
            let _ = self.popup.panel.set_rect(0.0, 0.0, self.screen.0 as f32, self.h as f32);
        } else {
            let _ = self.popup.panel.set_rect(0.0, 0.0, 0.0, 0.0);
        }
        let _ = self.bg.SetSize(Vector2 { X: self.screen.0 as f32, Y: self.h as f32 });
        match flat {
            Some((r, g, b)) => {
                if let Ok(br) = self.comp.CreateColorBrushWithColor(Color { A: 255, R: r, G: g, B: b }) {
                    let _ = self.bg.SetBrush(&br);
                }
                let _ = self.bg.SetOpacity(1.0);
            }
            None if !p.menubar_material => {
                if let Ok(br) = self.top_shade() {
                    let _ = self.bg.SetBrush(&br);
                    let _ = self.bg.SetOpacity(1.0);
                }
            }
            None => {
                let _ = self.bg.SetOpacity(0.0);
            }
        }
        let line = d.round().max(1.0);
        let _ = self.hair.SetSize(Vector2 { X: self.screen.0 as f32, Y: line });
        let _ = self.hair.SetOffset(Vector3 { X: 0.0, Y: self.h as f32 - line, Z: 0.0 });
        let hc = match (p.menubar_glass && !p.menubar_material, p.menubar_style) {
            _ if p.menubar_material => {
                Some(if prefs::look(prefs::Role::Themed).dark { Color { A: 18, R: 255, G: 255, B: 255 } } else { Color { A: 22, R: 0, G: 0, B: 0 } })
            }
            (false, _) => None,
            (true, BarStyle::Light) => Some(Color { A: 34, R: 0, G: 0, B: 0 }),
            (true, BarStyle::Dark) => Some(Color { A: 40, R: 255, G: 255, B: 255 }),
        };
        match hc.and_then(|c| self.comp.CreateColorBrushWithColor(c).ok()) {
            Some(br) => {
                let _ = self.hair.SetBrush(&br);
                let _ = self.hair.SetOpacity(1.0);
            }
            None => {
                let _ = self.hair.SetOpacity(0.0);
            }
        }
    }

    fn top_shade(&self) -> windows_core::Result<windows::UI::Composition::CompositionLinearGradientBrush> {
        let g = self.comp.CreateLinearGradientBrush()?;
        g.SetStartPoint(Vector2 { X: 0.0, Y: 0.0 })?;
        g.SetEndPoint(Vector2 { X: 0.0, Y: 1.0 })?;
        let stops = g.ColorStops()?;
        for (o, a) in [(0.0, 40u8), (0.45, 18), (1.0, 0)] {
            stops.Append(&self.comp.CreateColorGradientStopWithOffsetAndColor(o, Color { A: a, R: 0, G: 0, B: 0 })?)?;
        }
        Ok(g)
    }

    fn update_text_color(&mut self) -> bool {
        use nacre_core::settings::BarStyle;
        let before = self.dark_text;
        let p = prefs::get();
        self.dark_text = p.menubar_glass && !p.menubar_material && p.menubar_style == BarStyle::Light;
        before != self.dark_text
    }

    fn recheck_color(&mut self) {
        if self.update_text_color() {
            self.cache.clear();
            self.render();
        }
    }

    fn apply_look(&mut self) {
        self.update_text_color();
        self.apply_bg();
        self.cache.clear();
    }

    fn check_fullscreen(&mut self) {
        let fg = focus::foreground();
        let full = fg != 0 && fg != self.popup.hwnd.0 as isize && nacre_platform::wins::overlap::evaluate_in(fg, std::process::id(), (0, 0, 0, 0), windows::Win32::Foundation::RECT { left: self.screen_origin.0, top: self.screen_origin.1, right: self.screen_origin.0 + self.screen.0, bottom: self.screen_origin.1 + self.screen.1 }).fullscreen && !prefs::get().above_fullscreen;
        if full != self.fullscreen {
            self.fullscreen = full;
            if full && self.open.is_some() {
                self.menu.hide();
                self.open = None;
                self.update_capsule();
            }
        }
        self.place_strip();
        self.update_visibility();
    }

    fn behavior(&self) -> Behavior {
        let behavior = prefs::get().rail;
        if behavior == Behavior::Smart && self.strips.is_empty() { Behavior::Fixed } else { behavior }
    }

    fn interacting(&self) -> bool {
        self.tracking || self.open.is_some() || self.grace
    }

    fn is_selected(&self) -> bool {
        let target = prefs::get().menubar_monitor;
        target == u8::MAX || target == self.monitor_index
    }

    fn update_appbar(&self) {
        if self.is_selected() && self.behavior().reserves() {
            shell::appbar::reserve_top(self.popup.hwnd, windows::Win32::Foundation::RECT { left: self.screen_origin.0, top: self.screen_origin.1, right: self.screen_origin.0 + self.screen.0, bottom: self.screen_origin.1 + self.h }, self.h);
        } else {
            shell::appbar::release_top(Some(self.popup.hwnd));
        }
    }

    fn place_strip(&self) {
        for edge in &self.strips {
            edge.strip.place(self.screen_origin.0 + edge.x, self.screen_origin.1, edge.width, 2);
            edge.strip.set_visible(!self.fullscreen);
        }
    }

    fn sync_strips(&mut self) {
        let behavior = prefs::get().rail;
        let segments = if behavior == Behavior::Smart { edge_segments(self.screen_origin.0, self.screen.0, self.dpi, self.monitor_index) } else { vec![] };
        let current = self.strips.iter().map(|edge| (edge.x, edge.width)).collect::<Vec<_>>();
        if current != segments {
            self.strips = create_edge_strips(self.popup.hwnd, &segments);
        }
    }

    fn apply_behavior(&mut self) {
        let behavior = prefs::get().rail;
        self.sync_strips();
        if behavior != Behavior::Smart {
            self.edge_armed = false;
            self.grace = false;
            unsafe {
                let _ = KillTimer(Some(self.popup.hwnd), T_EDGE);
                let _ = KillTimer(Some(self.popup.hwnd), T_GRACE);
            }
        } else if self.strips.is_empty() {
            log::line("menubar", "borde inteligente no disponible; se mantiene visible");
        }
        self.update_appbar();
        self.place_strip();
        self.update_visibility();
    }

    fn update_visibility(&mut self) {
        let behavior = self.behavior();
        let overlapped = nacre_platform::wins::overlap::evaluate_in(
            focus::foreground(),
            std::process::id(),
            (self.screen_origin.0, self.screen_origin.1, self.screen_origin.0 + self.screen.0, self.screen_origin.1 + self.h),
            windows::Win32::Foundation::RECT { left: self.screen_origin.0, top: self.screen_origin.1, right: self.screen_origin.0 + self.screen.0, bottom: self.screen_origin.1 + self.screen.1 },
        )
        .overlapped;
        let inputs = Inputs {
            overlapped,
            interacting: self.interacting(),
            edge_armed: self.edge_armed,
            fullscreen: self.fullscreen,
            ..Default::default()
        };
        let mode = if behavior == Behavior::Smart { policy::Mode::OnDemand } else { behavior.mode() };
        let hide = !self.is_selected() || policy::should_hide(mode, &inputs);
        self.place_strip();
        if hide && self.shown {
            self.shown = false;
            let root = self.popup.root();
            let _ = crate::motion::vec3(
                &self.comp,
                root,
                "Offset",
                Vector3 { X: 0.0, Y: 0.0, Z: 0.0 },
                Vector3 { X: 0.0, Y: -(self.h as f32), Z: 0.0 },
                180,
            );
            unsafe { SetTimer(Some(self.popup.hwnd), T_HIDEWIN, 200, None); }
            log::line("menubar", "sembunyi");
        } else if !hide && !self.shown {
            self.shown = true;
            unsafe {
                let _ = KillTimer(Some(self.popup.hwnd), T_HIDEWIN);
                let _ = self.popup.root().SetOffset(Vector3 { X: 0.0, Y: -(self.h as f32), Z: 0.0 });
            }
            self.popup.show();
            let root = self.popup.root();
            let _ = crate::motion::vec3(
                &self.comp,
                root,
                "Offset",
                Vector3 { X: 0.0, Y: -(self.h as f32), Z: 0.0 },
                Vector3 { X: 0.0, Y: 0.0, Z: 0.0 },
                180,
            );
            crate::topmost::request_raise();
            log::line("menubar", "tampil");
        }
    }

    fn on_strip_enter(&mut self) {
        if self.behavior() == Behavior::Smart {
            unsafe { SetTimer(Some(self.popup.hwnd), T_EDGE, 200, None); }
        }
    }

    fn on_strip_leave(&mut self) {
        unsafe { let _ = KillTimer(Some(self.popup.hwnd), T_EDGE); }
        if self.edge_armed {
            self.edge_armed = false;
            self.grace = true;
            unsafe { SetTimer(Some(self.popup.hwnd), T_GRACE, 800, None); }
            self.update_visibility();
        }
    }

    fn refresh_app(&mut self) -> bool {
        self.refresh_app_for(focus::foreground())
    }

    fn refresh_app_for(&mut self, fg: isize) -> bool {
        if fg == 0 || fg == self.popup.hwnd.0 as isize {
            return false;
        }
        let mut changed = false;
        self.check_fullscreen();
        let native = nacre_platform::menus::read(fg).unwrap_or_default();
        let titles = |v: &Vec<nacre_platform::menus::Top>| v.iter().map(|t| t.title.clone()).collect::<Vec<_>>();
        if titles(&native) != titles(&self.native) {
            changed = true;
        }
        self.native = native;
        self.native_hwnd = fg;
        let wins = enumerate::list(std::process::id());
        let name = wins
            .iter()
            .find(|w| w.hwnd == fg)
            .map(|w| {
                if nacre_platform::wins::names::is_host(&w.path) {
                    w.title.chars().take(40).collect::<String>().trim().to_string()
                } else if !w.name.is_empty() {
                    w.name.clone()
                } else {
                    nacre_core::dock::items::name_from_path(&w.path)
                }
            })
            .filter(|n| !n.is_empty());
        let Some(name) = name else { return changed };
        let path = wins.iter().find(|w| w.hwnd == fg).map(|w| w.path.clone()).unwrap_or_default();
        let profile_changed = nacre_core::appmenus::exe_of(&path) != nacre_core::appmenus::exe_of(&self.app_path);
        self.app_path = path;
        if name == self.app && !profile_changed {
            return changed;
        }
        self.app = name;
        true
    }

    fn text_w(&self, s: &str, weight: i32) -> f32 {
        let px = (13.0 * self.dpi).round() as i32;
        measure(s, &Style { px, weight }).0 as f32 + 2.0
    }

    fn relayout(&mut self) {
        let d = self.dpi;
        let pad = 11.0 * d;
        let t = |s: &str, wgt: i32| self.text_w(s, wgt) + 2.0 * pad;
        let mut left = vec![
            LItem { id: LOGO, width: self.h as f32 * 1.05 },
            LItem { id: APP, width: t(&self.app_label(), APP_W) },
        ];
        if self.native.is_empty() {
            for (i, m) in self.profile().iter().enumerate() {
                left.push(LItem { id: PROF0 + i as u32, width: t(&profile_text(m.id, m.en), BAR_W) });
            }
        } else {
            for (i, top) in self.native.iter().enumerate().take(20) {
                left.push(LItem { id: NATIVE0 + i as u32, width: t(&native_text(&top.title), BAR_W) });
            }
        }
        let icon_w = 30.0 * d;
        let mut right = vec![
            LItem { id: CLOCK, width: t(&self.clock_text(), BAR_W) },
            LItem { id: CTRL, width: icon_w },
            LItem { id: SEARCH, width: icon_w },
        ];
        if prefs::get().stage {
            right.push(LItem { id: STAGEI, width: icon_w });
        }
        if self.wifi.is_some() {
            right.push(LItem { id: WIFI, width: icon_w });
        }
        if let Some((pct, _)) = sys::battery() {
            let extra = if prefs::get().battery_pct == 2 { self.small_w(&format!("{pct}%"), 12.0, BAR_W) + 5.0 * d } else { 0.0 };
            right.push(LItem { id: BATT, width: 40.0 * d + extra });
        }
        let gap = services::notch_for(self.monitor_index).map(|(l, r, _)| Gap { left: (l - self.screen_origin.0) as f32, right: (r - self.screen_origin.0) as f32 });
        let (l, r) = lay::layout(self.screen.0 as f32, 6.0 * d, &left, &right, gap);
        self.left = l;
        self.right = r;
        let want_h = services::notch_for(self.monitor_index).map(|(_, _, h)| h).unwrap_or((26.0 * d).round() as i32);
        if want_h != self.h {
            self.h = want_h;
            let _ = self.popup.set_rect(self.screen_origin.0, self.screen_origin.1, self.screen.0, self.h);
            let _ = self.popup.panel.set_radius(0.0);
            self.apply_bg();
            if !self.shown {
                let _ = self.popup.root().SetOffset(Vector3 { X: 0.0, Y: -(self.h as f32), Z: 0.0 });
            }
            self.update_appbar();
        }
        self.sync_strips();
        self.place_strip();
    }

    fn app_label(&self) -> String {
        if self.app.is_empty() {
            "ABRA".to_string()
        } else {
            self.app.clone()
        }
    }

    fn clock_text(&self) -> String {
        let n = time::local_now();
        let lang = prefs::get().lang;
        let wd = nacre_core::island::calendar::weekday(n.year, n.month, n.day) as usize;
        let days = nacre_core::i18n::weekday_labels(lang, false, nacre_core::island::calendar::weekday_labels(false));
        let months_en = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
        let months_id = ["Jan", "Feb", "Mar", "Apr", "Mei", "Jun", "Jul", "Agu", "Sep", "Okt", "Nov", "Des"];
        let months_es = ["ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic"];
        let mon = match lang {
            nacre_core::i18n::Lang::En => months_en[(n.month - 1) as usize % 12],
            nacre_core::i18n::Lang::Id => months_id[(n.month - 1) as usize % 12],
            nacre_core::i18n::Lang::Es => months_es[(n.month - 1) as usize % 12],
        };
        let time = if prefs::get().clock_24h {
            let separator = if lang == nacre_core::i18n::Lang::Id { '.' } else { ':' };
            format!("{:02}{separator}{:02}", n.hour, n.minute)
        } else {
            let h12 = if n.hour % 12 == 0 { 12 } else { n.hour % 12 };
            format!("{h12}:{:02} {}", n.minute, nacre_core::i18n::day_period(lang, n.hour))
        };
        match lang {
            nacre_core::i18n::Lang::En => format!("{} {} {}  {}", days[wd % 7], mon, n.day, time),
            nacre_core::i18n::Lang::Id | nacre_core::i18n::Lang::Es => format!("{} {} {}  {}", days[wd % 7], n.day, mon, time),
        }
    }

    fn sprite(&self, parent: &ContainerVisual, s: &CompositionDrawingSurface, x: f32, y: f32, w: f32, h: f32, op: f32) {
        let go = || -> windows_core::Result<()> {
            let b = self.comp.CreateSurfaceBrushWithSurface(s)?;
            b.SetStretch(CompositionStretch::None)?;
            let v = self.comp.CreateSpriteVisual()?;
            v.SetBrush(&b)?;
            v.SetSize(Vector2 { X: w, Y: h })?;
            let (dy, om) = if self.shadow { (1.0 * self.dpi, 0.38) } else { (0.0, 1.0) };
            v.SetOffset(Vector3 { X: x, Y: y + dy, Z: 0.0 })?;
            v.SetOpacity(op * om)?;
            parent.Children()?.InsertAtTop(&v)
        };
        let _ = go();
    }

    fn draw_text(&mut self, text: &str, weight: i32, x: f32, w: f32, op: f32) {
        let px = (13.0 * self.dpi).round() as i32;
        let rgb = self.fg();
        let key = format!("t|{px}|{weight}|{rgb:?}|{text}");
        let hit = self.cache.get(&key).cloned();
        let (surf, tw, th) = match hit {
            Some(v) => v,
            None => {
                let style = Style { px, weight };
                let (tw, th) = measure(text, &style);
                let tw = tw + 2;
                let Some(bmp) = render(text, &style, tw, th) else { return };
                let data = recolor(&bmp, rgb);
                let Ok(s) = self.gfx.surface_from_bgra(bmp.w, bmp.h, &data) else { return };
                if self.cache.len() > 120 {
                    self.cache.clear();
                }
                let v = (s, tw, th);
                self.cache.insert(key, v.clone());
                v
            }
        };
        let y = (self.h as f32 - th as f32) / 2.0;
        let layer = self.layer.clone();
        self.sprite(&layer, &surf, x + (w - tw as f32) / 2.0, y, tw as f32, th as f32, op);
    }

    fn draw_glyph(&mut self, glyph: &str, cx: f32, size: f32, op: f32) {
        let px = (size * self.dpi).round() as i32;
        let rgb = self.fg();
        if let Some(sym) = nacre_core::symbols::for_glyph(glyph) {
            let n = (size * 1.25 * self.dpi).round().max(8.0) as u32;
            let key = format!("sym|{sym:?}|{n}|{rgb:?}");
            let (surf, w, h) = match self.cache.get(&key).cloned() {
                Some(v) => v,
                None => {
                    let data = nacre_core::symbols::render(sym, n, rgb);
                    let Ok(s) = self.gfx.surface_from_bgra(n, n, &data) else { return };
                    let v = (s, n as i32, n as i32);
                    self.cache.insert(key, v.clone());
                    v
                }
            };
            let layer = self.layer.clone();
            self.sprite(&layer, &surf, (cx - w as f32 / 2.0).round(), ((self.h as f32 - h as f32) / 2.0).round(), w as f32, h as f32, op);
            return;
        }
        let key = format!("g|{px}|{rgb:?}|{glyph}");
        let hit = self.cache.get(&key).cloned();
        let (surf, w, h) = match hit {
            Some(v) => v,
            None => {
                let b = (size * 1.6 * self.dpi).round() as i32;
                let Some(bmp) = render_face(glyph, &Style { px, weight: 400 }, ICON_FACE, b, b) else { return };
                let (w, h) = (bmp.w as usize, bmp.h as usize);
                let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0usize, 0usize);
                for y in 0..h {
                    for x in 0..w {
                        if bmp.bgra[(y * w + x) * 4 + 3] > 8 {
                            x0 = x0.min(x);
                            x1 = x1.max(x);
                            y0 = y0.min(y);
                            y1 = y1.max(y);
                        }
                    }
                }
                if x0 > x1 {
                    return;
                }
                let (gw, gh) = (x1 - x0 + 1, y1 - y0 + 1);
                let mut data = vec![0u8; gw * gh * 4];
                let rec = recolor(&bmp, rgb);
                for y in 0..gh {
                    for x in 0..gw {
                        let s = ((y0 + y) * w + x0 + x) * 4;
                        let d = (y * gw + x) * 4;
                        data[d..d + 4].copy_from_slice(&rec[s..s + 4]);
                    }
                }
                let Ok(s) = self.gfx.surface_from_bgra(gw as u32, gh as u32, &data) else { return };
                let v = (s, gw as i32, gh as i32);
                self.cache.insert(key, v.clone());
                v
            }
        };
        let layer = self.layer.clone();
        self.sprite(&layer, &surf, cx - w as f32 / 2.0, (self.h as f32 - h as f32) / 2.0, w as f32, h as f32, op);
    }

    fn draw_logo(&mut self, x: f32, w: f32) {
        let size = (self.h as f32 * 0.54).round() as u32;
        let rgb = self.fg();
        let custom = crate::menulogo::path().is_some();
        let size = if custom { (self.h as f32 * 0.5).round() as u32 } else { size };
        let key = format!("logo|{size}|{rgb:?}|{}|{custom}", crate::menulogo::generation());
        let hit = self.cache.get(&key).cloned();
        let (surf, lw, lh) = match hit {
            Some(v) => v,
            None => {
                let (bw, bh, data) = match crate::menulogo::bitmap(size, rgb).filter(|_| custom) {
                    Some(b) => (b.w, b.h, b.bgra),
                    None => (size, size, nacre_core::logo::render_badge(size, rgb)),
                };
                let Ok(s) = self.gfx.surface_from_bgra(bw, bh, &data) else { return };
                let v = (s, bw as i32, bh as i32);
                self.cache.insert(key, v.clone());
                v
            }
        };
        let layer = self.layer.clone();
        self.sprite(&layer, &surf, x + (w - lw as f32) / 2.0, (self.h as f32 - lh as f32) / 2.0, lw as f32, lh as f32, 1.0);
    }

    fn small_w(&self, text: &str, size: f32, weight: i32) -> f32 {
        measure(text, &Style { px: (size * self.dpi).round() as i32, weight }).0 as f32 + 2.0
    }

    fn small_text(&mut self, text: &str, size: f32, weight: i32, rgb: [u8; 3], cx: f32, cy: f32, op: f32) {
        let px = (size * self.dpi).round() as i32;
        let key = format!("st|{px}|{weight}|{rgb:?}|{text}");
        let (surf, tw, th) = match self.cache.get(&key).cloned() {
            Some(v) => v,
            None => {
                let style = Style { px, weight };
                let (tw, th) = measure(text, &style);
                let Some(bmp) = render(text, &style, tw + 2, th) else { return };
                let Ok(s) = self.gfx.surface_from_bgra(bmp.w, bmp.h, &recolor(&bmp, rgb)) else { return };
                let v = (s, bmp.w as i32, bmp.h as i32);
                self.cache.insert(key, v.clone());
                v
            }
        };
        let layer = self.layer.clone();
        self.sprite(&layer, &surf, (cx - tw as f32 / 2.0).round(), (cy - th as f32 / 2.0).round(), tw as f32, th as f32, op);
    }

    fn draw_wifi(&mut self, cx: f32) {
        let d = self.dpi;
        let rgb = self.fg();
        let n = (17.0 * d).round() as u32;
        let level = if self.wifi == Some(false) { 0 } else { nacre_core::logo::wifi_level(self.wifi_bars) };
        let key = format!("wifi|{n}|{level}|{rgb:?}");
        let (surf, sw, sh) = match self.cache.get(&key).cloned() {
            Some(v) => v,
            None => {
                let data = nacre_core::logo::render_wifi(n, level, rgb, 0.32);
                let Ok(s) = self.gfx.surface_from_bgra(n, n, &data) else { return };
                let v = (s, n as i32, n as i32);
                self.cache.insert(key, v.clone());
                v
            }
        };
        let layer = self.layer.clone();
        let y = ((self.h as f32 - sh as f32) / 2.0 + 1.0 * d).round();
        self.sprite(&layer, &surf, (cx - sw as f32 / 2.0).round(), y, sw as f32, sh as f32, 1.0);
    }

    fn draw_battery(&mut self, x: f32, w: f32) {
        let Some((pct, charging)) = sys::battery() else { return };
        let d = self.dpi;
        let rgb = self.fg();
        let mode = prefs::get().battery_pct;
        let inside = mode == 1;
        let (bw, bh) = if inside { ((29.0 * d).round() as u32, (14.0 * d).round() as u32) } else { ((25.0 * d).round() as u32, (12.0 * d).round() as u32) };
        let fill = if charging { [52, 199, 89] } else if pct <= 10 { [255, 69, 58] } else { rgb };
        let key = format!("batt3|{bw}|{bh}|{pct}|{rgb:?}|{fill:?}|{inside}");
        let (surf, sw, sh) = match self.cache.get(&key).cloned() {
            Some(v) => v,
            None => {
                let mut data = nacre_core::logo::render_battery_pill(bw, bh, pct, rgb, fill, inside);
                if inside {
                    let style = Style { px: (9.5 * d).round() as i32, weight: BAR_W };
                    let label = pct.to_string();
                    let (tw, th) = measure(&label, &style);
                    if let Some(tb) = render(&label, &style, tw + 4, th + 2) {
                        knock_out(&mut data, bw, bh, &tb);
                    }
                }
                let Ok(s) = self.gfx.surface_from_bgra(bw, bh, &data) else { return };
                let v = (s, bw as i32, bh as i32);
                self.cache.insert(key, v.clone());
                v
            }
        };
        let text_w = if mode == 2 { self.small_w(&format!("{pct}%"), 12.0, BAR_W) + 5.0 * d } else { 0.0 };
        let bx = (x + text_w + (w - text_w - sw as f32) / 2.0).round();
        let by = ((self.h as f32 - sh as f32) / 2.0).round();
        let layer = self.layer.clone();
        self.sprite(&layer, &surf, bx, by, sw as f32, sh as f32, 1.0);
        let cy = self.h as f32 / 2.0;
        match mode {
            2 => {
                self.small_text(&format!("{pct}%"), 12.0, BAR_W, rgb, x + text_w / 2.0, cy, 0.95);
            }
            _ => {}
        }
    }

    fn rect(&self, parent: &ContainerVisual, x: f32, y: f32, w: f32, h: f32, r: f32, c: Color) {
        let go = || -> windows_core::Result<()> {
            let v = self.comp.CreateSpriteVisual()?;
            v.SetSize(Vector2 { X: w, Y: h })?;
            v.SetOffset(Vector3 { X: x, Y: y, Z: 0.0 })?;
            v.SetBrush(&self.comp.CreateColorBrushWithColor(c)?)?;
            if r > 0.0 {
                let g = self.comp.CreateRoundedRectangleGeometry()?;
                g.SetSize(Vector2 { X: w, Y: h })?;
                g.SetCornerRadius(Vector2 { X: r, Y: r })?;
                let clip = self.comp.CreateGeometricClip()?;
                clip.SetGeometry(&g)?;
                v.SetClip(&clip)?;
            }
            parent.Children()?.InsertAtTop(&v)
        };
        let _ = go();
    }

    fn draw_items(&mut self) {
        let lefts = self.left.clone();
        let rights = self.right.clone();
        let (app, clock) = (self.app_label(), self.clock_text());
        for p in &lefts {
            match p.id {
                LOGO => self.draw_logo(p.x, p.w),
                APP => self.draw_text(&app, APP_W, p.x, p.w, 1.0),
                n if (PROF0..PROF0 + 12).contains(&n) => {
                    let title = self.profile().get((n - PROF0) as usize).map(|m| profile_text(m.id, m.en)).unwrap_or_default();
                    self.draw_text(&title, BAR_W, p.x, p.w, 1.0)
                }
                n if n >= NATIVE0 && n < NATIVE0 + 20 => {
                    let title = self.native.get((n - NATIVE0) as usize).map(|t| native_text(&t.title)).unwrap_or_default();
                    self.draw_text(&title, BAR_W, p.x, p.w, 1.0)
                }
                _ => {}
            }
        }
        for p in &rights {
            let cx = p.x + p.w / 2.0;
            match p.id {
                CLOCK => self.draw_text(&clock, BAR_W, p.x, p.w, 1.0),
                CTRL => self.draw_glyph("\u{E9E9}", cx, 13.0, 0.95),
                SEARCH => self.draw_glyph("\u{E721}", cx, 13.0, 0.95),
                STAGEI => self.draw_glyph("\u{E8A0}", cx, 13.0, 0.95),
                WIFI => self.draw_wifi(cx),
                BATT => {
                    if !self.shadow {
                        self.draw_battery(p.x, p.w)
                    }
                }
                _ => {}
            }
        }
    }

    fn render(&mut self) {
        if let Ok(k) = self.layer.Children() {
            let _ = k.RemoveAll();
        }
        self.update_text_color();
        if !self.dark_text && !prefs::get().menubar_glass && !prefs::get().menubar_material {
            self.shadow = true;
            self.draw_items();
            self.shadow = false;
        }
        self.draw_items();
        self.update_capsule();
    }

    fn rect_of(&self, id: u32) -> Option<(f32, f32)> {
        self.left.iter().chain(self.right.iter()).find(|p| p.id == id).map(|p| (p.x, p.w))
    }

    fn update_capsule(&self) {
        use windows::Foundation::TimeSpan;
        use windows::UI::Composition::{CompositionObject, CompositionEasingFunction};
        use windows_core::{Interface, HSTRING};
        let cc = if crate::controlcenter::is_open() { Some(CTRL) } else { None };
        let active = self.open.or(cc).or(self.hover).filter(|id| *id != CLOCK || self.open.is_none());
        let d = self.dpi;
        let ms = |v: u32| TimeSpan { Duration: v as i64 * 10_000 };
        match active.and_then(|id| self.rect_of(id)) {
            Some((x, w)) => {
                let (ch, cy) = (self.h as f32 - 4.0 * d, 2.0 * d);
                let nw = w - 4.0 * d;
                let to = Vector3 { X: x + 2.0 * d, Y: cy, Z: 0.0 };
                let visible = self.capsule.Opacity().unwrap_or(0.0) > 0.5;
                let (old_off, old_w) = (self.capsule.Offset().unwrap_or(to), self.capsule.Size().map(|s| s.X).unwrap_or(nw));
                let moved = visible && ((old_off.X - to.X).abs() > 0.5 || (old_w - nw).abs() > 0.5);
                let _ = self.capsule.SetSize(Vector2 { X: nw, Y: ch });
                let _ = self.capsule.SetCenterPoint(Vector3 { X: nw / 2.0, Y: ch / 2.0, Z: 0.0 });
                let _ = self.capsule.SetOffset(to);
                let rgb = self.fg();
                let (a_top, a_bot) = if self.open.is_some() { (76u8, 46u8) } else { (52u8, 26u8) };
                if let Ok(gb) = self.comp.CreateLinearGradientBrush() {
                    let _ = gb.SetStartPoint(Vector2 { X: 0.0, Y: 0.0 });
                    let _ = gb.SetEndPoint(Vector2 { X: 0.0, Y: 1.0 });
                    if let (Ok(s0), Ok(s1), Ok(stops)) = (
                        self.comp.CreateColorGradientStopWithOffsetAndColor(0.0, Color { A: a_top, R: rgb[0], G: rgb[1], B: rgb[2] }),
                        self.comp.CreateColorGradientStopWithOffsetAndColor(1.0, Color { A: a_bot, R: rgb[0], G: rgb[1], B: rgb[2] }),
                        gb.ColorStops(),
                    ) {
                        let _ = stops.Append(&s0);
                        let _ = stops.Append(&s1);
                        let _ = self.capsule.SetBrush(&gb);
                    }
                }
                if let Ok(g) = self.comp.CreateRoundedRectangleGeometry() {
                    let _ = g.SetSize(Vector2 { X: nw, Y: ch });
                    let _ = g.SetCornerRadius(Vector2 { X: ch / 2.0, Y: ch / 2.0 });
                    if let Ok(clip) = self.comp.CreateGeometricClip() {
                        let _ = clip.SetGeometry(&g);
                        let _ = self.capsule.SetClip(&clip);
                    }
                }
                let ease = || -> windows_core::Result<CompositionEasingFunction> {
                    self.comp.CreateCubicBezierEasingFunction(Vector2 { X: 0.2, Y: 0.0 }, Vector2 { X: 0.0, Y: 1.0 })?.cast()
                };
                if !visible {
                    let go = || -> windows_core::Result<()> {
                        let e = ease()?;
                        let o = self.comp.CreateScalarKeyFrameAnimation()?;
                        o.InsertKeyFrame(0.0, 0.0)?;
                        o.InsertKeyFrameWithEasingFunction(1.0, 1.0, &e)?;
                        o.SetDuration(ms(150))?;
                        self.capsule.cast::<CompositionObject>()?.StartAnimation(&HSTRING::from("Opacity"), &o)?;
                        let sc = self.comp.CreateVector3KeyFrameAnimation()?;
                        sc.InsertKeyFrame(0.0, Vector3 { X: 0.90, Y: 0.86, Z: 1.0 })?;
                        sc.InsertKeyFrameWithEasingFunction(1.0, Vector3 { X: 1.0, Y: 1.0, Z: 1.0 }, &e)?;
                        sc.SetDuration(ms(190))?;
                        self.capsule.cast::<CompositionObject>()?.StartAnimation(&HSTRING::from("Scale"), &sc)
                    };
                    let _ = go();
                } else if moved {
                    let from_x = old_off.X + old_w / 2.0 - nw / 2.0;
                    let _ = crate::motion::vec3(&self.comp, &self.capsule, "Offset", Vector3 { X: from_x, Y: cy, Z: 0.0 }, to, crate::motion::SWITCHER_MOVE_MS + 50);
                    let _ = crate::motion::vec3(&self.comp, &self.capsule, "Scale", Vector3 { X: (old_w / nw).clamp(0.5, 2.0), Y: 1.0, Z: 1.0 }, Vector3 { X: 1.0, Y: 1.0, Z: 1.0 }, crate::motion::SWITCHER_MOVE_MS + 50);
                }
                let _ = self.capsule.SetOpacity(1.0);
            }
            None => {
                if self.capsule.Opacity().unwrap_or(0.0) > 0.01 {
                    let _ = crate::motion::scalar(&self.comp, &self.capsule, "Opacity", 1.0, 0.0, 150);
                    let _ = crate::motion::vec3(&self.comp, &self.capsule, "Scale", Vector3 { X: 1.0, Y: 1.0, Z: 1.0 }, Vector3 { X: 0.94, Y: 0.9, Z: 1.0 }, 150);
                    let _ = self.capsule.SetOpacity(0.0);
                }
            }
        }
    }

    fn press_feedback(&self) {
        use windows::Foundation::TimeSpan;
        use windows::UI::Composition::{CompositionEasingFunction, CompositionObject};
        use windows_core::{Interface, HSTRING};
        if self.capsule.Opacity().unwrap_or(0.0) < 0.5 {
            return;
        }
        let go = || -> windows_core::Result<()> {
            let e: CompositionEasingFunction = self.comp.CreateCubicBezierEasingFunction(Vector2 { X: 0.2, Y: 0.0 }, Vector2 { X: 0.0, Y: 1.0 })?.cast()?;
            let sc = self.comp.CreateVector3KeyFrameAnimation()?;
            sc.InsertKeyFrame(0.0, Vector3 { X: 1.0, Y: 1.0, Z: 1.0 })?;
            sc.InsertKeyFrameWithEasingFunction(0.25, Vector3 { X: 0.93, Y: 0.88, Z: 1.0 }, &e)?;
            sc.InsertKeyFrameWithEasingFunction(0.65, Vector3 { X: 1.035, Y: 1.04, Z: 1.0 }, &e)?;
            sc.InsertKeyFrameWithEasingFunction(1.0, Vector3 { X: 1.0, Y: 1.0, Z: 1.0 }, &e)?;
            sc.SetDuration(TimeSpan { Duration: 300 * 10_000 })?;
            self.capsule.cast::<CompositionObject>()?.StartAnimation(&HSTRING::from("Scale"), &sc)
        };
        let _ = go();
    }

    fn id_at(&self, x: f32) -> Option<u32> {
        lay::hit(&self.left, x).or_else(|| lay::hit(&self.right, x))
    }

    fn has_menu(id: u32) -> bool {
        matches!(id, LOGO | APP) || (NATIVE0..NATIVE0 + 20).contains(&id) || (PROF0..PROF0 + 12).contains(&id)
    }

    fn open_menu(&mut self, id: u32) {
        let items = self.menu_items(id);
        log::line("menubar", &format!("buka menu {id}: {} item", items.len()));
        let Some((x, _)) = self.rect_of(id) else { return };
        let (comp, gfx) = (self.comp.clone(), &self.gfx);
        let top = self.h;
        self.menu.set_bounds(self.screen_origin, self.screen);
        let r = self.menu.show_below(&comp, gfx, items, self.screen_origin.0 + x as i32, self.screen_origin.1 + top);
        if let Err(e) = &r {
            log::line("menubar", &format!("menu gagal: {e}"));
        }
        if r.is_ok() {
            self.open = Some(id);
            self.update_capsule();
            self.update_visibility();
        }
    }

    fn close_menu(&mut self) {
        let was = self.open.is_some();
        self.menu.hide();
        self.open = None;
        self.update_capsule();
        self.update_visibility();
    }

    fn menu_items(&self, id: u32) -> Vec<Item> {
        let items = self.menu_items_raw(id);
        if (NATIVE0..NATIVE0 + 20).contains(&id) {
            return items;
        }
        items.into_iter().map(|it| { let s = nacre_core::symbols::for_command(it.id); it.with_sym(s) }).collect()
    }

    fn menu_items_raw(&self, id: u32) -> Vec<Item> {
        let a = |id: usize, label: &str| Item::action(id, label, true);
        if (NATIVE0..NATIVE0 + 20).contains(&id) {
            let tops = nacre_platform::menus::read(self.native_hwnd).unwrap_or_else(|| self.native.clone());
            let Some(top) = tops.get((id - NATIVE0) as usize) else { return vec![] };
            return top
                .entries
                .iter()
                .map(|e| {
                    if e.separator {
                        return Item::separator();
                    }
                    let pad = "    ".repeat(e.depth as usize);
                    let label = format!("{}{}{}", if e.checked { "\u{2713} " } else { "" }, pad, native_text(&e.label));
                    let it = Item::action(if e.header { 0 } else { NATIVE_CMD + e.id as usize }, &label, e.enabled && !e.header);
                    if e.accel.is_empty() { it } else { it.with_accel(&e.accel) }
                })
                .collect();
        }
        match id {
            CTRL => {
                let on = |b: Option<bool>| match b {
                    Some(true) => format!("\u{2713} {}", prefs::tr("Aktif")),
                    Some(false) => prefs::tr("Mati").to_string(),
                    None => "-".to_string(),
                };
                let vol = nacre_platform::audio::latest();
                let vtxt = if vol.available { format!("{} {}%{}", prefs::tr("Volume"), (vol.level * 100.0).round() as i32, if vol.muted { format!(" {}", prefs::tr("(senyap)")) } else { String::new() }) } else { prefs::tr("Volume").to_string() };
                let btxt = match nacre_platform::brightness::latest() {
                    Some(v) => format!("{} {}%", prefs::tr("Kecerahan"), v),
                    None => prefs::tr("Kecerahan").to_string(),
                };
                vec![
                    a(800, &format!("Wi-Fi  \u{2013}  {}", on(self.wifi))).with_accel(""),
                    a(801, &format!("Bluetooth  \u{2013}  {}", on(self.bt))),
                    Item::separator(),
                    Item::action(0, &vtxt, false),
                    Item::action(802, &prefs::tr("Naikkan Volume"), vol.available),
                    Item::action(803, &prefs::tr("Turunkan Volume"), vol.available),
                    Item::action(804, &prefs::tr("Bisukan / Bunyikan"), vol.available),
                    Item::separator(),
                    Item::action(0, &btxt, false),
                    Item::action(805, &prefs::tr("Lebih Terang"), nacre_platform::brightness::latest().is_some()),
                    Item::action(806, &prefs::tr("Lebih Redup"), nacre_platform::brightness::latest().is_some()),
                    Item::separator(),
                    a(807, &format!("{}{}", if prefs::get().stage { "\u{2713} " } else { "" }, prefs::tr("Scene"))),
                    a(808, &prefs::tr("Overview")).with_accel("Ctrl+↑"),
                    a(809, &prefs::tr("Apps")).with_accel("Ctrl+Shift+Space"),
                    Item::separator(),
                    a(810, &prefs::tr("Pengaturan ABRA…")),
                    a(811, &prefs::tr("Pengaturan Sistem…")),
                ]
            }
            LOGO => vec![
                a(100, &prefs::tr("Tentang PC Ini")),
                Item::separator(),
                a(101, &prefs::tr("Pengaturan Sistem…")),
                a(102, &prefs::tr("Toko Microsoft…")),
                Item::separator(),
                a(103, &prefs::tr("Pengaturan ABRA…")),
                a(105, &prefs::tr("Apps")).with_accel("Ctrl+Shift+Space"),
                a(106, &prefs::tr("Overview")).with_accel("Ctrl+↑"),
                a(104, &prefs::tr("Paksa Keluar…")),
                Item::separator(),
                a(110, &prefs::tr("Tidur")),
                a(111, &prefs::tr("Mulai Ulang…")),
                a(112, &prefs::tr("Matikan…")),
                Item::separator(),
                a(113, &prefs::tr("Kunci Layar")),
                a(114, &prefs::tr("Keluar…")),
            ],
            APP => {
                let name = self.app_label();
                vec![
                    a(200, &format!("{} {}", prefs::tr("Tentang"), name)),
                    Item::separator(),
                    a(201, &format!("{} {}", prefs::tr("Sembunyikan"), name)).with_accel("Ctrl+Alt+H"),
                    a(202, &prefs::tr("Sembunyikan Lainnya")),
                    a(203, &prefs::tr("Tampilkan Semua")),
                    Item::separator(),
                    a(204, &format!("{} {}", prefs::tr("Keluar dari"), name)).with_accel("Ctrl+Alt+Q"),
                ]
            }
            n if (PROF0..PROF0 + 12).contains(&n) => {
                let Some(m) = self.profile().get((n - PROF0) as usize) else { return vec![] };
                m.lines
                    .iter()
                    .enumerate()
                    .map(|(j, l)| match l {
                        Line::Sep => Item::separator(),
                        Line::Cmd { id, en, accel, .. } => {
                            let it = a(PROF_CMD + (n - PROF0) as usize * 100 + j, &profile_text(id, en));
                            if accel.is_empty() { it } else { it.with_accel(accel) }
                        }
                    })
                    .collect()
            }
            _ => vec![],
        }
    }

    fn click(&mut self, id: u32) {
        if Self::has_menu(id) {
            if self.open == Some(id) {
                self.close_menu();
            } else {
                self.open_menu(id);
            }
            return;
        }
        match id {
            CTRL => {
                self.close_menu();
                crate::controlcenter::toggle_at(self.screen_origin.1 + self.h, self.screen_bounds());
            }
            STAGEI => prefs::update(|st| st.stage = !st.stage),
            SEARCH => crate::spotlight::toggle_at(self.screen_bounds()),
            WIFI => {
                if let Some((x, _)) = self.rect_of(WIFI) {
                    crate::netmenu::show(&self.comp, crate::netmenu::Which::Wifi,
                        crate::netmenu::Anchor::Below(self.screen_origin.0 + x as i32, self.screen_origin.1 + self.h), self.dpi);
                }
            }
            BATT => launch::open("ms-settings:batterysaver"),
            CLOCK => keys::chord(&[keys::WIN, 0x4E]),
            _ => {}
        }
    }

    fn pick(&mut self, cmd: usize) {
        self.close_menu();
        if cmd >= PROF_CMD {
            let (mi, li) = ((cmd - PROF_CMD) / 100, (cmd - PROF_CMD) % 100);
            match self.profile().get(mi).and_then(|m| m.lines.get(li)) {
                Some(Line::Cmd { act: Act::Keys(k), .. }) => keys::chord(k),
                Some(Line::Cmd { act: Act::Guide, .. }) => launch::open(GUIDE_URL),
                _ => {}
            }
            return;
        }
        if cmd >= NATIVE_CMD {
            nacre_platform::menus::invoke(self.native_hwnd, (cmd - NATIVE_CMD) as u32);
            return;
        }
        match cmd {
            100 => launch::open("ms-settings:about"),
            101 => launch::open("ms-settings:"),
            102 => launch::open("ms-windows-store:"),
            103 => crate::settings_win::open(&self.comp),
            104 => launch::show("taskmgr.exe", ""),
            105 => crate::launchpad::toggle_at(self.screen_bounds()),
            106 => crate::mission::toggle(),
            110 => confirm_power("Tidur sekarang?", "rundll32.exe", "powrprof.dll,SetSuspendState 0,1,0"),
            111 => confirm_power("Mulai ulang komputer sekarang?", "shutdown.exe", "/r /t 0"),
            112 => confirm_power("Matikan komputer sekarang?", "shutdown.exe", "/s /t 0"),
            113 => launch::run("rundll32.exe", "user32.dll,LockWorkStation"),
            114 => confirm_power("Keluar dari akun sekarang?", "shutdown.exe", "/l"),
            800 | 801 => {
                if let Some(rd) = &self.radios {
                    let (k, cur) = if cmd == 800 { (nacre_platform::radio::Kind::Wifi, self.wifi) } else { (nacre_platform::radio::Kind::Bluetooth, self.bt) };
                    let _ = rd.commands.send(nacre_platform::radio::Cmd::Set(k, !cur.unwrap_or(false)));
                }
            }
            802 | 803 => {
                let v = nacre_platform::audio::latest().level;
                services::set_volume((v + if cmd == 802 { 0.1 } else { -0.1 }).clamp(0.0, 1.0));
            }
            804 => services::toggle_mute(),
            805 | 806 => {
                if let Some(v) = nacre_platform::brightness::latest() {
                    services::set_brightness((v as f32 / 100.0 + if cmd == 805 { 0.1 } else { -0.1 }).clamp(0.0, 1.0));
                }
            }
            807 => prefs::update(|st| st.stage = !st.stage),
            808 => crate::mission::toggle(),
            809 => crate::launchpad::toggle_at(self.screen_bounds()),
            810 => crate::settings_win::open(&self.comp),
            811 => launch::open("ms-settings:"),
            200 => self.about_app(),
            201 => self.hide_app(false),
            202 => self.hide_app(true),
            203 => self.show_all(),
            204 => crate::macshortcuts::quit_foreground_app(),
            _ => {}
        }
    }

    fn profile(&self) -> &'static [AppMenu] {
        nacre_core::appmenus::for_path(&self.app_path)
    }

    fn about_app(&self) {
        let path = self.app_path.clone();
        if path.is_empty() {
            crate::settings_win::open(&self.comp);
        } else if path.to_lowercase().starts_with("shell:") || path.contains('?') {
            launch::open("ms-settings:appsfeatures");
        } else if !launch::properties(&path) {
            launch::open("ms-settings:appsfeatures");
        }
    }

    fn hide_app(&mut self, others: bool) {
        let fg = focus::foreground();
        let wins = enumerate::list(std::process::id());
        let Some(cur) = wins.iter().find(|w| w.hwnd == fg).map(|w| w.path.to_lowercase()) else { return };
        for w in &wins {
            let same = w.path.to_lowercase() == cur;
            if same != others && !focus::is_minimized(w.hwnd) {
                unsafe {
                    let _ = ShowWindow(HWND(w.hwnd as *mut _), SW_MINIMIZE);
                }
                self.hidden.push(w.hwnd);
            }
        }
    }

    fn show_all(&mut self) {
        for h in std::mem::take(&mut self.hidden) {
            if focus::is_alive(h) && focus::is_minimized(h) {
                unsafe {
                    let _ = ShowWindow(HWND(h as *mut _), SW_SHOWNOACTIVATE);
                }
            }
        }
    }
}

fn confirm_power(question: &'static str, file: &'static str, params: &'static str) {
    let text = prefs::tr(question).to_string();
    std::thread::spawn(move || unsafe {
        use windows::core::HSTRING;
        let r = MessageBoxW(None, &HSTRING::from(text), w!("ABRA"), MB_YESNO | MB_ICONQUESTION | MB_TOPMOST | MB_SETFOREGROUND);
        if r == IDYES {
            launch::run(file, params);
        }
    });
}

fn xy(lp: LPARAM) -> (f32, f32) {
    ((lp.0 as i16) as f32, ((lp.0 >> 16) as i16) as f32)
}

unsafe extern "system" fn bar_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    ACTIVE_BAR.with(|active| active.set(hwnd.0 as isize));
    match msg {
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_MOUSEMOVE => {
            let (x, _) = xy(lp);
            with_bar(|b| {
                if !b.tracking {
                    let mut t = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
                    if TrackMouseEvent(&mut t).is_ok() {
                        b.tracking = true;
                    }
                }
                if b.grace {
                    b.grace = false;
                    let _ = KillTimer(Some(hwnd), T_GRACE);
                }
                let id = b.id_at(x);
                if id != b.hover {
                    b.hover = id;
                    if let (Some(open), Some(new)) = (b.open, id) {
                        if new != open && Bar::has_menu(new) {
                            b.open_menu(new);
                        }
                    }
                    b.update_capsule();
                }
                b.update_visibility();
            });
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            with_bar(|b| {
                b.tracking = false;
                b.hover = None;
                b.update_capsule();
                if b.behavior() == Behavior::Smart && b.open.is_none() {
                    b.grace = true;
                    SetTimer(Some(hwnd), T_GRACE, 800, None);
                }
                b.update_visibility();
            });
            LRESULT(0)
        }
        STRIP_ENTER => {
            with_bar(|b| b.on_strip_enter());
            LRESULT(0)
        }
        STRIP_LEAVE => {
            with_bar(|b| b.on_strip_leave());
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let (x, _) = xy(lp);
            with_bar(|b| {
                if let Some(id) = b.id_at(x) {
                    b.press_feedback();
                    b.click(id);
                }
            });
            LRESULT(0)
        }
        MENU_PICK => {
            with_bar(|b| b.pick(wp.0));
            LRESULT(0)
        }
        MENU_CLOSE => {
            with_bar(|b| b.close_menu());
            LRESULT(0)
        }
        WM_NOTCH => {
            with_bar(|b| {
                b.relayout();
                b.render();
            });
            LRESULT(0)
        }
        WM_TEST_NET => {
            if wp.0 == 3 {
                crate::netmenu::test_toggle_others();
            } else if wp.0 == 4 || wp.0 == 8 {
                with_bar(|b| crate::controlcenter::toggle_at(b.screen_origin.1 + b.h, b.screen_bounds()));
            } else if wp.0 == 5 {
                crate::controlcenter::test_open_picker();
            } else if wp.0 == 9 {
                crate::controlcenter::test_enter_edit();
            } else if wp.0 == 7 {
                crate::netmenu::test_click_audio();
            } else {
                let which = if wp.0 == 1 || wp.0 == 6 { crate::netmenu::Which::Bluetooth } else { crate::netmenu::Which::Wifi };
                with_bar(|b| {
                    if let Some((x, _)) = b.rect_of(WIFI) {
                        crate::netmenu::show(&b.comp, which,
                            crate::netmenu::Anchor::Below(b.screen_origin.0 + x as i32, b.screen_origin.1 + b.h), b.dpi);
                    }
                });
            }
            LRESULT(0)
        }
        m if m == WM_APP + 22 => {
            with_bar(|b| {
                if let Some(r) = &b.radios {
                    while let Ok(s) = r.snapshots.try_recv() {
                        b.wifi = s.wifi;
                        b.bt = s.bluetooth;
                    }
                }
                b.relayout();
                b.render();
            });
            LRESULT(0)
        }
        WM_TIMER if wp.0 == T_SIG => {
            with_bar(|b| {
                let v = nacre_platform::wifi::link().map(|l| l.1);
                if v != b.wifi_bars {
                    b.wifi_bars = v;
                    b.render();
                }
            });
            LRESULT(0)
        }
        WM_TIMER => {
            let id = wp.0;
            if id == T_EDGE {
                with_bar(|b| {
                    b.edge_armed = true;
                    b.update_visibility();
                });
                return LRESULT(0);
            }
            if id == T_GRACE {
                with_bar(|b| {
                    b.grace = false;
                    b.update_visibility();
                });
                return LRESULT(0);
            }
            if id == T_HIDEWIN {
                with_bar(|b| {
                    if !b.shown {
                        b.popup.hide();
                    }
                });
                return LRESULT(0);
            }
            if id == T_CLOCK {
                let _ = SetTimer(Some(hwnd), T_CLOCK, time::ms_to_next_minute().max(200), None);
            }
            with_bar(|b| {
                b.relayout();
                b.render();
            });
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

fn knock_out(data: &mut [u8], w: u32, h: u32, text: &nacre_platform::icons::Bitmap) {
    let (tw, th) = (text.w as usize, text.h as usize);
    let (mut x0, mut y0, mut x1, mut y1) = (tw, th, 0usize, 0usize);
    for y in 0..th {
        for x in 0..tw {
            if text.bgra[(y * tw + x) * 4 + 3] > 24 {
                x0 = x0.min(x);
                x1 = x1.max(x);
                y0 = y0.min(y);
                y1 = y1.max(y);
            }
        }
    }
    if x0 > x1 {
        return;
    }
    let nub = (w as f32 * 0.06).max(1.5);
    let body = w as f32 - nub - 1.0;
    let (iw, ih) = ((x1 - x0 + 1) as f32, (y1 - y0 + 1) as f32);
    let ox = ((body - iw) / 2.0).round() as i32 - x0 as i32;
    let oy = ((h as f32 - ih) / 2.0).round() as i32 - y0 as i32;
    for y in 0..th {
        for x in 0..tw {
            let m = text.bgra[(y * tw + x) * 4 + 3] as u32;
            if m == 0 {
                continue;
            }
            let (dx, dy) = (x as i32 + ox, y as i32 + oy);
            if dx < 0 || dy < 0 || dx >= w as i32 || dy >= h as i32 {
                continue;
            }
            let i = ((dy as u32 * w + dx as u32) * 4) as usize;
            for c in 0..4 {
                data[i + c] = (data[i + c] as u32 * (255 - m) / 255) as u8;
            }
        }
    }
}
