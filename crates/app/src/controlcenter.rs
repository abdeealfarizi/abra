//! Control Center ala macOS Tahoe: grid modul Liquid Glass (Wi-Fi, Bluetooth, Now Playing, tombol bulat, slider
//! Layar/Suara) di kanan atas, dengan mode Edit (badge "−", slot kosong, galeri "Cari Kontrol", tambah/hapus/tata ulang).
//! Dibuat saat dibuka dan dihancurkan saat ditutup; susunan disimpan di `cc_layout.txt`.

use crate::popup::Popup;
use crate::{motion, paths, prefs, services};
use crate::dock::menu::{Item as MItem, Menu, MENU_CLOSE, MENU_PICK};
use nacre_core::controls::{self, Category, Kind, Placed, Size};
use nacre_core::symbols::{self, Sym};
use nacre_platform::icons::Bitmap;
use nacre_platform::media::{self, ArtUpdate, Command};
use nacre_platform::radio::{self, Cmd as RadioCmd, Kind as RK};
use nacre_platform::text::{measure, render, render_face, Style, ICON_FACE};
use nacre_platform::{audio, brightness, keys, launch, wifi};
use nacre_ui::glass::{GlassPanel, GlassParams, Tier};
use nacre_ui::gfx::Gfx;
use std::cell::RefCell;
use std::collections::HashMap;
use windows::core::w;
use windows::UI::Color;
use windows::UI::Composition::{CompositionDrawingSurface, CompositionStretch, Compositor, ContainerVisual, SpriteVisual};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, VK_BACK, VK_ESCAPE};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows_core::Interface;
use windows_numerics::{Vector2, Vector3};

const WM_CLOSE_CC: u32 = WM_APP + 130;
const WM_RADIO: u32 = WM_APP + 131;
/// Id tombol piringan ikon modul (Wi-Fi/Bluetooth: membalik radio).
const BTN_DISC: u8 = 20;
const WM_MEDIA: u32 = WM_APP + 132;
const WM_VALUES: u32 = WM_APP + 133;

type Rect = (f32, f32, f32, f32);

fn r_sep_visible(cy: f32, gh: f32, dpi: f32) -> bool {
    cy > 0.0 && cy < gh - 60.0 * dpi
}

fn hit(r: Rect, x: f32, y: f32) -> bool {
    x >= r.0 && x < r.0 + r.2 && y >= r.1 && y < r.1 + r.3
}

fn glyph_of(k: Kind) -> &'static str {
    match k {
        Kind::Wifi => "\u{E701}",
        Kind::Bluetooth => "\u{E702}",
        Kind::Hotspot => "\u{E704}",
        Kind::Share => "\u{E72D}",
        Kind::Airplane => "\u{E709}",
        Kind::NowPlaying => "\u{E8D6}",
        Kind::Stage => "\u{E8A0}",
        Kind::Mirror => "\u{EC15}",
        Kind::Focus => "\u{E708}",
        Kind::DarkMode => "\u{E790}",
        Kind::Screenshot => "\u{E722}",
        Kind::Display => "\u{E706}",
        Kind::Sound => "\u{E767}",
        Kind::Battery => "\u{E83F}",
        Kind::Lock => "\u{E72E}",
        Kind::Mission => "\u{E7C4}",
        Kind::Apps => "\u{E71D}",
        Kind::Calculator => "\u{E8EF}",
        Kind::Timer => "\u{E916}",
    }
}

struct Mod {
    placed: Placed,
    rect: Rect,
    /// Slider: area jalur dan sprite isian.
    track: Option<Rect>,
    fill: Option<SpriteVisual>,
    /// Slider: pil kenop kaca permanen di ujung isian (lapisan luar + inti).
    thumb: Vec<SpriteVisual>,
    /// Now Playing: tombol prev/play/next.
    btns: Vec<(Rect, u8)>,
    badge: Rect,
}

struct Panel {
    popup: Popup,
    dpi: f32,
    cell: f32,
    gap: f32,
    mods: Vec<Mod>,
    slots: Vec<Rect>,
    edit_btn: Rect,
    panels: Vec<GlassPanel>,
    drag: Option<(usize, f32, f32)>,
    slider: Option<usize>,
    w: f32,
    /// Chip "app sedang memakai mikrofon/kamera" di atas modul (true = hanya kamera).
    chip: Option<(Rect, bool)>,
}

struct Gallery {
    popup: Popup,
    dpi: f32,
    cat: Option<Category>,
    query: String,
    cards: Vec<(Kind, Rect)>,
    cats: Vec<(Option<Category>, Rect)>,
    done: Rect,
    scroll: f32,
    content_h: f32,
    panels: Vec<GlassPanel>,
}

struct Cc {
    ctl: HWND,
    comp: Compositor,
    gfx: Option<Gfx>,
    layout: Vec<Kind>,
    sizes: HashMap<Kind, Size>,
    /// Menu ukuran (klik kanan di mode Edit) dan modul yang sedang dipilih.
    menu: Option<Menu>,
    menu_kind: Option<Kind>,
    edit: bool,
    panel: Option<Panel>,
    gallery: Option<Gallery>,
    radios: Option<radio::Radios>,
    wifi: Option<bool>,
    bt: Option<bool>,
    ssid: Option<String>,
    media: Option<media::Media>,
    track: Option<(String, String, bool, bool)>, // judul, artis, ada media, memutar
    art: Option<Bitmap>,
    cache: HashMap<String, (CompositionDrawingSurface, i32, i32)>,
    top: i32,
    /// Build berikutnya memainkan animasi masuk (hanya saat panel baru dibuka).
    intro: bool,
    /// Modul yang menunggu animasi masuk: (kontainer, titik tumpu, jeda ms). Dimulai setelah jendela tampil.
    intro_items: Vec<(ContainerVisual, Vector3, u32)>,
    /// Animasi tutup sedang berjalan; jendela dihancurkan oleh timer.
    closing: bool,
}

const T_CLOSE: usize = 1;
/// Memulai animasi masuk sedikit setelah jendela tampil, supaya tidak habis sebelum frame pertama (bug buka pertama).
const T_INTRO: usize = 2;

thread_local! {
    static CC: RefCell<Option<Cc>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut Cc) -> R) -> Option<R> {
    CC.with(|c| c.try_borrow_mut().ok().and_then(|mut b| b.as_mut().map(f)))
}

fn layout_file() -> std::path::PathBuf {
    paths::config_dir().join("cc_layout.txt")
}

pub fn init(comp: &Compositor) -> windows_core::Result<()> {
    unsafe {
        let hi: HINSTANCE = GetModuleHandleW(None)?.into();
        let class = w!("AbraCcCtl");
        RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(ctl_proc), hInstance: hi, lpszClassName: class, ..Default::default() });
        let hwnd = CreateWindowExW(WINDOW_EX_STYLE(0), class, w!("abra-cc-ctl"), WS_POPUP, 0, 0, 0, 0, Some(HWND_MESSAGE), None, Some(hi), None)?;
        let (layout, sizes) = controls::from_text_sized(&std::fs::read_to_string(layout_file()).unwrap_or_default());
        CC.with(|c| {
            *c.borrow_mut() = Some(Cc {
                ctl: hwnd,
                comp: comp.clone(),
                gfx: None,
                layout,
                sizes,
                menu: None,
                menu_kind: None,
                edit: false,
                panel: None,
                gallery: None,
                radios: None,
                wifi: None,
                bt: None,
                ssid: None,
                media: None,
                track: None,
                art: None,
                cache: HashMap::new(),
                top: 30,
                intro: false,
                intro_items: Vec::new(),
                closing: false,
            })
        });
    }
    Ok(())
}

/// Buka/tutup panel; `top` = tepi bawah menu bar (px layar).
pub fn toggle(top: i32) {
    let open = with(|c| c.panel.is_some() && !c.closing).unwrap_or(false);
    if open {
        close();
    } else {
        with(|c| {
            c.finish_close();
            c.open(top)
        });
    }
}

pub fn is_open() -> bool {
    with(|c| c.panel.is_some() && !c.closing).unwrap_or(false)
}

pub fn close() {
    with(|c| c.close());
}

/// Khusus uji (`NACRE_TEST_NETMENU=cc`): buka pemilih dari modul Wi-Fi seperti diklik.
pub fn test_open_picker() {
    with(|c| {
        let i = c.panel.as_ref().and_then(|p| p.mods.iter().position(|m| m.placed.kind == Kind::Wifi));
        if let Some(i) = i {
            c.open_picker(i);
        }
    });
}

/// Minta keadaan radio terbaru (sakelar dibalik dari menu Wi-Fi/Bluetooth).
pub fn refresh_radios() {
    with(|c| {
        if let Some(r) = &c.radios {
            let _ = r.commands.send(RadioCmd::Refresh);
        }
    });
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

const WHITE: [u8; 3] = [255, 255, 255];

impl Cc {
    fn save_layout(&self) {
        let _ = std::fs::write(layout_file(), controls::to_text_sized(&self.layout, &self.sizes));
    }

    fn open(&mut self, top: i32) {
        self.top = top;
        self.edit = false;
        if self.gfx.is_none() {
            self.gfx = Gfx::new(&self.comp).ok();
        }
        let ctl = self.ctl.0 as isize;
        if self.radios.is_none() {
            self.radios = Some(radio::spawn(move || unsafe {
                let _ = PostMessageW(Some(HWND(ctl as *mut _)), WM_RADIO, WPARAM(0), LPARAM(0));
            }));
        }
        if self.media.is_none() && self.layout.contains(&Kind::NowPlaying) {
            self.media = Some(media::spawn(move || unsafe {
                let _ = PostMessageW(Some(HWND(ctl as *mut _)), WM_MEDIA, WPARAM(0), LPARAM(0));
            }));
        }
        self.ssid = wifi::ssid();
        self.intro = true;
        self.build();
        self.intro = false;
        if let Some(p) = &self.panel {
            // Tampilkan dulu dalam keadaan tak terlihat, lalu mulai animasi sebentar lagi. Membuka pertama kali memakan
            // waktu (jendela dan permukaan baru, font, kaca); bila animasi dimulai sebelum frame pertama tampil, ia sudah
            // habis saat panel akhirnya terlihat sehingga tampak muncul begitu saja.
            let _ = p.popup.root().SetOpacity(0.0);
            p.popup.show();
        }
        unsafe {
            SetTimer(Some(self.ctl), T_INTRO, 60, None);
        }
    }

    /// Mulai animasi masuk panel dan modul (dipanggil timer `T_INTRO`).
    fn start_intro(&mut self) {
        unsafe {
            let _ = KillTimer(Some(self.ctl), T_INTRO);
        }
        let items = std::mem::take(&mut self.intro_items);
        if self.closing {
            return;
        }
        let Some(p) = self.panel.as_ref() else { return };
        // panel "tumbuh" dari ikon Control Center (kanan atas) dengan pantulan kecil
        let _ = motion::pop_in(&self.comp, p.popup.root(), Vector3 { X: p.w - 24.0 * p.dpi, Y: 0.0, Z: 0.0 }, 0.45, 1.025, 0, motion::CC_OPEN_MS);
        for (v, center, delay) in items {
            let _ = motion::pop_in(&self.comp, &v, center, 0.7, 1.04, delay, 360);
        }
    }

    /// Mulai animasi tutup; jendela dihancurkan oleh timer (`finish_close`).
    fn close(&mut self) {
        if self.closing || self.panel.is_none() {
            return;
        }
        nacre_platform::input::end_dismiss();
        unsafe {
            let _ = KillTimer(Some(self.ctl), T_INTRO);
        }
        self.intro_items.clear();
        self.edit = false;
        if let Some(g) = self.gallery.take() {
            let h = g.popup.hwnd;
            drop(g);
            unsafe {
                let _ = DestroyWindow(h);
            }
        }
        let Some(p) = self.panel.as_ref() else { return };
        if motion::pop_out(&self.comp, p.popup.root(), Vector3 { X: p.w - 24.0 * p.dpi, Y: 0.0, Z: 0.0 }, 0.55, motion::CC_CLOSE_MS).is_ok() {
            self.closing = true;
            unsafe {
                SetTimer(Some(self.ctl), T_CLOSE, motion::CC_CLOSE_MS + 20, None);
            }
        } else {
            self.finish_close();
        }
    }

    fn finish_close(&mut self) {
        unsafe {
            let _ = KillTimer(Some(self.ctl), T_CLOSE);
        }
        self.closing = false;
        self.edit = false;
        if let Some(p) = self.panel.take() {
            let h = p.popup.hwnd;
            drop(p);
            unsafe {
                let _ = DestroyWindow(h);
            }
        }
    }

    // -----------------------------------------------------------------------------------------------------------
    // gambar bantu
    // -----------------------------------------------------------------------------------------------------------

    fn text_surf(&mut self, text: &str, px: i32, weight: i32, rgb: [u8; 3], maxw: i32) -> Option<(CompositionDrawingSurface, i32, i32)> {
        let key = format!("t|{px}|{weight}|{rgb:?}|{maxw}|{text}");
        if let Some(v) = self.cache.get(&key) {
            return Some(v.clone());
        }
        let style = Style { px, weight };
        let (tw, th) = measure(text, &style);
        let tw = (tw + 2).min(maxw.max(8));
        let bmp = render(text, &style, tw, th)?;
        let s = self.gfx.as_ref()?.surface_from_bgra(bmp.w, bmp.h, &recolor(&bmp, rgb)).ok()?;
        let v = (s, tw, th);
        self.cache.insert(key, v.clone());
        Some(v)
    }

    fn glyph_surf(&mut self, g: &str, px: i32, rgb: [u8; 3]) -> Option<(CompositionDrawingSurface, i32, i32)> {
        let key = format!("g|{px}|{rgb:?}|{g}");
        if let Some(v) = self.cache.get(&key) {
            return Some(v.clone());
        }
        let b = (px as f32 * 1.6) as i32;
        let bmp = render_face(g, &Style { px, weight: 400 }, ICON_FACE, b, b)?;
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
            return None;
        }
        let (gw, gh) = (x1 - x0 + 1, y1 - y0 + 1);
        let rec = recolor(&bmp, rgb);
        let mut data = vec![0u8; gw * gh * 4];
        for y in 0..gh {
            for x in 0..gw {
                let s = ((y0 + y) * w + x0 + x) * 4;
                let d = (y * gw + x) * 4;
                data[d..d + 4].copy_from_slice(&rec[s..s + 4]);
            }
        }
        let s = self.gfx.as_ref()?.surface_from_bgra(gw as u32, gh as u32, &data).ok()?;
        let v = (s, gw as i32, gh as i32);
        self.cache.insert(key, v.clone());
        Some(v)
    }

    /// Simbol vektor (set yang sama dengan menu bar) berpusat pada tinta di (`cx`,`cy`).
    fn symbol(&mut self, parent: &ContainerVisual, s: Sym, px: i32, rgb: [u8; 3], cx: f32, cy: f32, op: f32) {
        let n = px.max(8) as u32;
        let key = format!("sym|{s:?}|{n}|{rgb:?}");
        let hit = self.cache.get(&key).cloned();
        let (surf, w, h) = match hit {
            Some(v) => v,
            None => {
                let data = symbols::render(s, n, rgb);
                let Some((cd, cw, ch)) = symbols::crop_ink(&data, n, n) else { return };
                let Some(Ok(sf)) = self.gfx.as_ref().map(|g| g.surface_from_bgra(cw, ch, &cd)) else { return };
                let v = (sf, cw as i32, ch as i32);
                self.cache.insert(key, v.clone());
                v
            }
        };
        self.put(parent, &surf, cx - w as f32 / 2.0, cy - h as f32 / 2.0, w as f32, h as f32, op);
    }

    fn put(&self, parent: &ContainerVisual, s: &CompositionDrawingSurface, x: f32, y: f32, w: f32, h: f32, op: f32) {
        let go = || -> windows_core::Result<()> {
            let b = self.comp.CreateSurfaceBrushWithSurface(s)?;
            b.SetStretch(CompositionStretch::None)?;
            let v = self.comp.CreateSpriteVisual()?;
            v.SetBrush(&b)?;
            v.SetSize(Vector2 { X: w, Y: h })?;
            v.SetOffset(Vector3 { X: x, Y: y, Z: 0.0 })?;
            v.SetOpacity(op)?;
            parent.Children()?.InsertAtTop(&v)
        };
        let _ = go();
    }

    fn text(&mut self, parent: &ContainerVisual, t: &str, px: i32, weight: i32, rgb: [u8; 3], x: f32, y: f32, maxw: f32, op: f32) {
        if let Some((s, w, h)) = self.text_surf(t, px, weight, rgb, maxw as i32) {
            self.put(parent, &s, x, y, w as f32, h as f32, op);
        }
    }

    fn glyph(&mut self, parent: &ContainerVisual, g: &str, px: i32, rgb: [u8; 3], cx: f32, cy: f32, op: f32) {
        if let Some((s, w, h)) = self.glyph_surf(g, px, rgb) {
            self.put(parent, &s, cx - w as f32 / 2.0, cy - h as f32 / 2.0, w as f32, h as f32, op);
        }
    }

    fn rrect(&self, parent: &ContainerVisual, r: Rect, radius: f32, c: Color) -> Option<SpriteVisual> {
        let go = || -> windows_core::Result<SpriteVisual> {
            let v = self.comp.CreateSpriteVisual()?;
            v.SetSize(Vector2 { X: r.2, Y: r.3 })?;
            v.SetOffset(Vector3 { X: r.0, Y: r.1, Z: 0.0 })?;
            v.SetBrush(&self.comp.CreateColorBrushWithColor(c)?)?;
            let g = self.comp.CreateRoundedRectangleGeometry()?;
            g.SetSize(Vector2 { X: r.2, Y: r.3 })?;
            g.SetCornerRadius(Vector2 { X: radius, Y: radius })?;
            let clip = self.comp.CreateGeometricClip()?;
            clip.SetGeometry(&g)?;
            v.SetClip(&clip)?;
            parent.Children()?.InsertAtTop(&v)?;
            Ok(v)
        };
        go().ok()
    }

    fn glass(&self, parent: &ContainerVisual, r: Rect, radius: f32) -> Option<GlassPanel> {
        let look = prefs::look(prefs::Role::Dark);
        let base = GlassParams::dark((r.2, r.3), radius);
        let p = GlassPanel::new(&self.comp, parent, prefs::apply(base, &look)).ok()?;
        let _ = p.set_rect(r.0, r.1, r.2, r.3);
        let _ = p.set_radius(radius);
        Some(p)
    }

    // -----------------------------------------------------------------------------------------------------------
    // status modul
    // -----------------------------------------------------------------------------------------------------------

    fn is_on(&self, k: Kind) -> bool {
        match k {
            Kind::Wifi => self.wifi == Some(true),
            Kind::Bluetooth => self.bt == Some(true),
            Kind::Stage => prefs::get().stage,
            Kind::DarkMode => prefs::look(prefs::Role::Dark).dark && prefs::get().theme == nacre_core::settings::Theme::Dark,
            _ => false,
        }
    }

    fn status(&self, k: Kind) -> String {
        let on = prefs::tr("Aktif");
        let off = prefs::tr("Mati");
        match k {
            Kind::Wifi => match (self.wifi, &self.ssid) {
                (Some(true), Some(s)) => s.clone(),
                (Some(true), None) => on.to_string(),
                (Some(false), _) => off.to_string(),
                _ => "-".to_string(),
            },
            Kind::Bluetooth => match self.bt {
                Some(true) => on.to_string(),
                Some(false) => off.to_string(),
                _ => "-".to_string(),
            },
            Kind::Hotspot | Kind::Share => prefs::tr("Buka Pengaturan").to_string(),
            Kind::Battery => match nacre_platform::sys::battery() {
                Some((p, ch)) => format!("{p}%{}", if ch { " \u{26A1}" } else { "" }),
                None => "-".to_string(),
            },
            Kind::Focus => prefs::tr("Fokus").to_string(),
            _ => String::new(),
        }
    }

    // -----------------------------------------------------------------------------------------------------------
    // panel
    // -----------------------------------------------------------------------------------------------------------

    fn build(&mut self) {
        // hancurkan isi lama (popup dipertahankan bila sudah ada)
        let old = self.panel.take();
        let dpi;
        let popup = match old {
            Some(mut p) => {
                if let Ok(k) = p.popup.content.Children() {
                    let _ = k.RemoveAll();
                }
                p.panels.clear();
                dpi = p.dpi;
                p.popup
            }
            None => {
                let base = GlassParams::dark((10.0, 10.0), 0.0);
                let Ok(popup) = Popup::new(&self.comp, w!("AbraControlCenter"), Some(panel_proc), WINDOW_EX_STYLE(0), base) else { return };
                dpi = unsafe { GetDpiForWindow(popup.hwnd) }.max(96) as f32 / 96.0;
                popup
            }
        };
        let mut popup = popup;
        let (cell, gap) = (82.0 * dpi, 14.0 * dpi);
        let sizes = self.sizes.clone();
        let (placed, rows) = controls::layout_sized(&self.layout, &|k| *sizes.get(&k).unwrap_or(&k.default_size()));
        let rows = if self.edit { rows + 2 } else { rows };
        let (gw, gh) = (4.0 * cell + 3.0 * gap, rows as f32 * cell + (rows.max(1) - 1) as f32 * gap);
        let btn_h = 34.0 * dpi;
        // Tahoe: app yang sedang memakai mikrofon/kamera tampil sebagai chip di atas modul
        let inuse = if self.edit { Vec::new() } else { nacre_platform::privacy::active() };
        let top = if inuse.is_empty() { 0.0 } else { 36.0 * dpi };
        // tombol Edit Kontrol berada 14 dp di bawah grid; sisakan 4 dp agar tepi bawah kapsulnya tidak terpotong jendela
        let (pw, ph) = (gw, top + gh + 14.0 * dpi + btn_h + 4.0 * dpi);
        let (sw, _) = nacre_platform::sys::screen_size();
        let x = (sw as f32 - pw - 10.0 * dpi) as i32;
        let y = self.top + (6.0 * dpi) as i32;
        let _ = popup.set_rect(x, y, pw.ceil() as i32, ph.ceil() as i32);
        let _ = popup.panel.set_rect(0.0, 0.0, 0.0, 0.0);
        let content = popup.content.clone();

        let mut mods = Vec::new();
        let mut panels = Vec::new();
        let mut groups: Vec<(ContainerVisual, Rect)> = Vec::new();
        let mut occupied: Vec<(usize, usize)> = Vec::new();
        for p in &placed {
            let r: Rect = (p.col as f32 * (cell + gap), top + p.row as f32 * (cell + gap), p.w as f32 * cell + (p.w - 1) as f32 * gap, p.h as f32 * cell + (p.h - 1) as f32 * gap);
            for dy in 0..p.h {
                for dx in 0..p.w {
                    occupied.push((p.col + dx, p.row + dy));
                }
            }
            // Tahoe: 1×1 bulat, 2×1 kapsul, sedangkan kartu 2×2 dan slider lebar memakai sudut yang sama (±0,34 sel)
            let radius = if p.w == 1 && p.h == 1 { r.2 / 2.0 } else if p.h == 2 || p.w >= 3 { CARD_R * cell } else { r.3 / 2.0 };
            // tiap modul punya kontainer sendiri agar bisa dianimasikan bertahap saat panel dibuka
            let mc = self.comp.CreateContainerVisual().ok();
            if let Some(mc) = &mc {
                let _ = content.Children().and_then(|k| k.InsertAtTop(mc));
                groups.push((mc.clone(), r));
            }
            let parent = mc.as_ref().unwrap_or(&content);
            if let Some(g) = self.glass(parent, r, radius) {
                panels.push(g);
            }
            let m = self.draw_module(parent, p, r, dpi, radius);
            mods.push(m);
        }
        // slot kosong (mode edit): lingkaran transparan seperti referensi
        let mut slots = Vec::new();
        if self.edit {
            for row in 0..rows {
                for col in 0..4 {
                    if occupied.contains(&(col, row)) {
                        continue;
                    }
                    let r: Rect = (col as f32 * (cell + gap), row as f32 * (cell + gap), cell, cell);
                    let _ = self.rrect(&content, r, cell / 2.0, Color { A: 40, R: 255, G: 255, B: 255 });
                    slots.push(r);
                }
            }
        }
        // tombol Edit Kontrol / Selesai
        let label = if self.edit { prefs::tr("Selesai") } else { prefs::tr("Edit Kontrol…") };
        let bw = 128.0 * dpi;
        let bx = (pw - bw) / 2.0;
        let edit_btn: Rect = (bx, top + gh + 14.0 * dpi, bw, btn_h);
        let ec = self.comp.CreateContainerVisual().ok();
        if let Some(ec) = &ec {
            let _ = content.Children().and_then(|k| k.InsertAtTop(ec));
            groups.push((ec.clone(), edit_btn));
        }
        let eparent = ec.as_ref().unwrap_or(&content);
        if let Some(g) = self.glass(eparent, edit_btn, btn_h / 2.0) {
            panels.push(g);
        }
        let (lw, _) = measure(label, &Style { px: (13.0 * dpi) as i32, weight: 600 });
        self.text(eparent, label, (13.0 * dpi) as i32, 600, WHITE, bx + ((bw - lw as f32) / 2.0).max(8.0 * dpi), edit_btn.1 + 8.0 * dpi, bw - 16.0 * dpi, 1.0);
        let chip = self.draw_chip(&content, &inuse, pw, dpi, &mut groups, &mut panels);
        self.intro_items.clear();
        if self.intro {
            // stagger: modul paling dekat ikon (kanan atas) muncul lebih dulu. Animasinya baru dimulai oleh `start_intro`
            // (setelah jendela tampil); sampai saat itu modul disembunyikan agar tidak berkedip penuh.
            let icon = (pw - 24.0 * dpi, 0.0f32);
            let mut order: Vec<usize> = (0..groups.len()).collect();
            let dist = |r: &Rect| ((r.0 + r.2 / 2.0 - icon.0).powi(2) + (r.1 + r.3 / 2.0 - icon.1).powi(2)).sqrt();
            order.sort_by(|a, b| dist(&groups[*a].1).total_cmp(&dist(&groups[*b].1)));
            for (rank, i) in order.into_iter().enumerate() {
                let (v, r) = &groups[i];
                let center = Vector3 { X: r.0 + r.2 / 2.0, Y: r.1 + r.3 / 2.0, Z: 0.0 };
                let _ = v.SetOpacity(0.0);
                self.intro_items.push((v.clone(), center, 40 + rank as u32 * motion::CC_STAGGER_MS));
            }
        }

        let hwnd = popup.hwnd;
        let rect = popup.rect;
        self.panel = Some(Panel { popup, dpi, cell, gap, mods, slots, edit_btn, panels, drag: None, slider: None, w: pw, chip });
        if self.edit {
            nacre_platform::input::end_dismiss();
        } else {
            nacre_platform::input::begin_dismiss((rect.0, rect.1, rect.0 + rect.2, rect.1 + rect.3), hwnd.0 as isize, WM_CLOSE_CC);
        }
    }

    /// Chip kaca kanan atas: titik oranye (mikrofon) / hijau (kamera) + nama app, seperti indikator privasi Tahoe.
    fn draw_chip(&mut self, content: &ContainerVisual, inuse: &[nacre_platform::privacy::InUse], pw: f32, dpi: f32, groups: &mut Vec<(ContainerVisual, Rect)>, panels: &mut Vec<GlassPanel>) -> Option<(Rect, bool)> {
        if inuse.is_empty() {
            return None;
        }
        let mut apps: Vec<&str> = Vec::new();
        for u in inuse {
            if !apps.contains(&u.app.as_str()) {
                apps.push(&u.app);
            }
        }
        let (mic, cam) = (inuse.iter().any(|u| !u.camera), inuse.iter().any(|u| u.camera));
        let label = apps.iter().take(2).copied().collect::<Vec<_>>().join(", ");
        let style = Style { px: (12.5 * dpi) as i32, weight: 600 };
        let (lw, _) = measure(&label, &style);
        let ch = 28.0 * dpi;
        let dots = (mic as u32 + cam as u32) as f32 * 13.0 * dpi;
        let cw = (12.0 * dpi + dots + 3.0 * dpi + lw as f32 + 14.0 * dpi).min(pw);
        let cr: Rect = (pw - cw, 0.0, cw, ch);
        let cc = self.comp.CreateContainerVisual().ok()?;
        let _ = content.Children().and_then(|k| k.InsertAtTop(&cc));
        groups.push((cc.clone(), cr));
        if let Some(g) = self.glass(&cc, cr, ch / 2.0) {
            panels.push(g);
        }
        let mut x = cr.0 + 12.0 * dpi;
        let d = 8.0 * dpi;
        for (on, col) in [(mic, Color { A: 255, R: 255, G: 149, B: 0 }), (cam, Color { A: 255, R: 52, G: 199, B: 89 })] {
            if on {
                let _ = self.rrect(&cc, (x, cr.1 + (ch - d) / 2.0, d, d), d / 2.0, col);
                x += 13.0 * dpi;
            }
        }
        self.text(&cc, &label, style.px, 600, WHITE, x + 3.0 * dpi, cr.1 + 6.0 * dpi, lw as f32 + 4.0, 1.0);
        Some((cr, cam && !mic))
    }

    fn draw_module(&mut self, c: &ContainerVisual, p: &Placed, r: Rect, dpi: f32, radius: f32) -> Mod {
        let k = p.kind;
        let on = self.is_on(k);
        let px = |v: f32| (v * dpi) as i32;
        let mut m = Mod { placed: *p, rect: r, track: None, fill: None, thumb: vec![], btns: vec![], badge: (r.0 - 6.0 * dpi, r.1 - 6.0 * dpi, 24.0 * dpi, 24.0 * dpi) };
        let _ = radius;
        match k {
            Kind::Display | Kind::Sound => {
                // Proporsi diukur dari Control Center Tahoe (tinggi kartu = 1 sel): judul 15,5 px dengan jarak lega dari
                // sudut, jalur 10 dp berpusat di 65% tinggi, kenop pil 42×28, tombol keluaran Ø36 di kanan Sound.
                let title = prefs::tr(k.title());
                self.text(c, title, px(15.5), 600, WHITE, r.0 + 20.0 * dpi, r.1 + 12.0 * dpi, r.2 - 40.0 * dpi, 0.95);
                let th = 10.0 * dpi;
                // Sound: tombol bulat keluaran audio di kanan (memilih speaker/headphone)
                let out = if k == Kind::Sound && r.2 > 250.0 * dpi { 36.0 * dpi } else { 0.0 };
                let right = if out > 0.0 { 108.0 * dpi } else { 40.0 * dpi };
                let cy = r.1 + r.3 * 0.65;
                let (tx, ty) = (r.0 + 42.0 * dpi, cy - th / 2.0);
                let tw = r.2 - 42.0 * dpi - right;
                let _ = self.rrect(c, (tx, ty, tw, th), th / 2.0, Color { A: 45, R: 255, G: 255, B: 255 });
                let frac = if k == Kind::Display { brightness::latest().map(|v| v as f32 / 100.0).unwrap_or(0.5) } else { audio::latest().level };
                let (kw, kh) = (KNOB.0 * dpi, KNOB.1 * dpi);
                let kx = (tx + tw * frac).clamp(tx + kw / 2.0, tx + tw - kw / 2.0);
                let fill = self.rrect(c, (tx, ty, (kx - tx).max(th), th), th / 2.0, Color { A: 240, R: 255, G: 255, B: 255 });
                let (lo, hi, lo_px, hi_px) = if k == Kind::Display { (Sym::SunSmall, Sym::Sun, 15, 19) } else { (Sym::Speaker, Sym::SpeakerWave, 19, 25) };
                self.symbol(c, lo, px(lo_px as f32), WHITE, r.0 + 23.0 * dpi, cy, 0.85);
                let hi_x = if out > 0.0 { r.0 + r.2 - 86.0 * dpi } else { r.0 + r.2 - 21.0 * dpi };
                self.symbol(c, hi, px(hi_px as f32), WHITE, hi_x, cy, 0.85);
                if out > 0.0 {
                    let b: Rect = (r.0 + r.2 - 39.0 * dpi - out / 2.0, cy - out / 2.0, out, out);
                    let _ = self.rrect(c, b, out / 2.0, Color { A: 45, R: 255, G: 255, B: 255 });
                    self.symbol(c, Sym::AirPlay, px(19.0), WHITE, b.0 + out / 2.0, cy, 0.9);
                    m.btns.push((b, 10));
                }
                // kenop "tetes air": bayangan tipis, pil putih padat, dan inti kaca (tampak saat diseret)
                let mut thumb = Vec::new();
                let sh = 1.0 * dpi;
                let [s0, s1, s2] = DROP_IDLE;
                for (inset, col) in [(-sh, s0), (0.0, s1), (1.5 * dpi, s2)] {
                    let (w, h) = (kw - 2.0 * inset, kh - 2.0 * inset);
                    if let Some(v) = self.rrect(c, (kx - w / 2.0, cy - h / 2.0, w, h), h / 2.0, col) {
                        thumb.push(v);
                    }
                }
                m.thumb = thumb;
                m.track = Some((tx, ty - 10.0 * dpi, tw, th + 20.0 * dpi));
                m.fill = fill;
            }
            Kind::NowPlaying => {
                let (title, artist, has, playing) = self.track.clone().unwrap_or_default();
                if !has {
                    self.glyph(c, "\u{E8D6}", px(26.0), WHITE, r.0 + r.2 / 2.0, r.1 + r.3 / 2.0 - 10.0 * dpi, 0.6);
                    self.text(c, prefs::tr("Tidak diputar"), px(12.0), 500, WHITE, r.0 + 16.0 * dpi, r.1 + r.3 / 2.0 + 14.0 * dpi, r.2 - 20.0 * dpi, 0.7);
                } else {
                    let art = (r.2 * 0.24).min(40.0 * dpi);
                    if let (Some(a), Some(gfx)) = (self.art.clone(), self.gfx.as_ref()) {
                        if let Ok(s) = gfx.surface_from_bgra(a.w, a.h, &a.bgra) {
                            if let Ok(b) = self.comp.CreateSurfaceBrushWithSurface(&s) {
                                let _ = b.SetStretch(CompositionStretch::UniformToFill);
                                if let Ok(v) = self.comp.CreateSpriteVisual() {
                                    let _ = v.SetBrush(&b);
                                    let _ = v.SetSize(Vector2 { X: art, Y: art });
                                    let _ = v.SetOffset(Vector3 { X: r.0 + 14.0 * dpi, Y: r.1 + 14.0 * dpi, Z: 0.0 });
                                    if let Ok(g) = self.comp.CreateRoundedRectangleGeometry() {
                                        let _ = g.SetSize(Vector2 { X: art, Y: art });
                                        let _ = g.SetCornerRadius(Vector2 { X: 10.0 * dpi, Y: 10.0 * dpi });
                                        if let Ok(cl) = self.comp.CreateGeometricClip() {
                                            let _ = cl.SetGeometry(&g);
                                            let _ = v.SetClip(&cl);
                                        }
                                    }
                                    let _ = c.Children().and_then(|k| k.InsertAtTop(&v));
                                }
                            }
                        }
                    }
                    self.text(c, &title, px(13.0), 600, WHITE, r.0 + 16.0 * dpi, r.1 + 18.0 * dpi + art, r.2 - 28.0 * dpi, 1.0);
                    self.text(c, &artist, px(12.0), 400, WHITE, r.0 + 16.0 * dpi, r.1 + 38.0 * dpi + art, r.2 - 28.0 * dpi, 0.65);
                    let cy = r.1 + r.3 - 24.0 * dpi;
                    let cxs = [r.0 + r.2 * 0.22, r.0 + r.2 * 0.5, r.0 + r.2 * 0.78];
                    let syms = [Sym::Backward, if playing { Sym::Pause } else { Sym::Play }, Sym::Forward];
                    for (i, (cx, sy)) in cxs.iter().zip(syms).enumerate() {
                        self.symbol(c, sy, px(if i == 1 { 20.0 } else { 17.0 }), WHITE, *cx, cy, 1.0);
                        m.btns.push(((cx - 20.0 * dpi, cy - 20.0 * dpi, 40.0 * dpi, 40.0 * dpi), i as u8));
                    }
                }
            }
            _ if p.h == 2 => {
                // besar 2×2: piringan ikon di kiri atas, judul dan status di bawah
                let d = 46.0 * dpi;
                let disc: Rect = (r.0 + 12.0 * dpi, r.1 + 12.0 * dpi, d, d);
                let (bg, fg) = if on { (Color { A: 245, R: 255, G: 255, B: 255 }, prefs::accent()) } else { (Color { A: 70, R: 0, G: 0, B: 0 }, WHITE) };
                let _ = self.rrect(c, disc, d / 2.0, bg);
                self.glyph(c, glyph_of(k), px(20.0), fg, disc.0 + d / 2.0, disc.1 + d / 2.0, 1.0);
                m.btns.push((disc, BTN_DISC));
                let title = prefs::tr(k.title());
                self.text(c, title, px(14.0), 600, WHITE, r.0 + 16.0 * dpi, r.1 + r.3 - 48.0 * dpi, r.2 - 28.0 * dpi, 1.0);
                let status = self.status(k);
                if !status.is_empty() {
                    self.text(c, &status, px(12.0), 400, WHITE, r.0 + 16.0 * dpi, r.1 + r.3 - 28.0 * dpi, r.2 - 28.0 * dpi, 0.65);
                }
            }
            _ if p.w == 1 && p.h == 1 => {
                // tombol bulat: aktif = piringan biru berisi glyph putih
                if on {
                    let _ = self.rrect(c, r, r.2 / 2.0, prefs::accent_c(245));
                }
                self.glyph(c, glyph_of(k), px(20.0), WHITE, r.0 + r.2 / 2.0, r.1 + r.3 / 2.0, 1.0);
            }
            _ => {
                // kapsul 2×1: piringan ikon di kiri + judul + status
                // piringan ikon sekitar 58% tinggi kapsul (seperti contoh), bukan hampir penuh
                let d = r.3 * 0.58;
                let off = (r.3 - d) / 2.0;
                let disc: Rect = (r.0 + off, r.1 + off, d, d);
                let (bg, fg) = if on { (Color { A: 245, R: 255, G: 255, B: 255 }, prefs::accent()) } else { (Color { A: 70, R: 0, G: 0, B: 0 }, WHITE) };
                let _ = self.rrect(c, disc, d / 2.0, bg);
                self.glyph(c, glyph_of(k), px(16.0), fg, disc.0 + d / 2.0, disc.1 + d / 2.0, 1.0);
                m.btns.push((disc, BTN_DISC));
                let tx = disc.0 + d + 11.0 * dpi;
                let title = prefs::tr(k.title());
                let status = self.status(k);
                let ty = r.1 + r.3 / 2.0 - if status.is_empty() { 9.5 } else { 19.0 } * dpi;
                self.text(c, title, px(14.0), 600, WHITE, tx, ty, r.2 - d - off - 22.0 * dpi, 1.0);
                if !status.is_empty() {
                    self.text(c, &status, px(12.0), 400, WHITE, tx, ty + 19.0 * dpi, r.2 - d - off - 22.0 * dpi, 0.65);
                }
            }
        }
        if self.edit {
            // badge "−" di kiri atas
            let b = m.badge;
            let _ = self.rrect(c, b, b.2 / 2.0, Color { A: 235, R: 70, G: 70, B: 74 });
            let _ = self.rrect(c, (b.0 + 6.0 * dpi, b.1 + b.3 / 2.0 - 1.0 * dpi, b.2 - 12.0 * dpi, 2.0 * dpi), dpi, Color { A: 255, R: 255, G: 255, B: 255 });
        }
        m
    }

    // -----------------------------------------------------------------------------------------------------------
    // aksi
    // -----------------------------------------------------------------------------------------------------------

    fn set_slider(&mut self, i: usize, x: f32, commit: bool) {
        let Some(p) = self.panel.as_ref() else { return };
        let Some(m) = p.mods.get(i) else { return };
        let Some(t) = m.track else { return };
        let frac = ((x - t.0) / t.2).clamp(0.0, 1.0);
        if let Some(f) = &m.fill {
            place_slider(f, &m.thumb, t, frac, p.dpi);
        }
        let kind = m.placed.kind;
        if commit {
            match kind {
                Kind::Display => services::set_brightness(frac),
                _ => services::set_volume(frac),
            }
        }
    }

    /// Menu ukuran Kecil/Sedang/Besar untuk modul `i` (klik kanan di mode Edit).
    fn open_size_menu(&mut self, i: usize, x: f32, y: f32) {
        let Some(p) = self.panel.as_ref() else { return };
        let kind = p.mods[i].placed.kind;
        if !kind.resizable() {
            return;
        }
        let cur = *self.sizes.get(&kind).unwrap_or(&kind.default_size());
        let (rect, dpi, hwnd) = (p.popup.rect, p.dpi, p.popup.hwnd);
        let Some(gfx) = self.gfx.as_ref() else { return };
        if self.menu.is_none() {
            match Menu::new(&self.comp, hwnd, dpi, nacre_platform::sys::screen_size(), Tier::Full) {
                Ok(m) => self.menu = Some(m),
                Err(_) => return,
            }
        }
        let mk = |id: usize, label: &'static str, s: Size| MItem::action(id, &format!("{}{}", if cur == s { "\u{2713} " } else { "" }, prefs::tr(label)), true);
        let items = vec![mk(1, "Kecil", Size::S), mk(2, "Sedang", Size::M), mk(3, "Besar", Size::L)];
        self.menu_kind = Some(kind);
        if let Some(m) = self.menu.as_mut() {
            let _ = m.show(&self.comp, gfx, items, rect.0 + x as i32, rect.1 + y as i32);
        }
    }

    fn pick_size(&mut self, id: usize) {
        if let Some(m) = &self.menu {
            m.hide();
        }
        let Some(kind) = self.menu_kind.take() else { return };
        let s = match id {
            1 => Size::S,
            2 => Size::M,
            _ => Size::L,
        };
        self.sizes.insert(kind, s);
        self.build();
        self.save_layout();
    }

    /// Pemilih Wi-Fi/Bluetooth dari modul `i`: Control Center menutup dan jendela pemilih muncul di pojok kanan atas
    /// (tepi kanan dan atas sama dengan panel) dengan animasi Control Center.
    fn open_picker(&mut self, i: usize) {
        let Some(p) = self.panel.as_ref() else { return };
        let Some(m) = p.mods.get(i) else { return };
        let which = match m.placed.kind {
            Kind::Wifi => crate::netmenu::Which::Wifi,
            Kind::Bluetooth => crate::netmenu::Which::Bluetooth,
            _ => return,
        };
        let (rect, dpi) = (p.popup.rect, p.dpi);
        self.close();
        crate::netmenu::show(&self.comp, which, crate::netmenu::Anchor::Corner(rect.0 + rect.2, rect.1), dpi);
    }

    fn click(&mut self, i: usize, x: f32, y: f32) {
        let Some(p) = self.panel.as_ref() else { return };
        let m = &p.mods[i];
        let kind = m.placed.kind;
        let btn = m.btns.iter().find(|(r, _)| hit(*r, x, y)).map(|(_, b)| *b);
        let send = |cmd: RadioCmd, s: &Cc| {
            if let Some(r) = &s.radios {
                let _ = r.commands.send(cmd);
            }
        };
        let small = m.placed.w == 1 && m.placed.h == 1;
        match kind {
            // piringan ikon (atau modul bulat kecil) membalik radio; sisa modul membuka pemilih jaringan/perangkat
            Kind::Wifi | Kind::Bluetooth if !small && btn != Some(BTN_DISC) => self.open_picker(i),
            Kind::Wifi => send(RadioCmd::Set(RK::Wifi, !(self.wifi == Some(true))), self),
            Kind::Bluetooth => send(RadioCmd::Set(RK::Bluetooth, !(self.bt == Some(true))), self),
            Kind::NowPlaying => {
                if let (Some(b), Some(md)) = (btn, &self.media) {
                    let _ = md.commands.send(match b {
                        0 => Command::Prev,
                        1 => Command::PlayPause,
                        _ => Command::Next,
                    });
                }
            }
            Kind::Stage => {
                prefs::update(|s| s.stage = !s.stage);
                self.build();
            }
            Kind::DarkMode => {
                prefs::update(|s| {
                    s.theme = if s.theme == nacre_core::settings::Theme::Dark { nacre_core::settings::Theme::Light } else { nacre_core::settings::Theme::Dark }
                });
                self.build();
            }
            Kind::Display | Kind::Sound => {
                if btn == Some(10) {
                    self.close();
                    if kind == Kind::Sound {
                        // Win+Ctrl+V: pemilih keluaran suara bawaan Windows 11 (padanan menu keluaran Tahoe)
                        keys::chord(&[0x5B, 0x11, 0x56]);
                    } else {
                        launch::open("ms-settings:display");
                    }
                }
            }
            other => {
                self.close();
                match other {
                    Kind::Hotspot => launch::open("ms-settings:network-mobilehotspot"),
                    Kind::Share => launch::open("ms-settings:crossdevice"),
                    Kind::Airplane => launch::open("ms-settings:network-airplanemode"),
                    Kind::Mirror => keys::chord(&[keys::WIN, 0x4B]),
                    Kind::Focus => launch::open("ms-settings:notifications"),
                    Kind::Screenshot => keys::chord(&[keys::WIN, keys::SHIFT, 0x53]),
                    Kind::Battery => launch::open("ms-settings:batterysaver"),
                    Kind::Lock => launch::run("rundll32.exe", "user32.dll,LockWorkStation"),
                    Kind::Mission => crate::mission::toggle(),
                    Kind::Apps => crate::launchpad::toggle(),
                    Kind::Calculator => launch::run("calc.exe", ""),
                    Kind::Timer => launch::open("ms-clock:"),
                    _ => {}
                }
            }
        }
    }

    fn enter_edit(&mut self) {
        self.edit = true;
        self.build();
        self.open_gallery();
    }

    fn leave_edit(&mut self) {
        self.save_layout();
        self.edit = false;
        if let Some(g) = self.gallery.take() {
            let h = g.popup.hwnd;
            drop(g);
            unsafe {
                let _ = DestroyWindow(h);
            }
        }
        self.build();
    }

    fn remove(&mut self, kind: Kind) {
        self.layout.retain(|k| *k != kind);
        self.build();
        self.refresh_gallery();
    }

    fn add(&mut self, kind: Kind) {
        if !self.layout.contains(&kind) {
            self.layout.push(kind);
            if kind == Kind::NowPlaying && self.media.is_none() {
                let ctl = self.ctl.0 as isize;
                self.media = Some(media::spawn(move || unsafe {
                    let _ = PostMessageW(Some(HWND(ctl as *mut _)), WM_MEDIA, WPARAM(0), LPARAM(0));
                }));
            }
            self.build();
            self.refresh_gallery();
        }
    }

    fn move_to(&mut self, from: Kind, over: Kind) {
        let (Some(a), Some(b)) = (self.layout.iter().position(|k| *k == from), self.layout.iter().position(|k| *k == over)) else { return };
        if a == b {
            return;
        }
        let k = self.layout.remove(a);
        self.layout.insert(b, k);
        self.build();
    }

    // -----------------------------------------------------------------------------------------------------------
    // galeri "Cari Kontrol"
    // -----------------------------------------------------------------------------------------------------------

    fn open_gallery(&mut self) {
        let Some(p) = self.panel.as_ref() else { return };
        let dpi = p.dpi;
        let base = GlassParams::dark((10.0, 10.0), 26.0 * dpi);
        let look = prefs::look(prefs::Role::Dark);
        let Ok(mut popup) = Popup::new_with(&self.comp, w!("AbraCcGallery"), Some(gallery_proc), WINDOW_EX_STYLE(0), prefs::apply(base, &look), true) else { return };
        let (gw, gh) = (620.0 * dpi, 560.0 * dpi);
        let rect = p.popup.rect;
        let x = (rect.0 as f32 - gw - 16.0 * dpi).max(10.0 * dpi);
        let _ = popup.set_rect(x as i32, rect.1, gw as i32, gh as i32);
        let _ = popup.panel.set_radius(26.0 * dpi);
        self.gallery = Some(Gallery { popup, dpi, cat: None, query: String::new(), cards: vec![], cats: vec![], done: (0.0, 0.0, 0.0, 0.0), scroll: 0.0, content_h: 0.0, panels: vec![] });
        self.refresh_gallery();
        if let Some(g) = &self.gallery {
            g.popup.show();
            // galeri meluncur dari sisi panel Control Center
            let _ = motion::pop_in(&self.comp, g.popup.root(), Vector3 { X: g.popup.rect.2 as f32, Y: 0.0, Z: 0.0 }, 0.82, 1.02, 0, 340);
        }
    }

    fn refresh_gallery(&mut self) {
        let Some(g) = self.gallery.as_ref() else { return };
        let (dpi, cat, query, scroll) = (g.dpi, g.cat, g.query.to_lowercase(), g.scroll);
        let content = g.popup.content.clone();
        let (gw, gh) = (g.popup.rect.2 as f32, g.popup.rect.3 as f32);
        if let Ok(k) = content.Children() {
            let _ = k.RemoveAll();
        }
        let side = 190.0 * dpi;
        let mut cats = Vec::new();
        // kolom cari
        let sb: Rect = (16.0 * dpi, 16.0 * dpi, side - 8.0 * dpi, 36.0 * dpi);
        let _ = self.rrect(&content, sb, 10.0 * dpi, Color { A: 60, R: 255, G: 255, B: 255 });
        let q = if query.is_empty() { prefs::tr("Cari Kontrol").to_string() } else { g.query.clone() };
        self.text(&content, &q, (13.0 * dpi) as i32, 400, WHITE, sb.0 + 12.0 * dpi, sb.1 + 9.0 * dpi, sb.2 - 20.0 * dpi, if query.is_empty() { 0.55 } else { 1.0 });
        // kategori
        let mut y = 66.0 * dpi;
        let mut entries: Vec<(Option<Category>, &str)> = vec![(None, "Semua Kontrol")];
        entries.extend(Category::ALL.iter().map(|c| (Some(*c), c.title())));
        for (c, title) in entries {
            let r: Rect = (12.0 * dpi, y, side, 36.0 * dpi);
            if c == cat {
                let _ = self.rrect(&content, r, 10.0 * dpi, Color { A: 70, R: 255, G: 255, B: 255 });
            }
            // ikon berwarna kecil seperti sidebar Tahoe
            let (gl, col): (&str, [u8; 3]) = match c {
                None => ("\u{E71D}", [99, 99, 108]),
                Some(Category::Connectivity) => ("\u{E701}", [10, 132, 255]),
                Some(Category::Display) => ("\u{E706}", [10, 132, 255]),
                Some(Category::Sound) => ("\u{E995}", [255, 45, 85]),
                Some(Category::Media) => ("\u{E8D6}", [255, 69, 58]),
                Some(Category::Battery) => ("\u{E83F}", [52, 199, 89]),
                Some(Category::Utilities) => ("\u{E713}", [142, 142, 147]),
            };
            let ic: Rect = (r.0 + 10.0 * dpi, r.1 + 7.0 * dpi, 22.0 * dpi, 22.0 * dpi);
            let _ = self.rrect(&content, ic, 6.0 * dpi, Color { A: 255, R: col[0], G: col[1], B: col[2] });
            self.glyph(&content, gl, (12.0 * dpi) as i32, WHITE, ic.0 + ic.2 / 2.0, ic.1 + ic.3 / 2.0, 1.0);
            self.text(&content, prefs::tr(title), (13.0 * dpi) as i32, 500, WHITE, r.0 + 42.0 * dpi, r.1 + 9.0 * dpi, r.2 - 48.0 * dpi, 1.0);
            cats.push((c, r));
            y += 38.0 * dpi;
        }
        // kartu
        let mut cards = Vec::new();
        let list: Vec<Kind> = controls::ALL
            .iter()
            .copied()
            .filter(|k| cat.map(|c| k.category() == c).unwrap_or(true))
            .filter(|k| query.is_empty() || prefs::tr(k.title()).to_lowercase().contains(&query))
            .collect();
        let (cx0, mut cy) = (side + 36.0 * dpi, 20.0 * dpi - scroll);
        let (cw, ch, cg) = (130.0 * dpi, 112.0 * dpi, 14.0 * dpi);
        let total_y0 = 20.0 * dpi;
        // dikelompokkan per kategori dengan judul seksi dan garis pemisah (seperti galeri Tahoe)
        for c in Category::ALL {
            if cat.map(|x| x != c).unwrap_or(false) {
                continue;
            }
            let ks: Vec<Kind> = list.iter().copied().filter(|k| k.category() == c).collect();
            if ks.is_empty() {
                continue;
            }
            if cy + 24.0 * dpi > 0.0 && cy < gh - 60.0 * dpi {
                self.text(&content, prefs::tr(c.title()), (13.0 * dpi) as i32, 600, WHITE, cx0, cy, 3.0 * cw, 0.85);
            }
            cy += 28.0 * dpi;
            for (i, k) in ks.iter().enumerate() {
                let (col, row) = (i % 3, i / 3);
                let r: Rect = (cx0 + col as f32 * (cw + cg), cy + row as f32 * (ch + cg), cw, ch);
                if r.1 + r.3 < 0.0 || r.1 > gh - 60.0 * dpi {
                    continue;
                }
                let present = self.layout.contains(k);
                let _ = self.rrect(&content, r, 20.0 * dpi, Color { A: if present { 30 } else { 85 }, R: 255, G: 255, B: 255 });
                let disc: Rect = (r.0 + r.2 / 2.0 - 22.0 * dpi, r.1 + 18.0 * dpi, 44.0 * dpi, 44.0 * dpi);
                let _ = self.rrect(&content, disc, 22.0 * dpi, Color { A: if present { 60 } else { 150 }, R: 0, G: 0, B: 0 });
                self.glyph(&content, glyph_of(*k), (20.0 * dpi) as i32, WHITE, disc.0 + 22.0 * dpi, disc.1 + 22.0 * dpi, if present { 0.5 } else { 1.0 });
                self.text(&content, prefs::tr(k.title()), (12.0 * dpi) as i32, 500, WHITE, r.0 + 10.0 * dpi, r.1 + r.3 - 30.0 * dpi, r.2 - 20.0 * dpi, if present { 0.5 } else { 1.0 });
                cards.push((*k, r));
            }
            cy += ks.len().div_ceil(3) as f32 * (ch + cg);
            if r_sep_visible(cy, gh, dpi) {
                let _ = self.rrect(&content, (cx0, cy - cg / 2.0, 3.0 * cw + 2.0 * cg, 1.0), 0.5, Color { A: 40, R: 255, G: 255, B: 255 });
            }
            cy += 14.0 * dpi;
        }
        let content_h = cy + scroll - total_y0;
        if let Some(g) = self.gallery.as_mut() {
            g.content_h = content_h;
        }
        // keterangan + tombol Selesai
        self.text(&content, prefs::tr("Klik kontrol untuk menambahkannya ke Control Center."), (12.0 * dpi) as i32, 400, WHITE, side + 36.0 * dpi, gh - 34.0 * dpi, gw - side - 180.0 * dpi, 0.55);
        let done: Rect = (gw - 100.0 * dpi, gh - 46.0 * dpi, 84.0 * dpi, 32.0 * dpi);
        let _ = self.rrect(&content, done, 16.0 * dpi, prefs::accent_c(255));
        self.text(&content, prefs::tr("Selesai"), (13.0 * dpi) as i32, 600, WHITE, done.0 + 18.0 * dpi, done.1 + 7.0 * dpi, 70.0 * dpi, 1.0);
        if let Some(g) = self.gallery.as_mut() {
            g.cards = cards;
            g.cats = cats;
            g.done = done;
        }
    }

    fn update_values(&mut self) {
        // perbarui isian slider tanpa membangun ulang
        let Some(p) = self.panel.as_ref() else { return };
        for m in &p.mods {
            if let (Some(t), Some(f)) = (m.track, &m.fill) {
                let frac = if m.placed.kind == Kind::Display { brightness::latest().map(|v| v as f32 / 100.0).unwrap_or(0.5) } else { audio::latest().level };
                place_slider(f, &m.thumb, t, frac, p.dpi);
            }
        }
    }
}

/// Posisikan isian dan pil kenop kaca slider untuk nilai `frac` (kenop selalu berada di dalam jalur).
fn place_slider(fill: &SpriteVisual, thumb: &[SpriteVisual], t: Rect, frac: f32, dpi: f32) {
    let th = t.3 - 20.0 * dpi;
    let (kw, kh) = (KNOB.0 * dpi, KNOB.1 * dpi);
    let kx = (t.0 + t.2 * frac).clamp(t.0 + kw / 2.0, t.0 + t.2 - kw / 2.0);
    let cy = t.1 + t.3 / 2.0;
    let _ = fill.SetSize(Vector2 { X: (kx - t.0).max(th), Y: th });
    for (j, s) in thumb.iter().enumerate() {
        let inset = [-1.0, 0.0, 1.5].get(j).copied().unwrap_or(0.0) * dpi;
        let _ = s.SetOffset(Vector3 { X: kx - kw / 2.0 + inset, Y: cy - kh / 2.0 + inset, Z: 0.0 });
    }
}

/// Ukuran kenop slider (dp).
const KNOB: (f32, f32) = (42.0, 28.0);
/// Radius sudut kartu 2×2 dan slider lebar, relatif terhadap ukuran sel.
const CARD_R: f32 = 0.34;

/// Warna kenop diam: bayangan, pil putih padat, inti (tak terlihat).
const DROP_IDLE: [Color; 3] = [Color { A: 45, R: 0, G: 0, B: 0 }, Color { A: 250, R: 255, G: 255, B: 255 }, Color { A: 0, R: 255, G: 255, B: 255 }];
/// Warna kenop diseret: kaca bening dengan tepi terang dan inti hampir transparan.
const DROP_ACTIVE: [Color; 3] = [Color { A: 0, R: 0, G: 0, B: 0 }, Color { A: 125, R: 255, G: 255, B: 255 }, Color { A: 40, R: 255, G: 255, B: 255 }];

/// Kenop "tetes air" Tahoe: putih padat saat diam, berubah jadi kaca bening dan membesar saat diseret.
fn raindrop(comp: &Compositor, thumb: &[SpriteVisual], active: bool) {
    let (cols, from, to) = if active { (DROP_ACTIVE, 1.0, 1.22) } else { (DROP_IDLE, 1.22, 1.0) };
    for (j, v) in thumb.iter().enumerate() {
        if let Ok(b) = v.Brush() {
            let _ = motion::color(comp, &b, cols[j.min(2)], 150);
        }
        if let Ok(sz) = v.Size() {
            let _ = v.SetCenterPoint(Vector3 { X: sz.X / 2.0, Y: sz.Y / 2.0, Z: 0.0 });
        }
        let _ = motion::vec3(comp, v, "Scale", Vector3 { X: from, Y: from, Z: 1.0 }, Vector3 { X: to, Y: to, Z: 1.0 }, 170);
    }
}

unsafe extern "system" fn ctl_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_RADIO => {
            with(|c| {
                if let Some(r) = &c.radios {
                    while let Ok(s) = r.snapshots.try_recv() {
                        c.wifi = s.wifi;
                        c.bt = s.bluetooth;
                    }
                }
                c.ssid = wifi::ssid();
                if c.panel.is_some() && !c.edit {
                    c.build();
                }
            });
            LRESULT(0)
        }
        WM_MEDIA => {
            with(|c| {
                let mut changed = false;
                if let Some(m) = &c.media {
                    while let Ok(s) = m.snapshots.try_recv() {
                        c.track = Some((s.title, s.artist, s.has_media, s.playing));
                        match s.art {
                            ArtUpdate::New(b) => c.art = Some(b),
                            ArtUpdate::None => c.art = None,
                            ArtUpdate::Unchanged => {}
                        }
                        changed = true;
                    }
                }
                if changed && c.panel.is_some() && !c.edit {
                    c.build();
                }
            });
            LRESULT(0)
        }
        WM_VALUES => {
            with(|c| c.update_values());
            LRESULT(0)
        }
        WM_TIMER if wp.0 == T_CLOSE => {
            with(|c| c.finish_close());
            LRESULT(0)
        }
        WM_TIMER if wp.0 == T_INTRO => {
            with(|c| c.start_intro());
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

fn xy(lp: LPARAM) -> (f32, f32) {
    ((lp.0 as i16) as f32, ((lp.0 >> 16) as i16) as f32)
}

unsafe extern "system" fn panel_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        m if m == WM_CLOSE_CC => {
            close();
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let (x, y) = xy(lp);
            with(|c| {
                if c.closing {
                    return;
                }
                let Some(p) = c.panel.as_ref() else { return };
                if hit(p.edit_btn, x, y) {
                    if c.edit { c.leave_edit() } else { c.enter_edit() }
                    return;
                }
                if let Some((_, cam)) = p.chip.filter(|(r, _)| hit(*r, x, y)) {
                    c.close();
                    launch::open(if cam { "ms-settings:privacy-webcam" } else { "ms-settings:privacy-microphone" });
                    return;
                }
                let edit = c.edit;
                let Some(i) = p.mods.iter().position(|m| hit(m.rect, x, y) || (edit && hit(m.badge, x, y))) else { return };
                let m = &p.mods[i];
                if edit {
                    if hit(m.badge, x, y) {
                        let k = m.placed.kind;
                        c.remove(k);
                    } else if let Some(p) = c.panel.as_mut() {
                        p.drag = Some((i, x, y));
                        SetCapture(hwnd);
                    }
                } else if m.track.map(|t| hit(t, x, y)).unwrap_or(false) {
                    if let Some(p) = c.panel.as_mut() {
                        p.slider = Some(i);
                        raindrop(&c.comp, &p.mods[i].thumb, true);
                    }
                    SetCapture(hwnd);
                    c.set_slider(i, x, true);
                } else {
                    c.click(i, x, y);
                }
            });
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let (x, y) = xy(lp);
            let _ = y;
            with(|c| {
                let slider = c.panel.as_ref().and_then(|p| p.slider);
                if let Some(i) = slider {
                    c.set_slider(i, x, true);
                }
            });
            LRESULT(0)
        }
        m if m == MENU_PICK => {
            let id = wp.0;
            with(|c| c.pick_size(id));
            LRESULT(0)
        }
        m if m == MENU_CLOSE => {
            with(|c| {
                if let Some(mn) = &c.menu {
                    mn.hide();
                }
            });
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            let (x, y) = xy(lp);
            with(|c| {
                if c.closing {
                    return;
                }
                let Some(p) = c.panel.as_ref() else { return };
                let Some(i) = p.mods.iter().position(|m| hit(m.rect, x, y)) else { return };
                if c.edit {
                    c.open_size_menu(i, x, y);
                } else if matches!(p.mods[i].placed.kind, Kind::Wifi | Kind::Bluetooth) {
                    // klik kanan modul Wi-Fi/Bluetooth (juga yang bulat kecil): pemilih jaringan/perangkat
                    c.open_picker(i);
                }
            });
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let (x, y) = xy(lp);
            with(|c| {
                let (slider, drag) = match c.panel.as_mut() {
                    Some(p) => (p.slider.take(), p.drag.take()),
                    None => (None, None),
                };
                if let (Some(i), Some(p)) = (slider, c.panel.as_ref()) {
                    if let Some(m) = p.mods.get(i) {
                        raindrop(&c.comp, &m.thumb, false);
                    }
                }
                if slider.is_some() || drag.is_some() {
                    let _ = ReleaseCapture();
                }
                if let Some((i, _, _)) = drag {
                    let from = c.panel.as_ref().map(|p| p.mods[i].placed.kind);
                    let over = c.panel.as_ref().and_then(|p| p.mods.iter().find(|m| hit(m.rect, x, y)).map(|m| m.placed.kind));
                    if let (Some(f), Some(o)) = (from, over) {
                        c.move_to(f, o);
                    }
                }
            });
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe extern "system" fn gallery_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_LBUTTONUP => {
            let (x, y) = xy(lp);
            with(|c| {
                let Some(g) = c.gallery.as_ref() else { return };
                if hit(g.done, x, y) {
                    c.leave_edit();
                    return;
                }
                if let Some(cat) = g.cats.iter().find(|(_, r)| hit(*r, x, y)).map(|(c, _)| *c) {
                    if let Some(g) = c.gallery.as_mut() {
                        g.cat = cat;
                        g.scroll = 0.0;
                    }
                    c.refresh_gallery();
                    return;
                }
                if let Some(k) = g.cards.iter().find(|(_, r)| hit(*r, x, y)).map(|(k, _)| *k) {
                    c.add(k);
                }
            });
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = ((wp.0 >> 16) as i16) as f32;
            with(|c| {
                if let Some(g) = c.gallery.as_mut() {
                    let max = (g.content_h - (g.popup.rect.3 as f32 - 80.0 * g.dpi)).max(0.0);
                    g.scroll = (g.scroll - delta / 120.0 * 60.0 * g.dpi).clamp(0.0, max);
                }
                c.refresh_gallery();
            });
            LRESULT(0)
        }
        WM_CHAR => {
            let ch = char::from_u32(wp.0 as u32).unwrap_or('\0');
            if !ch.is_control() {
                with(|c| {
                    if let Some(g) = c.gallery.as_mut() {
                        g.query.push(ch);
                        g.scroll = 0.0;
                    }
                    c.refresh_gallery();
                });
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if wp.0 as u16 == VK_BACK.0 {
                with(|c| {
                    if let Some(g) = c.gallery.as_mut() {
                        g.query.pop();
                    }
                    c.refresh_gallery();
                });
            } else if wp.0 as u16 == VK_ESCAPE.0 {
                with(|c| c.leave_edit());
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
