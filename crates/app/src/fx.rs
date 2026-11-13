//! Animasi jendela ala macOS untuk jendela app lain: minimize (menyusut ke ikon dock), tutup (menyusut + pudar),
//! dan buka dari dock (membesar dari ikon). Cara kerja:
//! - Jendela yang mendapat fokus ditandai "transisi DWM dimatikan" (agar animasi bawaan Windows tidak bentrok) dan
//!   isinya ditangkap berkala (hanya jendela di depan, setengah resolusi) sebagai permukaan Composition.
//! - Saat jendela itu diminimalkan/dihancurkan, tangkapan terakhir dianimasikan di jendela overlay transparan
//!   (menembus klik) yang hanya tampil selama animasi.
//! - Restore dari dock: overlay membesar dari ikon, jendela asli dipulihkan tepat sebelum selesai.
//! Semua transisi DWM yang kita matikan dinyalakan lagi saat keluar atau saat animasi dimatikan.

use crate::prefs;
use nacre_platform::wins::{focus, snapshot, tracker};
use nacre_platform::{log, sys};
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
/// Jendela yang transisi DWM-nya kita matikan; dipulihkan saat keluar.
static FLAGGED: Mutex<Vec<isize>> = Mutex::new(Vec::new());

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
    screen: (i32, i32),
    snap: Option<Snap>,
    /// Tangkapan jendela yang sedang diminimalkan (untuk animasi restore dari dock); maksimal 4.
    parked: Vec<Snap>,
    current: isize,
    sprite: Option<SpriteVisual>,
    /// Akar irisan genie (minimize/restore).
    genie: Option<ContainerVisual>,
    pending_restore: Option<(isize, isize)>,
    seen: HashSet<isize>,
    shadow: Option<ShadowWin>,
    shadow_target: isize,
    shadow_key: (i32, i32, i32),
}

/// Jendela bayangan: layar transparan menembus klik yang diletakkan tepat di bawah jendela aktif.
struct ShadowWin {
    hwnd: HWND,
    surface: Surface,
    sprite: SpriteVisual,
}

const T_SHADOW: usize = 5;
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

/// Efek minimize Genie (bawaan) atau Skala.
fn genie_on() -> bool {
    prefs::get().min_effect == 0
}

/// Jeda sebelum animasi mulai: overlay baru ditampilkan perlu satu-dua frame sebelum terlihat. Tanpa jeda ini awal
/// animasi terlewat sehingga gerakannya terasa patah. Selama jeda, tangkapan diam di posisi awalnya.
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

/// Siapkan overlay dan pasang pendengar peristiwa jendela.
pub fn init(host: &Host) -> windows_core::Result<()> {
    unsafe {
        let hi: HINSTANCE = GetModuleHandleW(None)?.into();
        let class = w!("AbraFx");
        RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(proc), hInstance: hi, lpszClassName: class, ..Default::default() });
        let screen = sys::screen_size();
        let hwnd = CreateWindowExW(
            WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_LAYERED,
            class,
            w!("abra-fx"),
            WS_POPUP,
            0, 0, screen.0, screen.1,
            None, None, Some(hi), None,
        )?;
            let _ = SetLayeredWindowAttributes(hwnd, windows::Win32::Foundation::COLORREF(0), 255, LWA_ALPHA);
        let surface = attach(&host.compositor, hwnd, (screen.0 as f32, screen.1 as f32))?;
        let gfx = Gfx::new(&host.compositor)?;
        FX_HWND.store(hwnd.0 as isize, Ordering::Relaxed);
        FX.with(|c| {
            *c.borrow_mut() = Some(Fx {
                hwnd,
                comp: host.compositor.clone(),
                gfx,
                surface,
                screen,
                snap: None,
                parked: Vec::new(),
                current: 0,
                sprite: None,
                genie: None,
                pending_restore: None,
                seen: HashSet::new(),
                shadow: None,
                shadow_target: 0,
                shadow_key: (0, 0, 0),
            })
        });
        prefs::listen(hwnd);
        tracker::install(noop); // pastikan hook WinEvent terpasang walau tanpa dock
        tracker::install_fx(hook);
        if enabled() {
            SetTimer(Some(hwnd), T_REFRESH, 4000, None);
            // Jendela yang sudah di depan saat ABRA dimulai.
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

/// Nyalakan kembali transisi DWM semua jendela yang kita matikan (jalur keluar dan saat animasi dimatikan).
pub fn restore_all() {
    let list: Vec<isize> = std::mem::take(&mut *FLAGGED.lock().unwrap_or_else(|e| e.into_inner()));
    for h in list {
        snapshot::set_transitions_disabled(h, false);
    }
}

/// Bitmap bayangan lembut (setengah-resolusi, alfa ter-premultiply hitam) untuk jendela w×h di tengah kanvas
/// berbingkai `m` px; bayangan bergeser turun `dy` px. Gelap di dekat tepi, memudar halus ke luar.
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
            let d = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r; // > 0 di luar
            let t = (d / blur).clamp(0.0, 1.0);
            let a = peak * (1.0 - t) * (1.0 - t) * (1.0 - t);
            let i = ((y * bw + x) * 4) as usize;
            out[i + 3] = (a * 255.0).round() as u8; // hitam ter-premultiply: RGB = 0
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

/// Jendela app biasa (berjudul, bukan tool window/popup) milik proses lain.
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

/// Dipanggil dock: apakah restore beranimasi dimulai (jendela asli dipulihkan di tengah/akhir animasi).
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
}

impl Kind {
    /// Durasi animasi. Genie sedikit lebih panjang agar alirannya terbaca (seperti macOS), Skala lebih singkat.
    fn ms(self) -> u32 {
        let g = genie_on();
        match self {
            Kind::Minimize => if g { 540 } else { 380 },
            Kind::Restore => if g { 470 } else { 340 },
            Kind::Close => 190,
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

    /// Mainkan animasi `kind` dari/ke persegi `from`→`to` (px layar) memakai tangkapan `surface`.
    /// Untuk Minimize/Close: `from` = jendela, `to` = ikon. Untuk Restore: `from` = ikon, `to` = jendela.
    fn play(&mut self, surface: CompositionDrawingSurface, a: (i32, i32, i32, i32), b: (i32, i32, i32, i32), kind: Kind) {
        if kind != Kind::Close && genie_on() {
            let (win, icon) = if kind == Kind::Restore { (b, a) } else { (a, b) };
            match self.play_genie(surface, win, icon, kind == Kind::Restore) {
                Ok(()) => {
                    log::line("fx", &format!("animasi {} genie ({} ms) jendela {win:?} ikon {icon:?}", if kind == Kind::Restore { "restore" } else { "minimize" }, kind.ms()));
                    unsafe {
                        let _ = SetWindowPos(self.hwnd, Some(HWND_TOPMOST), 0, 0, self.screen.0, self.screen.1, SWP_NOACTIVATE);
                        let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                        SetTimer(Some(self.hwnd), T_HIDE, START_DELAY + kind.ms() + 80, None);
                    }
                }
                Err(e) => log::line("fx", &format!("genie gagal: {e}")),
            }
            return;
        }
        let mut go = || -> windows_core::Result<()> {
            // Pembagian tugas: `win` adalah persegi jendela, `icon` persegi ikon.
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
            // Offset agar pusat sprite berada di pusat ikon.
            let at_icon = Vector3 { X: ix + iw / 2.0 - ww / 2.0, Y: iy + ih / 2.0 - wh / 2.0, Z: 0.0 };
            let (sx, sy) = (iw / ww, ih / wh);
            let one = Vector3 { X: 1.0, Y: 1.0, Z: 1.0 };
            let ms = kind.ms();
            match kind {
                Kind::Minimize => {
                    sprite.SetOffset(at_win)?;
                    sprite.SetScale(one)?;
                    // Corong ala genie: tinggi turun duluan, lebar menyempit belakangan; isi bergerak ke ikon.
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
                    self.vec3(&obj, "Scale", one, &[(1.0, Vector3 { X: 0.9, Y: 0.9, Z: 1.0 }, (0.4, 0.0), (0.6, 1.0))], ms)?;
                    self.scalar(&obj, "Opacity", &[(0.0, 1.0), (1.0, 0.0)], ms)?;
                }
            }
            self.sprite = Some(sprite);
            log::line("fx", &format!("animasi {} ({ms} ms) dari {a:?} ke {b:?}", match kind { Kind::Minimize => "minimize", Kind::Restore => "restore", Kind::Close => "tutup" }));
            unsafe {
                let _ = SetWindowPos(self.hwnd, Some(HWND_TOPMOST), 0, 0, self.screen.0, self.screen.1, SWP_NOACTIVATE);
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                SetTimer(Some(self.hwnd), T_HIDE, START_DELAY + ms + 80, None);
            }
            Ok(())
        };
        if let Err(e) = go() {
            log::line("fx", &format!("animasi gagal: {e}"));
        }
    }

    /// Genie: jendela dipecah jadi irisan horizontal; tiap irisan menempuh jalur corong ke (atau dari) ikon. Geometri
    /// dari `nacre_core::genie` (diuji); di sini hanya diubah menjadi animasi compositor dengan banyak titik kunci
    /// supaya gerakannya mulus di 60 fps, dan dimulai setelah jeda singkat agar frame awal tidak terlewat.
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
        // ±1 titik kunci per 12 ms: interpolasi linear antar titik tidak lagi terasa patah
        let samples = ((ms / 12) as usize).clamp(24, 48);
        let root = self.comp.CreateContainerVisual()?;
        self.surface.root.Children()?.InsertAtTop(&root)?;
        let (dur, delay) = (span(ms), span(START_DELAY));
        let v3 = |p: (f32, f32), z: f32| Vector3 { X: p.0, Y: p.1, Z: z };
        for i in 0..n {
            let s = self.comp.CreateSpriteVisual()?;
            let b = self.comp.CreateSurfaceBrushWithSurface(&surface)?;
            b.SetStretch(CompositionStretch::Fill)?;
            // Tiap irisan menampilkan baris ke-i dari tangkapan penuh.
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
        // Pudar di ujung (minimize) atau muncul cepat (restore).
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

    fn capture_current(&mut self) {
        let h = self.current;
        if h == 0 || !enabled() || !eligible(h) || focus::is_minimized(h) {
            return;
        }
        // Setengah resolusi untuk jendela besar: cukup untuk ~300 ms animasi dan hemat memori.
        let Some(shot) = snapshot::capture(h, 2) else { return };
        if let Ok(surface) = self.gfx.surface_from_bgra(shot.bmp.w, shot.bmp.h, &shot.bmp.bgra) {
            log::line("fx", &format!("tangkap hwnd={h} {}x{} (px {}x{})", shot.rect.2, shot.rect.3, shot.bmp.w, shot.bmp.h));
            self.snap = Some(Snap { hwnd: h, rect: shot.rect, surface });
        }
    }

    fn icon_rect(&self, hwnd: isize) -> (i32, i32, i32, i32) {
        crate::dock::icon_rect_for(hwnd).unwrap_or((self.screen.0 / 2 - 24, self.screen.1 - 70, 48, 48))
    }

    fn on_event(&mut self, event: u32, h: isize) {
        self.on_look_event(event, h);
        if !enabled() {
            return;
        }
        match event {
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
                // Hanya bila belum ada animasi restore yang berjalan untuk jendela ini.
                if let Some(s) = &self.snap {
                    if s.hwnd == h && self.pending_restore.is_none() {
                        let (surface, rect) = (s.surface.clone(), s.rect);
                        let icon = self.icon_rect(h);
                        let keep = s.clone();
                        self.play(surface, rect, icon, Kind::Minimize);
                        self.parked.retain(|p| p.hwnd != h);
                        self.parked.push(keep);
                        if self.parked.len() > 4 {
                            self.parked.remove(0);
                        }
                    }
                }
            }
            tracker::EV_DESTROY => {
                self.parked.retain(|p| p.hwnd != h);
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

    // ---------- sudut dan bayangan jendela ----------

    fn hide_shadow(&mut self) {
        self.shadow_target = 0;
        if let Some(s) = &self.shadow {
            unsafe {
                let _ = ShowWindow(s.hwnd, SW_HIDE);
            }
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

    /// Letakkan bayangan di bawah `h`. `full` = buat ulang bitmap bila ukuran berubah; selain itu hanya memindahkan.
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
                // Tepat di bawah jendela target dalam urutan-Z.
                let below = GetWindow(HWND(h as *mut _), GW_HWNDNEXT).unwrap_or(HWND_BOTTOM);
                let _ = SetWindowPos(sh.hwnd, Some(below), x - m, y - m, fw, fh, SWP_NOACTIVATE | SWP_SHOWWINDOW);
            }
            Ok(())
        };
        if go(self).is_ok() {
            self.shadow_target = h;
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
                T_HIDE | T_CAPTURE | T_RESTORE | T_SHADOW => {
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
        WM_NCHITTEST => LRESULT(-1), // HTTRANSPARENT: klik menembus
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
