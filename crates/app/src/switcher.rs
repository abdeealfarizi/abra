//! Pengalih app ala macOS Cmd+Tab, menggantikan Alt+Tab bawaan Windows selama Stage Manager aktif.
//! Hook keyboard low-level hanya terpasang saat Stage Manager menyala (nol biaya selain itu). Alt+Tab ditelan;
//! Tab/Shift+Tab/panah memilih, Esc membatalkan, lepas Alt membawa app terpilih ke panggung.

use crate::popup::Popup;
use crate::{motion, prefs};
use nacre_platform::icons::{self, Bitmap};
use nacre_platform::text::{measure, render, Style};
use nacre_platform::wins::{enumerate, tracker};
use nacre_ui::gfx::Gfx;
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use windows::core::w;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_ESCAPE, VK_LEFT, VK_LMENU, VK_RIGHT, VK_RMENU, VK_SHIFT, VK_TAB};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::UI::Color;
use windows::UI::Composition::{CompositionStretch, Compositor, SpriteVisual};
use windows_numerics::{Vector2, Vector3};

const WM_NEXT: u32 = WM_APP + 120;
const WM_PREV: u32 = WM_APP + 121;
const WM_COMMIT: u32 = WM_APP + 122;
const WM_CANCEL: u32 = WM_APP + 123;
const WM_INSTALL: u32 = WM_APP + 124;

static HOOK: AtomicIsize = AtomicIsize::new(0);
static OPEN: AtomicBool = AtomicBool::new(false);
static TARGET: AtomicIsize = AtomicIsize::new(0);

struct Entry {
    path: String,
    title: String,
    hwnd: isize,
}

struct Ui {
    popup: Popup,
    entries: Vec<Entry>,
    sel: usize,
    cards: Vec<(f32, f32, f32, f32)>,
    ring: SpriteVisual,
    ring_in: SpriteVisual,
    dpi: f32,
}

struct Ctl {
    hwnd: HWND,
    comp: Compositor,
    gfx: Option<Gfx>,
    ui: Option<Ui>,
}

thread_local! {
    static CTL: RefCell<Option<Ctl>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut Ctl) -> R) -> Option<R> {
    CTL.with(|c| c.try_borrow_mut().ok().and_then(|mut b| b.as_mut().map(f)))
}

pub fn init(comp: &Compositor) -> windows_core::Result<()> {
    unsafe {
        let hi: HINSTANCE = GetModuleHandleW(None)?.into();
        let class = w!("AbraSwitcherCtl");
        RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(ctl_proc), hInstance: hi, lpszClassName: class, ..Default::default() });
        let hwnd = CreateWindowExW(WINDOW_EX_STYLE(0), class, w!("abra-switcher"), WS_POPUP, 0, 0, 0, 0, Some(HWND_MESSAGE), None, Some(hi), None)?;
        TARGET.store(hwnd.0 as isize, Ordering::Relaxed);
        CTL.with(|c| *c.borrow_mut() = Some(Ctl { hwnd, comp: comp.clone(), gfx: None, ui: None }));
        prefs::listen(hwnd);
    }
    sync();
    Ok(())
}

pub fn shutdown() {
    uninstall();
}

/// Pasang/lepas hook sesuai status Stage Manager (dan pengaturan).
fn sync() {
    if prefs::get().stage && prefs::get().switcher {
        install();
    } else {
        uninstall();
        with(|c| c.close());
    }
}

fn install() {
    if HOOK.load(Ordering::Relaxed) != 0 {
        return;
    }
    unsafe {
        let hi = GetModuleHandleW(None).ok().map(|m| HINSTANCE(m.0));
        if let Ok(h) = SetWindowsHookExW(WH_KEYBOARD_LL, Some(ll_proc), hi, 0) {
            HOOK.store(h.0 as isize, Ordering::Relaxed);
        }
    }
}

fn uninstall() {
    let h = HOOK.swap(0, Ordering::Relaxed);
    if h != 0 {
        unsafe {
            let _ = UnhookWindowsHookEx(HHOOK(h as *mut _));
        }
    }
    OPEN.store(false, Ordering::Relaxed);
}

unsafe extern "system" fn ll_proc(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code >= 0 {
        let k = &*(lp.0 as *const KBDLLHOOKSTRUCT);
        let down = wp.0 as u32 == WM_KEYDOWN || wp.0 as u32 == WM_SYSKEYDOWN;
        let up = wp.0 as u32 == WM_KEYUP || wp.0 as u32 == WM_SYSKEYUP;
        let alt = k.flags.0 & LLKHF_ALTDOWN.0 != 0;
        let open = OPEN.load(Ordering::Relaxed);
        let target = HWND(TARGET.load(Ordering::Relaxed) as *mut _);
        if !target.0.is_null() {
            let vk = k.vkCode as u16;
            if vk == VK_TAB.0 && down && (alt || open) {
                let shift = GetKeyState(VK_SHIFT.0 as i32) < 0;
                let _ = PostMessageW(Some(target), if shift { WM_PREV } else { WM_NEXT }, WPARAM(0), LPARAM(0));
                return LRESULT(1);
            }
            if open && down && vk == VK_ESCAPE.0 {
                let _ = PostMessageW(Some(target), WM_CANCEL, WPARAM(0), LPARAM(0));
                return LRESULT(1);
            }
            if open && down && (vk == VK_RIGHT.0 || vk == VK_LEFT.0) {
                let _ = PostMessageW(Some(target), if vk == VK_RIGHT.0 { WM_NEXT } else { WM_PREV }, WPARAM(0), LPARAM(0));
                return LRESULT(1);
            }
            if open && up && (vk == VK_LMENU.0 || vk == VK_RMENU.0) {
                let _ = PostMessageW(Some(target), WM_COMMIT, WPARAM(0), LPARAM(0));
            }
        }
    }
    CallNextHookEx(None, code, wp, lp)
}

fn name_of(w: &nacre_core::dock::items::Win) -> String {
    if !w.name.is_empty() {
        w.name.clone()
    } else {
        nacre_core::dock::items::name_from_path(&w.path)
    }
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

impl Ctl {
    fn close(&mut self) {
        OPEN.store(false, Ordering::Relaxed);
        if let Some(ui) = self.ui.take() {
            let h = ui.popup.hwnd;
            drop(ui);
            unsafe {
                let _ = DestroyWindow(h);
            }
        }
    }

    fn open(&mut self, backwards: bool) {
        // satu kartu per app (jendela terakhir dipakai), terurut dari yang terbaru
        let wins = enumerate::list(std::process::id());
        let mut apps: Vec<(i64, Entry)> = Vec::new();
        for w in &wins {
            let key = w.path.to_ascii_lowercase();
            if key.is_empty() {
                continue;
            }
            let rec = tracker::recent(w.hwnd);
            let title = if w.title.is_empty() { name_of(w) } else { w.title.clone() };
            if let Some(e) = apps.iter_mut().find(|(_, e)| e.path.to_ascii_lowercase() == key) {
                if rec > e.0 {
                    e.0 = rec;
                    e.1.hwnd = w.hwnd;
                    e.1.title = title;
                }
            } else {
                apps.push((rec, Entry { path: w.path.clone(), title, hwnd: w.hwnd }));
            }
        }
        apps.sort_by(|a, b| b.0.cmp(&a.0));
        let entries: Vec<Entry> = apps.into_iter().map(|(_, e)| e).take(9).collect();
        if entries.len() < 2 {
            return;
        }
        if self.gfx.is_none() {
            self.gfx = Gfx::new(&self.comp).ok();
        }
        let Some(gfx) = self.gfx.as_ref() else { return };
        let comp = self.comp.clone();
        let go = || -> windows_core::Result<Ui> {
            let mut look = prefs::look(prefs::Role::Dark);
            look.blur_sigma = look.blur_sigma.max(18.0);
            let base = nacre_ui::glass::GlassParams::dark((10.0, 10.0), 10.0);
            let mut popup = Popup::new(&comp, w!("AbraSwitcher"), Some(sw_proc), WINDOW_EX_STYLE(0), prefs::apply(base, &look))?;
            let dpi = unsafe { GetDpiForWindow(popup.hwnd) }.max(96) as f32 / 96.0;
            let (sw, sh) = nacre_platform::sys::screen_size();

            // tangkapan jendela + rasio
            // jendela yang diparkir Stage Manager tidak bisa ditangkap (hasilnya putih): pakai tangkapan tersimpan dari saat diparkir
            let shots: Vec<Option<Bitmap>> = entries
                .iter()
                .map(|e| {
                    let live = nacre_platform::wins::snapshot::capture(e.hwnd, 2).map(|s| s.bmp).filter(|b| !crate::stage::blank(b));
                    live.or_else(|| crate::stage::shot_for(&e.path))
                })
                .collect();
            // ukuran dasar (piksel layar @100%): kartu 296 tinggi (judul 38 + gambar 258), jarak 35, bantalan 58
            let (hdr, thumb_h, gap, pad) = (38.0f32, 258.0f32, 35.0f32, 58.0f32);
            let widths: Vec<f32> = shots
                .iter()
                .map(|b| b.as_ref().map(|b| (thumb_h * b.w as f32 / b.h.max(1) as f32).clamp(260.0, 470.0)).unwrap_or(380.0))
                .collect();
            let total = widths.iter().sum::<f32>() + gap * (widths.len() as f32 - 1.0) + 2.0 * pad;
            let k = ((sw as f32 - 80.0 * dpi) / (total * dpi)).min(1.0);
            let u = dpi * k;
            let (pw, ph) = (total * u, (thumb_h + hdr + 2.0 * pad) * u);
            popup.set_rect(((sw as f32 - pw) / 2.0) as i32, ((sh as f32 - ph) / 2.0) as i32, pw as i32, ph as i32)?;
            popup.panel.set_radius(10.0 * u)?;
            let content = popup.content.clone();
            let kids = content.Children()?;
            // kerudung abu-abu seperti akrilik
            let rrect = |w: f32, h: f32, r: f32, c: Color| -> windows_core::Result<SpriteVisual> {
                let v = comp.CreateSpriteVisual()?;
                v.SetSize(Vector2 { X: w, Y: h })?;
                v.SetBrush(&comp.CreateColorBrushWithColor(c)?)?;
                let g = comp.CreateRoundedRectangleGeometry()?;
                g.SetSize(Vector2 { X: w, Y: h })?;
                g.SetCornerRadius(Vector2 { X: r, Y: r })?;
                let cl = comp.CreateGeometricClip()?;
                cl.SetGeometry(&g)?;
                v.SetClip(&cl)?;
                Ok(v)
            };
            kids.InsertAtTop(&rrect(pw, ph, 10.0 * u, Color { A: 120, R: 128, G: 128, B: 128 })?)?;
            // cincin pilihan (biru, dengan celah abu-abu di dalamnya)
            let ch = (thumb_h + hdr) * u;
            let ring = rrect(widths[0] * u + 14.0 * u, ch + 14.0 * u, 14.0 * u, Color { A: 255, R: 0, G: 103, B: 192 })?;
            let ring_in = rrect(widths[0] * u + 6.0 * u, ch + 6.0 * u, 11.0 * u, Color { A: 255, R: 116, G: 116, B: 116 })?;
            kids.InsertAtTop(&ring)?;
            kids.InsertAtTop(&ring_in)?;
            let mut cards = Vec::new();
            let mut x = pad * u;
            for (i, e) in entries.iter().enumerate() {
                let cw = widths[i] * u;
                let cont = comp.CreateContainerVisual()?;
                cont.SetSize(Vector2 { X: cw, Y: ch })?;
                cont.SetOffset(Vector3 { X: x, Y: pad * u, Z: 0.0 })?;
                let g = comp.CreateRoundedRectangleGeometry()?;
                g.SetSize(Vector2 { X: cw, Y: ch })?;
                g.SetCornerRadius(Vector2 { X: 8.0 * u, Y: 8.0 * u })?;
                let cl = comp.CreateGeometricClip()?;
                cl.SetGeometry(&g)?;
                cont.SetClip(&cl)?;
                let bg = comp.CreateSpriteVisual()?;
                bg.SetSize(Vector2 { X: cw, Y: ch })?;
                bg.SetBrush(&comp.CreateColorBrushWithColor(Color { A: 255, R: 250, G: 250, B: 250 })?)?;
                cont.Children()?.InsertAtTop(&bg)?;
                if let Some(b) = &shots[i] {
                    let surf = gfx.surface_from_bgra(b.w, b.h, &b.bgra)?;
                    let br = comp.CreateSurfaceBrushWithSurface(&surf)?;
                    br.SetStretch(CompositionStretch::Fill)?;
                    let t = comp.CreateSpriteVisual()?;
                    t.SetBrush(&br)?;
                    t.SetSize(Vector2 { X: cw, Y: thumb_h * u })?;
                    t.SetOffset(Vector3 { X: 0.0, Y: hdr * u, Z: 0.0 })?;
                    cont.Children()?.InsertAtTop(&t)?;
                }
                if let Some(b) = icons::load(&e.path, (20.0 * u) as i32) {
                    let surf = gfx.surface_from_bgra(b.w, b.h, &b.bgra)?;
                    let br = comp.CreateSurfaceBrushWithSurface(&surf)?;
                    br.SetStretch(CompositionStretch::Uniform)?;
                    let v = comp.CreateSpriteVisual()?;
                    v.SetBrush(&br)?;
                    v.SetSize(Vector2 { X: 20.0 * u, Y: 20.0 * u })?;
                    v.SetOffset(Vector3 { X: 12.0 * u, Y: 9.0 * u, Z: 0.0 })?;
                    cont.Children()?.InsertAtTop(&v)?;
                }
                let style = Style { px: (14.0 * u).round() as i32, weight: 400 };
                let (tw, th) = measure(&e.title, &style);
                let tw = (tw + 2).min((cw - 54.0 * u) as i32).max(8);
                if let Some(bmp) = render(&e.title, &style, tw, th) {
                    let surf = gfx.surface_from_bgra(bmp.w, bmp.h, &recolor(&bmp, [20, 20, 20]))?;
                    let br = comp.CreateSurfaceBrushWithSurface(&surf)?;
                    br.SetStretch(CompositionStretch::None)?;
                    let v = comp.CreateSpriteVisual()?;
                    v.SetBrush(&br)?;
                    v.SetSize(Vector2 { X: tw as f32, Y: th as f32 })?;
                    v.SetOffset(Vector3 { X: 42.0 * u, Y: (hdr * u - th as f32) / 2.0, Z: 0.0 })?;
                    cont.Children()?.InsertAtTop(&v)?;
                }
                kids.InsertAtTop(&cont)?;
                cards.push((x, pad * u, cw, ch));
                x += cw + gap * u;
            }
            popup.show();
            motion::pop_in(&comp, popup.root(), Vector3 { X: pw / 2.0, Y: ph / 2.0, Z: 0.0 }, 0.94, 1.0, 0, 160)?;
            Ok(Ui { popup, entries, sel: 0, cards, ring, ring_in, dpi: u })
        };
        match go() {
            Ok(mut ui) => {
                ui.sel = if backwards { ui.entries.len() - 1 } else { 1 };
                OPEN.store(true, Ordering::Relaxed);
                self.ui = Some(ui);
                self.select(self.ui.as_ref().map(|u| u.sel).unwrap_or(1), false);
            }
            Err(e) => nacre_platform::log::line("switcher", &format!("gagal: {e}")),
        }
    }

    fn visible_count(&self) -> usize {
        self.ui.as_ref().map(|u| u.entries.len()).unwrap_or(0)
    }

    fn step(&mut self, delta: i32) {
        let n = self.visible_count() as i32;
        if n == 0 {
            return;
        }
        let cur = self.ui.as_ref().map(|u| u.sel as i32).unwrap_or(0);
        self.select(((cur + delta).rem_euclid(n)) as usize, true);
    }

    fn select(&mut self, i: usize, animate: bool) {
        let comp = self.comp.clone();
        let Some(ui) = self.ui.as_mut() else { return };
        ui.sel = i;
        let Some(&(x, y, w, h)) = ui.cards.get(i) else { return };
        let u = ui.dpi;
        for (v, grow, r) in [(&ui.ring, 7.0 * u, 14.0 * u), (&ui.ring_in, 3.0 * u, 11.0 * u)] {
            let to = Vector3 { X: x - grow, Y: y - grow, Z: 0.0 };
            let size = Vector2 { X: w + 2.0 * grow, Y: h + 2.0 * grow };
            if animate {
                let from = v.Offset().unwrap_or(to);
                let _ = motion::vec3(&comp, v, "Offset", from, to, motion::SWITCHER_MOVE_MS);
            }
            let _ = v.SetOffset(to);
            // ukuran kartu berbeda-beda: sesuaikan sorotan beserta klipnya
            let _ = v.SetSize(size);
            if let Ok(g) = comp.CreateRoundedRectangleGeometry() {
                let _ = g.SetSize(size);
                let _ = g.SetCornerRadius(Vector2 { X: r, Y: r });
                if let Ok(cl) = comp.CreateGeometricClip() {
                    let _ = cl.SetGeometry(&g);
                    let _ = v.SetClip(&cl);
                }
            }
        }
    }

    fn commit(&mut self) {
        let path = self.ui.as_ref().and_then(|u| u.entries.get(u.sel)).map(|e| e.path.clone());
        self.close();
        if let Some(p) = path {
            crate::stage::bring_app(&p);
        }
    }
}

unsafe extern "system" fn ctl_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        prefs::WM_PREFS => {
            sync();
            LRESULT(0)
        }
        WM_INSTALL => {
            install();
            LRESULT(0)
        }
        WM_NEXT | WM_PREV => {
            let open = OPEN.load(Ordering::Relaxed);
            with(|c| {
                if c.ui.is_some() {
                    c.step(if msg == WM_NEXT { 1 } else { -1 });
                } else if !open {
                    c.open(msg == WM_PREV);
                }
            });
            LRESULT(0)
        }
        WM_COMMIT => {
            with(|c| c.commit());
            LRESULT(0)
        }
        WM_CANCEL => {
            with(|c| c.close());
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe extern "system" fn sw_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
