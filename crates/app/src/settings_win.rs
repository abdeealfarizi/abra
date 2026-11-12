//! Jendela Pengaturan "ABRA": kartu kaca yang dibuka dari klik kanan area kosong dock.
//! Dibuat saat dibuka dan dihancurkan saat ditutup (tidak memakan RAM saat tidak dipakai).
//! Seluruh kontrol (toggle, segmented, slider) digambar dengan visual Composition dan hit-test region sendiri.

use crate::popup::Popup;
use crate::prefs::{self, Role, WM_PREFS};
use nacre_core::hud::slider;
use nacre_core::i18n::Lang;
use nacre_core::settings::{Behavior, Material, BlurStyle, Theme};
use nacre_platform::icons::Bitmap;
use nacre_platform::text::{measure, render, render_face, Style, ICON_FACE};
use nacre_platform::{log, sys};
use nacre_ui::gfx::Gfx;
use std::cell::RefCell;
use std::collections::HashMap;
use windows::core::w;
use windows::UI::Color;
use windows::UI::Composition::{CompositionDrawingSurface, CompositionStretch, Compositor, ContainerVisual};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows_core::Interface;
use windows_numerics::{Vector2, Vector3};

const WM_CLOSE_SELF: u32 = WM_APP + 50;
const W: f32 = 780.0;
const H: f32 = 560.0;
const SIDEBAR_W: f32 = 214.0;
const CONTENT_X: f32 = SIDEBAR_W + 24.0;
const CONTENT_W: f32 = W - SIDEBAR_W - 48.0;
const VIEW_TOP: f32 = 56.0;

#[derive(Clone, Copy, Debug)]
enum Action {
    Close,
    Takeover,
    Dock(Behavior),
    Notch(Behavior),
    Style(BlurStyle),
    Theme(Theme),
    Lang(Lang),
    Tab(u8),
    Autostart,
    Anim,
    Refraction,
    DockOn,
    MenuBarOn,
    Clock24,
    BarStyle(u8),
    HotCycle(usize),
    StageOn,
    SwitcherOn,
    /// 1 klik wallpaper, 2 jeda media, 3 judul hover.
    StageFlag(u8),
    StageRight(bool),
    StageIcons(u8),
    HideIcons,
    DockPlates,
    IconPack,
    DownloadsStack,
    RecentApps,
    AboveFull,
    ClipHistory,
    MacKeys,
    Accent(u8),
    IconStyle(u8),
    IconTint(u8),
    NotchOn,
    HoverOpen,
    WinShadow,
    Material(bool, Material),
    /// 0 blur, 1 transparansi; awal x dan lebar lintasan.
    Slider(u8, f32, f32),
}

struct Region {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    a: Action,
    /// Ikut tergulir bersama isi halaman.
    scroll: bool,
}

/// Satu baris pengaturan dalam kartu grup.
enum RowDef {
    Toggle(&'static str, bool, Action),
    /// Label, pilihan (teks, aksi), indeks terpilih.
    Seg(&'static str, Vec<(&'static str, Action)>, usize),
    Slider(&'static str, u8, u8),
    /// Tombol bernilai (mis. hot corner): label, nilai, aksi.
    Pick(&'static str, String, Action),
    /// Deretan warna aksen: label, indeks terpilih.
    Swatches(&'static str, usize),
}

struct Win {
    popup: Popup,
    gfx: Gfx,
    comp: Compositor,
    dpi: f32,
    size: (f32, f32),
    regions: Vec<Region>,
    cache: HashMap<String, (CompositionDrawingSurface, i32, i32)>,
    dragging: Option<(u8, f32, f32)>,
    /// Titik klik yang sedang ditekan (toggle): pegangan berubah jadi kaca bening selama ditekan.
    press: Option<(f32, f32)>,
    tab: u8,
    scroll: f32,
    content_h: f32,
}

thread_local! {
    static WIN: RefCell<Option<Box<Win>>> = const { RefCell::new(None) };
}

/// Buka (atau tampilkan lagi) jendela Pengaturan.
pub fn open(comp: &Compositor) {
    let existing = WIN.with(|w| w.borrow().as_ref().map(|b| b.popup.hwnd));
    if let Some(h) = existing {
        unsafe {
            let _ = SetWindowPos(h, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
        return;
    }
    match create(comp) {
        Ok(()) => log::line("settings", "dibuka"),
        Err(e) => log::line("settings", &format!("gagal: {e}")),
    }
}

fn create(comp: &Compositor) -> windows_core::Result<()> {
    let glass = prefs::glass((10.0, 10.0), 24.0, Role::Themed);
    let mut popup = Popup::new(comp, w!("AbraSettings"), Some(proc), WINDOW_EX_STYLE(0), glass)?;
    let dpi = unsafe { GetDpiForWindow(popup.hwnd) }.max(96) as f32 / 96.0;
    let gfx = Gfx::new(comp)?;
    let tier = {
        let c = sys::conditions();
        if !c.transparency || c.battery_saver || c.remote_session { nacre_ui::glass::Tier::Solid } else { nacre_ui::glass::Tier::Full }
    };
    let _ = popup.panel.set_tier(tier);
    let _ = popup.panel.set_radius(24.0 * dpi);
    let mut win = Box::new(Win { popup, gfx, comp: comp.clone(), dpi, size: (0.0, 0.0), regions: vec![], cache: HashMap::new(), dragging: None, press: None, tab: 0, scroll: 0.0, content_h: 0.0 });
    win.render();
    let (sw, sh) = sys::screen_size();
    let (w, h) = (win.size.0 as i32, win.size.1 as i32);
    let top = ((sh - h) / 2).clamp(12, (sh - h - 12).max(12));
    log::line("settings", &format!("layar={sw}x{sh} jendela={w}x{h} atas={top}"));
    win.popup.set_rect((sw - w) / 2, top, w, h)?;
    let _ = win.popup.panel.set_radius(24.0 * dpi);
    win.popup.show();
    prefs::listen(win.popup.hwnd);
    WIN.with(|s| *s.borrow_mut() = Some(win));
    Ok(())
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

fn col(rgb: [u8; 3], a: u8) -> Color {
    Color { A: a, R: rgb[0], G: rgb[1], B: rgb[2] }
}

impl Win {
    fn surface(&mut self, s: &str, px: i32, weight: i32, rgb: [u8; 3]) -> Option<(CompositionDrawingSurface, i32, i32)> {
        let key = format!("{px}|{weight}|{rgb:?}|{s}");
        if let Some(v) = self.cache.get(&key) {
            return Some(v.clone());
        }
        if self.cache.len() > 160 {
            self.cache.clear();
        }
        let style = Style { px, weight };
        let (tw, th) = measure(s, &style);
        let tw = tw + 2;
        let bmp = render(s, &style, tw, th)?;
        let data = recolor(&bmp, rgb);
        let surf = self.gfx.surface_from_bgra(bmp.w, bmp.h, &data).ok()?;
        let v = (surf, tw, th);
        self.cache.insert(key, v.clone());
        Some(v)
    }

    /// Teks di (x, y atas); `anchor`: 0 kiri, 1 tengah, 2 kanan. Mengembalikan lebar.
    #[allow(clippy::too_many_arguments)]
    fn text(&mut self, parent: &ContainerVisual, s: &str, x: f32, y: f32, size: f32, weight: i32, rgb: [u8; 3], alpha: f32, anchor: u8) -> f32 {
        let px = (size * self.dpi).round() as i32;
        let Some((surf, w, h)) = self.surface(s, px, weight, rgb) else { return 0.0 };
        let left = match anchor {
            1 => x - w as f32 / 2.0,
            2 => x - w as f32,
            _ => x,
        };
        let make = || -> windows_core::Result<()> {
            let b = self.comp.CreateSurfaceBrushWithSurface(&surf)?;
            b.SetStretch(CompositionStretch::None)?;
            let v = self.comp.CreateSpriteVisual()?;
            v.SetBrush(&b)?;
            v.SetSize(Vector2 { X: w as f32, Y: h as f32 })?;
            v.SetOffset(Vector3 { X: left, Y: y, Z: 0.0 })?;
            v.SetOpacity(alpha)?;
            parent.Children()?.InsertAtTop(&v)
        };
        let _ = make();
        w as f32
    }

    fn rect(&self, parent: &ContainerVisual, x: f32, y: f32, w: f32, h: f32, r: f32, c: Color) {
        let make = || -> windows_core::Result<()> {
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
        let _ = make();
    }

    /// Pegangan kaca bening (slider/toggle saat ditekan): isi putih tembus pandang, tepi 1 px terang, bayangan lembut.
    fn glass_knob(&self, parent: &ContainerVisual, x: f32, y: f32, w: f32, h: f32) {
        let d = self.dpi;
        self.rect(parent, x - 1.0 * d, y + 1.0 * d, w + 2.0 * d, h + 2.0 * d, h / 2.0 + d, Color { A: 46, R: 0, G: 0, B: 0 });
        let make = || -> windows_core::Result<()> {
            let geo = self.comp.CreateRoundedRectangleGeometry()?;
            geo.SetOffset(Vector2 { X: 0.5, Y: 0.5 })?;
            geo.SetSize(Vector2 { X: (w - 1.0).max(0.0), Y: (h - 1.0).max(0.0) })?;
            geo.SetCornerRadius(Vector2 { X: h / 2.0, Y: h / 2.0 })?;
            let shape = self.comp.CreateSpriteShapeWithGeometry(&geo)?;
            shape.SetFillBrush(&self.comp.CreateColorBrushWithColor(Color { A: 92, R: 255, G: 255, B: 255 })?)?;
            shape.SetStrokeBrush(&self.comp.CreateColorBrushWithColor(Color { A: 190, R: 255, G: 255, B: 255 })?)?;
            shape.SetStrokeThickness(1.0)?;
            let v = self.comp.CreateShapeVisual()?;
            v.SetSize(Vector2 { X: w, Y: h })?;
            v.SetOffset(Vector3 { X: x, Y: y, Z: 0.0 })?;
            v.Shapes()?.Append(&shape)?;
            parent.Children()?.InsertAtTop(&v)
        };
        let _ = make();
    }

    /// Glyph font ikon (Segoe Fluent) berpusat di (cx, cy).
    fn glyph_icon(&mut self, parent: &ContainerVisual, g: &str, cx: f32, cy: f32, size: f32, rgb: [u8; 3]) {
        let px = (size * self.dpi).round() as i32;
        let b = (size * 1.8 * self.dpi).round() as i32;
        let key = format!("ico|{px}|{rgb:?}|{g}");
        let hit = self.cache.get(&key).cloned();
        let (surf, w, h) = match hit {
            Some(v) => v,
            None => {
                let Some(bmp) = render_face(g, &Style { px, weight: 400 }, ICON_FACE, b, b) else { return };
                let Some((cd, cw, ch)) = nacre_core::symbols::crop_ink(&recolor(&bmp, rgb), bmp.w, bmp.h) else { return };
                let Ok(sf) = self.gfx.surface_from_bgra(cw, ch, &cd) else { return };
                let v = (sf, cw as i32, ch as i32);
                self.cache.insert(key, v.clone());
                v
            }
        };
        let make = || -> windows_core::Result<()> {
            let br = self.comp.CreateSurfaceBrushWithSurface(&surf)?;
            br.SetStretch(CompositionStretch::None)?;
            let v = self.comp.CreateSpriteVisual()?;
            v.SetBrush(&br)?;
            v.SetSize(Vector2 { X: w as f32, Y: h as f32 })?;
            v.SetOffset(Vector3 { X: cx - w as f32 / 2.0, Y: cy - h as f32 / 2.0, Z: 0.0 })?;
            parent.Children()?.InsertAtTop(&v)
        };
        let _ = make();
    }

    fn region(&mut self, x: f32, y: f32, w: f32, h: f32, a: Action) {
        self.regions.push(Region { x, y, w, h, a, scroll: true });
    }

    /// Wilayah tetap (sidebar, tombol tutup): tidak ikut tergulir.
    fn region_fixed(&mut self, x: f32, y: f32, w: f32, h: f32, a: Action) {
        self.regions.push(Region { x, y, w, h, a, scroll: false });
    }

    /// (min, maks, satuan) tiap slider.
    fn spec(id: u8) -> (f32, f32, &'static str) {
        match id {
            4 => (32.0, 72.0, " dp"),
            5 => (0.0, 24.0, " dp"),
            6 => (100.0, 250.0, "%"),
            7 => (0.0, 60.0, " dp"),
            8 => (70.0, 260.0, " dp"),
            9 => (0.0, 60.0, " dp"),
            10 => (2.0, 10.0, ""),
            11 => (50.0, 200.0, "%"),
            _ => (0.0, 100.0, "%"),
        }
    }

    fn row_height(r: &RowDef) -> f32 {
        match r {
            RowDef::Slider(..) => 50.0,
            _ => 44.0,
        }
    }

    /// Gambar satu kartu grup (sudut 12, pemisah halus) berisi `rows`; mengembalikan tinggi yang terpakai.
    fn card(&mut self, c: &ContainerVisual, y0: f32, rows: &[RowDef], look: &nacre_core::settings::Look) -> f32 {
        let d = self.dpi;
        let (x0, cw) = (CONTENT_X * d, CONTENT_W * d);
        let total: f32 = rows.iter().map(|r| Self::row_height(r) * d).sum();
        self.rect(c, x0, y0, cw, total, 12.0 * d, col(look.fg, if look.dark { 22 } else { 16 }));
        let mut y = y0;
        let accent = prefs::accent();
        for (i, r) in rows.iter().enumerate() {
            let rh = Self::row_height(r) * d;
            if i > 0 {
                self.rect(c, x0 + 16.0 * d, y, cw - 32.0 * d, 1.0, 0.0, col(look.fg, 28));
            }
            let (lx, rx) = (x0 + 16.0 * d, x0 + cw - 16.0 * d);
            match r {
                RowDef::Toggle(label, on, act) => {
                    self.text(c, label, lx, y + (rh - 17.0 * d) / 2.0, 13.5, 400, look.fg, 0.95, 0);
                    let (tw, th) = (42.0 * d, 24.0 * d);
                    let (tx, ty) = (rx - tw, y + (rh - th) / 2.0);
                    self.rect(c, tx, ty, tw, th, th / 2.0, if *on { col(accent, 255) } else { col(look.fg, 60) });
                    let knob = th - 4.0 * d;
                    let pressed = self.press.map_or(false, |(px, py)| px >= x0 && px < x0 + cw && py + self.scroll >= y && py + self.scroll < y + rh && py > VIEW_TOP * d);
                    let kw = if pressed { knob * 1.5 } else { knob * 1.25 };
                    let kx = if *on { tx + tw - kw - 2.0 * d } else { tx + 2.0 * d };
                    if pressed {
                        self.glass_knob(c, kx, ty + 2.0 * d, kw, knob);
                    } else {
                        self.rect(c, kx, ty + 2.0 * d, kw, knob, knob / 2.0, Color { A: 255, R: 255, G: 255, B: 255 });
                    }
                    self.region(x0, y, cw, rh, *act);
                }
                RowDef::Seg(label, opts, sel) => {
                    self.text(c, label, lx, y + (rh - 17.0 * d) / 2.0, 13.5, 400, look.fg, 0.95, 0);
                    let widths: Vec<f32> = opts.iter().map(|(t, _)| (measure(t, &Style { px: (12.5 * d).round() as i32, weight: 500 }).0 as f32 + 24.0 * d).max(54.0 * d)).collect();
                    let total_w: f32 = widths.iter().sum::<f32>() + 6.0 * d;
                    let (bx, bh) = (rx - total_w, 28.0 * d);
                    let by = y + (rh - bh) / 2.0;
                    self.rect(c, bx, by, total_w, bh, 9.0 * d, col(look.fg, 30));
                    let inverse = if look.dark { [10u8, 10, 12] } else { [255u8, 255, 255] };
                    let mut sx = bx + 3.0 * d;
                    for (i, (t, act)) in opts.iter().enumerate() {
                        let on = i == *sel;
                        if on {
                            self.rect(c, sx, by + 3.0 * d, widths[i], bh - 6.0 * d, 7.0 * d, col(look.fg, 235));
                        }
                        let rgb = if on { inverse } else { look.fg };
                        self.text(c, t, sx + widths[i] / 2.0, by + (bh - 16.0 * d) / 2.0 - 1.0, 12.5, if on { 600 } else { 500 }, rgb, if on { 1.0 } else { 0.85 }, 1);
                        self.region(sx, y, widths[i], rh, *act);
                        sx += widths[i];
                    }
                }
                RowDef::Slider(label, value, id) => {
                    let (min, max, unit) = Self::spec(*id);
                    let frac = ((*value as f32 - min) / (max - min)).clamp(0.0, 1.0);
                    self.text(c, label, lx, y + (rh - 17.0 * d) / 2.0, 13.5, 400, look.fg, 0.95, 0);
                    self.text(c, &format!("{value}{unit}"), rx, y + (rh - 17.0 * d) / 2.0, 12.5, 500, look.fg, 0.6, 2);
                    let (tw, th) = (190.0 * d, 5.0 * d);
                    let tx = rx - 56.0 * d - tw;
                    let ty = y + rh / 2.0 - th / 2.0;
                    self.rect(c, tx, ty, tw, th, th / 2.0, col(look.fg, 50));
                    self.rect(c, tx, ty, (tw * frac).max(th), th, th / 2.0, col(accent, 255));
                    let knob = 18.0 * d;
                    if self.dragging.map(|(i, _, _)| i) == Some(*id) {
                        let (kw, kh) = (knob * 1.9, knob * 1.25);
                        let cx = (tx + tw * frac).clamp(tx + kw / 2.0 - knob / 4.0, tx + tw - kw / 2.0 + knob / 4.0);
                        self.glass_knob(c, cx - kw / 2.0, ty + th / 2.0 - kh / 2.0, kw, kh);
                    } else {
                        let kx = (tx + tw * frac - knob / 2.0).clamp(tx - knob / 4.0, tx + tw - knob * 0.75);
                        self.rect(c, kx - 1.0 * d, ty + th / 2.0 - knob / 2.0, knob + 2.0 * d, knob + 2.0 * d, knob / 2.0 + d, Color { A: 60, R: 0, G: 0, B: 0 });
                        self.rect(c, kx, ty + th / 2.0 - knob / 2.0 - 1.0 * d, knob, knob, knob / 2.0, Color { A: 255, R: 255, G: 255, B: 255 });
                    }
                    self.region(tx - 8.0 * d, y + 4.0 * d, tw + 16.0 * d, rh - 8.0 * d, Action::Slider(*id, tx, tw));
                }
                RowDef::Swatches(label, sel) => {
                    self.text(c, label, lx, y + (rh - 17.0 * d) / 2.0, 13.5, 400, look.fg, 0.95, 0);
                    let (dia, gap) = (20.0 * d, 9.0 * d);
                    let total_w = nacre_core::settings::ACCENTS.len() as f32 * (dia + gap) - gap;
                    let mut sx = rx - total_w;
                    for (i, rgb) in nacre_core::settings::ACCENTS.iter().enumerate() {
                        let cy = y + (rh - dia) / 2.0;
                        self.rect(c, sx, cy, dia, dia, dia / 2.0, col(*rgb, 255));
                        if i == *sel {
                            // cincin pilihan: lingkaran putih kecil di tengah
                            self.rect(c, sx + dia * 0.30, cy + dia * 0.30, dia * 0.40, dia * 0.40, dia * 0.20, Color { A: 255, R: 255, G: 255, B: 255 });
                        }
                        self.region(sx - 2.0 * d, y, dia + 4.0 * d, rh, Action::Accent(i as u8));
                        sx += dia + gap;
                    }
                }
                RowDef::Pick(label, value, act) => {
                    self.text(c, label, lx, y + (rh - 17.0 * d) / 2.0, 13.5, 400, look.fg, 0.95, 0);
                    let vw = measure(value, &Style { px: (12.5 * d).round() as i32, weight: 500 }).0 as f32 + 30.0 * d;
                    let (bx, bh) = (rx - vw, 28.0 * d);
                    let by = y + (rh - bh) / 2.0;
                    self.rect(c, bx, by, vw, bh, 9.0 * d, col(look.fg, 30));
                    self.text(c, value, bx + 12.0 * d, by + (bh - 16.0 * d) / 2.0 - 1.0, 12.5, 500, look.fg, 1.0, 0);
                    self.text(c, "\u{2303}", bx + vw - 11.0 * d, by + (bh - 16.0 * d) / 2.0, 10.0, 600, look.fg, 0.6, 1);
                    self.region(x0, y, cw, rh, *act);
                }
            }
            y += rh;
        }
        total
    }

    /// Isi halaman `page`: daftar (judul grup, baris).
    fn page(&self, page: u8) -> Vec<(Option<&'static str>, Vec<RowDef>)> {
        let s = prefs::get();
        let behaviors = |dock: bool| -> Vec<(&'static str, Action)> {
            [(prefs::tr("Tetap"), Behavior::Fixed), (prefs::tr("Menimpa"), Behavior::Overlay), (prefs::tr("Auto-hide"), Behavior::AutoHide), (prefs::tr("Sentuh tepi"), Behavior::Peek)]
                .into_iter()
                .map(|(t, b)| (t, if dock { Action::Dock(b) } else { Action::Notch(b) }))
                .collect()
        };
        let beh_idx = |b: Behavior| match b {
            Behavior::Fixed => 0,
            Behavior::Overlay => 1,
            Behavior::AutoHide => 2,
            Behavior::Peek => 3,
        };
        match page {
            0 => vec![
                (None, vec![RowDef::Seg(prefs::tr("Bahasa"), vec![("Indonesia", Action::Lang(Lang::Id)), ("English", Action::Lang(Lang::En))], if s.lang == Lang::Id { 0 } else { 1 })]),
                (
                    Some(prefs::tr("Sistem")),
                    vec![
                        RowDef::Toggle(prefs::tr("Jalankan saat login Windows"), s.autostart, Action::Autostart),
                        RowDef::Toggle(prefs::tr("Sembunyikan taskbar Windows"), s.takeover, Action::Takeover),
                        RowDef::Toggle(prefs::tr("Sembunyikan ikon desktop saat ABRA berjalan"), s.hide_desktop_icons, Action::HideIcons),
                        RowDef::Toggle(prefs::tr("Selalu di atas jendela layar penuh"), s.above_fullscreen, Action::AboveFull),
                        RowDef::Toggle(prefs::tr("Pintasan ala Mac (Alt+` ganti jendela app)"), s.mac_shortcuts, Action::MacKeys),
                    ],
                ),
                (
                    Some(prefs::tr("Jendela")),
                    vec![
                        RowDef::Toggle(prefs::tr("Animasi jendela (minimize, tutup, buka)"), s.anim, Action::Anim),
                        RowDef::Toggle(prefs::tr("Bayangan halus di bawah jendela aktif"), s.win_shadow, Action::WinShadow),
                    ],
                ),
            ],
            1 => vec![
                (
                    None,
                    vec![
                        RowDef::Toggle(prefs::tr("Aktifkan dock"), s.dock_enabled, Action::DockOn),
                        RowDef::Seg(prefs::tr("Perilaku"), behaviors(true), beh_idx(s.dock)),
                        RowDef::Toggle(prefs::tr("Ikon seragam ala macOS (squircle)"), s.dock_plates, Action::DockPlates),
                        RowDef::Toggle(prefs::tr("Tumpukan Unduhan"), s.downloads_stack, Action::DownloadsStack),
                        RowDef::Toggle(prefs::tr("App terakhir dipakai"), s.recent_apps, Action::RecentApps),
                    ],
                ),
                (
                    Some(prefs::tr("Ukuran")),
                    vec![
                        RowDef::Slider(prefs::tr("Ukuran ikon"), s.dock_size, 4),
                        RowDef::Slider(prefs::tr("Jarak antar ikon"), s.dock_gap, 5),
                        RowDef::Slider(prefs::tr("Zoom saat hover"), s.dock_zoom, 6),
                        RowDef::Slider(prefs::tr("Jarak dari tepi bawah"), s.dock_margin, 7),
                        RowDef::Slider(prefs::tr("Kebulatan sudut"), s.dock_round, 2),
                    ],
                ),
            ],
            2 => vec![(
                None,
                vec![
                    RowDef::Toggle(prefs::tr("Aktifkan notch"), s.notch_enabled, Action::NotchOn),
                    RowDef::Seg(prefs::tr("Perilaku"), behaviors(false), beh_idx(s.notch)),
                    RowDef::Slider(prefs::tr("Kebulatan sudut"), s.notch_round, 3),
                    RowDef::Toggle(prefs::tr("Buka otomatis saat kursor di notch"), s.notch_hover_open, Action::HoverOpen),
                ],
            )],
            3 => {
                let cur = if !s.menubar_glass {
                    0
                } else {
                    match s.menubar_style {
                        nacre_core::settings::BarStyle::Light => 1,
                        nacre_core::settings::BarStyle::Dark => 2,
                    }
                };
                vec![(
                    None,
                    vec![
                        RowDef::Toggle(prefs::tr("Aktifkan menu bar"), s.menubar, Action::MenuBarOn),
                        RowDef::Seg(
                            prefs::tr("Latar menu bar"),
                            vec![(prefs::tr("Transparan"), Action::BarStyle(0)), (prefs::tr("Terang"), Action::BarStyle(1)), (prefs::tr("Gelap"), Action::BarStyle(2))],
                            cur,
                        ),
                        RowDef::Toggle(prefs::tr("Jam 24 jam"), s.clock_24h, Action::Clock24),
                    ],
                )]
            }
            4 => {
                let theme_idx = match s.theme {
                    Theme::Auto => 0,
                    Theme::Light => 1,
                    Theme::Dark => 2,
                };
                let blur_idx = match s.style {
                    BlurStyle::Frosted => 0,
                    BlurStyle::Clear => 1,
                    BlurStyle::Vivid => 2,
                };
                let ist = match s.icon_style {
                    nacre_core::settings::IconStyle::Default => 0,
                    nacre_core::settings::IconStyle::Dark => 1,
                    nacre_core::settings::IconStyle::Clear => 2,
                    nacre_core::settings::IconStyle::Tinted => 3,
                };
                let mut icons = vec![RowDef::Seg(
                    prefs::tr("Gaya ikon"),
                    vec![(prefs::tr("Bawaan"), Action::IconStyle(0)), (prefs::tr("Gelap"), Action::IconStyle(1)), (prefs::tr("Bening"), Action::IconStyle(2)), (prefs::tr("Berwarna"), Action::IconStyle(3))],
                    ist,
                )];
                icons.push(RowDef::Toggle(prefs::tr("Paket ikon (folder iconpack)"), s.icon_pack, Action::IconPack));
                if s.icon_style == nacre_core::settings::IconStyle::Tinted {
                    icons.push(RowDef::Seg(
                        prefs::tr("Warna tint"),
                        vec![(prefs::tr("Biru"), Action::IconTint(0)), (prefs::tr("Ungu"), Action::IconTint(1)), (prefs::tr("Merah Muda"), Action::IconTint(2)), (prefs::tr("Oranye"), Action::IconTint(3)), (prefs::tr("Hijau"), Action::IconTint(4))],
                        s.icon_tint as usize,
                    ));
                }
                vec![
                    (
                        None,
                        vec![
                            RowDef::Seg(prefs::tr("Tema"), vec![(prefs::tr("Otomatis"), Action::Theme(Theme::Auto)), (prefs::tr("Terang"), Action::Theme(Theme::Light)), (prefs::tr("Gelap"), Action::Theme(Theme::Dark))], theme_idx),
                            RowDef::Swatches(prefs::tr("Warna aksen"), s.accent as usize),
                        ],
                    ),
                    (
                        Some(prefs::tr("Kaca")),
                        vec![
                            RowDef::Seg(prefs::tr("Material"), vec![(prefs::tr("Liquid Glass"), Action::Material(true, Material::Glass)), (prefs::tr("Hitam buram"), Action::Material(true, Material::Black))], if s.dock_material == Material::Glass { 0 } else { 1 }),
                            RowDef::Seg(prefs::tr("Material notch"), vec![(prefs::tr("Liquid Glass"), Action::Material(false, Material::Glass)), (prefs::tr("Hitam buram"), Action::Material(false, Material::Black))], if s.notch_material == Material::Glass { 0 } else { 1 }),
                            RowDef::Seg(prefs::tr("Gaya blur"), vec![(prefs::tr("Buram"), Action::Style(BlurStyle::Frosted)), (prefs::tr("Bening"), Action::Style(BlurStyle::Clear)), (prefs::tr("Hidup"), Action::Style(BlurStyle::Vivid))], blur_idx),
                            RowDef::Slider(prefs::tr("Tingkat blur"), s.blur, 0),
                            RowDef::Slider(prefs::tr("Transparansi"), s.transparency, 1),
                            RowDef::Toggle(prefs::tr("Refraksi tepi kaca (dock)"), s.refraction, Action::Refraction),
                        ],
                    ),
                    (Some(prefs::tr("Ikon")), icons),
                ]
            }
            5 => vec![
                (
                    None,
                    vec![
                        RowDef::Toggle(prefs::tr("Aktifkan Stage Manager"), s.stage, Action::StageOn),
                        RowDef::Toggle(prefs::tr("Ganti Alt+Tab dengan pengalih ABRA"), s.switcher, Action::SwitcherOn),
                        RowDef::Toggle(prefs::tr("Klik wallpaper menyimpan semua jendela"), s.stage_desktop_click, Action::StageFlag(1)),
                        RowDef::Toggle(prefs::tr("Judul jendela saat hover"), s.stage_titles, Action::StageFlag(3)),
                        RowDef::Toggle(prefs::tr("Jeda media app di latar"), s.stage_media, Action::StageFlag(2)),
                    ],
                ),
                (
                    Some(prefs::tr("Daftar jendela")),
                    vec![
                        RowDef::Seg(prefs::tr("Posisi daftar jendela"), vec![(prefs::tr("Kiri"), Action::StageRight(false)), (prefs::tr("Kanan"), Action::StageRight(true))], if s.stage_right { 1 } else { 0 }),
                        RowDef::Seg(
                            prefs::tr("Ikon desktop"),
                            vec![(prefs::tr("Sembunyi"), Action::StageIcons(0)), (prefs::tr("Otomatis"), Action::StageIcons(1)), (prefs::tr("Tampil"), Action::StageIcons(2))],
                            s.stage_icons as usize,
                        ),
                        RowDef::Slider(prefs::tr("Ukuran kartu"), s.stage_size, 8),
                        RowDef::Slider(prefs::tr("Jarak antar kartu"), s.stage_gap, 9),
                        RowDef::Slider(prefs::tr("Jumlah kartu"), s.stage_count, 10),
                        RowDef::Slider(prefs::tr("Kecepatan animasi"), s.stage_speed, 11),
                    ],
                ),
            ],
            6 => {
                let names = ["Kiri atas", "Kanan atas", "Kiri bawah", "Kanan bawah"];
                vec![(None, names.iter().enumerate().map(|(i, nm)| RowDef::Pick(prefs::tr(nm), prefs::tr(hot_label(s.hot[i])).to_string(), Action::HotCycle(i))).collect())]
            }
            _ => vec![(None, vec![RowDef::Toggle(prefs::tr("Riwayat Papan Klip di Spotlight"), s.clipboard_history, Action::ClipHistory)])],
        }
    }

    fn render(&mut self) {
        let content = self.popup.content.clone();
        if let Ok(k) = content.Children() {
            let _ = k.RemoveAll();
        }
        self.regions.clear();
        let d = self.dpi;
        let look = prefs::look(Role::Themed);
        let c = &content;

        // lampu jendela (hanya merah yang berfungsi; kuning/hijau abu-abu seperti jendela tak aktif)
        for (i, rgb) in [[255u8, 95, 87], [254, 188, 46], [40, 200, 64]].iter().enumerate() {
            let (cx, cy) = ((22.0 + i as f32 * 20.0) * d, 24.0 * d);
            let color = if i == 0 { col(*rgb, 255) } else { col(look.fg, 45) };
            self.rect(c, cx - 6.0 * d, cy - 6.0 * d, 12.0 * d, 12.0 * d, 6.0 * d, color);
        }
        self.region_fixed(14.0 * d, 14.0 * d, 20.0 * d, 20.0 * d, Action::Close);

        // sidebar
        let pages: [(&'static str, &'static str, [u8; 3], u8); 8] = [
            (prefs::tr("Umum"), "\u{E713}", [142, 142, 147], 0),
            (prefs::tr("Dock"), "\u{E7C4}", [10, 132, 255], 1),
            (prefs::tr("Notch"), "\u{E7F4}", [58, 58, 60], 2),
            (prefs::tr("Menu Bar"), "\u{E700}", [90, 200, 250], 3),
            (prefs::tr("Tampilan"), "\u{E790}", [88, 86, 214], 4),
            (prefs::tr("Stage Manager"), "\u{E8A0}", [48, 209, 88], 5),
            (prefs::tr("Sudut Layar"), "\u{E73F}", [175, 82, 222], 6),
            (prefs::tr("Spotlight"), "\u{E721}", [255, 149, 10], 7),
        ];
        let mut sy = 56.0 * d;
        for (name, glyph, color, idx) in pages {
            let sel = idx == self.tab;
            let (rx, rw, rh) = (10.0 * d, (SIDEBAR_W - 20.0) * d, 36.0 * d);
            if sel {
                self.rect(c, rx, sy, rw, rh, 10.0 * d, crate::prefs::accent_c(235));
            }
            self.rect(c, rx + 8.0 * d, sy + 6.0 * d, 24.0 * d, 24.0 * d, 7.0 * d, col(color, 255));
            self.glyph_icon(c, glyph, rx + 20.0 * d, sy + 18.0 * d, 13.0, [255, 255, 255]);
            self.text(c, name, rx + 42.0 * d, sy + (rh - 17.0 * d) / 2.0, 13.5, if sel { 600 } else { 400 }, if sel { [255, 255, 255] } else { look.fg }, 1.0, 0);
            self.region_fixed(rx, sy, rw, rh, Action::Tab(idx));
            sy += 40.0 * d;
        }
        self.rect(c, SIDEBAR_W * d, 0.0, 1.0, H * d, 0.0, col(look.fg, 30));

        // judul halaman
        let title = pages.iter().find(|p| p.3 == self.tab).map(|p| p.0).unwrap_or("");
        self.text(c, title, CONTENT_X * d, 17.0 * d, 18.0, 700, look.fg, 1.0, 0);

        // isi (tergulir) di dalam viewport berklip
        let view = match (self.comp.CreateContainerVisual(), self.comp.CreateContainerVisual()) {
            (Ok(v), Ok(b)) => Some((v, b)),
            _ => None,
        };
        let Some((viewport, body)) = view else { return };
        let (vt, vh) = (VIEW_TOP * d, (H - VIEW_TOP - 10.0) * d);
        let _ = viewport.SetOffset(Vector3 { X: 0.0, Y: vt, Z: 0.0 });
        let _ = viewport.SetSize(Vector2 { X: W * d, Y: vh });
        if let Ok(clip) = self.comp.CreateInsetClip() {
            let _ = clip.SetLeftInset(0.0);
            let _ = clip.SetRightInset(0.0);
            let _ = clip.SetTopInset(0.0);
            let _ = clip.SetBottomInset(0.0);
            let _ = viewport.SetClip(&clip);
        }
        let _ = body.SetOffset(Vector3 { X: 0.0, Y: -self.scroll, Z: 0.0 });
        let _ = c.Children().and_then(|k| k.InsertAtTop(&viewport));
        let _ = viewport.Children().and_then(|k| k.InsertAtTop(&body));

        // koordinat isi dihitung dalam ruang halaman (y=VIEW_TOP+...) lalu dipindah ke viewport (dikurangi vt)
        let mut y = VIEW_TOP * d - vt; // 0
        y += 4.0 * d;
        let groups = self.page(self.tab);
        for (header, rows) in groups {
            if let Some(h) = header {
                self.text_abs(&body, h, CONTENT_X * d + 8.0 * d, y + 2.0 * d, 12.0, 600, look.fg, 0.55);
                y += 24.0 * d;
            }
            let used = self.card_in(&body, y, &rows, &look, vt);
            y += used + 20.0 * d;
        }
        self.content_h = y + 6.0 * d;
        let max_scroll = (self.content_h - vh).max(0.0);
        if self.scroll > max_scroll {
            self.scroll = max_scroll;
            let _ = body.SetOffset(Vector3 { X: 0.0, Y: -self.scroll, Z: 0.0 });
        }
        self.size = (W * d, H * d);
        let (x, yy, ww, hh) = self.popup.rect;
        let (nw, nh) = ((W * d) as i32, (H * d) as i32);
        if (nw, nh) != (ww, hh) {
            let _ = self.popup.set_rect(x, yy, nw, nh);
            let _ = self.popup.panel.set_radius(24.0 * d);
        }
    }

    /// Teks pada ruang isi (tanpa penyesuaian).
    #[allow(clippy::too_many_arguments)]
    fn text_abs(&mut self, parent: &ContainerVisual, s: &str, x: f32, y: f32, size: f32, weight: i32, rgb: [u8; 3], alpha: f32) {
        self.text(parent, s, x, y, size, weight, rgb, alpha, 0);
    }

    /// `card` yang menggambar di ruang viewport (y relatif atas viewport) dan mendaftarkan wilayah di ruang layar-gulir
    /// (y + VIEW_TOP): wilayah diuji dengan `hit` yang menambahkan gulir.
    fn card_in(&mut self, body: &ContainerVisual, y: f32, rows: &[RowDef], look: &nacre_core::settings::Look, vt: f32) -> f32 {
        let before = self.regions.len();
        let used = self.card(body, y, rows, look);
        // geser wilayah baru ke ruang klien (tambah offset viewport)
        for r in self.regions[before..].iter_mut() {
            r.y += vt;
        }
        used
    }

    fn hit(&self, x: f32, y: f32) -> Option<Action> {
        let d = self.dpi;
        let (vt, vb) = (VIEW_TOP * d, (H - 10.0) * d);
        self.regions
            .iter()
            .rev()
            .find(|r| {
                let yy = if r.scroll {
                    if y < vt || y > vb {
                        return false;
                    }
                    y + self.scroll
                } else {
                    y
                };
                x >= r.x && x < r.x + r.w && yy >= r.y && yy < r.y + r.h
            })
            .map(|r| r.a)
    }

    fn slide(&self, id: u8, x: f32, sx: f32, sw: f32, save: bool) {
        let (min, max, _) = Self::spec(id);
        let v = (min + slider::value_at(x, sx, sw, false) * (max - min)).round() as u8;
        prefs::update_with(save, |s| {
            match id {
                0 => s.blur = v,
                1 => s.transparency = v,
                2 => s.dock_round = v,
                3 => s.notch_round = v,
                4 => s.dock_size = v,
                5 => s.dock_gap = v,
                6 => s.dock_zoom = v,
                8 => s.stage_size = v,
                9 => s.stage_gap = v,
                10 => s.stage_count = v,
                11 => s.stage_speed = v,
                _ => s.dock_margin = v,
            }
        });
    }
}

fn hot_label(a: nacre_core::settings::HotAction) -> &'static str {
    use nacre_core::settings::HotAction::*;
    match a {
        None => "Tidak ada",
        Mission => "Mission Control",
        Launchpad => "Apps",
        Desktop => "Tampilkan Desktop",
        Stage => "Stage Manager",
        Notifications => "Pusat Notifikasi",
        Lock => "Kunci Layar",
    }
}

fn xy(lp: LPARAM) -> (f32, f32) {
    ((lp.0 as i16) as f32, ((lp.0 >> 16) as i16) as f32)
}

fn with_win<R>(f: impl FnOnce(&mut Win) -> R) -> Option<R> {
    WIN.with(|s| s.try_borrow_mut().ok().and_then(|mut b| b.as_mut().map(|w| f(w))))
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_NCHITTEST => {
            // Judul (di luar tombol tutup) bisa menyeret jendela.
            let mut r = windows::Win32::Foundation::RECT::default();
            let _ = GetWindowRect(hwnd, &mut r);
            let (px, py) = ((lp.0 as i16) as i32, ((lp.0 >> 16) as i16) as i32);
            let drag = with_win(|w| {
                let d = w.dpi;
                py - r.top < (VIEW_TOP * d) as i32 && px - r.left > (SIDEBAR_W * d) as i32
            })
            .unwrap_or(false);
            LRESULT(if drag { HTCAPTION } else { HTCLIENT } as isize)
        }
        WM_MOUSEWHEEL => {
            let delta = ((wp.0 >> 16) as i16) as f32;
            with_win(|w| {
                let d = w.dpi;
                let vh = (H - VIEW_TOP - 10.0) * d;
                let max = (w.content_h - vh).max(0.0);
                w.scroll = (w.scroll - delta / 120.0 * 48.0 * d).clamp(0.0, max);
                w.render();
            });
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let (x, y) = xy(lp);
            let started = with_win(|w| match w.hit(x, y) {
                Some(Action::Slider(id, sx, sw)) => {
                    w.dragging = Some((id, sx, sw));
                    w.slide(id, x, sx, sw, false);
                    w.render();
                    true
                }
                Some(_) => {
                    w.press = Some((x, y));
                    w.render();
                    false
                }
                None => false,
            })
            .unwrap_or(false);
            if started {
                let _ = SetCapture(hwnd);
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let (x, _) = xy(lp);
            with_win(|w| {
                if let Some((id, sx, sw)) = w.dragging {
                    w.slide(id, x, sx, sw, false);
                }
            });
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let (x, y) = xy(lp);
            let _ = ReleaseCapture();
            let act = with_win(|w| {
                let was_pressed = w.press.take().is_some();
                if let Some((id, sx, sw)) = w.dragging.take() {
                    w.slide(id, x, sx, sw, true);
                    w.render();
                    return None;
                }
                if was_pressed {
                    w.render();
                }
                w.hit(x, y)
            })
            .flatten();
            match act {
                Some(Action::Close) => {
                    let _ = PostMessageW(Some(hwnd), WM_CLOSE_SELF, WPARAM(0), LPARAM(0));
                }
                Some(Action::Takeover) => prefs::update(|s| s.takeover = !s.takeover),
                Some(Action::Dock(b)) => prefs::update(|s| s.dock = b),
                Some(Action::Notch(b)) => prefs::update(|s| s.notch = b),
                Some(Action::Style(b)) => prefs::update(|s| s.style = b),
                Some(Action::Theme(t)) => prefs::update(|s| s.theme = t),
                Some(Action::Lang(l)) => prefs::update(|s| s.lang = l),
                Some(Action::WinShadow) => prefs::update(|s| s.win_shadow = !s.win_shadow),
                Some(Action::HoverOpen) => prefs::update(|s| s.notch_hover_open = !s.notch_hover_open),
                Some(Action::MenuBarOn) => prefs::update(|s| s.menubar = !s.menubar),
                Some(Action::HotCycle(i)) => prefs::update(|s| s.hot[i] = s.hot[i].next()),
                Some(Action::StageOn) => prefs::update(|s| s.stage = !s.stage),
                Some(Action::SwitcherOn) => prefs::update(|s| s.switcher = !s.switcher),
                Some(Action::StageFlag(k)) => prefs::update(|s| match k {
                    1 => s.stage_desktop_click = !s.stage_desktop_click,
                    2 => s.stage_media = !s.stage_media,
                    _ => s.stage_titles = !s.stage_titles,
                }),
                Some(Action::StageRight(r)) => prefs::update(|s| s.stage_right = r),
                Some(Action::StageIcons(m)) => prefs::update(|s| s.stage_icons = m),
                Some(Action::MacKeys) => prefs::update(|s| s.mac_shortcuts = !s.mac_shortcuts),
                Some(Action::Accent(i)) => prefs::update(|s| s.accent = i),
                Some(Action::IconStyle(m)) => prefs::update(|s| {
                    s.icon_style = match m {
                        0 => nacre_core::settings::IconStyle::Default,
                        1 => nacre_core::settings::IconStyle::Dark,
                        2 => nacre_core::settings::IconStyle::Clear,
                        _ => nacre_core::settings::IconStyle::Tinted,
                    }
                }),
                Some(Action::IconTint(i)) => prefs::update(|s| s.icon_tint = i),
                Some(Action::ClipHistory) => prefs::update(|s| s.clipboard_history = !s.clipboard_history),
                Some(Action::AboveFull) => prefs::update(|s| s.above_fullscreen = !s.above_fullscreen),
                Some(Action::DockPlates) => prefs::update(|s| s.dock_plates = !s.dock_plates),
                Some(Action::IconPack) => prefs::update(|s| s.icon_pack = !s.icon_pack),
                Some(Action::DownloadsStack) => prefs::update(|s| s.downloads_stack = !s.downloads_stack),
                Some(Action::RecentApps) => prefs::update(|s| s.recent_apps = !s.recent_apps),
                Some(Action::HideIcons) => prefs::update(|s| s.hide_desktop_icons = !s.hide_desktop_icons),
                Some(Action::BarStyle(m)) => prefs::update(|s| match m {
                    0 => s.menubar_glass = false,
                    1 => {
                        s.menubar_glass = true;
                        s.menubar_style = nacre_core::settings::BarStyle::Light;
                    }
                    _ => {
                        s.menubar_glass = true;
                        s.menubar_style = nacre_core::settings::BarStyle::Dark;
                    }
                }),
                Some(Action::Clock24) => prefs::update(|s| s.clock_24h = !s.clock_24h),
                Some(Action::DockOn) => prefs::update(|s| s.dock_enabled = !s.dock_enabled),
                Some(Action::NotchOn) => prefs::update(|s| s.notch_enabled = !s.notch_enabled),
                Some(Action::Refraction) => prefs::update(|s| s.refraction = !s.refraction),
                Some(Action::Anim) => prefs::update(|s| s.anim = !s.anim),
                Some(Action::Autostart) => prefs::update(|s| s.autostart = !s.autostart),
                // material dock dan menu bar (true) dipilih terpisah dari material notch (false)
                Some(Action::Material(true, m)) => prefs::update(|s| s.dock_material = m),
                Some(Action::Material(false, m)) => prefs::update(|s| s.notch_material = m),
                Some(Action::Tab(t)) => {
                    with_win(|w| {
                        w.tab = t;
                        w.scroll = 0.0;
                        w.render();
                    });
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_PREFS => {
            with_win(|w| w.render());
            let (themed, _) = (prefs::look(Role::Themed), ());
            with_win(|w| {
                let base = nacre_ui::glass::GlassParams::dark((10.0, 10.0), 24.0 * w.dpi);
                let _ = w.popup.panel.set_params(prefs::apply(base, &themed));
            });
            LRESULT(0)
        }
        WM_CLOSE_SELF => {
            prefs::unlisten(hwnd);
            let win = WIN.with(|s| s.borrow_mut().take());
            drop(win);
            let _ = DestroyWindow(hwnd);
            log::line("settings", "ditutup");
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
