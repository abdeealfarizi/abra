//! Stage Manager ala macOS Tahoe: satu grup app di panggung, grup lain di strip kiri sebagai kartu miring
//! berisi tangkapan jendela (diperbarui pelan) dengan lencana ikon. Jendela yang keluar panggung DIPARKIR di luar
//! layar (bukan diminimalkan) sehingga tetap "hidup". Klik kartu menukar grup dengan animasi terbang.
//! Daftar jendela terparkir disimpan ke `parked.txt` agar bisa dipulihkan setelah ABRA mati mendadak.
//! Event-driven (hook foreground yang sudah ada + timer debounce), tanpa polling saat tidak aktif.

use crate::popup::Popup;
use crate::{motion, paths, prefs};
use nacre_core::stage::{Change, Stage};
use nacre_platform::icons::{self, Bitmap};
use nacre_platform::log;
use nacre_platform::wins::park::{self, Parked};
use nacre_platform::wins::{enumerate, focus, snapshot, tracker};
use nacre_ui::capture::{self, Capture};
use nacre_ui::gfx::Gfx;
use std::cell::RefCell;
use std::collections::HashMap;
use windows::core::w;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::UI::Composition::{CompositionBrush, CompositionSurfaceBrush, CompositionStretch, Compositor, ContainerVisual, SpriteVisual};
use windows_numerics::{Matrix4x4, Vector2, Vector3};

const WM_FG: u32 = WM_APP + 110;
const WM_LOC: u32 = WM_APP + 111;
const WM_EDGE_IN: u32 = WM_APP + 112;
const WM_EDGE_OUT: u32 = WM_APP + 113;
const WM_MOUSELEAVE: u32 = 0x02A3;
const T_FG: usize = 1;
const T_FLY: usize = 2;
const T_SHOT: usize = 3;
const T_OVERLAP: usize = 4;
const T_HIDE: usize = 5;
const T_PEEK: usize = 6;
const T_FLYEND: usize = 7;
const T_LIVE: usize = 8;

/// Kemiringan kartu strip (derajat) saat diam dan saat di-hover.
const TILT: f32 = 22.0;
const TILT_HOVER: f32 = 8.0;

type Rect = (f32, f32, f32, f32);

struct Slot {
    apps: Vec<String>,
    rect: Rect,
    cont: ContainerVisual,
    cards: Vec<SpriteVisual>,
    /// Kuas gambar tiap kartu (diganti di tempat saat tangkapan diperbarui, tanpa membangun ulang strip).
    brushes: Vec<(String, CompositionSurfaceBrush)>,
    /// Judul jendela (muncul saat hover).
    label: Option<SpriteVisual>,
}

struct StripUi {
    popup: Popup,
    dpi: f32,
    w: f32,
    slots: Vec<Slot>,
    hover: Option<usize>,
    tracking: bool,
    shown: bool,
    /// Strip di sisi kanan layar (kartu dicerminkan).
    right: bool,
}

struct Fly {
    popup: Popup,
    /// App yang jendelanya dikembalikan ke layar saat animasi selesai.
    show: Vec<String>,
    activate: isize,
}

struct St {
    ctl: HWND,
    comp: Compositor,
    gfx: Option<Gfx>,
    /// Perangkat GPU + tangkapan jendela hidup (Windows.Graphics.Capture) untuk kartu strip; hanya selama Stage Manager aktif.
    live_gfx: Option<Gfx>,
    live: HashMap<String, Capture>,
    active: bool,
    model: Stage,
    parked: Vec<(Parked, String)>,
    shots: HashMap<String, Bitmap>,
    icons: HashMap<String, Option<Bitmap>>,
    strip: Option<StripUi>,
    edge: Option<crate::strip::Strip>,
    fly: Option<Fly>,
    /// Overlay animasi yang sedang memudar sebelum dihancurkan.
    dying: Option<Popup>,
    /// Daftar app strip terakhir (untuk animasi muncul saat isi strip berubah).
    sig: String,
    peek: bool,
    /// Jendela yang seharusnya jadi foreground setelah tukar (diaktifkan ulang bila Windows memindahkan fokus).
    want_fg: isize,
    /// App yang jendelanya menolak diparkir (mis. dijalankan sebagai Administrator): tidak dikelola agar tidak tampil ganda.
    refused: Vec<String>,
    /// Selama animasi tukar berjalan, perubahan foreground otomatis dari Windows diabaikan.
    busy_until: std::time::Instant,
    /// Konfigurasi tata letak terakhir yang diterapkan (untuk mendeteksi perubahan pengaturan).
    cfg: (bool, u8, u8, u8, u8),
}

/// Media yang kita jeda: (app, id sumber) — dilanjutkan saat app kembali ke panggung. Diisi dari thread pekerja.
static PAUSED: std::sync::Mutex<Vec<(String, Vec<String>)>> = std::sync::Mutex::new(Vec::new());

fn take_paused(app: Option<&str>) -> Vec<Vec<String>> {
    let Ok(mut g) = PAUSED.lock() else { return vec![] };
    let mut out = Vec::new();
    g.retain(|(a, ids)| {
        if app.map(|x| x == a).unwrap_or(true) {
            out.push(ids.clone());
            false
        } else {
            true
        }
    });
    out
}

/// Program yang dikecualikan dari Stage Manager (tidak pernah diparkir): satu potongan jalur/nama per baris di `stage_exclude.txt`.
fn excluded(app: &str) -> bool {
    let Ok(t) = std::fs::read_to_string(paths::config_dir().join("stage_exclude.txt")) else { return false };
    t.lines().map(|l| l.trim().to_ascii_lowercase()).any(|l| !l.is_empty() && !l.starts_with('#') && app.contains(&l))
}

thread_local! {
    static ST: RefCell<Option<St>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut St) -> R) -> Option<R> {
    ST.with(|c| c.try_borrow_mut().ok().and_then(|mut b| b.as_mut().map(f)))
}

fn hook(ev: u32, hwnd: isize) {
    let target = ST.with(|c| c.try_borrow().ok().and_then(|b| b.as_ref().map(|s| (s.ctl, s.active))));
    let Some((ctl, true)) = target else { return };
    unsafe {
        if ev == tracker::EV_FOREGROUND {
            let _ = PostMessageW(Some(ctl), WM_FG, WPARAM(0), LPARAM(0));
        } else if ev == tracker::EV_LOCATION || ev == tracker::EV_MOVESIZE_END {
            let _ = PostMessageW(Some(ctl), WM_LOC, WPARAM(hwnd as usize), LPARAM(0));
        }
    }
}

pub fn init(comp: &Compositor) -> windows_core::Result<()> {
    // sisa jendela terparkir dari proses sebelumnya yang mati mendadak
    let n = park::restore_file(&parked_file());
    if n > 0 {
        log::line("stage", &format!("{n} jendela terparkir dari sesi lalu dikembalikan"));
    }
    unsafe {
        let hi: HINSTANCE = GetModuleHandleW(None)?.into();
        let class = w!("AbraStageCtl");
        RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(ctl_proc), hInstance: hi, lpszClassName: class, ..Default::default() });
        let hwnd = CreateWindowExW(WINDOW_EX_STYLE(0), class, w!("abra-stage"), WS_POPUP, 0, 0, 0, 0, Some(HWND_MESSAGE), None, Some(hi), None)?;
        ST.with(|c| {
            *c.borrow_mut() = Some(St {
                ctl: hwnd,
                comp: comp.clone(),
                gfx: None,
                live_gfx: None,
                live: HashMap::new(),
                active: false,
                model: Stage::default(),
                parked: Vec::new(),
                shots: HashMap::new(),
                icons: HashMap::new(),
                strip: None,
                edge: None,
                fly: None,
                dying: None,
                sig: String::new(),
                peek: false,
                want_fg: 0,
                refused: Vec::new(),
                busy_until: std::time::Instant::now(),
                cfg: (false, 0, 0, 0, 0),
            })
        });
        prefs::listen(hwnd);
        tracker::install(noop);
        tracker::install_fx(hook);
    }
    sync();
    with(|s| s.apply_icons());
    Ok(())
}

fn noop() {}

/// App yang berjalan (punya jendela), tanpa program yang dikecualikan dari Stage Manager.
fn running_apps(wins: &[nacre_core::dock::items::Win], refused: &[String]) -> Vec<String> {
    let mut running: Vec<String> = Vec::new();
    for w in wins {
        let k = key_of(&w.path);
        if !k.is_empty() && !running.contains(&k) && !excluded(&k) && !refused.contains(&k) {
            running.push(k);
        }
    }
    running
}

/// Tangkapan kosong (satu warna rata, mis. putih/hitam dari PrintWindow yang gagal)? Diuji dari 144 titik sampel.
pub(crate) fn blank(b: &Bitmap) -> bool {
    if b.w < 4 || b.h < 4 {
        return true;
    }
    let first = &b.bgra[0..3];
    for sy in 0..12u32 {
        for sx in 0..12u32 {
            let (x, y) = ((sx * (b.w - 1)) / 11, (sy * (b.h - 1)) / 11);
            let i = ((y * b.w + x) * 4) as usize;
            let d = (0..3).map(|c| (b.bgra[i + c] as i32 - first[c] as i32).abs()).max().unwrap_or(0);
            if d > 6 {
                return false;
            }
        }
    }
    true
}

/// Apakah ikon desktop harus terlihat: pengaturan global "sembunyikan" menang; selain itu ikuti mode Stage Manager.
fn icons_visible(stage_active: bool, stage_empty: bool) -> bool {
    let p = prefs::get();
    if p.hide_desktop_icons {
        return false;
    }
    if !stage_active {
        return true;
    }
    match p.stage_icons {
        0 => false,
        1 => stage_empty,
        _ => true,
    }
}

fn parked_file() -> std::path::PathBuf {
    paths::config_dir().join("parked.txt")
}

/// Dipanggil saat keluar normal: kembalikan semua jendela yang kita parkir.
pub fn shutdown() {
    with(|s| s.deactivate());
    nacre_platform::desktopicons::set_visible(true);
}

/// Dipakai pengalih app (Alt+Tab pengganti): apakah Stage Manager menyala.
pub fn is_active() -> bool {
    with(|s| s.active).unwrap_or(false)
}

/// Bawa app `path` ke panggung (dipakai pengalih app).
/// Tangkapan terakhir yang bagus untuk `app` (jalur exe), dipakai pengalih app untuk jendela yang sedang diparkir.
pub fn shot_for(app: &str) -> Option<Bitmap> {
    with(|s| s.shots.get(&key_of(app)).cloned()).flatten()
}

pub fn bring_app(path: &str) {
    log::line("stage", &format!("bring {path}"));
    with(|s| s.focus_app(&key_of(path), 0));
}

fn sync() {
    let p = prefs::get();
    let want = p.stage;
    with(|s| {
        if want && !s.active {
            s.activate();
        } else if !want && s.active {
            s.deactivate();
        } else if want && s.active {
            let cfg = (p.stage_right, p.stage_size, p.stage_gap, p.stage_count, p.stage_titles as u8);
            if cfg != s.cfg {
                s.cfg = cfg;
                s.model.max = p.stage_count as usize;
                s.edge = None;
                s.destroy_strip();
                s.rebuild();
                s.check_overlap();
            }
        }
        s.apply_icons();
    });
}

fn key_of(path: &str) -> String {
    path.to_ascii_lowercase()
}

fn is_desktop(hwnd: isize) -> bool {
    unsafe {
        let mut cls = [0u16; 64];
        let n = GetClassNameW(HWND(hwnd as *mut _), &mut cls);
        let c = String::from_utf16_lossy(&cls[..n.max(0) as usize]);
        c == "Progman" || c == "WorkerW"
    }
}

fn is_own(hwnd: isize) -> bool {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(HWND(hwnd as *mut _), Some(&mut pid));
        pid == std::process::id()
    }
}

/// Matriks perspektif berpusat di (cx, cy) untuk container induk kartu (kedalaman d).
fn perspective(cx: f32, cy: f32, d: f32) -> Matrix4x4 {
    Matrix4x4 {
        M11: 1.0, M12: 0.0, M13: 0.0, M14: 0.0,
        M21: 0.0, M22: 1.0, M23: 0.0, M24: 0.0,
        M31: -cx / d, M32: -cy / d, M33: 1.0, M34: -1.0 / d,
        M41: 0.0, M42: 0.0, M43: 0.0, M44: 1.0,
    }
}

impl St {
    // -----------------------------------------------------------------------------------------------------------
    // siklus hidup
    // -----------------------------------------------------------------------------------------------------------

    fn activate(&mut self) {
        self.active = true;
        let p = prefs::get();
        self.model = Stage { max: p.stage_count as usize, ..Default::default() };
        self.cfg = (p.stage_right, p.stage_size, p.stage_gap, p.stage_count, p.stage_titles as u8);
        log::line("stage", "aktif");
        self.refresh();
        self.apply_icons();
        unsafe {
            SetTimer(Some(self.ctl), T_SHOT, 3000, None);
            SetTimer(Some(self.ctl), T_LIVE, 50, None);
        }
    }

    fn deactivate(&mut self) {
        self.refused.clear();
        if !self.active && self.parked.is_empty() {
            return;
        }
        self.active = false;
        unsafe {
            let _ = KillTimer(Some(self.ctl), T_SHOT);
            let _ = KillTimer(Some(self.ctl), T_LIVE);
        }
        self.live.clear();
        self.live_gfx = None;
        self.finish_fly();
        self.kill_dying();
        for (p, _) in self.parked.drain(..) {
            park::unpark(&p);
        }
        for ids in take_paused(None) {
            nacre_platform::media::resume_ids(ids);
        }
        let _ = std::fs::remove_file(parked_file());
        self.destroy_strip();
        self.shots.clear();
        self.model = Stage::default();
        log::line("stage", "nonaktif");
    }

    /// Ikon desktop sesuai mode: 0 sembunyi, 1 otomatis (tampil hanya saat panggung kosong), 2 tampil.
    fn apply_icons(&self) {
        nacre_platform::desktopicons::set_visible(icons_visible(self.active, self.model.current.is_empty()));
    }

    fn destroy_strip(&mut self) {
        self.edge = None;
        if let Some(s) = self.strip.take() {
            let h = s.popup.hwnd;
            drop(s);
            unsafe {
                let _ = DestroyWindow(h);
            }
        }
    }

    fn save(&self) {
        let list: Vec<Parked> = self.parked.iter().map(|(p, _)| *p).collect();
        park::save(&parked_file(), &list);
    }

    // -----------------------------------------------------------------------------------------------------------
    // model ⇄ jendela
    // -----------------------------------------------------------------------------------------------------------

    /// Foreground berubah: perbarui panggung.
    fn refresh(&mut self) {
        let now = std::time::Instant::now();
        if self.fly.is_some() || now < self.busy_until {
            // perubahan foreground selama animasi tidak boleh hilang: periksa ulang begitu animasi selesai
            let wait = self.busy_until.saturating_duration_since(now).as_millis() as u32 + 60;
            unsafe {
                SetTimer(Some(self.ctl), T_FG, wait.max(80), None);
            }
            return;
        }
        let fg = focus::foreground();
        if fg == 0 || is_own(fg) {
            return;
        }
        if self.want_fg != 0 {
            // Windows kadang memindahkan fokus segera setelah tukar: aktifkan lagi jendela yang dipilih (sekali)
            let w = std::mem::take(&mut self.want_fg);
            if fg != w && unsafe { IsWindow(Some(HWND(w as *mut _))).as_bool() } {
                focus::force_foreground(w);
                self.busy_until = std::time::Instant::now() + std::time::Duration::from_millis(350);
                unsafe {
                    SetTimer(Some(self.ctl), T_FG, 420, None);
                }
                return;
            }
        }
        let wins = enumerate::list(std::process::id());
        let running = running_apps(&wins, &self.refused);
        self.model.prune(&running);
        let ch = if is_desktop(fg) {
            if !prefs::get().stage_desktop_click {
                return;
            }
            self.model.desktop()
        } else {
            let Some(w) = wins.iter().find(|w| w.hwnd == fg) else { return };
            let app = key_of(&w.path);
            // program kosong/dikecualikan tidak ikut dikelola
            if app.is_empty() || excluded(&app) || self.refused.contains(&app) {
                return;
            }
            self.model.focus(&app, &running)
        };
        self.apply(ch, &wins, 0);
    }

    /// Klik kartu / pengalih app: jadikan `app` fokus panggung.
    fn focus_app(&mut self, app: &str, _from: isize) {
        let wins = enumerate::list(std::process::id());
        let running = running_apps(&wins, &self.refused);
        let target = self.parked.iter().find(|(_, a)| a == app).map(|(p, _)| p.hwnd).or_else(|| wins.iter().find(|w| key_of(&w.path) == app).map(|w| w.hwnd)).unwrap_or(0);
        let ch = self.model.focus(app, &running);
        self.apply(ch, &wins, target);
    }

    fn slot_rect_of(&self, app: &str) -> Option<Rect> {
        let s = self.strip.as_ref()?;
        let base = s.popup.rect;
        s.slots.iter().find(|sl| sl.apps.iter().any(|a| a == app)).map(|sl| (base.0 as f32 + sl.rect.0, base.1 as f32 + sl.rect.1, sl.rect.2, sl.rect.3))
    }

    fn apply(&mut self, ch: Change, wins: &[nacre_core::dock::items::Win], activate: isize) {
        self.finish_fly();
        if activate != 0 {
            self.want_fg = activate;
        }
        self.busy_until = std::time::Instant::now() + std::time::Duration::from_millis(motion::stage_ms(motion::STAGE_SWAP_MS) as u64 + 350);
        let mut items: Vec<(Bitmap, Rect, Rect)> = Vec::new();

        // yang masuk panggung: catat asal kartunya sebelum strip dibangun ulang
        let show_from: Option<(String, Rect)> = ch.show.iter().find_map(|a| self.slot_rect_of(a).map(|r| (a.clone(), r)));

        // yang keluar panggung: tangkap lalu parkir
        let mut outs: Vec<(String, Rect)> = Vec::new();
        let mut refused_now = false;
        let media_on = prefs::get().stage_media;
        for app in &ch.show {
            for ids in take_paused(Some(app)) {
                nacre_platform::media::resume_ids(ids);
            }
        }
        // tangkapan segar untuk jendela yang akan terbang masuk (kartu tidak boleh "meloncat" saat jendela asli muncul)
        for app in &ch.show {
            if let Some((p, _)) = self.parked.iter().find(|(_, a)| a == app) {
                if let Some(shot) = snapshot::capture(p.hwnd, 2) {
                    if !blank(&shot.bmp) || !self.shots.contains_key(app) {
                        self.shots.insert(app.clone(), shot.bmp);
                    }
                }
            }
        }
        for app in &ch.park {
            if excluded(app) {
                continue;
            }
            if media_on {
                let stem = std::path::Path::new(app).file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
                if !stem.is_empty() {
                    let key = app.clone();
                    nacre_platform::media::pause_app(stem, move |ids| {
                        if !ids.is_empty() {
                            if let Ok(mut g) = PAUSED.lock() {
                                g.push((key, ids));
                            }
                        }
                    });
                }
            }
            for w in wins.iter().filter(|w| key_of(&w.path) == *app) {
                if focus::is_minimized(w.hwnd) || park::is_parked(w.hwnd) || self.parked.iter().any(|(p, _)| p.hwnd == w.hwnd) {
                    continue;
                }
                if let Some(shot) = snapshot::capture(w.hwnd, 2) {
                    let first_of_app = !outs.iter().any(|(a, _)| a == app);
                    let good = !blank(&shot.bmp);
                    // semua app yang keluar panggung ikut terbang ke strip (maks. 4), bukan hanya yang pertama
                    if first_of_app && good && outs.len() < 4 {
                        outs.push((app.clone(), (shot.rect.0 as f32, shot.rect.1 as f32, shot.rect.2 as f32, shot.rect.3 as f32)));
                    }
                    if (first_of_app && good) || !self.shots.contains_key(app) {
                        self.shots.insert(app.clone(), shot.bmp);
                    }
                }
                if let Some(p) = park::park(w.hwnd) {
                    if park::is_parked(w.hwnd) {
                        self.parked.push((p, app.clone()));
                    } else {
                        park::unpark(&p); // jendela menolak dipindah (mis. dijalankan sebagai admin)
                        if !self.refused.contains(app) {
                            log::line("stage", &format!("{app} menolak diparkir; tidak dikelola"));
                            self.refused.push(app.clone());
                            refused_now = true;
                        }
                    }
                }
            }
        }
        if refused_now {
            // app yang tidak bisa diparkir dikeluarkan dari model agar tidak muncul sebagai kartu kosong
            let running = running_apps(&wins, &self.refused);
            self.model.prune(&running);
        }
        self.save();
        self.rebuild();

        for (app, from) in outs {
            if let (Some(bmp), Some(to)) = (self.shots.get(&app).cloned(), self.slot_rect_of(&app)) {
                items.push((bmp, from, to));
            }
        }

        // yang masuk panggung: terbang dari kartu ke posisi jendela, lalu dikembalikan ke layar
        let mut fly_in = false;
        if let Some((app, from)) = show_from {
            let to = self.parked.iter().find(|(_, a)| *a == app).map(|(p, _)| {
                let (l, t, r, b) = p.rect;
                (l as f32, t as f32, (r - l) as f32, (b - t) as f32)
            });
            if let (Some(bmp), Some(to)) = (self.shots.get(&app).cloned(), to) {
                items.push((bmp, from, to));
                fly_in = true;
            }
        }
        if !items.is_empty() {
            self.start_fly(items, if fly_in { ch.show.clone() } else { vec![] }, activate);
        }
        if !fly_in {
            self.unpark_apps(&ch.show);
            if activate != 0 {
                focus::force_foreground(activate);
            }
        }
        self.apply_icons();
        self.check_overlap();
    }

    fn unpark_apps(&mut self, apps: &[String]) {
        let mut keep = Vec::new();
        for (p, a) in self.parked.drain(..) {
            if apps.contains(&a) {
                let mut q = p;
                // jendela tidak boleh menutupi strip: geser ke kanan strip bila perlu (bukan yang dimaksimalkan)
                let sw = nacre_platform::sys::screen_size().0;
                let right = self.strip.as_ref().map(|s| s.right).unwrap_or(false);
                let edge = self.strip.as_ref().map(|s| (s.w + 8.0 * s.dpi) as i32).unwrap_or(0);
                if !q.maximized && right && q.rect.2 > sw - edge {
                    let dx = (q.rect.2 - (sw - edge)).min(q.rect.0.max(0));
                    q.rect = (q.rect.0 - dx, q.rect.1, q.rect.2 - dx, q.rect.3);
                } else if !q.maximized && !right && q.rect.0 < edge {
                    let dx = edge - q.rect.0;
                    q.rect = (q.rect.0 + dx, q.rect.1, q.rect.2 + dx, q.rect.3);
                    if q.rect.2 > sw {
                        q.rect.2 = sw;
                    }
                }
                park::unpark(&q);
            } else {
                keep.push((p, a));
            }
        }
        self.parked = keep;
        self.save();
    }

    // -----------------------------------------------------------------------------------------------------------
    // animasi terbang (overlay layar penuh sementara)
    // -----------------------------------------------------------------------------------------------------------

    fn start_fly(&mut self, items: Vec<(Bitmap, Rect, Rect)>, show: Vec<String>, activate: isize) {
        let comp = self.comp.clone();
        if self.gfx.is_none() {
            self.gfx = Gfx::new(&comp).ok();
        }
        let Some(gfx) = self.gfx.as_ref() else { return };
        let (sw, sh) = nacre_platform::sys::screen_size();
        let go = || -> windows_core::Result<Popup> {
            let base = nacre_ui::glass::GlassParams::dark((10.0, 10.0), 0.0);
            let mut popup = Popup::new(&comp, w!("AbraStageFly"), Some(fly_proc), WS_EX_TRANSPARENT, base)?;
            popup.set_rect(0, 0, sw, sh)?;
            popup.panel.set_rect(0.0, 0.0, 0.0, 0.0)?;
            popup.content.SetTransformMatrix(perspective(sw as f32 / 2.0, sh as f32 / 2.0, 1400.0))?;
            for (bmp, from, to) in &items {
                let surf = gfx.surface_from_bgra(bmp.w, bmp.h, &bmp.bgra)?;
                let brush = comp.CreateSurfaceBrushWithSurface(&surf)?;
                brush.SetStretch(CompositionStretch::Fill)?;
                let v = comp.CreateSpriteVisual()?;
                v.SetBrush(&brush)?;
                v.SetSize(Vector2 { X: to.2, Y: to.3 })?;
                let g = comp.CreateRoundedRectangleGeometry()?;
                g.SetSize(Vector2 { X: to.2, Y: to.3 })?;
                g.SetCornerRadius(Vector2 { X: 10.0, Y: 10.0 })?;
                let clip = comp.CreateGeometricClip()?;
                clip.SetGeometry(&g)?;
                v.SetClip(&clip)?;
                let (cx, cy) = (to.2 / 2.0, to.3 / 2.0);
                v.SetCenterPoint(Vector3 { X: cx, Y: cy, Z: 0.0 })?;
                let (sx, sy) = (from.2 / to.2.max(1.0), from.3 / to.3.max(1.0));
                popup.content.Children()?.InsertAtTop(&v)?;
                // skala berpusat di tengah: sesuaikan offset agar pojok kiri-atas visual tetap di `from`/`to`
                let off0 = Vector3 { X: from.0 - cx * (1.0 - sx), Y: from.1 - cy * (1.0 - sy), Z: 0.0 };
                motion::vec3(&comp, &v, "Offset", off0, Vector3 { X: to.0, Y: to.1, Z: 0.0 }, motion::stage_ms(motion::STAGE_SWAP_MS))?;
                // kemiringan perspektif: jendela keluar mulai datar lalu miring ke strip, jendela masuk sebaliknya
                let sign = if prefs::get().stage_right { -1.0f32 } else { 1.0 };
                let shrinking = from.2 > to.2;
                v.SetRotationAxis(Vector3 { X: 0.0, Y: 1.0, Z: 0.0 })?;
                let (a0, a1) = if shrinking { (0.0, sign * TILT) } else { (sign * 35.0, 0.0) };
                motion::scalar(&comp, &v, "RotationAngleInDegrees", a0, a1, motion::stage_ms(motion::STAGE_SWAP_MS))?;
                motion::vec3(&comp, &v, "Scale", Vector3 { X: sx, Y: sy, Z: 1.0 }, Vector3 { X: 1.0, Y: 1.0, Z: 1.0 }, motion::stage_ms(motion::STAGE_SWAP_MS))?;
            }
            popup.show();
            Ok(popup)
        };
        match go() {
            Ok(popup) => {
                self.fly = Some(Fly { popup, show, activate });
                unsafe {
                    SetTimer(Some(self.ctl), T_FLY, motion::stage_ms(motion::STAGE_SWAP_MS) + 30, None);
                }
            }
            Err(e) => {
                log::line("stage", &format!("animasi gagal: {e}"));
                self.unpark_apps(&show);
                if activate != 0 {
                    focus::force_foreground(activate);
                }
            }
        }
    }

    fn finish_fly(&mut self) {
        unsafe {
            let _ = KillTimer(Some(self.ctl), T_FLY);
        }
        if let Some(f) = self.fly.take() {
            self.unpark_apps(&f.show);
            if f.activate != 0 {
                focus::force_foreground(f.activate);
            }
            if let Some(old) = self.dying.take() {
                let h = old.hwnd;
                drop(old);
                unsafe {
                    let _ = DestroyWindow(h);
                }
            }
            // overlay memudar halus di atas jendela asli yang baru muncul
            let _ = motion::scalar(&self.comp, f.popup.root(), "Opacity", 1.0, 0.0, 140);
            let _ = f.popup.root().SetOpacity(0.0);
            self.dying = Some(f.popup);
            unsafe {
                SetTimer(Some(self.ctl), T_FLYEND, 180, None);
            }
        }
    }

    fn kill_dying(&mut self) {
        unsafe {
            let _ = KillTimer(Some(self.ctl), T_FLYEND);
        }
        if let Some(old) = self.dying.take() {
            let h = old.hwnd;
            drop(old);
            unsafe {
                let _ = DestroyWindow(h);
            }
        }
    }

    // -----------------------------------------------------------------------------------------------------------
    // strip
    // -----------------------------------------------------------------------------------------------------------

    fn icon(&mut self, app: &str, px: i32) -> Option<Bitmap> {
        if !self.icons.contains_key(app) {
            let b = icons::load(app, px);
            self.icons.insert(app.to_string(), b);
        }
        self.icons.get(app).cloned().flatten()
    }

    fn ensure_strip(&mut self) -> bool {
        if self.strip.is_some() {
            return true;
        }
        let mut look = prefs::look(prefs::Role::Dark);
        look.blur_sigma = 0.0;
        let base = nacre_ui::glass::GlassParams::dark((10.0, 10.0), 0.0);
        let Ok(popup) = Popup::new(&self.comp, w!("AbraStageStrip"), Some(strip_proc), WINDOW_EX_STYLE(0), prefs::apply(base, &look)) else { return false };
        let dpi = unsafe { GetDpiForWindow(popup.hwnd) }.max(96) as f32 / 96.0;
        let _ = popup.panel.set_rect(0.0, 0.0, 0.0, 0.0);
        self.strip = Some(StripUi { popup, dpi, w: 150.0 * dpi, slots: vec![], hover: None, tracking: false, shown: true, right: false });
        self.edge = crate::strip::Strip::new(self.ctl, WM_EDGE_IN, WM_EDGE_OUT).ok();
        true
    }

    /// Bangun ulang isi strip dari model dan tangkapan.
    fn rebuild(&mut self) {
        let groups = self.model.strip.clone();
        if groups.is_empty() {
            self.destroy_strip();
            return;
        }
        if !self.ensure_strip() {
            return;
        }
        if self.gfx.is_none() {
            self.gfx = Gfx::new(&self.comp).ok();
        }
        // ikon dulu (butuh &mut self)
        let dpi = self.strip.as_ref().map(|s| s.dpi).unwrap_or(1.0);
        let icon_px = (prefs::get().stage_size as f32 * 0.22 * dpi) as i32;
        let mut icon_bmps: HashMap<String, Bitmap> = HashMap::new();
        for g in &groups {
            for a in g.iter().take(2) {
                if let Some(b) = self.icon(a, icon_px) {
                    icon_bmps.insert(a.clone(), b);
                }
            }
        }
        let comp = self.comp.clone();
        let (sw, sh) = nacre_platform::sys::screen_size();
        let mut ars: HashMap<String, f32> = self.shots.iter().map(|(k, b)| (k.clone(), b.w as f32 / b.h.max(1) as f32)).collect();
        for (k, c) in &self.live {
            if c.has_frame {
                let (w, h) = c.size();
                if h > 0 {
                    ars.insert(k.clone(), w as f32 / h as f32);
                }
            }
        }
        let shots = &self.shots;
        let Some(gfx) = self.gfx.as_ref() else { return };
        let Some(strip) = self.strip.as_mut() else { return };

        let cfg = prefs::get();
        let right = cfg.stage_right;
        let sign = if right { -1.0f32 } else { 1.0 };
        let (tw, th_max, gap) = (cfg.stage_size as f32 * dpi, cfg.stage_size as f32 * 0.72 * dpi, cfg.stage_gap as f32 * dpi);
        strip.w = tw + 56.0 * dpi;
        strip.right = right;
        let slot_h = th_max + 22.0 * dpi;
        let n = groups.len() as f32;
        let mut total = n * slot_h + (n - 1.0) * gap;
        let mut scale = 1.0;
        let avail = sh as f32 - 120.0 * dpi;
        if total > avail {
            scale = avail / total;
            total = avail;
        }
        let (slot_h, gap, tw, th_max) = (slot_h * scale, gap * scale, tw * scale, th_max * scale);
        let y0 = ((sh as f32 - total) / 2.0).max(40.0 * dpi);
        let pad = 26.0 * dpi;
        let px = if right { sw - strip.w as i32 } else { 0 };
        let _ = strip.popup.set_rect(px, y0 as i32, strip.w as i32, total.ceil() as i32);
        let card_x = if right { strip.w - pad - tw } else { pad };
        let show_titles = cfg.stage_titles;
        let _ = strip.popup.panel.set_rect(0.0, 0.0, 0.0, 0.0);
        if let Ok(k) = strip.popup.content.Children() {
            let _ = k.RemoveAll();
        }
        strip.slots.clear();
        let go = |strip: &mut StripUi| -> windows_core::Result<()> {
            for (i, g) in groups.iter().enumerate() {
                let sy = i as f32 * (slot_h + gap);
                let cont = comp.CreateContainerVisual()?;
                cont.SetSize(Vector2 { X: tw, Y: slot_h })?;
                cont.SetOffset(Vector3 { X: card_x, Y: sy, Z: 0.0 })?;
                cont.SetCenterPoint(Vector3 { X: tw / 2.0, Y: slot_h / 2.0, Z: 0.0 })?;
                cont.SetTransformMatrix(perspective(tw / 2.0, slot_h / 2.0, 600.0 * dpi))?;
                let mut cards = Vec::new();
                let mut brushes: Vec<(String, CompositionSurfaceBrush)> = Vec::new();
                // belakang → depan: app ke-3, ke-2, lalu ke-1
                let count = g.len().min(3);
                for j in (0..count).rev() {
                    let app = &g[j];
                    let (bw, bh) = gfx_dims(ars.get(app).copied(), tw, th_max);
                    let off = j as f32;
                    let v = comp.CreateSpriteVisual()?;
                    v.SetSize(Vector2 { X: bw, Y: bh })?;
                    let hug = if right { tw - bw } else { 0.0 };
                    v.SetOffset(Vector3 { X: hug + sign * off * 7.0 * dpi * scale, Y: slot_h - bh - 14.0 * dpi * scale - off * 9.0 * dpi * scale, Z: 0.0 })?;
                    v.SetCenterPoint(Vector3 { X: bw / 2.0, Y: bh / 2.0, Z: 0.0 })?;
                    v.SetRotationAxis(Vector3 { X: 0.0, Y: 1.0, Z: 0.0 })?;
                    v.SetRotationAngleInDegrees(sign * TILT)?;
                    if j > 0 {
                        v.SetOpacity(0.78)?;
                    }
                    let brush: CompositionBrush = match shots.get(app) {
                        Some(b) => {
                            let surf = gfx.surface_from_bgra(b.w, b.h, &b.bgra)?;
                            let br = comp.CreateSurfaceBrushWithSurface(&surf)?;
                            br.SetStretch(CompositionStretch::Fill)?;
                            brushes.push((app.clone(), br.clone()));
                            windows_core::Interface::cast::<CompositionBrush>(&br)?
                        }
                        None => {
                            // belum ada tangkapan: kuas gelap sementara yang tetap bisa diganti gambar hidup begitu frame pertama tiba
                            let surf = gfx.surface_from_bgra(1, 1, &[34, 29, 26, 170])?;
                            let br = comp.CreateSurfaceBrushWithSurface(&surf)?;
                            br.SetStretch(CompositionStretch::Fill)?;
                            brushes.push((app.clone(), br.clone()));
                            windows_core::Interface::cast::<CompositionBrush>(&br)?
                        }
                    };
                    v.SetBrush(&brush)?;
                    let geo = comp.CreateRoundedRectangleGeometry()?;
                    geo.SetSize(Vector2 { X: bw, Y: bh })?;
                    geo.SetCornerRadius(Vector2 { X: 9.0 * dpi * scale, Y: 9.0 * dpi * scale })?;
                    let clip = comp.CreateGeometricClip()?;
                    clip.SetGeometry(&geo)?;
                    v.SetClip(&clip)?;
                    cont.Children()?.InsertAtTop(&v)?;
                    cards.push(v);
                }
                // lencana ikon (kiri-bawah)
                for (k, a) in g.iter().take(2).enumerate() {
                    if let Some(b) = icon_bmps.get(a) {
                        let surf = gfx.surface_from_bgra(b.w, b.h, &b.bgra)?;
                        let br = comp.CreateSurfaceBrushWithSurface(&surf)?;
                        br.SetStretch(CompositionStretch::Uniform)?;
                        let v = comp.CreateSpriteVisual()?;
                        v.SetBrush(&br)?;
                        let isz = tw * 0.22;
                        v.SetSize(Vector2 { X: isz, Y: isz })?;
                        let bx = if right { tw - isz + 0.08 * tw - k as f32 * 14.0 * dpi * scale } else { -0.08 * tw + k as f32 * 14.0 * dpi * scale };
                        v.SetOffset(Vector3 { X: bx, Y: slot_h - isz + 2.0 * dpi, Z: 0.0 })?;
                        cont.Children()?.InsertAtTop(&v)?;
                    }
                }
                // judul jendela (muncul saat hover)
                let mut label = None;
                if show_titles {
                    if let Some((surf, lw, lh)) = title_surface(gfx, &g[0], (11.0 * dpi) as i32, tw as i32) {
                        let br = comp.CreateSurfaceBrushWithSurface(&surf)?;
                        br.SetStretch(CompositionStretch::None)?;
                        let v = comp.CreateSpriteVisual()?;
                        v.SetBrush(&br)?;
                        v.SetSize(Vector2 { X: lw as f32, Y: lh as f32 })?;
                        v.SetOffset(Vector3 { X: (tw - lw as f32) / 2.0, Y: 0.0, Z: 0.0 })?;
                        v.SetOpacity(0.0)?;
                        cont.Children()?.InsertAtTop(&v)?;
                        label = Some(v);
                    }
                }
                strip.popup.content.Children()?.InsertAtTop(&cont)?;
                strip.slots.push(Slot { apps: g.clone(), rect: (card_x, sy, tw, slot_h), cont, cards, brushes, label });
            }
            Ok(())
        };
        if let Err(e) = go(strip) {
            log::line("stage", &format!("strip gagal: {e}"));
            return;
        }
        if let Some(e) = &self.edge {
            let ew = (2.0 * dpi) as i32;
            e.place(if right { sw - ew } else { 0 }, 0, ew, sh);
            e.set_visible(true);
        }
        let was_shown = strip.shown;
        strip.hover = None;
        if was_shown {
            strip.popup.show();
        }
        self.sync_live();
    }

    /// Mulai/hentikan tangkapan jendela hidup untuk app yang tampil di strip, lalu pasang permukaannya ke kartu.
    fn sync_live(&mut self) {
        if !capture::supported() {
            return;
        }
        let wanted: Vec<String> = self.model.strip.iter().flat_map(|g| g.iter().take(3).cloned()).collect();
        self.live.retain(|a, _| wanted.contains(a));
        if wanted.is_empty() {
            return;
        }
        if self.live_gfx.is_none() {
            self.live_gfx = Gfx::new_hardware(&self.comp).ok().or_else(|| Gfx::new(&self.comp).ok());
        }
        let Some(lg) = self.live_gfx.as_ref() else { return };
        for app in wanted {
            if self.live.contains_key(&app) {
                continue;
            }
            let Some(hwnd) = self.parked.iter().find(|(_, a)| *a == app).map(|(p, _)| p.hwnd) else { continue };
            match Capture::start(lg, hwnd) {
                Ok(c) => {
                    self.live.insert(app, c);
                }
                Err(e) => log::line("stage", &format!("tangkapan hidup gagal untuk {app}: {e}")),
            }
        }
        self.apply_live_brushes();
    }

    /// Kuas kartu memakai permukaan hidup untuk app yang sudah menerima frame.
    fn apply_live_brushes(&self) {
        let Some(strip) = self.strip.as_ref() else { return };
        for sl in &strip.slots {
            for (a, br) in &sl.brushes {
                if let Some(c) = self.live.get(a) {
                    if c.has_frame {
                        let _ = br.SetSurface(&c.surface);
                    }
                }
            }
        }
    }

    /// Ambil frame terbaru semua tangkapan (dipanggil timer ±20 fps selama strip terlihat).
    fn poll_live(&mut self) {
        if self.live.is_empty() || !self.strip.as_ref().is_some_and(|s| s.shown) {
            return;
        }
        let Some(lg) = self.live_gfx.as_ref() else { return };
        let mut first = false;
        for c in self.live.values_mut() {
            let had = c.has_frame;
            if c.poll(lg) && !had {
                first = true;
            }
        }
        if first {
            // frame pertama: bentuk kartu mengikuti ukuran jendela sebenarnya, jadi bangun ulang sekali
            self.rebuild();
        }
    }

    // -----------------------------------------------------------------------------------------------------------
    // strip menyingkir bila tertimpa jendela panggung
    // -----------------------------------------------------------------------------------------------------------

    fn check_overlap(&mut self) {
        let fg = focus::foreground();
        if fg == 0 || is_own(fg) || is_desktop(fg) {
            return;
        }
        let Some(strip) = self.strip.as_ref() else { return };
        let mut r = RECT::default();
        unsafe {
            if GetWindowRect(HWND(fg as *mut _), &mut r).is_err() {
                return;
            }
        }
        let overlap = if strip.right {
            (r.right as f32) > nacre_platform::sys::screen_size().0 as f32 - strip.w + 6.0 * strip.dpi
        } else {
            (r.left as f32) < strip.w - 6.0 * strip.dpi
        };
        let want_shown = !overlap || self.peek;
        self.set_strip_shown(want_shown);
    }

    fn set_strip_shown(&mut self, show: bool) {
        let comp = self.comp.clone();
        let Some(s) = self.strip.as_mut() else { return };
        if s.shown == show {
            return;
        }
        s.shown = show;
        let out = if s.right { s.w } else { -s.w };
        let from = if show { out } else { 0.0 };
        let to = if show { 0.0 } else { out };
        let _ = motion::vec3(&comp, &s.popup.content, "Offset", Vector3 { X: from, Y: 0.0, Z: 0.0 }, Vector3 { X: to, Y: 0.0, Z: 0.0 }, motion::stage_ms(motion::STRIP_SLIDE_MS));
        unsafe {
            if show {
                let _ = KillTimer(Some(self.ctl), T_HIDE);
                s.popup.show();
            } else {
                SetTimer(Some(self.ctl), T_HIDE, motion::stage_ms(motion::STRIP_SLIDE_MS) + 20, None);
            }
        }
    }

    /// Perbarui tangkapan kartu strip (jendela terparkir tetap bisa ditangkap).
    fn refresh_shots(&mut self) {
        if self.strip.is_none() || self.fly.is_some() {
            return;
        }
        let mut need_rebuild = false;
        let apps: Vec<String> = self.model.strip.iter().flat_map(|g| g.iter().take(3).cloned()).collect();
        for app in apps {
            let Some(hwnd) = self.parked.iter().find(|(_, a)| *a == app).map(|(p, _)| p.hwnd) else { continue };
            let Some(shot) = snapshot::capture(hwnd, 2) else { continue };
            if blank(&shot.bmp) || self.live.get(&app).is_some_and(|c| c.has_frame) {
                continue; // simpan tangkapan lama yang bagus; app berstatus hidup tidak perlu snapshot
            }
            let old = self.shots.get(&app).map(|b| (b.w, b.h));
            let same_shape = old.map(|(w, h)| ((w as f32 / h.max(1) as f32) - (shot.bmp.w as f32 / shot.bmp.h.max(1) as f32)).abs() < 0.02).unwrap_or(false);
            let Some(gfx) = self.gfx.as_ref() else { continue };
            // ganti gambar di tempat (tanpa membangun ulang strip): tidak berkedip dan hover tidak hilang
            let mut done = false;
            if same_shape {
                if let (Some(strip), Ok(surf)) = (self.strip.as_ref(), gfx.surface_from_bgra(shot.bmp.w, shot.bmp.h, &shot.bmp.bgra)) {
                    for sl in &strip.slots {
                        for (a, br) in &sl.brushes {
                            if *a == app {
                                let _ = br.SetSurface(&surf);
                                done = true;
                            }
                        }
                    }
                }
            }
            if !done {
                need_rebuild = true;
            }
            self.shots.insert(app, shot.bmp);
        }
        if need_rebuild {
            self.rebuild();
        }
    }

    // -----------------------------------------------------------------------------------------------------------
    // interaksi strip
    // -----------------------------------------------------------------------------------------------------------

    fn set_hover(&mut self, i: Option<usize>) {
        let comp = self.comp.clone();
        let Some(s) = self.strip.as_mut() else { return };
        if s.hover == i {
            return;
        }
        let sign = if s.right { -1.0f32 } else { 1.0 };
        let ms = motion::stage_ms(motion::CARD_HOVER_MS);
        let tilt = |sl: &Slot, on: bool| {
            let (a, b, sc) = if on { (sign * TILT, sign * TILT_HOVER, 1.06) } else { (sign * TILT_HOVER, sign * TILT, 1.0) };
            for c in &sl.cards {
                let _ = motion::scalar(&comp, c, "RotationAngleInDegrees", a, b, ms);
                let _ = c.SetRotationAngleInDegrees(b);
            }
            let from = if on { 1.0 } else { 1.06 };
            let _ = motion::vec3(&comp, &sl.cont, "Scale", Vector3 { X: from, Y: from, Z: 1.0 }, Vector3 { X: sc, Y: sc, Z: 1.0 }, ms);
            let _ = sl.cont.SetScale(Vector3 { X: sc, Y: sc, Z: 1.0 });
            if let Some(l) = &sl.label {
                let (o0, o1) = if on { (0.0, 1.0) } else { (1.0, 0.0) };
                let _ = motion::scalar(&comp, l, "Opacity", o0, o1, ms);
                let _ = l.SetOpacity(o1);
            }
        };
        if let Some(old) = s.hover.and_then(|o| s.slots.get(o)) {
            tilt(old, false);
        }
        if let Some(new) = i.and_then(|n| s.slots.get(n)) {
            tilt(new, true);
        }
        s.hover = i;
    }
}

/// Judul app (nama berkas tanpa ekstensi, huruf awal besar) sebagai permukaan teks putih.
fn title_surface(gfx: &Gfx, app: &str, px: i32, maxw: i32) -> Option<(windows::UI::Composition::CompositionDrawingSurface, i32, i32)> {
    use nacre_platform::text::{measure, render, Style};
    let stem = std::path::Path::new(app).file_stem()?.to_string_lossy().to_string();
    let mut chars = stem.chars();
    let name: String = chars.next()?.to_uppercase().chain(chars).collect();
    let style = Style { px, weight: 600 };
    let (tw, th) = measure(&name, &style);
    let tw = (tw + 2).min(maxw.max(8));
    let bmp = render(&name, &style, tw, th)?;
    let mut data = bmp.bgra.clone();
    for p in data.chunks_exact_mut(4) {
        let a = p[3] as u32;
        p[0] = (255 * a / 255) as u8;
        p[1] = (255 * a / 255) as u8;
        p[2] = (255 * a / 255) as u8;
    }
    let surf = gfx.surface_from_bgra(bmp.w, bmp.h, &data).ok()?;
    Some((surf, tw, th))
}

/// Ukuran kartu: muat di (tw × th_max) mempertahankan rasio tangkapan (16:10 bila belum ada).
fn gfx_dims(ar: Option<f32>, tw: f32, th_max: f32) -> (f32, f32) {
    let ar = ar.unwrap_or(1.6);
    let (mut w, mut h) = (tw, tw / ar);
    if h > th_max {
        h = th_max;
        w = h * ar;
    }
    (w, h)
}

unsafe extern "system" fn ctl_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        prefs::WM_PREFS => {
            sync();
            LRESULT(0)
        }
        WM_FG => {
            SetTimer(Some(hwnd), T_FG, 140, None);
            LRESULT(0)
        }
        WM_LOC => {
            SetTimer(Some(hwnd), T_OVERLAP, 160, None);
            LRESULT(0)
        }
        WM_EDGE_IN => {
            with(|s| {
                s.peek = true;
                s.set_strip_shown(true);
            });
            LRESULT(0)
        }
        WM_EDGE_OUT => {
            SetTimer(Some(hwnd), T_PEEK, 700, None);
            LRESULT(0)
        }
        WM_TIMER => {
            match wp.0 {
                T_FG => {
                    let _ = KillTimer(Some(hwnd), T_FG);
                    with(|s| {
                        if s.active {
                            s.refresh();
                        }
                    });
                }
                T_FLY => {
                    with(|s| s.finish_fly());
                }
                T_FLYEND => {
                    with(|s| s.kill_dying());
                }
                T_LIVE => {
                    with(|s| {
                        if s.active {
                            s.poll_live();
                        }
                    });
                }
                T_SHOT => {
                    with(|s| {
                        if s.active {
                            s.refresh_shots();
                        }
                    });
                }
                T_OVERLAP => {
                    let _ = KillTimer(Some(hwnd), T_OVERLAP);
                    with(|s| s.check_overlap());
                }
                T_HIDE => {
                    let _ = KillTimer(Some(hwnd), T_HIDE);
                    with(|s| {
                        if let Some(st) = &s.strip {
                            if !st.shown {
                                st.popup.hide();
                            }
                        }
                    });
                }
                T_PEEK => {
                    let _ = KillTimer(Some(hwnd), T_PEEK);
                    with(|s| {
                        // masih di atas strip? tunda lagi; selainnya kembali ke aturan tertimpa
                        let over = s.strip.as_ref().map(|st| st.hover.is_some()).unwrap_or(false);
                        if !over {
                            s.peek = false;
                            s.check_overlap();
                        }
                    });
                }
                _ => {}
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe extern "system" fn fly_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe extern "system" fn strip_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let (x, y) = ((lp.0 as i16) as f32, ((lp.0 >> 16) as i16) as f32);
    match msg {
        WM_MOUSEMOVE => {
            with(|s| {
                let Some(st) = s.strip.as_mut() else { return };
                if !st.tracking {
                    let mut t = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
                    if TrackMouseEvent(&mut t).is_ok() {
                        st.tracking = true;
                    }
                }
                let i = st.slots.iter().position(|sl| x >= sl.rect.0 - 8.0 && x < sl.rect.0 + sl.rect.2 + 8.0 && y >= sl.rect.1 && y < sl.rect.1 + sl.rect.3);
                s.set_hover(i);
            });
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            with(|s| {
                if let Some(st) = s.strip.as_mut() {
                    st.tracking = false;
                }
                s.set_hover(None);
                if s.peek {
                    SetTimer(Some(s.ctl), T_PEEK, 500, None);
                }
            });
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            with(|s| {
                let Some(st) = s.strip.as_ref() else { return };
                let Some(sl) = st.slots.iter().find(|sl| x >= sl.rect.0 - 8.0 && x < sl.rect.0 + sl.rect.2 + 8.0 && y >= sl.rect.1 && y < sl.rect.1 + sl.rect.3) else { return };
                let shift = windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState(windows::Win32::UI::Input::KeyboardAndMouse::VK_SHIFT.0 as i32) < 0;
                let apps = sl.apps.clone();
                if shift {
                    // Shift-klik: gabungkan ke grup panggung
                    let ch = s.model.add_to_stage(&apps[0]);
                    let wins = enumerate::list(std::process::id());
                    s.apply(ch, &wins, 0);
                } else {
                    s.focus_app(&apps[0], 0);
                }
            });
            LRESULT(0)
        }
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
