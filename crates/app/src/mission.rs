use crate::popup::Popup;
use crate::prefs;
use nacre_core::mission::{grid, Rect};
use nacre_platform::icons::Bitmap;
use nacre_platform::text::{measure, render, Style};
use nacre_platform::wins::{enumerate, focus, snapshot, thumbnail};
use nacre_platform::{log, sys};
use nacre_ui::gfx::Gfx;
use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Instant;
use windows::core::w;
use windows::UI::Color;
use windows::UI::Composition::{CompositionDrawingSurface, CompositionStretch, Compositor, SpriteVisual};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, ReleaseCapture, SetCapture, UnregisterHotKey, MOD_CONTROL, MOD_NOREPEAT, VK_ESCAPE, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows_numerics::{Vector2, Vector3};

const HOTKEY_ID: i32 = 0xAB03;
const T_ANIM: usize = 1;
const ANIM_MS: f32 = 240.0;

struct Tile {
    hwnd: isize,
    handle: isize,
    from: Rect,
    to: Rect,
    title: String,
}

struct Drag {
    idx: usize,
    start: (f32, f32),
    active: bool,
}

struct Mc {
    popup: Popup,
    gfx: Gfx,
    comp: Compositor,
    dpi: f32,
    tiles: Vec<Tile>,
    t0: Instant,
    done: bool,
    hover: Option<usize>,
    highlight: SpriteVisual,
    label: Option<SpriteVisual>,
    desks: Vec<(Rect, usize)>,
    plus: Option<Rect>,
    cur_desk: usize,
    text_cache: HashMap<String, (CompositionDrawingSurface, i32, i32)>,
    drag: Option<Drag>,
    desk_hover: Option<usize>,
    desk_ring: Option<SpriteVisual>,
}

struct Ctl {
    hwnd: HWND,
    comp: Compositor,
}

thread_local! {
    static MC: RefCell<Option<Box<Mc>>> = const { RefCell::new(None) };
    static CTL: RefCell<Option<Ctl>> = const { RefCell::new(None) };
}

fn with_mc<R>(f: impl FnOnce(&mut Mc) -> R) -> Option<R> {
    MC.with(|c| c.try_borrow_mut().ok().and_then(|mut b| b.as_mut().map(|b| f(b))))
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

pub fn init(comp: &Compositor) -> windows_core::Result<()> {
    unsafe {
        let hi: HINSTANCE = GetModuleHandleW(None)?.into();
        let class = w!("AbraMissionCtl");
        RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(ctl_proc), hInstance: hi, lpszClassName: class, ..Default::default() });
        let hwnd = CreateWindowExW(WINDOW_EX_STYLE(0), class, w!("abra-mc-ctl"), WS_POPUP, 0, 0, 0, 0, Some(HWND_MESSAGE), None, Some(hi), None)?;
        if RegisterHotKey(Some(hwnd), HOTKEY_ID, MOD_CONTROL | MOD_NOREPEAT, VK_UP.0 as u32).is_err() {
            log::line("mission", "Ctrl+Atas dipakai program lain; hot key tidak aktif");
        }
        CTL.with(|c| *c.borrow_mut() = Some(Ctl { hwnd, comp: comp.clone() }));
    }
    Ok(())
}

pub fn shutdown() {
    CTL.with(|c| {
        if let Some(ctl) = c.borrow().as_ref() {
            unsafe {
                let _ = UnregisterHotKey(Some(ctl.hwnd), HOTKEY_ID);
            }
        }
    });
}

pub fn toggle() {
    if MC.with(|c| c.borrow().is_some()) {
        close();
        return;
    }
    let comp = CTL.with(|c| c.borrow().as_ref().map(|c| c.comp.clone()));
    let Some(comp) = comp else { return };
    match create(&comp) {
        Ok(Some(m)) => {
            MC.with(|c| *c.borrow_mut() = Some(m));
            with_mc(|m| {
                m.popup.show_active();
                m.t0 = Instant::now();
                m.step();
                unsafe {
                    SetTimer(Some(m.popup.hwnd), T_ANIM, 16, None);
                }
            });
        }
        Ok(None) => log::line("mission", "tidak ada jendela untuk ditampilkan"),
        Err(e) => log::line("mission", &format!("gagal: {e}")),
    }
}

pub fn close() {
    let m = MC.with(|c| c.borrow_mut().take());
    if let Some(m) = m {
        for t in &m.tiles {
            thumbnail::unregister(t.handle);
        }
        let h = m.popup.hwnd;
        drop(m);
        unsafe {
            let _ = DestroyWindow(h);
        }
        nacre_platform::sys::trim_later(5000);
    }
}

fn create(comp: &Compositor) -> windows_core::Result<Option<Box<Mc>>> {
    let screen = sys::screen_size();
    let mut look = prefs::look(prefs::Role::Dark);
    look.blur_sigma = look.blur_sigma.max(20.0);
    look.tint = [120, 12, 14, 18];
    let base = nacre_ui::glass::GlassParams::dark((10.0, 10.0), 0.0);
    let mut popup = Popup::new_with(comp, w!("AbraMission"), Some(mc_proc), WINDOW_EX_STYLE(0), prefs::apply(base, &look), true)?;
    let dpi = unsafe { GetDpiForWindow(popup.hwnd) }.max(96) as f32 / 96.0;
    popup.set_rect(0, 0, screen.0, screen.1)?;
    popup.panel.set_radius(0.0)?;
    let dst = popup.hwnd.0 as isize;

    let wins: Vec<_> = enumerate::list(std::process::id()).into_iter().filter(|w| !focus::is_minimized(w.hwnd)).collect();
    let mut tiles = Vec::new();
    let mut sizes = Vec::new();
    for w in &wins {
        let Some(handle) = thumbnail::register(dst, w.hwnd) else { continue };
        let Some((sw, sh)) = thumbnail::source_size(handle) else {
            thumbnail::unregister(handle);
            continue;
        };
        let (fx, fy, fw, fh) = snapshot::frame_rect(w.hwnd).unwrap_or((0, 0, sw, sh));
        sizes.push((sw as f32, sh as f32));
        let title = if w.name.is_empty() { focus::title(w.hwnd) } else { w.name.clone() };
        tiles.push(Tile { hwnd: w.hwnd, handle, from: Rect { x: fx as f32, y: fy as f32, w: fw as f32, h: fh as f32 }, to: Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 }, title });
    }
    if tiles.is_empty() {
        return Ok(None);
    }
    let area = Rect { x: 60.0 * dpi, y: 150.0 * dpi, w: screen.0 as f32 - 120.0 * dpi, h: screen.1 as f32 - 210.0 * dpi };
    let g = grid(&sizes, area, 30.0 * dpi, 34.0 * dpi);
    for (t, r) in tiles.iter_mut().zip(g) {
        t.to = r;
    }
    let highlight = comp.CreateSpriteVisual()?;
    highlight.SetOpacity(0.0)?;
    popup.content.Children()?.InsertAtTop(&highlight)?;
    let gfx = Gfx::shared(comp)?;
    Ok(Some(Box::new(Mc { popup, gfx, comp: comp.clone(), dpi, tiles, t0: Instant::now(), done: false, hover: None, highlight, label: None, desks: vec![], plus: None, cur_desk: 0, text_cache: HashMap::new(), drag: None, desk_hover: None, desk_ring: None })))
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

impl Mc {
    fn step(&mut self) {
        let t = (self.t0.elapsed().as_millis() as f32 / ANIM_MS).clamp(0.0, 1.0);
        let e = 1.0 - (1.0 - t).powi(3);
        for tile in &self.tiles {
            let (f, to) = (tile.from, tile.to);
            let r = (
                lerp(f.x, to.x, e).round() as i32,
                lerp(f.y, to.y, e).round() as i32,
                (lerp(f.x, to.x, e) + lerp(f.w, to.w, e)).round() as i32,
                (lerp(f.y, to.y, e) + lerp(f.h, to.h, e)).round() as i32,
            );
            thumbnail::place(tile.handle, r, 255);
        }
        if t >= 1.0 && !self.done {
            self.done = true;
            unsafe {
                let _ = KillTimer(Some(self.popup.hwnd), T_ANIM);
            }
            self.draw_desktops();
        }
    }

    fn put_text(&mut self, text: &str, px: i32, weight: i32, x: f32, y: f32, maxw: f32, op: f32) -> Option<SpriteVisual> {
        let key = format!("{px}|{weight}|{text}|{}", maxw as i32);
        let hit = self.text_cache.get(&key).cloned();
        let (surf, w, h) = match hit {
            Some(v) => v,
            None => {
                let style = Style { px, weight };
                let (tw, th) = measure(text, &style);
                let tw = (tw + 2).min(maxw.max(40.0) as i32);
                let bmp = render(text, &style, tw, th)?;
                let sf = self.gfx.surface_from_bgra(bmp.w, bmp.h, &recolor(&bmp, [255, 255, 255])).ok()?;
                let v = (sf, tw, th);
                self.text_cache.insert(key, v.clone());
                v
            }
        };
        let go = || -> windows_core::Result<SpriteVisual> {
            let b = self.comp.CreateSurfaceBrushWithSurface(&surf)?;
            b.SetStretch(CompositionStretch::None)?;
            let v = self.comp.CreateSpriteVisual()?;
            v.SetBrush(&b)?;
            v.SetSize(Vector2 { X: w as f32, Y: h as f32 })?;
            v.SetOffset(Vector3 { X: x + (maxw - w as f32) / 2.0, Y: y, Z: 0.0 })?;
            v.SetOpacity(op)?;
            self.popup.content.Children()?.InsertAtTop(&v)?;
            Ok(v)
        };
        go().ok()
    }

    fn card(&self, r: Rect, radius: f32, c: Color) {
        let go = || -> windows_core::Result<()> {
            let v = self.comp.CreateSpriteVisual()?;
            v.SetSize(Vector2 { X: r.w, Y: r.h })?;
            v.SetOffset(Vector3 { X: r.x, Y: r.y, Z: 0.0 })?;
            v.SetBrush(&self.comp.CreateColorBrushWithColor(c)?)?;
            let g = self.comp.CreateRoundedRectangleGeometry()?;
            g.SetSize(Vector2 { X: r.w, Y: r.h })?;
            g.SetCornerRadius(Vector2 { X: radius, Y: radius })?;
            let clip = self.comp.CreateGeometricClip()?;
            clip.SetGeometry(&g)?;
            v.SetClip(&clip)?;
            self.popup.content.Children()?.InsertAtTop(&v)
        };
        let _ = go();
    }

    fn draw_desktops(&mut self) {
        let d = self.dpi;
        let (names, cur) = nacre_platform::vdesk::list();
        self.cur_desk = cur;
        let (cw, ch, gap) = (180.0 * d, 100.0 * d, 18.0 * d);
        let n = names.len() as f32;
        let plus_w = 64.0 * d;
        let total = n * cw + (n - 1.0) * gap + gap + plus_w;
        let sw = self.popup.rect.2 as f32;
        let x0 = (sw - total) / 2.0;
        let y = 28.0 * d;
        self.desks.clear();
        for (i, name) in names.iter().enumerate() {
            let r = Rect { x: x0 + i as f32 * (cw + gap), y, w: cw, h: ch };
            if i == cur {
                self.card(Rect { x: r.x - 3.0 * d, y: r.y - 3.0 * d, w: r.w + 6.0 * d, h: r.h + 6.0 * d }, 14.0 * d, crate::prefs::accent_c(255));
            }
            self.card(r, 12.0 * d, Color { A: 110, R: 255, G: 255, B: 255 });
            let _ = self.put_text(name, (13.0 * d) as i32, 600, r.x, r.y + r.h / 2.0 - 8.0 * d, r.w, 1.0);
            self.desks.push((r, i));
        }
        let r = Rect { x: x0 + n * (cw + gap), y, w: plus_w, h: ch };
        let pr = r;
        self.card(pr, 12.0 * d, Color { A: 70, R: 255, G: 255, B: 255 });
        let _ = self.put_text("+", (28.0 * d) as i32, 300, pr.x, pr.y + pr.h / 2.0 - 18.0 * d, pr.w, 1.0);
        self.plus = Some(pr);
    }

    fn set_desk_hover(&mut self, idx: Option<usize>) {
        if idx == self.desk_hover {
            return;
        }
        self.desk_hover = idx;
        if let Some(old) = self.desk_ring.take() {
            if let Ok(k) = self.popup.content.Children() {
                let _ = k.Remove(&old);
            }
        }
        let Some(i) = idx else { return };
        let Some(&(r, _)) = self.desks.iter().find(|(_, di)| *di == i) else { return };
        let d = self.dpi;
        let go = || -> windows_core::Result<SpriteVisual> {
            let v = self.comp.CreateSpriteVisual()?;
            v.SetSize(Vector2 { X: r.w + 8.0 * d, Y: r.h + 8.0 * d })?;
            v.SetOffset(Vector3 { X: r.x - 4.0 * d, Y: r.y - 4.0 * d, Z: 0.0 })?;
            v.SetBrush(&self.comp.CreateColorBrushWithColor(Color { A: 120, R: 255, G: 255, B: 255 })?)?;
            let g = self.comp.CreateRoundedRectangleGeometry()?;
            g.SetSize(Vector2 { X: r.w + 8.0 * d, Y: r.h + 8.0 * d })?;
            g.SetCornerRadius(Vector2 { X: 14.0 * d, Y: 14.0 * d })?;
            let clip = self.comp.CreateGeometricClip()?;
            clip.SetGeometry(&g)?;
            v.SetClip(&clip)?;
            self.popup.content.Children()?.InsertAtTop(&v)?;
            Ok(v)
        };
        self.desk_ring = go().ok();
    }

    fn place_tile(&self, idx: usize, r: Rect, alpha: u8) {
        if let Some(t) = self.tiles.get(idx) {
            thumbnail::place(t.handle, (r.x.round() as i32, r.y.round() as i32, (r.x + r.w).round() as i32, (r.y + r.h).round() as i32), alpha);
        }
    }

    fn tile_at(&self, x: f32, y: f32) -> Option<usize> {
        let pos = |t: &Tile| if self.done { t.to } else { t.from };
        self.tiles.iter().position(|t| {
            let r = pos(t);
            x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h
        })
    }

    fn set_hover(&mut self, i: Option<usize>) {
        if i == self.hover {
            return;
        }
        self.hover = i;
        if let Some(old) = self.label.take() {
            if let Ok(k) = self.popup.content.Children() {
                let _ = k.Remove(&old);
            }
        }
        if let (Some(idx), true) = (i, self.done) {
            let (r, title) = (self.tiles[idx].to, self.tiles[idx].title.clone());
            if !title.is_empty() {
                let d = self.dpi;
                self.label = self.put_text(&title, (13.0 * d) as i32, 600, r.x, r.y + r.h + 14.0 * d, r.w.max(120.0), 1.0);
            }
        }
        match i.map(|i| self.tiles[i].to) {
            Some(r) if self.done => {
                let pad = 7.0 * self.dpi;
                let go = || -> windows_core::Result<()> {
                    self.highlight.SetSize(Vector2 { X: r.w + 2.0 * pad, Y: r.h + 2.0 * pad })?;
                    self.highlight.SetOffset(Vector3 { X: r.x - pad, Y: r.y - pad, Z: 0.0 })?;
                    self.highlight.SetBrush(&self.comp.CreateColorBrushWithColor(crate::prefs::accent_c(235))?)?;
                    let g = self.comp.CreateRoundedRectangleGeometry()?;
                    g.SetSize(Vector2 { X: r.w + 2.0 * pad, Y: r.h + 2.0 * pad })?;
                    g.SetCornerRadius(Vector2 { X: 10.0 * self.dpi, Y: 10.0 * self.dpi })?;
                    let clip = self.comp.CreateGeometricClip()?;
                    clip.SetGeometry(&g)?;
                    self.highlight.SetClip(&clip)?;
                    self.highlight.SetOpacity(1.0)
                };
                let _ = go();
            }
            _ => {
                let _ = self.highlight.SetOpacity(0.0);
            }
        }
    }
}

unsafe extern "system" fn ctl_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_HOTKEY => {
            toggle();
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe extern "system" fn mc_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_TIMER if wp.0 == T_ANIM => {
            with_mc(|m| m.step());
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let (x, y) = ((lp.0 as i16) as f32, ((lp.0 >> 16) as i16) as f32);
            with_mc(|m| {
                if m.done {
                    if let Some(idx) = m.tile_at(x, y) {
                        m.drag = Some(Drag { idx, start: (x, y), active: false });
                        let _ = SetCapture(hwnd);
                    }
                }
            });
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let (x, y) = ((lp.0 as i16) as f32, ((lp.0 >> 16) as i16) as f32);
            with_mc(|m| {
                if let Some(mut d) = m.drag.take() {
                    let dist = ((x - d.start.0).powi(2) + (y - d.start.1).powi(2)).sqrt();
                    if d.active || dist > 8.0 * m.dpi {
                        d.active = true;
                        m.set_hover(None);
                        let orig = m.tiles[d.idx].to;
                        let (w, h) = (orig.w * 0.42, orig.h * 0.42);
                        m.place_tile(d.idx, Rect { x: x - w / 2.0, y: y - h / 2.0, w, h }, 220);
                        let dh = m.desks.iter().find(|(r, _)| x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h).map(|(_, di)| *di);
                        m.set_desk_hover(dh.filter(|di| *di != m.cur_desk));
                    }
                    m.drag = Some(d);
                    return;
                }
                let i = m.tile_at(x, y);
                m.set_hover(i);
            });
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if wp.0 as u16 == VK_ESCAPE.0 {
                close();
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let (x, y) = ((lp.0 as i16) as f32, ((lp.0 >> 16) as i16) as f32);
            let _ = ReleaseCapture();
            let in_r = |r: &Rect| x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h;
            let dragged = with_mc(|m| {
                m.set_desk_hover(None);
                let d = m.drag.take()?;
                if !d.active {
                    return None;
                }
                let drop_desk = m.desks.iter().find(|(r, _)| x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h).map(|(_, di)| *di);
                if let Some(target_desk) = drop_desk.filter(|di| *di != m.cur_desk) {
                    let hwnd_tile = m.tiles[d.idx].hwnd;
                    if nacre_platform::vdesk::move_window_to_desktop(hwnd_tile, target_desk) {
                        let rem = m.tiles.remove(d.idx);
                        thumbnail::unregister(rem.handle);
                        return Some(true);
                    }
                }
                let orig = m.tiles[d.idx].to;
                m.place_tile(d.idx, orig, 255);
                Some(false)
            })
            .flatten();
            if let Some(moved) = dragged {
                if moved {
                    let empty = with_mc(|m| m.tiles.is_empty()).unwrap_or(false);
                    if empty {
                        close();
                    }
                }
                return LRESULT(0);
            }
            let desk = with_mc(|m| {
                if let Some(p) = m.plus.filter(|p| in_r(p)) {
                    let _ = p;
                    return Some(Err(()));
                }
                m.desks.iter().find(|(r, _)| in_r(r)).map(|(_, i)| Ok((*i as i32) - (m.cur_desk as i32)))
            })
            .flatten();
            if let Some(act) = desk {
                close();
                match act {
                    Err(()) => nacre_platform::keys::chord(&[nacre_platform::keys::WIN, nacre_platform::keys::CTRL, 0x44]),
                    Ok(delta) => {
                        for _ in 0..delta.abs() {
                            nacre_platform::keys::chord(&[nacre_platform::keys::WIN, nacre_platform::keys::CTRL, if delta > 0 { 0x27 } else { 0x25 }]);
                        }
                    }
                }
                return LRESULT(0);
            }
            let target = with_mc(|m| m.tile_at(x, y).map(|i| m.tiles[i].hwnd)).flatten();
            close();
            if let Some(h) = target {
                focus::activate(h, 0);
            }
            LRESULT(0)
        }
        WM_ACTIVATE => {
            if (wp.0 & 0xFFFF) == 0 {
                let _ = PostMessageW(Some(hwnd), WM_APP + 90, WPARAM(0), LPARAM(0));
            }
            LRESULT(0)
        }
        m if m == WM_APP + 90 => {
            close();
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
