//! Menu konteks kaca kustom. Menggantikan menu Win32 bawaan: tidak ada loop pesan bersarang
//! (timer dan event dock tetap berjalan), dan menutup saat klik di luar lewat penangkapan mouse
//! hook mouse yang hanya dipasang selama menu terbuka (SetCapture tidak andal untuk window NOACTIVATE).

use crate::popup::Popup;
use nacre_platform::text::{measure, render, Style};
use nacre_ui::gfx::Gfx;
use nacre_ui::glass::{GlassParams, Tier};
use std::cell::RefCell;
use std::sync::atomic::{AtomicIsize, Ordering};
use windows::core::w;
use windows::UI::Color;
use windows::UI::Composition::{CompositionStretch, Compositor, SpriteVisual};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows_core::Result;
use windows_numerics::{Vector2, Vector3};

pub const MENU_PICK: u32 = WM_APP + 12;
pub const MENU_CLOSE: u32 = WM_APP + 13;

static TARGET: AtomicIsize = AtomicIsize::new(0);

#[derive(Clone)]
pub struct Item {
    pub id: usize,
    pub label: String,
    pub enabled: bool,
    pub separator: bool,
    /// Teks pintasan di kanan (mis. "Ctrl+S"), redup.
    pub accel: String,
    /// Simbol di kiri label (opsional).
    pub sym: Option<nacre_core::symbols::Sym>,
    /// Sakelar di kanan (menu Wi-Fi/Bluetooth): `Some(nyala)`.
    pub toggle: Option<bool>,
    /// Judul bagian kecil redup (mis. "Jaringan Dikenal"), tidak bisa diklik.
    pub heading: bool,
    /// Judul tebal di baris pertama (mis. "Wi-Fi").
    pub title: bool,
    /// Ikon dalam lingkaran di kiri; `true` = aktif (lingkaran warna aksen).
    pub lead: Option<(nacre_core::symbols::Sym, bool)>,
    /// Simbol kecil di kanan (gembok, chevron).
    pub trail: Vec<nacre_core::symbols::Sym>,
    /// Klik tidak menutup menu (sakelar, buka/tutup "Jaringan Lain").
    pub keep_open: bool,
}

impl Item {
    pub fn action(id: usize, label: &str, enabled: bool) -> Item {
        Item { id, label: label.to_string(), enabled, separator: false, accel: String::new(), sym: None, toggle: None, heading: false, title: false, lead: None, trail: Vec::new(), keep_open: false }
    }
    pub fn separator() -> Item {
        Item { separator: true, ..Item::action(0, "", false) }
    }
    pub fn heading(label: &str) -> Item {
        Item { heading: true, ..Item::action(0, label, false) }
    }
    /// Baris judul tebal dengan sakelar (klik di mana saja pada baris membalik sakelar; menu tetap terbuka).
    pub fn switch(id: usize, label: &str, on: bool) -> Item {
        Item { title: true, toggle: Some(on), keep_open: true, ..Item::action(id, label, true) }
    }
    pub fn with_lead(mut self, s: nacre_core::symbols::Sym, on: bool) -> Item {
        self.lead = Some((s, on));
        self
    }
    pub fn with_trail(mut self, s: Vec<nacre_core::symbols::Sym>) -> Item {
        self.trail = s;
        self
    }
    pub fn keep_open(mut self) -> Item {
        self.keep_open = true;
        self
    }
    pub fn with_sym(mut self, s: Option<nacre_core::symbols::Sym>) -> Item {
        self.sym = s;
        self
    }
    pub fn with_accel(mut self, a: &str) -> Item {
        self.accel = a.to_string();
        self
    }
}

/// Gaya "kartu" (jendela Wi-Fi/Bluetooth): sudut sebesar kartu slider Control Center, tepi dalam lebih lega, dan
/// animasi buka/tutup Control Center (tumbuh dari titik jangkar dengan pantulan kecil, baris muncul berurutan).
#[derive(Clone, Copy, Debug)]
pub struct Card {
    /// Radius sudut (dp).
    pub radius: f32,
    /// Jarak tepi ke baris (dp).
    pub pad: f32,
    /// Lebar minimum (dp).
    pub min_w: f32,
}

/// Timer jendela menu: sembunyikan setelah animasi tutup kartu selesai.
const T_HIDE: usize = 77;

/// Keadaan yang dibaca window proc (thread UI yang sama dengan pemiliknya).
struct Live {
    rows: Vec<(f32, f32, Item)>, // (y, tinggi, item)
    size: (f32, f32),
    hover: Option<usize>,
    highlight: SpriteVisual,
    pad: f32,
    /// Radius sorotan maksimum untuk gaya kartu (px); `None` = sorotan menu biasa.
    hl_max: Option<f32>,
    /// Pemilik menutup menu sendiri (dengan animasi) setelah item dipilih.
    owner_hides: bool,
}

thread_local! {
    static LIVE: RefCell<Option<Live>> = const { RefCell::new(None) };
}

/// Warnai bitmap teks putih ter-premultiply menjadi `rgb`.
fn tint(src: &[u8], rgb: [u8; 3]) -> Vec<u8> {
    let mut out = src.to_vec();
    for px in out.chunks_exact_mut(4) {
        let a = px[3] as u32;
        px[0] = (rgb[2] as u32 * a / 255) as u8;
        px[1] = (rgb[1] as u32 * a / 255) as u8;
        px[2] = (rgb[0] as u32 * a / 255) as u8;
    }
    out
}

fn row_at(l: &Live, x: f32, y: f32) -> Option<usize> {
    if x < 0.0 || y < 0.0 || x >= l.size.0 || y >= l.size.1 {
        return None;
    }
    l.rows.iter().position(|(ry, rh, it)| y >= *ry && y < ry + rh && !it.separator && it.enabled)
}

/// Pindahkan sorotan ke baris `hit` (atau sembunyikan).
fn set_hover(live: &mut Live, hit: Option<usize>) {
    live.hover = hit;
    match hit {
        Some(i) => {
            let (ry, rh, _) = &live.rows[i];
            let _ = live.highlight.SetOffset(Vector3 { X: live.pad, Y: *ry, Z: 0.0 });
            let _ = live.highlight.SetSize(Vector2 { X: live.size.0 - 2.0 * live.pad, Y: *rh });
            // sudut bulat ala Tahoe (radius ±8)
            if let Ok(comp) = live.highlight.Compositor() {
                if let (Ok(g), Ok(clip)) = (comp.CreateRoundedRectangleGeometry(), comp.CreateGeometricClip()) {
                    let r = match live.hl_max {
                        Some(m) => (*rh / 2.0).min(m),
                        None => (*rh / 3.0).clamp(6.0, 9.0),
                    };
                    let _ = g.SetSize(Vector2 { X: live.size.0 - 2.0 * live.pad, Y: *rh });
                    let _ = g.SetCornerRadius(Vector2 { X: r, Y: r });
                    let _ = clip.SetGeometry(&g);
                    let _ = live.highlight.SetClip(&clip);
                }
            }
            let _ = live.highlight.SetOpacity(1.0);
        }
        None => {
            let _ = live.highlight.SetOpacity(0.0);
        }
    }
}

/// Kotak membulat berwarna rata.
fn rounded(comp: &Compositor, r: (f32, f32, f32, f32), radius: f32, c: Color) -> Result<SpriteVisual> {
    let v = comp.CreateSpriteVisual()?;
    v.SetBrush(&comp.CreateColorBrushWithColor(c)?)?;
    v.SetSize(Vector2 { X: r.2, Y: r.3 })?;
    v.SetOffset(Vector3 { X: r.0, Y: r.1, Z: 0.0 })?;
    let g = comp.CreateRoundedRectangleGeometry()?;
    g.SetSize(Vector2 { X: r.2, Y: r.3 })?;
    g.SetCornerRadius(Vector2 { X: radius, Y: radius })?;
    let clip = comp.CreateGeometricClip()?;
    clip.SetGeometry(&g)?;
    v.SetClip(&clip)?;
    Ok(v)
}

fn xy(lp: LPARAM) -> (f32, f32) {
    ((lp.0 as i16) as f32, ((lp.0 >> 16) as i16) as f32)
}

unsafe extern "system" fn proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    let target = HWND(TARGET.load(Ordering::Relaxed) as *mut _);
    match m {
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_MOUSEMOVE => {
            let (x, y) = xy(l);
            LIVE.with(|c| {
                if let Some(live) = c.borrow_mut().as_mut() {
                    let hit = row_at(live, x, y).filter(|i| live.rows[*i].2.toggle.is_none());
                    if hit != live.hover {
                        set_hover(live, hit);
                    }
                }
            });
            LRESULT(0)
        }
        // Klik pertama di luar menu menutupnya (dan tidak diteruskan ke aplikasi di bawah).
        WM_LBUTTONUP | WM_RBUTTONUP => {
            let (x, y) = xy(l);
            let picked = LIVE.with(|c| c.borrow().as_ref().and_then(|live| row_at(live, x, y).map(|i| (live.rows[i].2.id, live.rows[i].2.keep_open || live.owner_hides))));
            if let Some((id, keep)) = picked {
                if !keep {
                    nacre_platform::input::end_dismiss();
                    let _ = ShowWindow(h, SW_HIDE);
                }
                let _ = PostMessageW(Some(target), MENU_PICK, WPARAM(id), LPARAM(0));
            }
            LRESULT(0)
        }
        WM_TIMER if w.0 == T_HIDE => {
            let _ = KillTimer(Some(h), T_HIDE);
            let _ = ShowWindow(h, SW_HIDE);
            LRESULT(0)
        }
        _ => DefWindowProcW(h, m, w, l),
    }
}

pub struct Menu {
    target: HWND,
    popup: Popup,
    content: windows::UI::Composition::ContainerVisual,
    highlight: SpriteVisual,
    dpi: f32,
    screen: (i32, i32),
    card: Option<Card>,
    /// Animasi tutup kartu sedang berjalan.
    closing: bool,
    /// Titik jangkar animasi kartu (px, relatif jendela).
    pop_at: Vector3,
}

impl Menu {
    pub fn new(comp: &Compositor, dock_hwnd: HWND, dpi: f32, screen: (i32, i32), tier: Tier) -> Result<Menu> {
        let mut popup = Popup::new(comp, w!("NacreMenu"), Some(proc), WINDOW_EX_STYLE(0), crate::prefs::glass((10.0, 10.0), 12.0 * dpi, crate::prefs::Role::Dark))?;
        let _ = popup.panel.set_tier(tier);
        let highlight = comp.CreateSpriteVisual()?;
        highlight.SetBrush(&comp.CreateColorBrushWithColor(crate::prefs::accent_c(235))?)?;
        highlight.SetOpacity(0.0)?;
        let clip = comp.CreateRoundedRectangleGeometry()?;
        clip.SetCornerRadius(Vector2 { X: 7.0 * dpi, Y: 7.0 * dpi })?;
        let _ = &mut popup;
        let content = comp.CreateContainerVisual()?;
        popup.content.Children()?.InsertAtTop(&highlight)?;
        popup.content.Children()?.InsertAtTop(&content)?;
        Ok(Menu { target: dock_hwnd, popup, content, highlight, dpi, screen, card: None, closing: false, pop_at: Vector3 { X: 0.0, Y: 0.0, Z: 0.0 } })
    }

    /// Pakai gaya kartu (lihat [`Card`]) untuk menu ini.
    pub fn set_card(&mut self, card: Option<Card>) {
        self.card = card;
    }

    /// Kartu dengan tepi atas di `top`; `right_aligned` = `x` adalah tepi KANAN (pojok kanan layar), selain itu tepi kiri.
    pub fn show_card(&mut self, comp: &Compositor, gfx: &Gfx, items: Vec<Item>, x: i32, top: i32, right_aligned: bool) -> Result<()> {
        self.show_impl(comp, gfx, items, x, top, false, right_aligned)
    }

    /// Tutup kartu dengan animasi tutup Control Center (mengecil ke titik jangkar sambil memudar).
    pub fn hide_animated(&mut self, comp: &Compositor) {
        if !self.popup.is_visible() || self.closing {
            return;
        }
        nacre_platform::input::end_dismiss();
        LIVE.with(|c| *c.borrow_mut() = None);
        if crate::motion::pop_out(comp, self.popup.root(), self.pop_at, 0.55, crate::motion::CC_CLOSE_MS).is_ok() {
            self.closing = true;
            unsafe {
                SetTimer(Some(self.popup.hwnd), T_HIDE, crate::motion::CC_CLOSE_MS + 20, None);
            }
        } else {
            self.popup.hide();
        }
    }

    pub fn apply_look(&mut self, p: GlassParams) {
        let _ = self.popup.panel.set_params(p);
    }

    pub fn set_tier(&mut self, t: Tier) {
        let _ = self.popup.panel.set_tier(t);
    }

    /// Tampilkan di `(x, y)` (sudut kiri-bawah menu di dekat kursor; digeser agar tetap di layar).
    pub fn show(&mut self, comp: &Compositor, gfx: &Gfx, items: Vec<Item>, x: i32, y: i32) -> Result<()> {
        self.show_impl(comp, gfx, items, x, y, true, false)
    }

    /// Seperti `show`, tetapi `top` adalah tepi ATAS menu (dropdown menu bar muncul tepat di bawah bar).
    pub fn show_below(&mut self, comp: &Compositor, gfx: &Gfx, items: Vec<Item>, x: i32, top: i32) -> Result<()> {
        self.show_impl(comp, gfx, items, x, top, false, false)
    }

    #[allow(clippy::too_many_arguments)]
    fn show_impl(&mut self, comp: &Compositor, gfx: &Gfx, items: Vec<Item>, x: i32, y: i32, above: bool, right_aligned: bool) -> Result<()> {
        TARGET.store(self.target.0 as isize, Ordering::Relaxed);
        let d = self.dpi;
        // dibuka lagi saat animasi tutup belum selesai: sembunyikan dulu lalu buka dengan animasi baru
        if self.closing {
            self.closing = false;
            unsafe {
                let _ = KillTimer(Some(self.popup.hwnd), T_HIDE);
            }
            self.popup.hide();
        }
        // menu yang sudah terbuka digambar ulang di tempat (daftar jaringan diperbarui, sakelar dibalik): tanpa animasi muncul
        let refresh = self.popup.is_visible();
        let card = self.card;
        let radius = card.map_or(13.0, |c| c.radius) * d;
        // kaca dan warna teks mengikuti tema (Tahoe: menu terang di mode terang)
        let look = crate::prefs::look(crate::prefs::Role::Themed);
        let base = GlassParams::dark((10.0, 10.0), radius);
        let _ = self.popup.panel.set_params(crate::prefs::apply(base, &look));
        let ink: [u8; 3] = if look.dark { [255, 255, 255] } else { [24, 24, 28] };
        let pad = card.map_or(6.0, |c| c.pad) * d;
        let (row_h, sep_h, text_px) = (24.0 * d, 11.0 * d, (14.0 * d).round() as i32);
        let style = Style { px: text_px, weight: 400 };
        let head_px = (12.0 * d).round() as i32;
        let style_of = |it: &Item| Style { px: if it.heading { head_px } else { text_px }, weight: if it.title { 700 } else if it.heading { 600 } else { 400 } };
        let has_sym = items.iter().any(|i| i.sym.is_some());
        let sym_w = if has_sym { 24.0 * d } else { 0.0 };
        let lead_d = 26.0 * d;
        let sw = (32.0 * d, 19.0 * d);
        let trail_px = 14.0 * d;
        let label_x = |it: &Item| if it.lead.is_some() { pad + 8.0 * d + lead_d + 10.0 * d } else { pad + 12.0 * d + sym_w };
        let rich = items.iter().any(|i| i.lead.is_some() || i.toggle.is_some());
        let widest = items
            .iter()
            .filter(|i| !i.separator)
            .map(|i| {
                let mut r = label_x(i) + measure(&i.label, &style_of(i)).0 as f32;
                if !i.accel.is_empty() {
                    r += measure(&i.accel, &style).0 as f32 + 28.0 * d;
                }
                if i.toggle.is_some() {
                    r += sw.0 + 20.0 * d;
                }
                if !i.trail.is_empty() {
                    r += i.trail.len() as f32 * (trail_px + 6.0 * d) + 14.0 * d;
                }
                r
            })
            .fold(80.0f32, f32::max);
        let w = (widest + pad + 12.0 * d).max(card.map_or(if rich { 270.0 } else { 170.0 }, |c| c.min_w) * d);
        let mut rows = Vec::new();
        let mut cy = pad;
        for it in items {
            let h = if it.separator {
                sep_h
            } else if it.title {
                34.0 * d
            } else if it.lead.is_some() {
                lead_d + 8.0 * d
            } else if it.heading {
                26.0 * d
            } else {
                row_h
            };
            rows.push((cy, h, it));
            cy += h;
        }
        let h = cy + pad;

        if let Ok(kids) = self.content.Children() {
            let _ = kids.RemoveAll();
        }
        let tex = |label: &str, st: &Style| -> Result<Option<(SpriteVisual, f32, f32)>> {
            let (tw, th) = measure(label, st);
            let tw = tw + 2;
            let Some(bmp) = render(label, st, tw, th) else { return Ok(None) };
            let surface = gfx.surface_from_bgra(bmp.w, bmp.h, &tint(&bmp.bgra, ink))?;
            let brush = comp.CreateSurfaceBrushWithSurface(&surface)?;
            brush.SetStretch(CompositionStretch::None)?;
            let s = comp.CreateSpriteVisual()?;
            s.SetBrush(&brush)?;
            s.SetSize(Vector2 { X: tw as f32, Y: th as f32 })?;
            Ok(Some((s, tw as f32, th as f32)))
        };
        let symbol = |sym: nacre_core::symbols::Sym, n: u32, rgb: [u8; 3]| -> Result<SpriteVisual> {
            let data = nacre_core::symbols::render(sym, n, rgb);
            let surface = gfx.surface_from_bgra(n, n, &data)?;
            let brush = comp.CreateSurfaceBrushWithSurface(&surface)?;
            brush.SetStretch(CompositionStretch::None)?;
            let s = comp.CreateSpriteVisual()?;
            s.SetBrush(&brush)?;
            s.SetSize(Vector2 { X: n as f32, Y: n as f32 })?;
            Ok(s)
        };
        let gray = Color { A: if look.dark { 46 } else { 30 }, R: ink[0], G: ink[1], B: ink[2] };
        // tiap baris di wadahnya sendiri: kartu memunculkannya berurutan seperti modul Control Center
        let mut boxes: Vec<(windows::UI::Composition::ContainerVisual, f32)> = Vec::new();
        for (ry, rh, it) in &rows {
            let rb = comp.CreateContainerVisual()?;
            self.content.Children()?.InsertAtTop(&rb)?;
            boxes.push((rb.clone(), ry + rh / 2.0));
            if it.separator {
                let line = comp.CreateSpriteVisual()?;
                line.SetSize(Vector2 { X: w - 2.0 * pad - 8.0 * d, Y: 1.0 })?;
                line.SetOffset(Vector3 { X: pad + 4.0 * d, Y: ry + rh / 2.0, Z: 0.0 })?;
                line.SetBrush(&comp.CreateColorBrushWithColor(Color { A: if look.dark { 36 } else { 46 }, R: ink[0], G: ink[1], B: ink[2] })?)?;
                rb.Children()?.InsertAtTop(&line)?;
                continue;
            }
            if let Some((s, _, th)) = tex(&it.label, &style_of(it))? {
                // judul bagian sedikit turun agar dekat dengan baris di bawahnya
                let dy = if it.heading { 3.0 * d } else { 0.0 };
                s.SetOffset(Vector3 { X: label_x(it), Y: ry + (rh - th) / 2.0 + dy, Z: 0.0 })?;
                s.SetOpacity(if it.heading { 0.5 } else if it.enabled { 1.0 } else { 0.38 })?;
                rb.Children()?.InsertAtTop(&s)?;
            }
            if let Some(sym) = it.sym {
                let n = (15.0 * d).round() as u32;
                let s = symbol(sym, n, ink)?;
                s.SetOffset(Vector3 { X: pad + 12.0 * d, Y: ry + (rh - n as f32) / 2.0, Z: 0.0 })?;
                s.SetOpacity(if it.enabled { 0.9 } else { 0.38 })?;
                rb.Children()?.InsertAtTop(&s)?;
            }
            if let Some((sym, on)) = it.lead {
                let (cx, cy) = (pad + 8.0 * d, ry + (rh - lead_d) / 2.0);
                let disc = rounded(comp, (cx, cy, lead_d, lead_d), lead_d / 2.0, if on { crate::prefs::accent_c(255) } else { gray })?;
                rb.Children()?.InsertAtTop(&disc)?;
                let n = (15.0 * d).round() as u32;
                let s = symbol(sym, n, if on { [255, 255, 255] } else { ink })?;
                s.SetOffset(Vector3 { X: (cx + (lead_d - n as f32) / 2.0).round(), Y: (cy + (lead_d - n as f32) / 2.0).round(), Z: 0.0 })?;
                s.SetOpacity(if on { 1.0 } else { 0.85 })?;
                rb.Children()?.InsertAtTop(&s)?;
            }
            if let Some(on) = it.toggle {
                let (tx, ty) = (w - pad - 10.0 * d - sw.0, ry + (rh - sw.1) / 2.0);
                let off = Color { A: if look.dark { 70 } else { 50 }, ..gray };
                let track = rounded(comp, (tx, ty, sw.0, sw.1), sw.1 / 2.0, if on { crate::prefs::accent_c(255) } else { off })?;
                rb.Children()?.InsertAtTop(&track)?;
                let k = sw.1 - 4.0 * d;
                let kx = if on { tx + sw.0 - 2.0 * d - k } else { tx + 2.0 * d };
                let knob = rounded(comp, (kx, ty + 2.0 * d, k, k), k / 2.0, Color { A: 255, R: 255, G: 255, B: 255 })?;
                rb.Children()?.InsertAtTop(&knob)?;
            }
            let mut rx = w - pad - 12.0 * d;
            for sym in it.trail.iter().rev() {
                let n = trail_px.round() as u32;
                let s = symbol(*sym, n, ink)?;
                rx -= n as f32;
                s.SetOffset(Vector3 { X: rx.round(), Y: (ry + (rh - n as f32) / 2.0).round(), Z: 0.0 })?;
                s.SetOpacity(0.7)?;
                rb.Children()?.InsertAtTop(&s)?;
                rx -= 6.0 * d;
            }
            if !it.accel.is_empty() {
                if let Some((s, aw, ah)) = tex(&it.accel, &style)? {
                    s.SetOffset(Vector3 { X: w - pad - 12.0 * d - aw, Y: ry + (rh - ah) / 2.0, Z: 0.0 })?;
                    s.SetOpacity(if it.enabled { 0.55 } else { 0.3 })?;
                    rb.Children()?.InsertAtTop(&s)?;
                }
            }
        }
        let (wi, hi) = (w.ceil() as i32, h.ceil() as i32);
        let anchor = x;
        let x = if right_aligned { x - wi } else { x };
        let px = x.clamp(4, (self.screen.0 - wi - 4).max(4));
        let py = if above { (y - hi).clamp(4, (self.screen.1 - hi - 4).max(4)) } else { y.clamp(0, (self.screen.1 - hi - 4).max(0)) };
        self.popup.set_rect(px, py, wi, hi)?;
        self.popup.panel.set_radius(radius)?;
        self.highlight.SetOpacity(0.0)?;
        self.highlight.SetBrush(&comp.CreateColorBrushWithColor(crate::prefs::accent_c(235))?)?;
        LIVE.with(|c| {
            *c.borrow_mut() = Some(Live { rows, size: (w, h), hover: None, highlight: self.highlight.clone(), pad, hl_max: card.map(|_| (radius - pad).max(6.0 * d)), owner_hides: card.is_some() });
        });
        if refresh {
            // pertahankan sorotan di bawah kursor setelah digambar ulang
            let mut pt = windows::Win32::Foundation::POINT::default();
            unsafe {
                let _ = GetCursorPos(&mut pt);
            }
            let (cx, cy) = ((pt.x - px) as f32, (pt.y - py) as f32);
            LIVE.with(|c| {
                if let Some(live) = c.borrow_mut().as_mut() {
                    let hit = row_at(live, cx, cy).filter(|i| live.rows[*i].2.toggle.is_none());
                    set_hover(live, hit);
                }
            });
        } else if card.is_some() {
            // animasi Control Center: kartu tumbuh dari jangkar (pojok kanan atas, atau ikon menu bar) dengan pantulan kecil,
            // lalu barisnya muncul berurutan; jeda awal pendek agar frame pertama sudah tampil (jendela baru)
            let ax = if right_aligned { wi as f32 - 24.0 * d } else { ((anchor - px) as f32 + 12.0 * d).clamp(0.0, wi as f32) };
            self.pop_at = Vector3 { X: ax, Y: 0.0, Z: 0.0 };
            self.popup.show();
            let _ = crate::motion::pop_in(comp, self.popup.root(), self.pop_at, 0.45, 1.025, 40, crate::motion::CC_OPEN_MS);
            for (i, (rb, cy)) in boxes.iter().enumerate() {
                let _ = crate::motion::pop_in(comp, rb, Vector3 { X: w / 2.0, Y: *cy, Z: 0.0 }, 0.7, 1.04, 80 + i as u32 * crate::motion::CC_STAGGER_MS, 360);
            }
        } else {
            self.popup.show();
            self.popup.fade_in(comp, !above);
        }
        // Klik di luar menu menutupnya: hook mouse dipasang hanya selama menu terbuka.
        nacre_platform::input::begin_dismiss((px, py, px + wi, py + hi), self.target.0 as isize, MENU_CLOSE);
        Ok(())
    }

    /// Naikkan ke atas jendela topmost lain (menu yang dibuka dari panel Control Center).
    pub fn raise(&self) {
        unsafe {
            let _ = SetWindowPos(self.popup.hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }

    pub fn hide(&self) {
        nacre_platform::input::end_dismiss();
        unsafe {
            let _ = KillTimer(Some(self.popup.hwnd), T_HIDE);
        }
        self.popup.hide();
    }

    pub fn is_open(&self) -> bool {
        self.popup.is_visible() && !self.closing
    }
}

