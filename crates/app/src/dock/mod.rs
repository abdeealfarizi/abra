//! Kontroler dock: window, input, model item, dan sinkronisasi ke visual Composition.
//! Satu thread (UI). Logika keputusan ada di `nacre-core`; berkas ini hanya merangkai.

mod custom_icons;
pub mod menu;
mod pins;
mod preview;
mod tooltip;
mod view;

use crate::paths;
use nacre_core::dock::items::{key_of, merge, Item, Pin};
use nacre_core::dock::layout::{hit, layout, Layout, Metrics, Slot};
use nacre_core::dock::reorder;
use nacre_core::dock::ring::next_in_ring;
use nacre_core::visibility::policy::{should_hide, Inputs, Mode};
use nacre_platform::icons;
use nacre_platform::wins::overlap::{self, Overlap};
use nacre_platform::wins::{enumerate, focus, tracker};
use nacre_platform::{launch, log, sys};
use nacre_ui::gfx::Gfx;
use nacre_ui::glass::{GlassPanel, GlassParams, Tier};
use nacre_ui::host::{Host, Surface};
use menu::{Menu, MENU_CLOSE, MENU_PICK};
use preview::{Preview, PREVIEW_CLICK, PREVIEW_ENTER, PREVIEW_LEAVE};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::time::{Duration, Instant};
use crate::strip::Strip;
use tooltip::Tooltip;
use view::{spring, Kind, View};
use windows::core::w;
use windows::UI::Composition::{
    CompositionDrawingSurface, CompositionObject, CompositionPropertySet, Compositor, ContainerVisual,
    ExpressionAnimation, SpringScalarNaturalMotionAnimation,
};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{CreateRectRgn, SetWindowRgn};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows_core::{Interface, HSTRING};
use windows_numerics::Vector3;

pub const WM_ICONS: u32 = WM_APP + 3;
pub const WM_APPBAR_SYNC: u32 = WM_APP + 4;
const STRIP_ENTER: u32 = WM_APP + 5;
const STRIP_LEAVE: u32 = WM_APP + 6;
/// Konstanta ini tidak diekspor oleh `windows` di modul yang kita impor; tanpa ini `match` akan
/// menganggapnya variabel pengikat dan menelan semua pesan.
const WM_MOUSELEAVE: u32 = 0x02A3;
const WM_SETTINGCHANGE: u32 = 0x001A;
const WM_POWERBROADCAST: u32 = 0x0218;

const TIMER_WINDOWS: usize = 100;
const TIMER_APPBAR: usize = 101;
const TIMER_TIP: usize = 102;
const TIMER_PREVIEW: usize = 103;
const TIMER_GRACE: usize = 104;
const TIMER_DWELL: usize = 105;
const TIMER_PCLOSE: usize = 106;
const TIMER_HIDEWIN: usize = 107;
const TIMER_START: usize = 108;
const TIMER_REGION: usize = 109;
const TIMER_RECONF: usize = 110;
const TIMER_TRASH: usize = 111;
/// Selang cek isi Recycle Bin untuk ikon kosong/berisi.
const TRASH_POLL_MS: u32 = 4000;
const WM_DISPLAYCHANGE: u32 = 0x007E;
const WM_DPICHANGED: u32 = 0x02E0;

static DOCK_HWND: AtomicIsize = AtomicIsize::new(0);
static BUSY: AtomicBool = AtomicBool::new(false);

const CMD_OPEN: usize = 1;
const CMD_PIN: usize = 2;
const CMD_CLOSE: usize = 3;
const CMD_QUIT: usize = 4;
const CMD_SETTINGS: usize = 5;
const CMD_ICON: usize = 6;
const CMD_ICON_RESET: usize = 7;
const CMD_ADD: usize = 8;
const CMD_TRASH_OPEN: usize = 9;
const CMD_TRASH_EMPTY: usize = 10;
const CMD_STACK_FOLDER: usize = 11;
/// Berkas stack Unduhan memakai id CMD_STACK + indeks.
const CMD_STACK: usize = 100;
/// Banyak app terakhir yang diingat dan yang ditampilkan di dock.
const RECENTS_KEEP: usize = 8;
const RECENTS_SHOW: usize = 3;
/// Kunci khusus menu klik-kanan ikon Unduhan.
const DOWNLOADS_KEY: &str = "\u{0}downloads";

/// Kunci ikon Unduhan: path folder aslinya (ikon dimuat lewat jalur ikon biasa).
fn downloads_key() -> String {
    nacre_platform::downloads::folder().map(|p| p.to_string_lossy().to_string()).unwrap_or_default()
}

pub struct Options {
    /// Cadangkan strip layar (window lain tidak menimpa dock).
    pub appbar: bool,
    pub mode: Mode,
    /// Paksa tier material; `None` = otomatis dari kondisi sistem.
    pub glass: Option<Tier>,
}

#[derive(Clone, Copy)]
enum Entry {
    Start,
    /// Tombol Launchpad (kisi titik berwarna) tepat setelah tombol Mulai, seperti macOS.
    Launchpad,
    App(usize),
    Sep,
    /// Tempat sampah di ujung kanan seperti macOS (Recycle Bin).
    Trash,
    /// Tumpukan Unduhan tepat sebelum tempat sampah.
    Downloads,
}

/// Nama penguraian shell untuk Recycle Bin (ikon dimuat lewat jalur ikon biasa).
const TRASH_PATH: &str = "shell:RecycleBinFolder";

/// Kunci ikon kustom tombol Mulai (bukan jalur berkas sungguhan; hanya dipakai sebagai kunci penimpa ikon).
const START_KEY: &str = "abra:start";
/// Kunci ikon kustom tombol Launchpad.
const LAUNCHPAD_KEY: &str = "abra:launchpad";

struct MenuInfo {
    item: Option<Item>,
    trash: bool,
    /// Klik kanan pada tombol khusus (Mulai/Launchpad): kunci ikon kustomnya.
    special: Option<&'static str>,
}

/// Tombol kiri sedang ditahan pada sebuah slot (klik atau awal drag).
struct Press {
    slot: usize,
    x: f32,
    y: f32,
    /// Indeks item bila slot itu app yang dipin (boleh di-drag).
    pinned_item: Option<usize>,
}

pub struct Dock {
    hwnd: HWND,
    main_hwnd: HWND,
    comp: Compositor,
    gfx: Gfx,
    _surface: Surface,
    stage: ContainerVisual,
    stage_anim: SpringScalarNaturalMotionAnimation,
    panel: GlassPanel,
    content: ContainerVisual,
    /// Properti bersama untuk ekspresi compositor: posisi kursor (`cx`) dan pengaruh magnifikasi (`k`, 0..1).
    props: CompositionPropertySet,
    k_anim: SpringScalarNaturalMotionAnimation,
    exprs_ok: bool,
    k_on: bool,

    dpi: f32,
    m: Metrics,
    screen: (i32, i32),
    win: (i32, i32),
    win_pos: (i32, i32),
    card_y: f32,
    card_h: f32,
    /// Jarak kartu dari tepi bawah layar (px), dari pengaturan.
    margin: f32,
    /// Dinonaktifkan dari Pengaturan: disembunyikan penuh dan tidak mencadangkan ruang.
    off: bool,
    icon_px: i32,
    icon_y: f32,
    dot_y: f32,
    pad_side: f32,
    opts: Options,

    pins: Vec<Pin>,
    pins_path: PathBuf,
    /// App terakhir dipakai yang bukan pin (terbaru di depan), disimpan di recents.txt.
    recents: Vec<Pin>,
    recents_path: PathBuf,
    /// Berkas Unduhan yang sedang tampil di menu stack (id perintah CMD_STACK + indeks).
    stack_files: Vec<String>,
    items: Vec<Item>,
    signature: Vec<(String, bool, usize)>,
    /// Gaya ikon yang sedang dipakai (untuk memuat ulang ikon saat berubah).
    icon_key: (nacre_core::settings::IconStyle, u8, bool, bool),
    seen: Vec<String>,

    entries: Vec<Entry>,
    slots: Vec<Slot>,
    views: Vec<View>,
    layout: Layout,

    surfaces: HashMap<String, CompositionDrawingSurface>,
    requested: HashSet<String>,
    /// Recycle Bin berisi (ikon tempat sampah versi kertas).
    trash_full: bool,
    worker: icons::Worker,
    /// Nilai `dock_plates` yang sedang diterapkan.
    plates: bool,

    tracking: bool,
    region_full: bool,
    cursor: Option<f32>,
    last_apply: Instant,
    last_card: (f32, f32, f32, f32),
    start_open_at_press: bool,
    hover: Option<usize>,
    press: Option<Press>,
    dragging: Option<usize>,

    shown: bool,
    starting: bool,
    overlap: Overlap,
    grace: bool,
    edge_armed: bool,
    menu_open: bool,
    preview_hover: bool,
    tier: Tier,
    strip: Option<Strip>,
    tooltip: Option<Tooltip>,
    preview: Option<Preview>,
    menu: Option<Menu>,
    pending_menu: Option<MenuInfo>,
}

fn f32_ref(v: f32) -> windows_core::Result<windows::Foundation::IReference<f32>> {
    windows::Foundation::PropertyValue::CreateSingle(v)?.cast()
}

fn with_dock<R>(hwnd: HWND, f: impl FnOnce(&mut Dock) -> R) -> Option<R> {
    let p = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut Dock;
    if p.is_null() || BUSY.swap(true, Ordering::Acquire) {
        return None;
    }
    // Pinjaman tunggal: pesan yang masuk saat kita sedang bekerja (SetWindowPos, dll.) dilewati.
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

/// Minta dock mendaftar ulang strip AppBar-nya (mis. setelah taskbar disembunyikan/dipulihkan,
/// karena shell menempatkan strip kita di atas taskbar yang masih terlihat).
pub fn request_appbar_resync() {
    let h = DOCK_HWND.load(Ordering::Relaxed);
    if h != 0 {
        unsafe {
            let _ = PostMessageW(Some(HWND(h as *mut _)), WM_APPBAR_SYNC, WPARAM(0), LPARAM(0));
        }
    }
}

fn on_windows_changed() {
    let h = DOCK_HWND.load(Ordering::Relaxed);
    if h != 0 {
        // SetTimer dengan id sama mereset hitungan: debounce yang mempertahankan event terakhir.
        timer(HWND(h as *mut _), TIMER_WINDOWS, 120);
    }
}

/// Persegi ikon dock untuk jendela `hwnd` (untuk animasi jendela).
pub fn icon_rect_for(hwnd: isize) -> Option<(i32, i32, i32, i32)> {
    let h = HWND(DOCK_HWND.load(Ordering::Relaxed) as *mut _);
    with_dock(h, |d| d.icon_rect(hwnd)).flatten()
}

/// Perintah CLI (`--add-app`, `--set-icon`): berguna untuk skrip/uji tanpa dialog berkas.
pub fn cli_commands(hwnd: HWND, add: Option<String>, icon: Option<(String, String)>) {
    with_dock(hwnd, |d| {
        if let Some(p) = add {
            d.add_app(&p);
        }
        if let Some((app, img)) = icon {
            let app = if app.eq_ignore_ascii_case("start") {
                START_KEY.to_string()
            } else if app.eq_ignore_ascii_case("launchpad") {
                LAUNCHPAD_KEY.to_string()
            } else {
                app
            };
            if custom_icons::set(&app, &img) {
                d.reload_icon(&app);
                if app == START_KEY || app == LAUNCHPAD_KEY {
                    d.signature.clear();
                    d.refresh_windows();
                }
            }
        }
    });
}

/// Metrik dock dari pengaturan pengguna.
fn metrics_from_prefs(dpi: f32) -> Metrics {
    let s = crate::prefs::get();
    Metrics::custom(s.dock_size as f32, s.dock_gap as f32, s.dock_zoom as f32 / 100.0, dpi)
}

impl Dock {
    pub fn create(host: &Host, main_hwnd: HWND, opts: Options) -> windows_core::Result<HWND> {
        unsafe {
            let hi: HINSTANCE = GetModuleHandleW(None)?.into();
            let class = w!("NacreDock");
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
                w!("nacre-dock"),
                WS_POPUP,
                0, 0, 400, 120,
                None, None, Some(hi), None,
            )?;
            let dpi = GetDpiForWindow(hwnd).max(96) as f32 / 96.0;
            let screen = sys::screen_size();
            let m = metrics_from_prefs(dpi);

            let surface = host.attach(hwnd, (400.0, 120.0))?;
            let stage = host.compositor.CreateContainerVisual()?;
            surface.root.Children()?.InsertAtTop(&stage)?;
            let mut panel = GlassPanel::new(&host.compositor, &stage, crate::prefs::glass((10.0, 10.0), 22.0 * dpi * crate::prefs::round(crate::prefs::Role::Dock), crate::prefs::Role::Dock))?;
            let tier = opts.glass.unwrap_or_else(|| auto_tier(&sys::conditions()));
            panel.set_tier(tier)?;
            let content = host.compositor.CreateContainerVisual()?;
            stage.Children()?.InsertAtTop(&content)?;

            let gfx = Gfx::new(&host.compositor)?;
            let hw = hwnd.0 as isize;
            let px = ((m.icon * m.max_scale * 1.15) as i32).clamp(64, 256);
            let worker = icons::spawn_worker(px, move || {
                let _ = PostMessageW(Some(HWND(hw as *mut _)), WM_ICONS, WPARAM(0), LPARAM(0));
            });

            // ikon kustom harus terdaftar SEBELUM dock meminta ikon pertama, kalau tidak ikon asli yang termuat dan tersimpan
            custom_icons::install();
            nacre_platform::iconpack::configure(&paths::config_dir().join("iconpack"), crate::prefs::get().icon_pack);
            let pins_path = paths::config_dir().join("pins.txt");
            let pins = pins::load(&pins_path);
            let recents_path = paths::config_dir().join("recents.txt");
            // buang app yang sudah tidak ada sekali saat memuat (bukan tiap refresh dock)
            let recents: Vec<Pin> = pins::load(&recents_path)
                .into_iter()
                .filter(|r| r.path.to_ascii_lowercase().starts_with("shell:") || std::path::Path::new(&r.path).exists())
                .take(RECENTS_KEEP)
                .collect();
            let mode = opts.mode;

            let mut dock = Box::new(Dock {
                hwnd,
                main_hwnd,
                comp: host.compositor.clone(),
                gfx,
                _surface: surface,
                stage,
                stage_anim: spring(&host.compositor, 0.78, 150)?,
                panel,
                content,
                props: {
                    let p = host.compositor.CreatePropertySet()?;
                    p.InsertScalar(&HSTRING::from("cx"), -10000.0)?;
                    p.InsertScalar(&HSTRING::from("k"), 0.0)?;
                    p
                },
                k_anim: spring(&host.compositor, 0.72, 85)?,
                exprs_ok: false,
                k_on: false,
                dpi,
                m,
                screen,
                win: (400, 120),
                win_pos: (0, 0),
                card_y: 0.0,
                card_h: 0.0,
                margin: 8.0 * dpi,
                off: !crate::prefs::get().dock_enabled,
                icon_px: px,
                icon_y: 0.0,
                dot_y: 0.0,
                pad_side: 10.0 * dpi,
                opts,
                pins,
                pins_path,
                recents,
                recents_path,
                stack_files: Vec::new(),
                items: vec![],
                signature: vec![],
                icon_key: { let p = crate::prefs::get(); (p.icon_style, p.icon_tint, p.icon_pack, crate::prefs::icon_dark()) },
                seen: vec![],
                entries: vec![],
                slots: vec![],
                views: vec![],
                layout: Layout { cells: vec![], total: 0.0 },
                surfaces: HashMap::new(),
                requested: HashSet::new(),
                trash_full: nacre_platform::recycle::is_full(),
                worker,
                plates: crate::prefs::get().dock_plates,
                tracking: false,
                region_full: false,
                cursor: None,
                last_apply: Instant::now(),
                last_card: (0.0, 0.0, 0.0, 0.0),
                start_open_at_press: false,
                hover: None,
                press: None,
                dragging: None,
                shown: true,
                starting: true,
                overlap: Overlap::default(),
                grace: false,
                edge_armed: false,
                menu_open: false,
                preview_hover: false,
                tier,
                strip: if mode == Mode::Always { None } else { Strip::new(hwnd, STRIP_ENTER, STRIP_LEAVE).ok() },
                tooltip: None,
                preview: None,
                menu: None,
                pending_menu: None,
            });
            dock.compute_geometry();
            dock.refresh_windows();
            // Masuk dari bawah layar dengan spring; ShowWindow dulu agar animasi terlihat.
            let _ = dock.stage.SetOffset(Vector3 { X: 0.0, Y: dock.win.1 as f32 + 8.0, Z: 0.0 });
            let appbar = dock.opts.appbar;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(dock) as isize);
            DOCK_HWND.store(hwnd.0 as isize, Ordering::Relaxed);
            windows::Win32::UI::Shell::DragAcceptFiles(hwnd, true);
            crate::prefs::listen(hwnd);
            tracker::install(on_windows_changed);
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            with_dock(hwnd, |d| {
                d.animate_stage(0.0);
                d.sync_strip();
                if appbar {
                    d.reserve_strip();
                }
            });
            timer(hwnd, TIMER_START, 1600);
            timer(hwnd, TIMER_TRASH, TRASH_POLL_MS);
            log::line("dock", &format!("dibuat tier={tier:?} mode={mode:?}"));
            Ok(hwnd)
        }
    }

    // ---------- geometri ----------

    fn compute_geometry(&mut self) {
        let d = self.dpi;
        let icon = self.m.icon;
        let (pad_top, pad_bottom) = (12.0 * d, 18.0 * d);
        let margin = crate::prefs::get().dock_margin as f32 * d;
        self.margin = margin;
        self.card_h = pad_top + icon + pad_bottom;
        let headroom = icon * (self.m.max_scale - 1.0) + 6.0 * d;
        let win_h = (margin + self.card_h + headroom).ceil();
        self.card_y = win_h - margin - self.card_h;
        self.icon_y = self.card_y + pad_top;
        self.dot_y = self.icon_y + icon + (pad_bottom - 4.0 * d) / 2.0 + 1.0 * d;
        self.win.1 = win_h as i32;
    }

    /// Lebar terbesar isi dock di semua posisi kursor (untuk ukuran window tetap, tanpa resize saat hover).
    fn max_content_width(&self) -> f32 {
        let area = 4000.0;
        let base = layout(&self.slots, None, area, &self.m);
        let (Some(first), Some(last)) = (base.cells.first(), base.cells.last()) else { return 0.0 };
        let (from, to) = (first.x, last.x + last.w);
        let mut best = base.total;
        let mut x = from;
        while x <= to {
            best = best.max(layout(&self.slots, Some(x), area, &self.m).total);
            x += self.m.icon / 4.0;
        }
        best
    }

    fn resize_window(&mut self) {
        let need = self.max_content_width() + 2.0 * self.pad_side + 40.0 * self.dpi;
        let w = (need.ceil() as i32).clamp(200, self.screen.0);
        self.win.0 = w;
        self.win_pos = ((self.screen.0 - w) / 2, self.screen.1 - self.win.1);
        unsafe {
            let _ = SetWindowPos(self.hwnd, None, self.win_pos.0, self.win_pos.1, w, self.win.1, SWP_NOACTIVATE | SWP_NOZORDER);
        }
        let _ = self._surface.root.SetSize(windows_numerics::Vector2 { X: w as f32, Y: self.win.1 as f32 });
        self.place_strip();
        log::line(
            "dock",
            &format!("jendela=({},{})-({},{}) layar={}x{} dpi={}", self.win_pos.0, self.win_pos.1, self.win_pos.0 + w, self.win_pos.1 + self.win.1, self.screen.0, self.screen.1, self.dpi),
        );
    }

    /// Resolusi/DPI berubah saat berjalan: hitung ulang metrik dan susun ulang seluruh dock.
    fn on_reconfigure(&mut self) {
        let screen = sys::screen_size();
        let dpi = unsafe { GetDpiForWindow(self.hwnd) }.max(96) as f32 / 96.0;
        if screen == self.screen && (dpi - self.dpi).abs() < 0.01 {
            return;
        }
        log::line("dock", &format!("reconfigurasi {}x{}@{} → {}x{}@{}", self.screen.0, self.screen.1, self.dpi, screen.0, screen.1, dpi));
        self.hide_popups();
        self.tooltip = None;
        self.preview = None;
        self.screen = screen;
        self.dpi = dpi;
        self.m = metrics_from_prefs(dpi);
        self.pad_side = 10.0 * dpi;
        self.compute_geometry();
        let _ = self.panel.set_radius(22.0 * dpi * crate::prefs::round(crate::prefs::Role::Dock));
        self.last_card = (0.0, 0.0, 0.0, 0.0);
        self.signature.clear();
        self.refresh_windows();
        if self.opts.appbar {
            self.reserve_strip();
        }
    }

    /// Ukuran/jarak/zoom/margin dari pengaturan berubah: hitung ulang metrik, ukuran jendela, dan susun ulang.
    fn apply_metrics(&mut self) {
        let s = crate::prefs::get();
        let m = metrics_from_prefs(self.dpi);
        let same = (m.icon - self.m.icon).abs() < 0.01
            && (m.gap - self.m.gap).abs() < 0.01
            && (m.max_scale - self.m.max_scale).abs() < 0.001
            && (s.dock_margin as f32 * self.dpi - self.margin).abs() < 0.01;
        if same {
            return;
        }
        self.hide_popups();
        self.m = m;
        let px = ((m.icon * m.max_scale * 1.15) as i32).clamp(64, 256);
        if px != self.icon_px {
            // Ikon harus dimuat ulang pada resolusi yang sesuai.
            self.icon_px = px;
            let hw = self.hwnd.0 as isize;
            self.worker = icons::spawn_worker(px, move || unsafe {
                let _ = PostMessageW(Some(HWND(hw as *mut _)), WM_ICONS, WPARAM(0), LPARAM(0));
            });
            self.surfaces.clear();
            self.requested.clear();
        }
        self.compute_geometry();
        self.last_card = (0.0, 0.0, 0.0, 0.0);
        self.signature.clear();
        self.refresh_windows();
        if self.opts.appbar {
            self.reserve_strip();
        }
        log::line("dock", &format!("metrik: ikon={} jarak={} zoom={} margin={}", s.dock_size, s.dock_gap, s.dock_zoom, s.dock_margin));
    }

    /// Refraksi tepi pada rak dock (hanya material Liquid Glass dan tier penuh); peta dibuat ulang saat ukuran rak berubah.
    fn refresh_refraction(&mut self, w: f32, h: f32) {
        let on = crate::prefs::get().dock_material == nacre_core::settings::Material::Glass && self.tier == Tier::Full && crate::prefs::get().refraction;
        let d = self.dpi;
        let radius = 22.0 * d * crate::prefs::round(crate::prefs::Role::Dock);
        let arg = on.then(|| (w.round() as u32, h.round() as u32, radius, 15.0 * d, 9.0 * d));
        if let Err(e) = self.panel.set_refraction(&self.gfx, arg) {
            log::line("dock", &format!("refraksi gagal: {e}"));
        }
    }

    fn reserve_strip(&mut self) {
        if self.off {
            return;
        }
        let d = self.dpi;
        let strip = (self.margin + self.card_h + 4.0 * d).ceil() as i32;
        let r = nacre_platform::shell::appbar::reserve_bottom(self.hwnd, self.screen.0, self.screen.1, strip);
        log::line("appbar", &format!("strip={strip} hasil={r:?}"));
    }

    /// Hanya kartu yang menerima mouse saat diam; saat di-hover seluruh window (ikon yang membesar
    /// melewati tepi kartu). Area transparan lain tidak menelan klik ke window di bawahnya.
    fn set_region(&mut self, full: bool) {
        self.region_full = full;
        let (l, t, r, b) = if full || self.layout.cells.is_empty() {
            (0, 0, self.win.0, self.win.1)
        } else {
            let rest = layout(&self.slots, None, self.win.0 as f32, &self.m);
            let cw = rest.total + 2.0 * self.pad_side;
            let cx = (self.win.0 as f32 - cw) / 2.0;
            (cx as i32, self.card_y as i32, (cx + cw).ceil() as i32, (self.card_y + self.card_h).ceil() as i32)
        };
        unsafe {
            let rgn = CreateRectRgn(l, t, r, b);
            // Setelah berhasil, region dimiliki sistem.
            SetWindowRgn(self.hwnd, Some(rgn), true);
        }
    }

    // ---------- visibilitas ----------

    fn interacting(&self) -> bool {
        self.tracking || self.preview_hover || self.menu_open || self.dragging.is_some() || self.grace
    }

    fn card_screen_rect(&self) -> (i32, i32, i32, i32) {
        let rest = layout(&self.slots, None, self.win.0 as f32, &self.m);
        let cw = rest.total + 2.0 * self.pad_side;
        let cx = (self.win.0 as f32 - cw) / 2.0;
        let l = self.win_pos.0 + cx as i32;
        let t = self.win_pos.1 + self.card_y as i32;
        (l, t, l + cw.ceil() as i32, t + (self.card_h + self.margin) as i32)
    }

    /// Letakkan strip deteksi tepi tepat di tepi bawah layar selebar jendela dock.
    fn place_strip(&self) {
        if let Some(s) = &self.strip {
            s.place(self.win_pos.0, self.screen.1 - 2, self.win.0, 2);
        }
    }

    fn sync_strip(&self) {
        if self.off {
            if let Some(s) = &self.strip {
                s.set_visible(false);
            }
            return;
        }
        if let Some(s) = &self.strip {
            s.set_visible(!self.overlap.fullscreen);
        }
    }

    fn animate_stage(&self, y: f32) {
        if let Ok(v) = f32_ref(y) {
            if self.stage_anim.SetFinalValue(&v).is_ok() {
                let _ = self.stage.StartAnimation(&HSTRING::from("Offset.Y"), &self.stage_anim);
            }
        }
    }

    fn update_visibility(&mut self) {
        let inputs = Inputs {
            overlapped: self.overlap.overlapped,
            interacting: self.interacting(),
            edge_armed: self.edge_armed,
            starting: self.starting,
            fullscreen: self.overlap.fullscreen,
        };
        let hide = self.off || should_hide(self.opts.mode, &inputs);
        self.sync_strip();
        if hide && self.shown {
            self.shown = false;
            self.hide_popups();
            self.animate_stage(self.win.1 as f32 + 8.0);
            timer(self.hwnd, TIMER_HIDEWIN, 500);
            log::line("dock", "sembunyi");
        } else if !hide && !self.shown {
            self.shown = true;
            kill(self.hwnd, TIMER_HIDEWIN);
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
            }
            self.animate_stage(0.0);
            log::line("dock", "tampil");
        }
    }

    fn start_grace(&mut self) {
        self.grace = true;
        timer(self.hwnd, TIMER_GRACE, 800);
    }

    // ---------- model → visual ----------

    pub fn refresh_windows(&mut self) {
        let wins = enumerate::list(std::process::id());
        tracker::prune(focus::is_alive);
        let mut items = merge(&self.pins, &wins, &tracker::recent, &mut self.seen);
        self.track_recents(&items);
        self.append_recents(&mut items);
        let sig: Vec<_> = items.iter().map(|i| (i.key.clone(), i.pinned, i.hwnds.len())).collect();
        self.items = items;
        if sig != self.signature {
            self.signature = sig;
            self.rebuild();
        }
        self.overlap = overlap::evaluate(tracker::external_foreground(), std::process::id(), self.card_screen_rect(), self.screen);
        if self.overlap.fullscreen && crate::prefs::get().above_fullscreen {
            // tetap tampil di atas jendela layar penuh
            self.overlap = overlap::Overlap { overlapped: false, fullscreen: false };
        }
        self.update_visibility();
    }

    fn rebuild(&mut self) {
        log::line(
            "dock",
            &format!(
                "item: {}",
                self.items.iter().map(|i| format!("{}[{}]{}", i.name, i.hwnds.len(), if i.pinned { "*" } else { "" })).collect::<Vec<_>>().join(", ")
            ),
        );
        self.entries.clear();
        self.slots.clear();
        self.entries.push(Entry::Start);
        self.entries.push(Entry::Launchpad);
        let pinned = self.items.iter().filter(|i| i.pinned).count();
        for i in 0..self.items.len() {
            if i == pinned && pinned > 0 {
                self.entries.push(Entry::Sep);
            }
            self.entries.push(Entry::App(i));
        }
        self.entries.push(Entry::Sep);
        if crate::prefs::get().downloads_stack && nacre_platform::downloads::folder().is_some() {
            self.entries.push(Entry::Downloads);
        }
        self.entries.push(Entry::Trash);
        for e in &self.entries {
            self.slots.push(if matches!(e, Entry::Sep) { Slot::Separator } else { Slot::Item });
        }

        if let Ok(kids) = self.content.Children() {
            let _ = kids.RemoveAll();
        }
        self.views.clear();
        let icon = self.m.icon;
        // ikon Launchpad bawaan dibuat sendiri; bila pengguna mengganti ikonnya, pemuat ikon yang memakai gambar pilihan
        if !icons::is_themed(LAUNCHPAD_KEY) && !self.surfaces.contains_key(LAUNCHPAD_KEY) {
            let px = ((self.m.icon * self.m.max_scale * 1.15) as i32).clamp(64, 256) as u32;
            let p = crate::prefs::get();
            let data = nacre_core::iconplate::style_icon_v(&nacre_core::iconplate::launchpad_icon(px), p.icon_style, p.icon_tint, crate::prefs::icon_dark());
            if let Ok(s) = self.gfx.surface_from_bgra(px, px, &data) {
                self.surfaces.insert(LAUNCHPAD_KEY.to_string(), s);
            }
        }
        if !icons::is_themed(TRASH_PATH) && !self.surfaces.contains_key(TRASH_PATH) {
            self.make_trash_surface();
        }
        for e in self.entries.clone() {
            let v = match e {
                Entry::Start if custom_icons::has(START_KEY) => View::app(&self.comp, &self.content, START_KEY.to_string(), icon, false, self.dpi),
                Entry::Start => View::start(&self.comp, &self.content, icon),
                Entry::Launchpad => View::app(&self.comp, &self.content, LAUNCHPAD_KEY.to_string(), icon, false, self.dpi),
                Entry::Sep => View::separator(&self.comp, &self.content, self.m.separator, icon),
                Entry::Trash => View::app(&self.comp, &self.content, TRASH_PATH.to_string(), icon, false, self.dpi),
                Entry::Downloads => View::app(&self.comp, &self.content, downloads_key(), icon, false, self.dpi),
                Entry::App(i) => {
                    let it = &self.items[i];
                    View::app(&self.comp, &self.content, it.path.clone(), icon, it.running(), self.dpi)
                }
            };
            match v {
                Ok(mut v) => {
                    if let Kind::App { path } = &v.kind {
                        if let Some(s) = self.surfaces.get(path) {
                            let _ = v.set_surface(&self.comp, s);
                        } else if self.requested.insert(path.clone()) {
                            let _ = self.worker.requests.send(path.clone());
                        }
                    }
                    self.views.push(v);
                }
                Err(e) => log::line("dock", &format!("gagal membuat view: {e}")),
            }
        }
        // jaga paralelitas entries ↔ views bila ada view gagal dibuat
        if self.views.len() != self.entries.len() {
            self.entries.truncate(self.views.len());
            self.slots.truncate(self.views.len());
        }
        self.hover = None;
        self.resize_window();
        self.apply(None, false);
        self.bind_expressions();
        self.set_region(self.tracking);
    }

    /// Magnifikasi sepenuhnya di compositor: skala tiap ikon, posisi, titik, dan lebar kartu dihitung dari satu
    /// properti (`cx`). Proses kita hanya menulis satu angka per gerakan kursor, tanpa alokasi/animasi per ikon.
    fn bind_expressions(&mut self) {
        self.exprs_ok = false;
        self.k_on = false;
        let _ = self.props.InsertScalar(&HSTRING::from("k"), 0.0);
        if self.views.is_empty() {
            return;
        }
        let r = (|| -> windows_core::Result<()> {
            let n = self.views.len();
            let (icon, gap, sep) = (self.m.icon, self.m.gap, self.m.separator);
            let area = self.win.0 as f32;
            let rest = layout(&self.slots, None, area, &self.m);
            let is_item = |i: usize| self.slots[i] == Slot::Item;
            let nodes: Vec<CompositionObject> = self.views.iter().map(|v| v.node.cast::<CompositionObject>()).collect::<Result<_, _>>()?;
            let props: CompositionObject = self.props.cast()?;
            let make = |text: &str| -> windows_core::Result<ExpressionAnimation> {
                let e = self.comp.CreateExpressionAnimationWithExpression(&HSTRING::from(text))?;
                e.SetReferenceParameter(&HSTRING::from("p"), &props)?;
                for (j, node) in nodes.iter().enumerate() {
                    if is_item(j) {
                        e.SetReferenceParameter(&HSTRING::from(format!("n{j}")), node)?;
                    }
                }
                Ok(e)
            };
            let const_total = self.slots.iter().filter(|s| **s == Slot::Separator).count() as f32 * sep + gap * (n as f32 - 1.0);
            let mut total = format!("{const_total:.3}");
            for j in 0..n {
                if is_item(j) {
                    total += &format!(" + {icon:.3}*n{j}.Scale.X");
                }
            }
            let start = format!("({area:.3} - ({total}))/2");
            let reach = self.m.reach.max(1.0);
            let amp = self.m.max_scale - 1.0;
            for i in 0..n {
                let seps_before = (0..i).filter(|j| !is_item(*j)).count() as f32;
                let mut prefix = format!("{:.3}", seps_before * sep + gap * i as f32);
                for j in 0..i {
                    if is_item(j) {
                        prefix += &format!(" + {icon:.3}*n{j}.Scale.X");
                    }
                }
                let (w_i, base_w) = if is_item(i) { (format!("{icon:.3}*n{i}.Scale.X"), icon) } else { (format!("{sep:.3}"), sep) };
                let box_x = format!("({start} + ({prefix}))");
                let left = make(&format!("{box_x} + ({w_i})/2 - {:.3}", base_w / 2.0))?;
                nodes[i].StartAnimation(&HSTRING::from("Offset.X"), &left)?;
                if is_item(i) {
                    let c = rest.cells[i].x + rest.cells[i].w / 2.0;
                    let scale = make(&format!(
                        "1 + {amp:.4}*p.k*0.5*(1 + Cos(3.14159265*Clamp(Abs(p.cx - {c:.3})/{reach:.3}, 0, 1)))"
                    ))?;
                    nodes[i].StartAnimation(&HSTRING::from("Scale.X"), &scale)?;
                    nodes[i].StartAnimation(&HSTRING::from("Scale.Y"), &scale)?;
                }
                if let Some(dot) = &self.views[i].dot {
                    let half = dot.Size()?.X / 2.0;
                    let dx = make(&format!("{box_x} + ({w_i})/2 - {half:.3}"))?;
                    let d: CompositionObject = dot.cast()?;
                    d.StartAnimation(&HSTRING::from("Offset.X"), &dx)?;
                }
            }
            let card_w = format!("({total}) + {:.3}", 2.0 * self.pad_side);
            self.panel.bind_x_width(&make, &format!("({area:.3} - ({card_w}))/2"), &card_w)?;
            Ok(())
        })();
        match r {
            Ok(()) => self.exprs_ok = true,
            Err(e) => log::line("dock", &format!("ekspresi magnifikasi gagal, memakai jalur animasi per ikon: {e}")),
        }
    }

    fn set_influence(&mut self, on: bool) {
        if self.k_on == on {
            return;
        }
        self.k_on = on;
        if let Ok(v) = f32_ref(if on { 1.0 } else { 0.0 }) {
            if self.k_anim.SetFinalValue(&v).is_ok() {
                let o: Result<CompositionObject, _> = self.props.cast();
                if let Ok(o) = o {
                    let _ = o.StartAnimation(&HSTRING::from("k"), &self.k_anim);
                }
            }
        }
    }

    fn apply(&mut self, cursor: Option<f32>, animate: bool) {
        let l = layout(&self.slots, cursor, self.win.0 as f32, &self.m);
        let card_w = l.total + 2.0 * self.pad_side;
        let cx = (self.win.0 as f32 - card_w) / 2.0;
        let card = (cx, self.card_y, card_w, self.card_h);
        let moved = |a: f32, b: f32| (a - b).abs() >= 0.5;
        if moved(card.0, self.last_card.0) || moved(card.2, self.last_card.2) || moved(card.1, self.last_card.1) || moved(card.3, self.last_card.3) {
            let _ = self.panel.set_rect(card.0, card.1, card.2, card.3);
            self.last_card = card;
        }
        if cursor.is_none() {
            self.refresh_refraction(card.2, card.3);
        }
        for (i, (v, c)) in self.views.iter().zip(l.cells.iter()).enumerate() {
            if self.dragging == Some(i) {
                continue;
            }
            let _ = v.place(c.x, c.w, c.scale, self.icon_y, self.dot_y, animate);
        }
        self.layout = l;
        self.last_apply = Instant::now();
    }

    /// Ikon tempat sampah Tahoe buatan sendiri (kosong/berisi), digayakan seperti ikon lain.
    fn make_trash_surface(&mut self) {
        let px = ((self.m.icon * self.m.max_scale * 1.15) as i32).clamp(64, 256) as u32;
        let p = crate::prefs::get();
        let raw = nacre_core::iconplate::free(&nacre_core::trash::icon(px, self.trash_full), px);
        let data = nacre_core::iconplate::style_icon_v(&raw, p.icon_style, p.icon_tint, crate::prefs::icon_dark());
        if let Ok(s) = self.gfx.surface_from_bgra(px, px, &data) {
            self.surfaces.insert(TRASH_PATH.to_string(), s);
        }
    }

    /// Recycle Bin berubah kosong ↔ berisi: ganti gambar ikon tanpa membangun ulang dock.
    fn update_trash_icon(&mut self) {
        if icons::is_themed(TRASH_PATH) {
            return;
        }
        self.surfaces.remove(TRASH_PATH);
        self.make_trash_surface();
        if let Some(s) = self.surfaces.get(TRASH_PATH).cloned() {
            for v in self.views.iter_mut() {
                if matches!(&v.kind, Kind::App { path } if path == TRASH_PATH) {
                    let _ = v.set_surface(&self.comp, &s);
                }
            }
        }
    }

    pub fn on_icons(&mut self) {
        while let Ok((path, bmp)) = self.worker.results.try_recv() {
            let Some(mut b) = bmp else { continue };
            // ikon seragam ala macOS (kecuali tempat sampah dan ikon pilihan pengguna)
            if crate::prefs::get().dock_plates && b.w == b.h && !icons::is_themed(&path) {
                b.bgra = if path == TRASH_PATH || path == downloads_key() { nacre_core::iconplate::free(&b.bgra, b.w) } else { nacre_core::iconplate::plate(&b.bgra, b.w) };
            }
            if !icons::is_themed(&path) {
                let p = crate::prefs::get();
                b.bgra = nacre_core::iconplate::style_icon_v(&b.bgra, p.icon_style, p.icon_tint, crate::prefs::icon_dark());
            }
            match self.gfx.surface_from_bgra(b.w, b.h, &b.bgra) {
                Ok(surface) => {
                    for v in self.views.iter_mut() {
                        if matches!(&v.kind, Kind::App { path: p } if *p == path) {
                            let _ = v.set_surface(&self.comp, &surface);
                        }
                    }
                    self.surfaces.insert(path, surface);
                }
                Err(e) => log::line("icons", &format!("unggah gagal: {e}")),
            }
        }
    }

    /// Terapkan pengaturan: perilaku (mode/cadangan ruang) dan tampilan kaca. Dipanggil saat `WM_PREFS`.
    fn apply_prefs(&mut self) {
        let s = crate::prefs::get();
        let key = (s.icon_style, s.icon_tint, s.icon_pack, crate::prefs::icon_dark());
        if key != self.icon_key {
            self.icon_key = key;
            nacre_platform::iconpack::configure(&paths::config_dir().join("iconpack"), s.icon_pack);
            self.surfaces.clear();
            self.requested.clear();
            self.signature.clear();
            self.refresh_windows();
        }
        if s.dock_plates != self.plates {
            // gaya ikon berubah: muat ulang semua ikon dan bangun ulang tampilan
            self.plates = s.dock_plates;
            self.surfaces.clear();
            self.requested.clear();
            self.signature.clear();
            self.refresh_windows();
        }
        if self.off == s.dock_enabled {
            self.off = !s.dock_enabled;
            if self.off {
                nacre_platform::shell::appbar::release_bottom();
            } else if self.opts.appbar {
                self.reserve_strip();
            }
            log::line("dock", if self.off { "dinonaktifkan" } else { "diaktifkan" });
            self.update_visibility();
        }
        let (mode, appbar) = (s.dock.mode(), s.dock.reserves());
        if mode != self.opts.mode || appbar != self.opts.appbar {
            let was_appbar = self.opts.appbar;
            self.opts.mode = mode;
            self.opts.appbar = appbar;
            if mode == Mode::Always {
                self.strip = None;
                self.edge_armed = false;
            } else if self.strip.is_none() {
                // Bug lama: strip baru lahir di (0,0) 10×2 px dan baru dipindah saat dock dibangun ulang, sehingga
                // setelah mengganti perilaku ke sembunyi, kursor di tepi bawah tidak pernah memunculkan dock.
                self.strip = Strip::new(self.hwnd, STRIP_ENTER, STRIP_LEAVE).ok();
                self.place_strip();
            }
            if appbar {
                self.reserve_strip();
            } else if was_appbar {
                nacre_platform::shell::appbar::release_bottom();
            }
            self.sync_strip();
            self.update_visibility();
            log::line("dock", &format!("perilaku → {mode:?} cadangan={appbar}"));
        }
        self.apply_metrics();
        let (themed, dark) = (crate::prefs::look(crate::prefs::Role::Dock), crate::prefs::look(crate::prefs::Role::Dark));
        let base = GlassParams::dark((10.0, 10.0), 22.0 * self.dpi);
        let _ = self.panel.set_params(crate::prefs::apply(base, &themed));
        let (cw, ch) = (self.last_card.2, self.last_card.3);
        if cw > 0.0 {
            self.refresh_refraction(cw, ch);
        }
        let _ = self.panel.set_radius(22.0 * self.dpi * crate::prefs::round(crate::prefs::Role::Dock));
        let pd = crate::prefs::apply(base, &dark);
        if let Some(t) = self.tooltip.as_mut() {
            t.apply_look(pd);
        }
        if let Some(p) = self.preview.as_mut() {
            let _ = p.panel_mut().set_params(pd);
        }
        if let Some(m) = self.menu.as_mut() {
            m.apply_look(pd);
        }
    }

    fn refresh_tier(&mut self) {
        let t = self.opts.glass.unwrap_or_else(|| auto_tier(&sys::conditions()));
        if t != self.tier {
            self.tier = t;
            let _ = self.panel.set_tier(t);
            if let Some(tt) = self.tooltip.as_mut() {
                tt.set_tier(t);
            }
            if let Some(p) = self.preview.as_mut() {
                let _ = p.panel_mut().set_tier(t);
            }
            if let Some(m) = self.menu.as_mut() {
                m.set_tier(t);
            }
            log::line("dock", &format!("tier={t:?}"));
        }
    }

    // ---------- popup: tooltip + pratinjau ----------

    fn hide_popups(&mut self) {
        kill(self.hwnd, TIMER_TIP);
        kill(self.hwnd, TIMER_PREVIEW);
        if let Some(t) = &self.tooltip {
            t.hide();
        }
        if let Some(p) = self.preview.as_mut() {
            p.hide();
        }
        self.preview_hover = false;
    }

    /// (pusat-x, atas ikon yang sudah diskalakan) dalam piksel layar.
    fn icon_anchor(&self, slot: usize) -> Option<(i32, i32)> {
        let c = self.layout.cells.get(slot)?;
        let cx = self.win_pos.0 as f32 + c.x + c.w / 2.0;
        let top = self.win_pos.1 as f32 + self.icon_y + self.m.icon - self.m.icon * c.scale;
        Some((cx as i32, top as i32))
    }

    fn show_tooltip(&mut self) {
        let Some(slot) = self.hover else { return };
        if self.dragging.is_some() || self.preview.as_ref().is_some_and(|p| p.is_visible()) {
            return;
        }
        let label = match self.entries.get(slot) {
            Some(Entry::Start) => crate::prefs::tr("Mulai").to_string(),
            Some(Entry::Launchpad) => crate::prefs::tr("Apps").to_string(),
            Some(Entry::Trash) => crate::prefs::tr("Tempat sampah").to_string(),
            Some(Entry::Downloads) => crate::prefs::tr("Unduhan").to_string(),
            // Seperti macOS: tooltip hanya nama app, bukan judul jendela.
            Some(Entry::App(i)) => match self.items.get(*i) {
                Some(it) => {
                    let f = nacre_platform::wins::names::friendly(&it.path);
                    if f.is_empty() { it.name.clone() } else { f }
                }
                None => return,
            },
            _ => return,
        };
        let Some((cx, top)) = self.icon_anchor(slot) else { return };
        if self.tooltip.is_none() {
            self.tooltip = Tooltip::new(&self.comp, self.dpi, self.screen.0).ok().map(|mut t| {
                t.set_tier(self.tier);
                t
            });
        }
        if let Some(t) = self.tooltip.as_mut() {
            let bottom = top - (3.0 * self.dpi) as i32;
            if let Err(e) = t.show(&self.comp, &self.gfx, &label, cx, bottom) {
                log::line("tooltip", &format!("gagal: {e}"));
            }
        }
    }

    fn show_preview(&mut self) {
        let Some(slot) = self.hover else { return };
        if self.dragging.is_some() {
            return;
        }
        let Some(Entry::App(i)) = self.entries.get(slot).copied() else { return };
        let Some(it) = self.items.get(i) else { return };
        if !it.running() {
            return;
        }
        let wins: Vec<(isize, String, bool)> =
            it.hwnds.iter().map(|h| (*h, focus::title(*h), focus::is_minimized(*h))).collect();
        let icon = self.surfaces.get(&it.path).cloned();
        let Some((cx, top)) = self.icon_anchor(slot) else { return };
        if self.preview.is_none() {
            self.preview = Preview::new(&self.comp, self.hwnd, self.dpi, self.screen.0).ok().map(|mut p| {
                let _ = p.panel_mut().set_tier(self.tier);
                p
            });
        }
        if let Some(t) = &self.tooltip {
            t.hide();
        }
        if let Some(p) = self.preview.as_mut() {
            let bottom = top - (10.0 * self.dpi) as i32;
            if let Err(e) = p.show(&self.comp, &self.gfx, &wins, icon.as_ref(), cx, bottom) {
                log::line("preview", &format!("gagal: {e}"));
            }
        }
    }

    // ---------- input ----------

    fn entry_at(&self, x: f32) -> Option<(usize, Entry)> {
        let i = hit(&self.slots, &self.layout, x, &self.m)?;
        Some((i, *self.entries.get(i)?))
    }

    fn on_move(&mut self, x: f32, y: f32) {
        if self.press.is_some() {
            self.drag_update(x, y);
            return;
        }
        if !self.region_full {
            kill(self.hwnd, TIMER_REGION);
            self.set_region(true);
        }
        if self.grace {
            self.grace = false;
            kill(self.hwnd, TIMER_GRACE);
        }
        let slot = self.entry_at(x).map(|(i, _)| i);
        if slot != self.hover {
            self.hover = slot;
            if let Some(t) = &self.tooltip {
                t.hide();
            }
            kill(self.hwnd, TIMER_TIP);
            kill(self.hwnd, TIMER_PREVIEW);
            if slot.is_some() {
                let preview_up = self.preview.as_ref().is_some_and(|p| p.is_visible());
                if preview_up {
                    // Pratinjau sudah terbuka: pindah ke app lain langsung, tanpa jeda.
                    self.show_preview();
                    if !self.preview.as_ref().is_some_and(|p| p.is_visible()) {
                        timer(self.hwnd, TIMER_TIP, 250);
                    }
                } else {
                    timer(self.hwnd, TIMER_TIP, 250);
                    timer(self.hwnd, TIMER_PREVIEW, 450);
                }
            }
        }
        // Jalur ekspresi murah (satu angka per gerakan): jangan membuang event, atau posisi akhir kursor bisa hilang
        // (gerakan cepat ke ujung dock berhenti pada nilai lama). Throttle hanya untuk jalur animasi per ikon.
        if self.cursor.is_some_and(|c| (c - x).abs() < 0.5) || (!self.exprs_ok && self.last_apply.elapsed() < Duration::from_millis(10)) {
            return;
        }
        self.cursor = Some(x);
        if self.exprs_ok {
            // hit-test memakai tata letak Rust (tanpa COM); tampilan dihitung compositor dari `cx`.
            self.layout = layout(&self.slots, Some(x), self.win.0 as f32, &self.m);
            let _ = self.props.InsertScalar(&HSTRING::from("cx"), x);
            self.set_influence(true);
            self.last_apply = Instant::now();
        } else {
            self.apply(Some(x), true);
        }
        self.update_visibility();
    }

    fn on_leave(&mut self) {
        self.tracking = false;
        if self.dragging.is_some() {
            return;
        }
        self.cursor = None;
        self.hover = None;
        kill(self.hwnd, TIMER_TIP);
        kill(self.hwnd, TIMER_PREVIEW);
        if let Some(t) = &self.tooltip {
            t.hide();
        }
        if self.preview.as_ref().is_some_and(|p| p.is_visible()) {
            timer(self.hwnd, TIMER_PCLOSE, 200);
        }
        if self.exprs_ok {
            self.set_influence(false);
            self.layout = layout(&self.slots, None, self.win.0 as f32, &self.m);
        } else {
            self.apply(None, true);
        }
        // Ikon masih mengecil; potong region setelah animasi selesai.
        timer(self.hwnd, TIMER_REGION, 450);
        self.start_grace();
        self.update_visibility();
    }

    fn on_down(&mut self, x: f32, y: f32) {
        self.start_open_at_press = launch::is_start_open();
        if let Some((slot, entry)) = self.entry_at(x) {
            let pinned_item = match entry {
                Entry::App(i) if self.items.get(i).is_some_and(|it| it.pinned) => Some(i),
                _ => None,
            };
            self.press = Some(Press { slot, x, y, pinned_item });
            unsafe {
                SetCapture(self.hwnd);
            }
        }
    }

    fn drag_update(&mut self, x: f32, y: f32) {
        let Some(p) = &self.press else { return };
        if self.dragging.is_none() {
            let far = (x - p.x).abs().max((y - p.y).abs()) > 6.0 * self.dpi;
            if !far || p.pinned_item.is_none() {
                return;
            }
            self.dragging = Some(p.slot);
            self.hide_popups();
            self.cursor = None;
            self.apply(None, false);
        }
        if let Some(v) = self.dragging.and_then(|s| self.views.get(s)) {
            let _ = v.drag_to(x - self.m.icon / 2.0, self.icon_y - 14.0 * self.dpi);
        }
        self.shift_neighbors(x);
    }

    /// Tetangga bergeser memberi ruang di posisi sisip saat sebuah pin diseret (hasilnya sama dengan `finish_drag`).
    fn shift_neighbors(&mut self, x: f32) {
        let Some(dragged) = self.dragging else { return };
        let rest = layout(&self.slots, None, self.win.0 as f32, &self.m);
        let pinned_slots: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| matches!(e, Entry::App(i) if self.items.get(*i).is_some_and(|it| it.pinned)))
            .map(|(s, _)| s)
            .collect();
        let Some(from) = pinned_slots.iter().position(|s| *s == dragged) else { return };
        let centers: Vec<f32> = pinned_slots.iter().map(|s| rest.cells[*s].x + rest.cells[*s].w / 2.0).collect();
        let mut order = pinned_slots.clone();
        reorder::move_to(&mut order, from, reorder::target_index(&centers, x));
        let mut display: Vec<usize> = (0..self.views.len()).collect();
        for (k, pos) in pinned_slots.iter().enumerate() {
            display[*pos] = order[k];
        }
        for (pos, view_idx) in display.iter().enumerate() {
            if *view_idx == dragged {
                continue;
            }
            if let (Some(v), Some(c)) = (self.views.get(*view_idx), rest.cells.get(pos)) {
                let _ = v.place(c.x, c.w, 1.0, self.icon_y, self.dot_y, true);
            }
        }
    }

    fn on_up(&mut self, x: f32, y: f32) {
        unsafe {
            let _ = ReleaseCapture();
        }
        let press = self.press.take();
        match (self.dragging.take(), press) {
            (Some(_), Some(p)) => self.finish_drag(x, y, p),
            (None, Some(_)) => self.on_click(x),
            _ => {}
        }
    }

    /// Lepas drag: di atas dock = lepas sematan; di dalam = pindah ke posisi baru di antara pin.
    fn finish_drag(&mut self, x: f32, y: f32, p: Press) {
        let Some(from) = p.pinned_item else { return };
        let rest = layout(&self.slots, None, self.win.0 as f32, &self.m);
        let centers: Vec<f32> = self
            .entries
            .iter()
            .zip(rest.cells.iter())
            .filter(|(e, _)| matches!(e, Entry::App(i) if self.items.get(*i).is_some_and(|it| it.pinned)))
            .map(|(_, c)| c.x + c.w / 2.0)
            .collect();
        let dropped_above = y < self.card_y - self.m.icon * 0.9;
        let changed = if dropped_above {
            let key = self.items.get(from).map(|i| i.key.clone());
            let before = self.pins.len();
            if let Some(k) = key {
                self.pins.retain(|p| key_of(&p.path) != k);
            }
            self.pins.len() != before
        } else {
            let to = reorder::target_index(&centers, x);
            reorder::move_to(&mut self.pins, from, to)
        };
        if changed {
            if let Err(e) = pins::save(&self.pins_path, &self.pins) {
                log::line("pins", &format!("simpan gagal: {e}"));
            }
        }
        log::line("dock", &format!("drag selesai dari={from} atas={dropped_above} berubah={changed}"));
        self.signature.clear();
        self.refresh_windows();
        self.apply(self.cursor, false);
    }

    fn on_click(&mut self, x: f32) {
        let Some((idx, entry)) = self.entry_at(x) else { return };
        self.hide_popups();
        match entry {
            Entry::Start => launch::toggle_start(self.start_open_at_press),
            Entry::Launchpad => crate::launchpad::toggle(),
            Entry::App(i) => {
                let Some(it) = self.items.get(i).cloned() else { return };
                if it.running() {
                    let target = if it.hwnds.len() > 1 {
                        next_in_ring(&it.hwnds, focus::foreground()).unwrap_or(it.hwnds[0])
                    } else {
                        it.hwnds[0]
                    };
                    let ok = self.activate_target(target);
                    log::line("dock", &format!("aktifkan {} hwnd={target} ok={ok}", it.name));
                } else {
                    launch::open(&it.path);
                    if let Some(v) = self.views.get(idx) {
                        let _ = v.bounce(self.icon_y);
                    }
                    log::line("dock", &format!("luncurkan {}", it.path));
                }
            }
            Entry::Sep => {}
            Entry::Trash => launch::open("shell:RecycleBinFolder"),
            Entry::Downloads => self.open_stack(),
        }
    }

    /// Aktifkan jendela; jendela yang diminimalkan dipulihkan dengan animasi membesar dari ikon dock.
    fn activate_target(&self, target: isize) -> bool {
        if focus::is_minimized(target) {
            if let Some(r) = self.icon_rect(target) {
                if crate::fx::restore(target, r, self.hwnd.0 as isize) {
                    return true;
                }
            }
        }
        focus::activate(target, self.hwnd.0 as isize)
    }

    /// Persegi ikon (px layar, tanpa magnifikasi) dari item yang memiliki jendela `hwnd`.
    fn icon_rect(&self, hwnd: isize) -> Option<(i32, i32, i32, i32)> {
        let item = self.items.iter().position(|it| it.hwnds.contains(&hwnd))?;
        let j = self.entries.iter().position(|e| matches!(e, Entry::App(i) if *i == item))?;
        let rest = layout(&self.slots, None, self.win.0 as f32, &self.m);
        let c = rest.cells.get(j)?;
        Some((self.win_pos.0 + c.x as i32, self.win_pos.1 + self.icon_y as i32, c.w as i32, self.m.icon as i32))
    }

    fn on_middle_click(&mut self, x: f32) {
        if let Some((_, Entry::App(i))) = self.entry_at(x) {
            if let Some(it) = self.items.get(i) {
                launch::open(&it.path);
            }
        }
    }

    fn menu_info(&self, x: f32) -> MenuInfo {
        match self.entry_at(x) {
            Some((_, Entry::App(i))) => MenuInfo { item: self.items.get(i).cloned(), trash: false, special: None },
            Some((_, Entry::Trash)) => MenuInfo { item: None, trash: true, special: None },
            Some((_, Entry::Downloads)) => MenuInfo { item: None, trash: false, special: Some(DOWNLOADS_KEY) },
            Some((_, Entry::Start)) => MenuInfo { item: None, trash: false, special: Some(START_KEY) },
            Some((_, Entry::Launchpad)) => MenuInfo { item: None, trash: false, special: Some(LAUNCHPAD_KEY) },
            _ => MenuInfo { item: None, trash: false, special: None },
        }
    }

    /// Buka menu kaca di atas dock; tidak ada loop pesan bersarang, hasilnya datang sebagai `MENU_PICK`/`MENU_CLOSE`.
    fn open_menu(&mut self, x: f32) {
        self.menu_open = true;
        self.hide_popups();
        let info = self.menu_info(x);
        let mut items = Vec::new();
        if info.trash {
            items.push(menu::Item::action(CMD_TRASH_OPEN, crate::prefs::tr("Buka"), true));
            items.push(menu::Item::action(CMD_TRASH_EMPTY, crate::prefs::tr("Kosongkan tempat sampah"), true));
        }
        if let Some(it) = &info.item {
            items.push(menu::Item::action(CMD_OPEN, crate::prefs::tr("Buka"), true));
            let can_pin = it.pinned || !it.path.contains('?');
            items.push(menu::Item::action(CMD_PIN, crate::prefs::tr(if it.pinned { "Hapus dari dock" } else { "Simpan di dock" }), can_pin));
            items.push(menu::Item::action(CMD_ICON, crate::prefs::tr("Ubah ikon…"), true));
            if custom_icons::has(&it.path) {
                items.push(menu::Item::action(CMD_ICON_RESET, crate::prefs::tr("Kembalikan ikon asli"), true));
            }
            // seperti macOS: menutup app selalu paling bawah (refleks klik kanan → item terakhir)
            if it.running() {
                items.push(menu::Item::separator());
                items.push(menu::Item::action(CMD_CLOSE, crate::prefs::tr("Tutup"), true));
            }
        }
        if info.special == Some(DOWNLOADS_KEY) {
            items.push(menu::Item::action(CMD_STACK_FOLDER, crate::prefs::tr("Buka folder Unduhan"), true));
            items.push(menu::Item::separator());
        } else if let Some(key) = info.special {
            items.push(menu::Item::action(CMD_OPEN, crate::prefs::tr(if key == START_KEY { "Buka menu Mulai" } else { "Buka Apps" }), true));
            items.push(menu::Item::action(CMD_ICON, crate::prefs::tr("Ubah ikon…"), true));
            if custom_icons::has(key) {
                items.push(menu::Item::action(CMD_ICON_RESET, crate::prefs::tr("Kembalikan ikon asli"), true));
            }
            items.push(menu::Item::separator());
        }
        // "Keluar dari ABRA" hanya di area kosong dock dan ikon Start, bukan di ikon app
        let general = info.item.is_none() && !info.trash && (info.special.is_none() || info.special == Some(START_KEY));
        if general {
            items.push(menu::Item::action(CMD_ADD, crate::prefs::tr("Tambah app ke dock…"), true));
            items.push(menu::Item::action(CMD_SETTINGS, crate::prefs::tr("Pengaturan ABRA…"), true));
            items.push(menu::Item::separator());
            items.push(menu::Item::action(CMD_QUIT, crate::prefs::tr("Keluar dari ABRA"), true));
        }
        while items.last().is_some_and(|i| i.separator) {
            items.pop();
        }
        self.pending_menu = Some(info);
        if self.menu.is_none() {
            self.menu = Menu::new(&self.comp, self.hwnd, self.dpi, self.screen, self.tier).ok();
        }
        let mut pt = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut pt);
        }
        let bottom = self.win_pos.1 + self.card_y as i32 - (6.0 * self.dpi) as i32;
        log::line("dock", &format!("menu: kursor=({},{}) bawah={bottom} dari klik x={x}", pt.x, pt.y));
        if let Some(m) = self.menu.as_mut() {
            if let Err(e) = m.show(&self.comp, &self.gfx, items, pt.x, bottom) {
                log::line("menu", &format!("gagal: {e}"));
                self.on_menu_close();
            }
        } else {
            self.on_menu_close();
        }
    }

    /// Klik ikon Unduhan: menu kaca berisi berkas terbaru (terbaru di bawah, dekat ikon) dan "Buka folder Unduhan".
    fn open_stack(&mut self) {
        self.menu_open = true;
        self.hide_popups();
        let files = nacre_platform::downloads::latest(10);
        let mut items = Vec::new();
        self.stack_files.clear();
        for (name, path) in files.iter().rev() {
            let mut label: String = name.chars().take(48).collect();
            if name.chars().count() > 48 {
                label.push('…');
            }
            items.push(menu::Item::action(CMD_STACK + self.stack_files.len(), &label, true));
            self.stack_files.push(path.clone());
        }
        if !items.is_empty() {
            items.push(menu::Item::separator());
        }
        items.push(menu::Item::action(CMD_STACK_FOLDER, crate::prefs::tr("Buka folder Unduhan"), true));
        self.pending_menu = Some(MenuInfo { item: None, trash: false, special: Some(DOWNLOADS_KEY) });
        if self.menu.is_none() {
            self.menu = Menu::new(&self.comp, self.hwnd, self.dpi, self.screen, self.tier).ok();
        }
        let mut pt = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut pt);
        }
        let bottom = self.win_pos.1 + self.card_y as i32 - (6.0 * self.dpi) as i32;
        if let Some(m) = self.menu.as_mut() {
            if let Err(e) = m.show(&self.comp, &self.gfx, items, pt.x, bottom) {
                log::line("menu", &format!("gagal: {e}"));
                self.on_menu_close();
            }
        } else {
            self.on_menu_close();
        }
    }

    /// Ingat app non-pin yang sedang berjalan (terbaru di depan); disimpan hanya bila daftar berubah.
    fn track_recents(&mut self, items: &[Item]) {
        if !crate::prefs::get().recent_apps {
            return;
        }
        let mut changed = false;
        for it in items.iter().filter(|i| i.running() && !i.pinned && !i.path.is_empty() && !i.path.contains('?')).rev() {
            if !self.recents.iter().any(|r| key_of(&r.path) == it.key) {
                self.recents.insert(0, Pin { path: it.path.clone(), name: it.name.clone() });
                changed = true;
            }
        }
        self.recents.truncate(RECENTS_KEEP);
        if changed {
            if let Err(e) = pins::save(&self.recents_path, &self.recents) {
                log::line("recents", &format!("simpan gagal: {e}"));
            }
        }
    }

    /// Tampilkan hingga 3 app terakhir (sudah tertutup, bukan pin) di akhir daftar app.
    fn append_recents(&self, items: &mut Vec<Item>) {
        if !crate::prefs::get().recent_apps {
            return;
        }
        let mut added = 0;
        for r in &self.recents {
            if added >= RECENTS_SHOW {
                break;
            }
            let k = key_of(&r.path);
            if items.iter().any(|i| i.key == k) {
                continue;
            }
            items.push(Item { key: k, name: r.name.clone(), path: r.path.clone(), pinned: false, hwnds: vec![] });
            added += 1;
        }
    }

    fn on_menu_pick(&mut self, id: usize) {
        log::line("dock", &format!("menu: pilih {id}"));
        self.menu_open = false;
        self.start_grace();
        if let Some(info) = self.pending_menu.take() {
            self.run_command(id, info);
        }
        self.update_visibility();
    }

    fn on_menu_close(&mut self) {
        log::line("dock", "menu: ditutup");
        if let Some(m) = &self.menu {
            m.hide();
        }
        self.menu_open = false;
        self.pending_menu = None;
        self.start_grace();
        self.update_visibility();
    }

    /// Muat ulang ikon satu app (setelah ikon kustom diubah/dikembalikan).
    fn reload_icon(&mut self, path: &str) {
        self.surfaces.remove(path);
        self.requested.insert(path.to_string());
        let _ = self.worker.requests.send(path.to_string());
    }

    /// Tambah app ke dock dari jalur berkas (.exe atau .lnk). Mengabaikan duplikat dan berkas bukan app.
    pub fn add_app(&mut self, picked: &str) {
        let lower = picked.to_ascii_lowercase();
        let Some(target) = nacre_platform::pick::resolve_shortcut(picked) else {
            log::line("dock", &format!("pintasan tak terbaca: {picked}"));
            return;
        };
        let is_store = target.to_ascii_lowercase().starts_with("shell:appsfolder");
        if !(lower.ends_with(".exe") || lower.ends_with(".lnk")) || !(is_store || std::path::Path::new(&target).exists()) {
            log::line("dock", &format!("bukan app yang valid: {picked}"));
            return;
        }
        let k = key_of(&target);
        if self.pins.iter().any(|p| key_of(&p.path) == k) {
            return;
        }
        let name = if lower.ends_with(".lnk") {
            std::path::Path::new(picked).file_stem().and_then(|s| s.to_str()).map(str::to_string).unwrap_or_else(|| pins::name_for(&target))
        } else {
            pins::name_for(&target)
        };
        log::line("dock", &format!("tambah app: {name} -> {target}"));
        self.pins.push(Pin { path: target, name });
        if let Err(e) = pins::save(&self.pins_path, &self.pins) {
            log::line("pins", &format!("simpan gagal: {e}"));
        }
        self.signature.clear();
        self.refresh_windows();
    }

    /// Berkas dijatuhkan ke dock (drag dari Explorer).
    fn on_drop(&mut self, hdrop: windows::Win32::UI::Shell::HDROP) {
        use windows::Win32::UI::Shell::{DragFinish, DragQueryFileW};
        unsafe {
            let n = DragQueryFileW(hdrop, u32::MAX, None);
            for i in 0..n {
                let len = DragQueryFileW(hdrop, i, None) as usize;
                let mut buf = vec![0u16; len + 1];
                DragQueryFileW(hdrop, i, Some(&mut buf));
                self.add_app(&String::from_utf16_lossy(&buf[..len]));
            }
            DragFinish(hdrop);
        }
    }

    fn run_command(&mut self, cmd: usize, info: MenuInfo) {
        match cmd {
            CMD_STACK_FOLDER => {
                if let Some(d) = nacre_platform::downloads::folder() {
                    launch::open(&d.to_string_lossy());
                }
            }
            c if c >= CMD_STACK => {
                // hanya path yang kita daftarkan sendiri dari folder Unduhan
                if let Some(p) = self.stack_files.get(c - CMD_STACK).cloned() {
                    launch::open(&p);
                }
            }
            CMD_OPEN => {
                if let Some(it) = &info.item {
                    launch::open(&it.path);
                } else if let Some(key) = info.special {
                    if key == START_KEY {
                        launch::toggle_start(false);
                    } else {
                        crate::launchpad::toggle();
                    }
                }
            }
            CMD_CLOSE => {
                if let Some(it) = &info.item {
                    for h in &it.hwnds {
                        focus::request_close(*h);
                    }
                }
            }
            CMD_PIN => {
                if let Some(it) = &info.item {
                    if it.pinned {
                        self.pins.retain(|p| key_of(&p.path) != it.key);
                    } else if !it.path.contains('?') {
                        self.pins.push(Pin { path: it.path.clone(), name: it.name.clone() });
                    }
                    if let Err(e) = pins::save(&self.pins_path, &self.pins) {
                        log::line("pins", &format!("simpan gagal: {e}"));
                    }
                    self.signature.clear();
                    self.refresh_windows();
                }
            }
            CMD_SETTINGS => crate::settings_win::open(&self.comp),
            CMD_TRASH_OPEN => launch::open("shell:RecycleBinFolder"),
            CMD_TRASH_EMPTY => unsafe {
                // Dialog konfirmasi bawaan Windows tetap muncul (flag 0): tidak ada penghapusan tanpa persetujuan.
                let _ = windows::Win32::UI::Shell::SHEmptyRecycleBinW(Some(self.hwnd), windows::core::PCWSTR::null(), 0);
                timer(self.hwnd, TIMER_TRASH, 300);
            },
            CMD_ADD => {
                let picked = nacre_platform::pick::open_file(
                    crate::prefs::tr("Pilih aplikasi"),
                    &[(crate::prefs::tr("Aplikasi"), "*.exe;*.lnk"), ("*.*", "*.*")],
                );
                if let Some(p) = picked {
                    self.add_app(&p);
                }
            }
            CMD_ICON if info.special.is_some() => {
                let key = info.special.unwrap_or(START_KEY);
                let picked = nacre_platform::pick::open_file(
                    crate::prefs::tr("Pilih gambar ikon"),
                    &[(crate::prefs::tr("Gambar"), "*.png;*.jpg;*.jpeg;*.ico;*.bmp;*.gif")],
                );
                if let Some(img) = picked {
                    if custom_icons::set(key, &img) {
                        self.reload_icon(key);
                        self.signature.clear();
                        self.refresh_windows();
                    } else {
                        log::line("dock", "gambar ikon tidak valid");
                    }
                }
            }
            CMD_ICON_RESET if info.special.is_some() => {
                let key = info.special.unwrap_or(START_KEY);
                custom_icons::reset(key);
                self.surfaces.remove(key);
                self.signature.clear();
                self.refresh_windows();
            }
            CMD_ICON => {
                if let Some(it) = &info.item {
                    let picked = nacre_platform::pick::open_file(
                        crate::prefs::tr("Pilih gambar ikon"),
                        &[(crate::prefs::tr("Gambar"), "*.png;*.jpg;*.jpeg;*.ico;*.bmp;*.gif")],
                    );
                    if let Some(img) = picked {
                        if custom_icons::set(&it.path, &img) {
                            self.reload_icon(&it.path);
                        } else {
                            log::line("dock", "gambar ikon tidak valid");
                        }
                    }
                }
            }
            CMD_ICON_RESET => {
                if let Some(it) = &info.item {
                    custom_icons::reset(&it.path);
                    self.reload_icon(&it.path);
                }
            }
            CMD_QUIT => unsafe {
                let _ = PostMessageW(Some(self.main_hwnd), WM_COMMAND, WPARAM(crate::mainwin::ID_QUIT), LPARAM(0));
            },
            _ => {}
        }
    }

    // ---------- timer & pesan pendamping ----------

    fn on_timer(&mut self, id: usize) {
        kill(self.hwnd, id);
        match id {
            TIMER_WINDOWS => self.refresh_windows(),
            TIMER_APPBAR => {
                if self.opts.appbar {
                    self.reserve_strip();
                }
            }
            TIMER_TIP => self.show_tooltip(),
            TIMER_PREVIEW => self.show_preview(),
            TIMER_GRACE => {
                self.grace = false;
                self.update_visibility();
            }
            TIMER_DWELL => {
                self.edge_armed = true;
                self.update_visibility();
            }
            TIMER_PCLOSE => {
                if !self.preview_hover && !self.tracking {
                    if let Some(p) = self.preview.as_mut() {
                        p.hide();
                    }
                }
                self.update_visibility();
            }
            TIMER_HIDEWIN => {
                if !self.shown {
                    unsafe {
                        let _ = ShowWindow(self.hwnd, SW_HIDE);
                    }
                }
            }
            TIMER_START => {
                self.starting = false;
                self.update_visibility();
            }
            TIMER_REGION => {
                if !self.tracking {
                    self.set_region(false);
                }
            }
            TIMER_RECONF => self.on_reconfigure(),
            TIMER_TRASH => {
                let full = nacre_platform::recycle::is_full();
                if full != self.trash_full {
                    self.trash_full = full;
                    self.update_trash_icon();
                }
                timer(self.hwnd, TIMER_TRASH, TRASH_POLL_MS);
            }
            _ => {}
        }
    }

    fn on_strip_enter(&mut self) {
        if self.opts.mode != Mode::Always {
            timer(self.hwnd, TIMER_DWELL, 200);
        }
    }

    fn on_strip_leave(&mut self) {
        kill(self.hwnd, TIMER_DWELL);
        if self.edge_armed {
            self.edge_armed = false;
            self.start_grace();
            self.update_visibility();
        }
    }

    fn on_preview_click(&mut self, x: i32) {
        let target = self.preview.as_ref().and_then(|p| p.hit(x));
        if let Some(h) = target {
            let ok = self.activate_target(h);
            log::line("dock", &format!("pratinjau → aktifkan hwnd={h} ok={ok}"));
        }
        self.hide_popups();
        self.start_grace();
    }
}

fn auto_tier(c: &sys::Conditions) -> Tier {
    if !c.transparency || c.battery_saver || c.remote_session {
        Tier::Solid
    } else {
        Tier::Full
    }
}

fn xy(lp: LPARAM) -> (f32, f32) {
    ((lp.0 as i16) as f32, ((lp.0 >> 16) as i16) as f32)
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_MOUSEMOVE => {
            let (x, y) = xy(lp);
            with_dock(hwnd, |d| {
                if !d.tracking {
                    let mut t = TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    if TrackMouseEvent(&mut t).is_ok() {
                        d.tracking = true;
                        d.update_visibility();
                    }
                }
                d.on_move(x, y);
            });
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            with_dock(hwnd, |d| d.on_leave());
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let (x, y) = xy(lp);
            with_dock(hwnd, |d| d.on_down(x, y));
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let (x, y) = xy(lp);
            with_dock(hwnd, |d| d.on_up(x, y));
            LRESULT(0)
        }
        WM_CAPTURECHANGED => {
            with_dock(hwnd, |d| {
                if d.dragging.take().is_some() {
                    d.press = None;
                    d.apply(None, false);
                    d.bind_expressions();
                }
            });
            LRESULT(0)
        }
        WM_MBUTTONUP => {
            let (x, _) = xy(lp);
            with_dock(hwnd, |d| d.on_middle_click(x));
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            let (x, _) = xy(lp);
            with_dock(hwnd, |d| d.open_menu(x));
            LRESULT(0)
        }
        MENU_PICK => {
            with_dock(hwnd, |d| d.on_menu_pick(wp.0));
            LRESULT(0)
        }
        MENU_CLOSE => {
            with_dock(hwnd, |d| d.on_menu_close());
            LRESULT(0)
        }
        WM_TIMER => {
            with_dock(hwnd, |d| d.on_timer(wp.0));
            LRESULT(0)
        }
        WM_ICONS => {
            with_dock(hwnd, |d| d.on_icons());
            LRESULT(0)
        }
        WM_APPBAR_SYNC => {
            // Beri waktu taskbar menyelesaikan perubahan state sebelum strip dihitung ulang.
            timer(hwnd, TIMER_APPBAR, 400);
            LRESULT(0)
        }
        WM_DROPFILES => {
            let h = windows::Win32::UI::Shell::HDROP(wp.0 as *mut _);
            with_dock(hwnd, |d| d.on_drop(h));
            LRESULT(0)
        }
        crate::prefs::WM_PREFS => {
            with_dock(hwnd, |d| d.apply_prefs());
            LRESULT(0)
        }
        STRIP_ENTER => {
            with_dock(hwnd, |d| d.on_strip_enter());
            LRESULT(0)
        }
        STRIP_LEAVE => {
            with_dock(hwnd, |d| d.on_strip_leave());
            LRESULT(0)
        }
        PREVIEW_ENTER => {
            with_dock(hwnd, |d| {
                d.preview_hover = true;
                kill(hwnd, TIMER_PCLOSE);
            });
            LRESULT(0)
        }
        PREVIEW_LEAVE => {
            with_dock(hwnd, |d| {
                d.preview_hover = false;
                timer(hwnd, TIMER_PCLOSE, 200);
            });
            LRESULT(0)
        }
        PREVIEW_CLICK => {
            let x = wp.0 as isize as i32;
            with_dock(hwnd, |d| d.on_preview_click(x));
            LRESULT(0)
        }
        WM_DISPLAYCHANGE | WM_DPICHANGED => {
            // Pesan ini datang bertubi-tubi saat resolusi berganti; tunggu stabil dulu.
            timer(hwnd, TIMER_RECONF, 500);
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_SETTINGCHANGE | WM_POWERBROADCAST => {
            with_dock(hwnd, |d| d.refresh_tier());
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_DESTROY => {
            nacre_platform::shell::appbar::release();
            DOCK_HWND.store(0, Ordering::Relaxed);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}











