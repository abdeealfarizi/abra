//! Menu bar ala macOS Tahoe di atas layar: kaca seperti dock, logo ABRA di ujung kiri, nama app aktif (tebal) dan
//! menu standar, ikon status di kanan, dan celah di tengah untuk notch (item kiri yang tak muat disembunyikan
//! seperti MacBook ber-notch). Dropdown memakai menu kaca bersama (`dock::menu`); hover ke judul lain saat satu
//! menu terbuka langsung berpindah. Dibuat saat dinyalakan dan dihancurkan saat dimatikan (tanpa RAM saat mati).

use crate::dock::menu::{Item, Menu, MENU_CLOSE, MENU_PICK};
use crate::popup::Popup;
use crate::{prefs, services};
use nacre_core::menubar::{self as lay, Gap, Item as LItem, Placed};
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
/// Pesan ke jendela kontrol: foreground berubah.
const WM_APPCHG: u32 = WM_APP + 70;
/// Klik mouse pada jendela (wp = hwnd akar): nama app di menu bar mengikuti jendela yang terakhir diklik.
const WM_CLICKWIN: u32 = WM_APP + 74;
static MOUSE_HOOK: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
static BAR_CTL: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
/// Pesan ke jendela bar: posisi notch berubah (diposting `services::set_notch`).
const WM_NOTCH: u32 = WM_APP + 71;
/// Khusus uji (`NACRE_TEST_NETMENU`): buka menu Wi-Fi/Bluetooth tanpa klik.
const WM_TEST_NET: u32 = WM_APP + 75;
const T_CLOCK: usize = 1;
const T_STATUS: usize = 2;
const T_APP: usize = 3;
const T_FS: usize = 4;

// id item bar
const LOGO: u32 = 1;
const APP: u32 = 2;
const M_FILE: u32 = 3;
const M_EDIT: u32 = 4;
const M_VIEW: u32 = 5;
const M_WIN: u32 = 6;
const M_HELP: u32 = 7;
const CLOCK: u32 = 10;
const CTRL: u32 = 11;
const SEARCH: u32 = 12;
const WIFI: u32 = 13;
const BATT: u32 = 14;
const STAGEI: u32 = 15;
/// Judul menu klasik app aktif: NATIVE0 + indeks. Id perintahnya: NATIVE_CMD + id WM_COMMAND.
const NATIVE0: u32 = 50;
const NATIVE_CMD: usize = 100_000;

struct Bar {
    popup: Popup,
    comp: Compositor,
    gfx: Gfx,
    dpi: f32,
    screen: (i32, i32),
    h: i32,
    layer: ContainerVisual,
    capsule: SpriteVisual,
    /// Latar rata (putih/hitam pekat) untuk gaya Terang/Gelap.
    bg: SpriteVisual,
    /// Garis tipis di tepi bawah bar (pemisah halus ala macOS).
    hair: SpriteVisual,
    cache: HashMap<String, (CompositionDrawingSurface, i32, i32)>,
    left: Vec<Placed>,
    right: Vec<Placed>,
    app: String,
    hidden: Vec<isize>,
    menu: Menu,
    open: Option<u32>,
    hover: Option<u32>,
    tracking: bool,
    wifi: Option<bool>,
    bt: Option<bool>,
    radios: Option<radio::Radios>,
    /// Teks gelap (latar terang) atau putih (latar gelap); dipilih dari kecerahan layar di bawah bar.
    dark_text: bool,
    /// Sedang menggambar bayangan teks (hitam, turun 1 px, buram tipis).
    shadow: bool,
    /// Menu klasik (HMENU) jendela aktif; kosong bila app tidak punya (dipakai menu standar).
    native: Vec<nacre_platform::menus::Top>,
    native_hwnd: isize,
    /// Jendela aktif memenuhi layar: bar disembunyikan.
    fullscreen: bool,
}

struct Ctl {
    hwnd: HWND,
    comp: Compositor,
}

thread_local! {
    static BAR: RefCell<Option<Box<Bar>>> = const { RefCell::new(None) };
    static CTL: RefCell<Option<Ctl>> = const { RefCell::new(None) };
}

fn with_bar<R>(f: impl FnOnce(&mut Bar) -> R) -> Option<R> {
    BAR.with(|c| c.try_borrow_mut().ok().and_then(|mut b| b.as_mut().map(|b| f(b))))
}

/// Minta keadaan radio terbaru (sakelar Wi-Fi/Bluetooth dibalik dari menu jaringan) agar ikon Wi-Fi ikut berubah.
pub fn refresh_radios() {
    with_bar(|b| {
        if let Some(r) = &b.radios {
            let _ = r.commands.send(radio::Cmd::Refresh);
        }
    });
}

/// Judul menu: kunci berawalan "Menu:" memisahkan terjemahan dari tab Settings bernama sama.
fn mt(key: &'static str) -> String {
    let t = prefs::tr(key);
    t.strip_prefix("Menu:").unwrap_or(t).to_string()
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

// ---------------------------------------------------------------------------------------------------------------
// siklus hidup
// ---------------------------------------------------------------------------------------------------------------

fn noop_fg() {
    CTL.with(|c| {
        if let Some(ctl) = c.borrow().as_ref() {
            unsafe {
                let _ = PostMessageW(Some(ctl.hwnd), WM_APPCHG, WPARAM(0), LPARAM(0));
            }
        }
    });
}

/// Hook mouse tingkat rendah: pada klik tombol, kirim jendela akar di bawah kursor ke menu bar (hanya meneruskan, tanpa kerja berat).
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

/// Perubahan ukuran jendela depan (mis. F11): periksa ulang mode layar penuh.
fn loc_hook(ev: u32, hwnd: isize) {
    if ev == tracker::EV_LOCATION && hwnd != 0 && hwnd == focus::foreground() {
        CTL.with(|c| {
            if let Some(ctl) = c.borrow().as_ref() {
                unsafe {
                    // timer sekali-tembak (debounce): hanya memeriksa layar penuh, tanpa mengenumerasi jendela
                    SetTimer(Some(ctl.hwnd), T_FS, 200, None);
                }
            }
        });
    }
}

fn is_fullscreen(hwnd: isize) -> bool {
    unsafe {
        let h = HWND(hwnd as *mut _);
        let mut cls = [0u16; 64];
        let n = GetClassNameW(h, &mut cls);
        let cls = String::from_utf16_lossy(&cls[..n.max(0) as usize]);
        if cls == "Progman" || cls == "WorkerW" || cls == "Shell_TrayWnd" {
            return false;
        }
        let mut r = windows::Win32::Foundation::RECT::default();
        if GetWindowRect(h, &mut r).is_err() {
            return false;
        }
        let (sw, sh) = sys::screen_size();
        r.left <= 0 && r.top <= 0 && r.right >= sw && r.bottom >= sh
    }
}

/// Siapkan pendengar pengaturan; bar dibuat/dihancurkan sesuai `menubar` di pengaturan.
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
        BAR_CTL.store(hwnd.0 as isize, std::sync::atomic::Ordering::Relaxed);
        if let Ok(h) = SetWindowsHookExW(WH_MOUSE_LL, Some(ll_mouse), None, 0) {
            MOUSE_HOOK.store(h.0 as isize, std::sync::atomic::Ordering::Relaxed);
        }
        apply_prefs();
    }
    Ok(())
}

/// Nyalakan/matikan bar sesuai pengaturan; juga menyegarkan tampilan bila sudah ada.
fn apply_prefs() {
    let want = prefs::get().menubar;
    let have = BAR.with(|c| c.borrow().is_some());
    if want && !have {
        let comp = CTL.with(|c| c.borrow().as_ref().map(|c| c.comp.clone()));
        if let Some(comp) = comp {
            match create(&comp) {
                Ok(b) => {
                    let hwnd = b.popup.hwnd;
                    BAR.with(|c| *c.borrow_mut() = Some(b));
                    services::register_bar(hwnd);
                    with_bar(|b| {
                        b.refresh_app();
                        b.relayout();
                        b.render();
                    });
                    // notch di atas bar agar celahnya tampak utuh
                    services::post_island(WM_APP + 72);
                    CTL.with(|c| {
                        if let Some(ctl) = c.borrow().as_ref() {
                            unsafe {
                                SetTimer(Some(ctl.hwnd), T_APP, 900, None);
                            }
                        }
                    });
                    log::line("menubar", "dibuat");
                    crate::hotcorners::raise();
                }
                Err(e) => log::line("menubar", &format!("gagal: {e}")),
            }
        }
    } else if !want && have {
        let b = BAR.with(|c| c.borrow_mut().take());
        if let Some(b) = b {
            b.menu.hide();
            unsafe {
                let _ = DestroyWindow(b.popup.hwnd);
            }
        }
        shell::appbar::release_top();
        log::line("menubar", "dihancurkan");
        // notch boleh memesan ruang atasnya lagi
        services::post_island(crate::prefs::WM_PREFS);
    } else if want {
        with_bar(|b| {
            b.apply_look();
            b.relayout();
            b.render();
        });
    }
}

fn create(comp: &Compositor) -> windows_core::Result<Box<Bar>> {
    let screen = sys::screen_size();
    let glass = prefs::apply(nacre_ui::glass::GlassParams::dark((10.0, 10.0), 0.0), &prefs::bar_look());
    let mut popup = Popup::new(comp, w!("AbraMenuBar"), Some(bar_proc), WINDOW_EX_STYLE(0), glass)?;
    let dpi = unsafe { GetDpiForWindow(popup.hwnd) }.max(96) as f32 / 96.0;
    let notch_h = services::notch().map(|(_, _, h)| h as f32 / dpi);
    let h = (notch_h.unwrap_or(26.0) * dpi).round() as i32;
    let tier = {
        let c = sys::conditions();
        if !c.transparency || c.battery_saver || c.remote_session { Tier::Solid } else { Tier::Full }
    };
    let _ = popup.panel.set_tier(tier);
    popup.set_rect(0, 0, screen.0, h)?;
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
    let gfx = Gfx::new(comp)?;
    let menu = Menu::new(comp, popup.hwnd, dpi, screen, tier)?;
    popup.show();
    shell::appbar::reserve_top(popup.hwnd, screen.0, h);
    unsafe {
        SetTimer(Some(popup.hwnd), T_CLOCK, time::ms_to_next_minute().max(200), None);
        SetTimer(Some(popup.hwnd), T_STATUS, 60_000, None);
    }
    // Wi-Fi: baca status radio sekali dan setiap berubah (pekerja menyala hanya sekali)
    let hw = popup.hwnd.0 as isize;
    let radios = radio::spawn(move || unsafe {
        let _ = PostMessageW(Some(HWND(hw as *mut _)), WM_APP + 22, WPARAM(0), LPARAM(0));
    });
    // uji tanpa klik: "wifi", "wifi-others" (Jaringan Lain dibuka), "bt"
    if let Ok(v) = std::env::var("NACRE_TEST_NETMENU") {
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(2500));
            let code = match v.as_str() {
                "bt" => 1,
                "wifi-others" => 2,
                "cc" => 4,
                "bt-click" => 6,
                _ => 0,
            };
            unsafe {
                let _ = PostMessageW(Some(HWND(hw as *mut _)), WM_TEST_NET, WPARAM(code), LPARAM(0));
            }
            if code == 2 || code == 4 || code == 6 {
                std::thread::sleep(std::time::Duration::from_millis(2500));
                unsafe {
                    let _ = PostMessageW(Some(HWND(hw as *mut _)), WM_TEST_NET, WPARAM(code + 1), LPARAM(0));
                }
            }
        });
    }
    let mut bar = Box::new(Bar {
        popup,
        comp: comp.clone(),
        gfx,
        dpi,
        screen,
        h,
        layer,
        capsule,
        bg,
        hair,
        cache: HashMap::new(),
        left: vec![],
        right: vec![],
        app: String::new(),
        hidden: vec![],
        menu,
        open: None,
        hover: None,
        tracking: false,
        wifi: None,
        bt: None,
        radios: Some(radios),
        dark_text: false,
        shadow: false,
        native: vec![],
        native_hwnd: 0,
        fullscreen: false,
    });
    bar.apply_bg();
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
            // abaikan jendela ABRA sendiri, desktop, dan taskbar/tray Windows
            let shell = matches!(cls.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "NotifyIconOverflowWindow" | "TopLevelWindowForOverflowXamlIsland");
            if pid != std::process::id() && !shell {
                with_bar(|b| {
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
            with_bar(|b| {
                b.check_fullscreen();
                b.recheck_color();
            });
            LRESULT(0)
        }
        WM_TIMER if wp.0 == T_APP => {
            let _ = KillTimer(Some(hwnd), T_APP);
            with_bar(|b| {
                if b.refresh_app() {
                    b.relayout();
                    b.render();
                } else {
                    b.recheck_color();
                }
            });
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

// ---------------------------------------------------------------------------------------------------------------
// tampilan
// ---------------------------------------------------------------------------------------------------------------

impl Bar {
    fn fg(&self) -> [u8; 3] {
        if self.shadow {
            [0, 0, 0]
        } else if self.dark_text {
            [18, 18, 22]
        } else {
            [255, 255, 255]
        }
    }

    /// Latar bar: transparan seperti Tahoe (bawaan), atau warna rata Terang/Gelap.
    fn apply_bg(&mut self) {
        use nacre_core::settings::BarStyle;
        let p = prefs::get();
        let d = self.dpi;
        let flat = if p.menubar_glass && !p.menubar_material { Some(match p.menubar_style { BarStyle::Light => (248, 248, 250), BarStyle::Dark => (0, 0, 0) }) } else { None };
        if p.menubar_material {
            // strip memakai material global (Blur / Liquid Glass / Hitam buram) lewat panel kaca yang sama dengan dock
            let look = prefs::look(prefs::Role::Themed);
            let base = nacre_ui::glass::GlassParams::dark((self.screen.0 as f32, self.h as f32), 0.0);
            // strip lebar layar: tanpa garis tepi/kilau kaca (rim akan terlihat sebagai outline putih di sekeliling bar)
            let mut gp = prefs::apply(base, &look);
            gp.rim.A = 0;
            gp.highlight = 0.0;
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
            None => {
                let _ = self.bg.SetOpacity(0.0);
            }
        }
        // garis tepi bawah: hanya untuk latar rata
        let line = d.round().max(1.0);
        let _ = self.hair.SetSize(Vector2 { X: self.screen.0 as f32, Y: line });
        let _ = self.hair.SetOffset(Vector3 { X: 0.0, Y: self.h as f32 - line, Z: 0.0 });
        let hc = match (p.menubar_glass && !p.menubar_material, p.menubar_style) {
            _ if p.menubar_material => {
                // garis tepi bawah halus mengikuti terang/gelap material
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

    /// Warna teks bar: selalu putih seperti macOS Tahoe, kecuali latar rata Terang (putih di atas putih tak terbaca).
    /// Mengembalikan true bila warna teks berubah.
    fn update_text_color(&mut self) -> bool {
        use nacre_core::settings::BarStyle;
        let before = self.dark_text;
        let p = prefs::get();
        self.dark_text = p.menubar_glass && !p.menubar_material && p.menubar_style == BarStyle::Light;
        before != self.dark_text
    }

    /// Ukur ulang warna teks (wallpaper/jendela di bawah bar berubah) dan gambar ulang hanya bila berbalik.
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

    /// Layar penuh: sembunyikan bar (muncul lagi saat jendela depan tidak penuh).
    fn check_fullscreen(&mut self) {
        let fg = focus::foreground();
        if fg == 0 || fg == self.popup.hwnd.0 as isize {
            return;
        }
        let full = is_fullscreen(fg) && !prefs::get().above_fullscreen;
        if full != self.fullscreen {
            self.fullscreen = full;
            if full {
                self.close_menu();
                self.popup.hide();
            } else {
                self.popup.show();
            }
        }
    }

    /// Nama app yang sedang aktif. Mengembalikan true bila berubah.
    fn refresh_app(&mut self) -> bool {
        self.refresh_app_for(focus::foreground())
    }

    /// Sama seperti `refresh_app` tetapi untuk jendela `fg` tertentu (mis. jendela yang baru diklik).
    fn refresh_app_for(&mut self, fg: isize) -> bool {
        if fg == 0 || fg == self.popup.hwnd.0 as isize {
            return false;
        }
        let mut changed = false;
        self.check_fullscreen();
        // menu klasik app aktif
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
                if !w.name.is_empty() {
                    w.name.clone()
                } else {
                    nacre_core::dock::items::name_from_path(&w.path)
                }
            })
            .filter(|n| !n.is_empty());
        let Some(name) = name else { return changed };
        if name == self.app {
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
            LItem { id: APP, width: t(&self.app_label(), 650) },
        ];
        if self.native.is_empty() {
            left.push(LItem { id: M_FILE, width: t(&mt("Berkas"), 500) });
            left.push(LItem { id: M_EDIT, width: t(&mt("Edit"), 500) });
            left.push(LItem { id: M_VIEW, width: t(&mt("Menu:Tampilan"), 500) });
            left.push(LItem { id: M_WIN, width: t(&mt("Menu:Jendela"), 500) });
            left.push(LItem { id: M_HELP, width: t(&mt("Bantuan"), 500) });
        } else {
            for (i, top) in self.native.iter().enumerate().take(20) {
                left.push(LItem { id: NATIVE0 + i as u32, width: t(&top.title, 500) });
            }
        }
        let icon_w = 30.0 * d;
        let mut right = vec![
            LItem { id: CLOCK, width: t(&self.clock_text(), 500) },
            LItem { id: CTRL, width: icon_w },
            LItem { id: SEARCH, width: icon_w },
        ];
        if prefs::get().stage {
            right.push(LItem { id: STAGEI, width: icon_w });
        }
        if self.wifi.is_some() {
            right.push(LItem { id: WIFI, width: icon_w });
        }
        if sys::battery().is_some() {
            right.push(LItem { id: BATT, width: 40.0 * d });
        }
        // notch: celah di tengah (px layar); tinggi bar mengikuti pil notch
        let gap = services::notch().map(|(l, r, _)| Gap { left: l as f32, right: r as f32 });
        let (l, r) = lay::layout(self.screen.0 as f32, 6.0 * d, &left, &right, gap);
        self.left = l;
        self.right = r;
        let want_h = services::notch().map(|(_, _, h)| h).unwrap_or((26.0 * d).round() as i32);
        if want_h != self.h {
            self.h = want_h;
            let _ = self.popup.set_rect(0, 0, self.screen.0, self.h);
            let _ = self.popup.panel.set_radius(0.0);
            self.apply_bg();
            shell::appbar::reserve_top(self.popup.hwnd, self.screen.0, self.h);
        }
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
        let mon = match lang {
            nacre_core::i18n::Lang::En => months_en[(n.month - 1) as usize % 12],
            nacre_core::i18n::Lang::Id => months_id[(n.month - 1) as usize % 12],
        };
        let time = if prefs::get().clock_24h {
            format!("{:02}.{:02}", n.hour, n.minute)
        } else {
            let (h12, ap) = (if n.hour % 12 == 0 { 12 } else { n.hour % 12 }, if n.hour < 12 { "AM" } else { "PM" });
            format!("{h12}:{:02} {ap}", n.minute)
        };
        match lang {
            nacre_core::i18n::Lang::En => format!("{} {} {}  {}", days[wd % 7], mon, n.day, time),
            nacre_core::i18n::Lang::Id => format!("{} {} {}  {}", days[wd % 7], n.day, mon, time),
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

    /// Teks di dalam item [x, x+w] berpusat horizontal.
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
        // simbol vektor buatan sendiri (bentuk ala Tahoe); glyph lain tetap dari font ikon
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
                // pusatkan glyph pada kotak
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
        let key = format!("logo|{size}|{rgb:?}");
        let hit = self.cache.get(&key).cloned();
        let (surf, lw, lh) = match hit {
            Some(v) => v,
            None => {
                let data = nacre_core::logo::render_badge(size, rgb);
                let Ok(s) = self.gfx.surface_from_bgra(size, size, &data) else { return };
                let v = (s, size as i32, size as i32);
                self.cache.insert(key, v.clone());
                v
            }
        };
        let layer = self.layer.clone();
        self.sprite(&layer, &surf, x + (w - lw as f32) / 2.0, (self.h as f32 - lh as f32) / 2.0, lw as f32, lh as f32, 1.0);
    }

    fn draw_battery(&mut self, x: f32, w: f32) {
        let Some((pct, charging)) = sys::battery() else { return };
        let d = self.dpi;
        let rgb = self.fg();
        let (bw, bh) = ((26.0 * d).round() as u32, (12.0 * d).round() as u32);
        let fill = if charging { [52, 199, 89] } else if pct <= 10 { [255, 69, 58] } else { rgb };
        let key = format!("batt|{bw}|{bh}|{pct}|{rgb:?}|{fill:?}");
        let hit = self.cache.get(&key).cloned();
        let (surf, sw, sh) = match hit {
            Some(v) => v,
            None => {
                let data = nacre_core::logo::render_battery(bw, bh, pct, rgb, fill);
                let Ok(s) = self.gfx.surface_from_bgra(bw, bh, &data) else { return };
                let v = (s, bw as i32, bh as i32);
                self.cache.insert(key, v.clone());
                v
            }
        };
        let layer = self.layer.clone();
        self.sprite(&layer, &surf, (x + (w - sw as f32) / 2.0).round(), ((self.h as f32 - sh as f32) / 2.0).round(), sw as f32, sh as f32, 1.0);
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
                APP => self.draw_text(&app, 650, p.x, p.w, 1.0),
                M_FILE => self.draw_text(&mt("Berkas"), 500, p.x, p.w, 1.0),
                M_EDIT => self.draw_text(&mt("Edit"), 500, p.x, p.w, 1.0),
                M_VIEW => self.draw_text(&mt("Menu:Tampilan"), 500, p.x, p.w, 1.0),
                M_WIN => self.draw_text(&mt("Menu:Jendela"), 500, p.x, p.w, 1.0),
                M_HELP => self.draw_text(&mt("Bantuan"), 500, p.x, p.w, 1.0),
                n if n >= NATIVE0 && n < NATIVE0 + 20 => {
                    let title = self.native.get((n - NATIVE0) as usize).map(|t| t.title.clone()).unwrap_or_default();
                    self.draw_text(&title, 500, p.x, p.w, 1.0)
                }
                _ => {}
            }
        }
        for p in &rights {
            let cx = p.x + p.w / 2.0;
            match p.id {
                CLOCK => self.draw_text(&clock, 500, p.x, p.w, 1.0),
                CTRL => self.draw_glyph("\u{E9E9}", cx, 13.0, 0.95),
                SEARCH => self.draw_glyph("\u{E721}", cx, 13.0, 0.95),
                STAGEI => self.draw_glyph("\u{E8A0}", cx, 13.0, 0.95),
                WIFI => self.draw_glyph("\u{E701}", cx, 14.0, if self.wifi == Some(true) { 0.95 } else { 0.4 }),
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
        // Latar transparan: bayangan teks tipis di bawah (hanya untuk teks putih) agar tetap terbaca di atas wallpaper terang.
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

    /// Sorotan judul: kapsul kaca yang muncul dengan membesar halus dari 0,9 sambil memudar, meluncur dan melebar
    /// (cairan) ke judul lain, lalu memudar saat kursor pergi. Animasi hanya dimulai bila posisi/lebar benar-benar berubah.
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
                // kaca: isian bergradasi (lebih terang di atas) + garis tepi atas tipis, bukan warna rata
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
                    // muncul: membesar dari 0,9 + memudar masuk
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
                    // pindah ke judul lain: meluncur dan melebar seperti cairan (pusat lama → pusat baru)
                    let from_x = old_off.X + old_w / 2.0 - nw / 2.0;
                    let _ = crate::motion::vec3(&self.comp, &self.capsule, "Offset", Vector3 { X: from_x, Y: cy, Z: 0.0 }, to, crate::motion::SWITCHER_MOVE_MS + 50);
                    let _ = crate::motion::vec3(&self.comp, &self.capsule, "Scale", Vector3 { X: (old_w / nw).clamp(0.5, 2.0), Y: 1.0, Z: 1.0 }, Vector3 { X: 1.0, Y: 1.0, Z: 1.0 }, crate::motion::SWITCHER_MOVE_MS + 50);
                }
                let _ = self.capsule.SetOpacity(1.0);
            }
            None => {
                if self.capsule.Opacity().unwrap_or(0.0) > 0.01 {
                    // memudar pelan dan mengecil sedikit, tidak langsung lenyap
                    let _ = crate::motion::scalar(&self.comp, &self.capsule, "Opacity", 1.0, 0.0, 150);
                    let _ = crate::motion::vec3(&self.comp, &self.capsule, "Scale", Vector3 { X: 1.0, Y: 1.0, Z: 1.0 }, Vector3 { X: 0.94, Y: 0.9, Z: 1.0 }, 150);
                    let _ = self.capsule.SetOpacity(0.0);
                }
            }
        }
    }

    /// Umpan balik tekan: kapsul menyusut ke 0,93 lalu memantul kembali, seperti tombol Tahoe yang ditekan.
    /// Dipanggil untuk judul yang sedang disorot; tanpa sorotan tidak ada yang dianimasikan.
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

    // -----------------------------------------------------------------------------------------------------------
    // interaksi
    // -----------------------------------------------------------------------------------------------------------

    fn id_at(&self, x: f32) -> Option<u32> {
        lay::hit(&self.left, x).or_else(|| lay::hit(&self.right, x))
    }

    fn has_menu(id: u32) -> bool {
        matches!(id, LOGO | APP | M_FILE | M_EDIT | M_VIEW | M_WIN | M_HELP) || (NATIVE0..NATIVE0 + 20).contains(&id)
    }

    fn open_menu(&mut self, id: u32) {
        let items = self.menu_items(id);
        log::line("menubar", &format!("buka menu {id}: {} item", items.len()));
        let Some((x, _)) = self.rect_of(id) else { return };
        let (comp, gfx) = (self.comp.clone(), &self.gfx);
        let top = self.h;
        let r = self.menu.show_below(&comp, gfx, items, x as i32, top);
        if let Err(e) = &r {
            log::line("menubar", &format!("menu gagal: {e}"));
        }
        if r.is_ok() {
            self.open = Some(id);
            self.update_capsule();
        }
    }

    fn close_menu(&mut self) {
        self.menu.hide();
        self.open = None;
        self.update_capsule();
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
            // baca ulang agar status centang/nonaktif terkini
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
                    let label = format!("{}{}{}", if e.checked { "\u{2713} " } else { "" }, pad, e.label);
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
                    a(807, &format!("{}{}", if prefs::get().stage { "\u{2713} " } else { "" }, prefs::tr("Stage Manager"))),
                    a(808, &prefs::tr("Mission Control")).with_accel("Ctrl+↑"),
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
                a(106, &prefs::tr("Mission Control")).with_accel("Ctrl+↑"),
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
            M_FILE => vec![
                a(300, &prefs::tr("Baru")).with_accel("Ctrl+N"),
                a(301, &prefs::tr("Buka…")).with_accel("Ctrl+O"),
                Item::separator(),
                a(302, &prefs::tr("Simpan")).with_accel("Ctrl+S"),
                a(303, &prefs::tr("Tutup Jendela")).with_accel("Ctrl+W"),
                Item::separator(),
                a(304, &prefs::tr("Cetak…")).with_accel("Ctrl+P"),
            ],
            M_EDIT => vec![
                a(400, &prefs::tr("Urungkan")).with_accel("Ctrl+Z"),
                a(401, &prefs::tr("Ulangi")).with_accel("Ctrl+Y"),
                Item::separator(),
                a(402, &prefs::tr("Potong")).with_accel("Ctrl+X"),
                a(403, &prefs::tr("Salin")).with_accel("Ctrl+C"),
                a(404, &prefs::tr("Tempel")).with_accel("Ctrl+V"),
                a(405, &prefs::tr("Pilih Semua")).with_accel("Ctrl+A"),
                Item::separator(),
                a(406, &prefs::tr("Cari…")).with_accel("Ctrl+F"),
            ],
            M_VIEW => vec![
                a(500, &prefs::tr("Layar Penuh")).with_accel("F11"),
                Item::separator(),
                a(501, &prefs::tr("Perbesar")).with_accel("Ctrl++"),
                a(502, &prefs::tr("Perkecil")).with_accel("Ctrl+-"),
                a(503, &prefs::tr("Ukuran Asli")).with_accel("Ctrl+0"),
            ],
            M_WIN => vec![
                a(600, &prefs::tr("Minimalkan")).with_accel("Win+↓"),
                a(601, &prefs::tr("Zoom")).with_accel("Win+↑"),
            ],
            M_HELP => vec![a(700, &prefs::tr("Cari Bantuan")).with_accel("F1")],
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
                crate::controlcenter::toggle(self.h);
            }
            STAGEI => prefs::update(|st| st.stage = !st.stage),
            SEARCH => crate::spotlight::toggle(),
            WIFI => {
                // pemilih jaringan ala macOS tepat di bawah ikon (bukan membuka Pengaturan)
                if let Some((x, _)) = self.rect_of(WIFI) {
                    crate::netmenu::show(&self.comp, crate::netmenu::Which::Wifi, crate::netmenu::Anchor::Below(x as i32, self.h), self.dpi);
                }
            }
            BATT => launch::open("ms-settings:batterysaver"),
            CLOCK => keys::chord(&[keys::WIN, 0x4E]),
            _ => {}
        }
    }

    fn pick(&mut self, cmd: usize) {
        self.close_menu();
        if cmd >= NATIVE_CMD {
            nacre_platform::menus::invoke(self.native_hwnd, (cmd - NATIVE_CMD) as u32);
            return;
        }
        match cmd {
            100 => launch::open("ms-settings:about"),
            101 => launch::open("ms-settings:"),
            102 => launch::open("ms-windows-store:"),
            103 => crate::settings_win::open(&self.comp),
            104 => launch::run("taskmgr.exe", ""),
            105 => crate::launchpad::toggle(),
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
            809 => crate::launchpad::toggle(),
            810 => crate::settings_win::open(&self.comp),
            811 => launch::open("ms-settings:"),
            200 => crate::settings_win::open(&self.comp),
            201 => self.hide_app(false),
            202 => self.hide_app(true),
            203 => self.show_all(),
            204 => crate::macshortcuts::quit_foreground_app(),
            300 => keys::chord(&[keys::CTRL, 0x4E]),
            301 => keys::chord(&[keys::CTRL, 0x4F]),
            302 => keys::chord(&[keys::CTRL, 0x53]),
            303 => keys::chord(&[keys::CTRL, 0x57]),
            304 => keys::chord(&[keys::CTRL, 0x50]),
            400 => keys::chord(&[keys::CTRL, 0x5A]),
            401 => keys::chord(&[keys::CTRL, 0x59]),
            402 => keys::chord(&[keys::CTRL, 0x58]),
            403 => keys::chord(&[keys::CTRL, 0x43]),
            404 => keys::chord(&[keys::CTRL, 0x56]),
            405 => keys::chord(&[keys::CTRL, 0x41]),
            406 => keys::chord(&[keys::CTRL, 0x46]),
            500 => keys::chord(&[0x7A]),
            501 => keys::chord(&[keys::CTRL, 0xBB]),
            502 => keys::chord(&[keys::CTRL, 0xBD]),
            503 => keys::chord(&[keys::CTRL, 0x30]),
            600 => keys::chord(&[keys::WIN, 0x28]),
            601 => keys::chord(&[keys::WIN, 0x26]),
            700 => keys::chord(&[0x70]),
            _ => {}
        }
    }

    /// Sembunyikan (minimalkan) jendela app aktif, atau semua selain app aktif.
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

/// Konfirmasi (di thread terpisah agar UI ABRA tetap hidup) lalu jalankan perintah daya.
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
                let id = b.id_at(x);
                if id != b.hover {
                    b.hover = id;
                    // menu sedang terbuka: pindah ke judul lain tanpa klik
                    if let (Some(open), Some(new)) = (b.open, id) {
                        if new != open && Bar::has_menu(new) {
                            b.open_menu(new);
                        }
                    }
                    b.update_capsule();
                }
            });
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            with_bar(|b| {
                b.tracking = false;
                b.hover = None;
                b.update_capsule();
            });
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let (x, _) = xy(lp);
            with_bar(|b| {
                if let Some(id) = b.id_at(x) {
                    // sorotan menyusut lalu memantul (umpan balik tekan), baru aksinya dijalankan
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
            } else if wp.0 == 4 {
                let h = with_bar(|b| b.h).unwrap_or(30);
                crate::controlcenter::toggle(h);
            } else if wp.0 == 5 {
                crate::controlcenter::test_open_picker();
            } else if wp.0 == 7 {
                crate::netmenu::test_click_audio();
            } else {
                let which = if wp.0 == 1 || wp.0 == 6 { crate::netmenu::Which::Bluetooth } else { crate::netmenu::Which::Wifi };
                with_bar(|b| {
                    if let Some((x, _)) = b.rect_of(WIFI) {
                        crate::netmenu::show(&b.comp, which, crate::netmenu::Anchor::Below(x as i32, b.h), b.dpi);
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
        WM_TIMER => {
            let id = wp.0;
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
