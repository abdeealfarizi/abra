use crate::popup::Popup;
use crate::{motion, prefs, traffic};
use nacre_platform::icons;
use nacre_platform::text::{measure, render, render_face, Style};
use nacre_ui::gfx::Gfx;
use std::cell::RefCell;
use std::path::Path;
use windows::core::w;
use windows::UI::Color;
use windows::UI::Composition::{CompositionStretch, Compositor, ContainerVisual, SpriteVisual};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT, VK_ESCAPE, VK_RETURN, VK_SPACE};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows_numerics::{Vector2, Vector3};

const WM_MOUSELEAVE: u32 = 0x02A3;

struct Ql {
    popup: Popup,
    comp: Compositor,
    d: f32,
    dark: bool,
    tl: ContainerVisual,
    tl_hover: bool,
    path: String,
    open_rect: (f32, f32, f32, f32),
}

thread_local! {
    static QL: RefCell<Option<Ql>> = const { RefCell::new(None) };
}

fn is_image(ext: &str) -> bool {
    matches!(ext, "png" | "jpg" | "jpeg" | "webp" | "bmp" | "gif" | "ico" | "tif" | "tiff")
}

fn is_text(ext: &str) -> bool {
    matches!(
        ext,
        "txt"
            | "md"
            | "rs"
            | "py"
            | "js"
            | "ts"
            | "tsx"
            | "jsx"
            | "json"
            | "toml"
            | "yaml"
            | "yml"
            | "ini"
            | "cfg"
            | "log"
            | "csv"
            | "xml"
            | "html"
            | "css"
            | "ps1"
            | "sh"
            | "bat"
            | "sql"
            | "c"
            | "cpp"
            | "h"
            | "hpp"
            | "java"
            | "go"
    )
}

fn fmt_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

fn recolor(src: &[u8], rgb: [u8; 3]) -> Vec<u8> {
    let mut out = src.to_vec();
    for px in out.chunks_exact_mut(4) {
        let a = px[3] as u32;
        px[0] = (rgb[2] as u32 * a / 255) as u8;
        px[1] = (rgb[1] as u32 * a / 255) as u8;
        px[2] = (rgb[0] as u32 * a / 255) as u8;
    }
    out
}

pub fn close() {
    let cur = QL.with(|c| c.borrow_mut().take());
    if let Some(q) = cur {
        let h = q.popup.hwnd;
        drop(q);
        unsafe {
            let _ = DestroyWindow(h);
        }
    }
}

pub fn open(comp: &Compositor, path: &str) {
    close();
    if path.is_empty() {
        return;
    }
    let mon = nacre_platform::monitors::under_cursor();
    let d = mon.scale() * prefs::ui_scale();
    let (w, h) = ((560.0 * d).round(), (440.0 * d).round());
    let wi = w as i32;
    let hi = h as i32;
    let px = mon.area.x + (mon.area.w - wi) / 2;
    let py = mon.area.y + (mon.area.h - hi) / 2;

    let look = prefs::look(prefs::Role::Themed);
    let base = nacre_ui::glass::GlassParams::dark((w, h), 26.0 * d);
    let Ok(mut popup) = Popup::new_with(comp, w!("AbraQuickLook"), Some(proc), WINDOW_EX_STYLE(0), prefs::apply(base, &look), true) else {
        return;
    };
    let _ = popup.set_rect(px, py, wi, hi);
    let _ = popup.panel.set_radius(26.0 * d);

    let Ok(gfx) = Gfx::shared(comp) else { return };
    let root: ContainerVisual = popup.content.clone();
    let fg = look.fg;

    let put_rect = |parent: &ContainerVisual, x: f32, y: f32, rw: f32, rh: f32, rad: f32, col: Color| {
        let go = || -> windows_core::Result<()> {
            let v = comp.CreateSpriteVisual()?;
            v.SetSize(Vector2 { X: rw, Y: rh })?;
            v.SetOffset(Vector3 { X: x, Y: y, Z: 0.0 })?;
            v.SetBrush(&comp.CreateColorBrushWithColor(col)?)?;
            if rad > 0.0 {
                let g = comp.CreateRoundedRectangleGeometry()?;
                g.SetSize(Vector2 { X: rw, Y: rh })?;
                g.SetCornerRadius(Vector2 { X: rad, Y: rad })?;
                let cl = comp.CreateGeometricClip()?;
                cl.SetGeometry(&g)?;
                v.SetClip(&cl)?;
            }
            parent.Children()?.InsertAtTop(&v)
        };
        let _ = go();
    };

    let put_text = |parent: &ContainerVisual, text: &str, px_sz: f32, weight: i32, x: f32, y: f32, maxw: f32, op: f32, face: Option<&str>| {
        let st = Style { px: (px_sz * d).round() as i32, weight };
        let (tw, th) = measure(text, &st);
        let tw = (tw + 2).min(maxw.max(24.0) as i32);
        let bmp = match face {
            Some(f) => render_face(text, &st, f, tw, th).or_else(|| render(text, &st, tw, th)),
            None => render(text, &st, tw, th),
        };
        let Some(bmp) = bmp else { return };
        let Ok(sf) = gfx.surface_from_bgra(bmp.w, bmp.h, &recolor(&bmp.bgra, fg)) else { return };
        let go = || -> windows_core::Result<()> {
            let b = comp.CreateSurfaceBrushWithSurface(&sf)?;
            b.SetStretch(CompositionStretch::None)?;
            let v: SpriteVisual = comp.CreateSpriteVisual()?;
            v.SetBrush(&b)?;
            v.SetSize(Vector2 { X: bmp.w as f32, Y: bmp.h as f32 })?;
            v.SetOffset(Vector3 { X: x, Y: y, Z: 0.0 })?;
            v.SetOpacity(op)?;
            parent.Children()?.InsertAtTop(&v)
        };
        let _ = go();
    };

    let put_bmp = |parent: &ContainerVisual, bmp: &icons::Bitmap, x: f32, y: f32, bw: f32, bh: f32| {
        let Ok(sf) = gfx.surface_from_bgra(bmp.w, bmp.h, &bmp.bgra) else { return };
        let go = || -> windows_core::Result<()> {
            let b = comp.CreateSurfaceBrushWithSurface(&sf)?;
            b.SetStretch(CompositionStretch::Uniform)?;
            let v = comp.CreateSpriteVisual()?;
            v.SetBrush(&b)?;
            v.SetSize(Vector2 { X: bw, Y: bh })?;
            v.SetOffset(Vector3 { X: x, Y: y, Z: 0.0 })?;
            parent.Children()?.InsertAtTop(&v)
        };
        let _ = go();
    };

    let p = Path::new(path);
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or(path);
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let meta = std::fs::metadata(p).ok();
    let is_dir = meta.as_ref().is_some_and(|m| m.is_dir());
    let sub = match &meta {
        Some(m) if m.is_dir() => prefs::tr("Folder").to_string(),
        Some(m) => {
            if ext.is_empty() {
                fmt_size(m.len())
            } else {
                format!("{} · {}", ext.to_ascii_uppercase(), fmt_size(m.len()))
            }
        }
        None => path.to_string(),
    };

    let pad = 16.0 * d;
    let hdr_h = 52.0 * d;
    let Ok(tl_layer) = comp.CreateContainerVisual() else { return };
    let _ = root.Children().and_then(|k| k.InsertAtTop(&tl_layer));
    traffic::render(
        comp,
        &tl_layer,
        traffic::Layout::tahoe(d),
        traffic::Opts { active: true, hover: false, dark: look.dark, can_min: false, can_zoom: false },
        d,
    );
    let title_x = 92.0 * d;
    let open_label = prefs::tr("Buka");
    let ow = (measure(open_label, &Style { px: (12.0 * d).round() as i32, weight: 600 }).0 as f32 + 24.0 * d).max(58.0 * d);
    let open_rect = (w - pad - ow, 12.0 * d, ow, 28.0 * d);
    let title_max = (open_rect.0 - title_x - 42.0 * d).max(80.0 * d);
    if let Some(ico) = icons::load(path, (28.0 * d) as i32) {
        put_bmp(&root, &ico, title_x, 12.0 * d, 28.0 * d, 28.0 * d);
    }
    put_text(&root, name, 14.5, 600, title_x + 36.0 * d, 10.0 * d, title_max, 0.96, None);
    put_text(&root, &sub, 11.5, 400, title_x + 36.0 * d, 29.0 * d, title_max, 0.62, None);

    put_rect(&root, open_rect.0, open_rect.1, open_rect.2, open_rect.3, 14.0 * d, prefs::accent_c(215));
    let ot = Style { px: (12.0 * d).round() as i32, weight: 600 };
    let (otw, _) = measure(open_label, &ot);
    if let Some(obmp) = render(open_label, &ot, otw + 2, (16.0 * d) as i32) {
        if let Ok(osf) = gfx.surface_from_bgra(obmp.w, obmp.h, &recolor(&obmp.bgra, [255, 255, 255])) {
            if let Ok(br) = comp.CreateSurfaceBrushWithSurface(&osf) {
                let _ = br.SetStretch(CompositionStretch::None);
                if let Ok(ov) = comp.CreateSpriteVisual() {
                    let _ = ov.SetBrush(&br);
                    let _ = ov.SetSize(Vector2 { X: obmp.w as f32, Y: obmp.h as f32 });
                    let _ = ov.SetOffset(Vector3 { X: open_rect.0 + (open_rect.2 - obmp.w as f32) / 2.0, Y: open_rect.1 + (open_rect.3 - obmp.h as f32) / 2.0, Z: 0.0 });
                    let _ = root.Children().and_then(|k| k.InsertAtTop(&ov));
                }
            }
        }
    }

    put_rect(&root, pad, hdr_h, w - 2.0 * pad, 1.0, 0.0, Color { A: 28, R: fg[0], G: fg[1], B: fg[2] });

    let (bx, by, bw, bh) = (pad, hdr_h + 10.0 * d, w - 2.0 * pad, h - hdr_h - pad - 10.0 * d);
    put_rect(&root, bx, by, bw, bh, 12.0 * d, Color { A: 18, R: fg[0], G: fg[1], B: fg[2] });

    if !is_dir && is_image(&ext) {
        let side = bw.min(bh) as u32;
        if let Some(bmp) = nacre_platform::wic::decode_file_fit(path, side.max(64)) {
            put_bmp(&root, &bmp, bx + (bw - side as f32) / 2.0, by + (bh - side as f32) / 2.0, side as f32, side as f32);
        }
    } else if !is_dir && is_text(&ext) {
        let content = std::fs::read(p)
            .ok()
            .map(|raw| {
                let slice = &raw[..raw.len().min(8192)];
                String::from_utf8_lossy(slice).to_string()
            })
            .unwrap_or_default();
        let line_h = 18.0 * d;
        let max_lines = ((bh - 20.0 * d) / line_h).floor().max(1.0) as usize;
        for (i, line) in content.lines().take(max_lines).enumerate() {
            let trimmed: String = line.chars().take(120).collect();
            put_text(&root, &trimmed, 12.0, 400, bx + 12.0 * d, by + 10.0 * d + i as f32 * line_h, bw - 24.0 * d, 0.88, Some("Consolas"));
        }
    } else {
        let th_px = (128.0 * d).round() as i32;
        if let Some(bmp) = icons::thumbnail(path, th_px).or_else(|| icons::load(path, th_px)) {
            put_bmp(&root, &bmp, bx + (bw - th_px as f32) / 2.0, by + (bh - th_px as f32) / 2.0 - 16.0 * d, th_px as f32, th_px as f32);
        }
        put_text(&root, path, 11.5, 400, bx + 16.0 * d, by + bh - 34.0 * d, bw - 32.0 * d, 0.58, None);
    }

    popup.show_active();
    let _ = motion::pop_in(comp, popup.root(), Vector3 { X: w / 2.0, Y: h / 2.0, Z: 0.0 }, 0.92, 1.0, 0, 160);
    QL.with(|c| {
        *c.borrow_mut() = Some(Ql {
            popup,
            comp: comp.clone(),
            d,
            dark: look.dark,
            tl: tl_layer,
            tl_hover: false,
            path: path.to_string(),
            open_rect,
        });
    });
}

fn in_rect(r: (f32, f32, f32, f32), x: f32, y: f32) -> bool {
    x >= r.0 && x < r.0 + r.2 && y >= r.1 && y < r.1 + r.3
}

fn repaint_tl(q: &Ql) {
    if let Ok(k) = q.tl.Children() {
        let _ = k.RemoveAll();
    }
    traffic::render(
        &q.comp,
        &q.tl,
        traffic::Layout::tahoe(q.d),
        traffic::Opts { active: true, hover: q.tl_hover, dark: q.dark, can_min: false, can_zoom: false },
        q.d,
    );
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_NCHITTEST => {
            let mut r = windows::Win32::Foundation::RECT::default();
            let _ = GetWindowRect(hwnd, &mut r);
            let (px, py) = ((lp.0 as i16) as f32, ((lp.0 >> 16) as i16) as f32);
            let drag = QL.with(|c| {
                let g = c.borrow();
                let Some(q) = g.as_ref() else { return false };
                let (lx, ly) = (px - r.left as f32, py - r.top as f32);
                ly < 52.0 * q.d && !traffic::in_cluster(traffic::Layout::tahoe(q.d), lx, ly) && !in_rect(q.open_rect, lx, ly)
            });
            LRESULT(if drag { HTCAPTION } else { HTCLIENT } as isize)
        }
        WM_MOUSEMOVE => {
            let (x, y) = ((lp.0 as i16) as f32, ((lp.0 >> 16) as i16) as f32);
            let mut t = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
            let _ = TrackMouseEvent(&mut t);
            QL.with(|c| {
                if let Some(q) = c.borrow_mut().as_mut() {
                    let h = traffic::in_cluster(traffic::Layout::tahoe(q.d), x, y);
                    if h != q.tl_hover {
                        q.tl_hover = h;
                        repaint_tl(q);
                    }
                }
            });
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            QL.with(|c| {
                if let Some(q) = c.borrow_mut().as_mut() {
                    if q.tl_hover {
                        q.tl_hover = false;
                        repaint_tl(q);
                    }
                }
            });
            LRESULT(0)
        }
        WM_KEYDOWN => {
            let vk = wp.0 as u16;
            if vk == VK_ESCAPE.0 || vk == VK_SPACE.0 {
                close();
            } else if vk == VK_RETURN.0 {
                let p = QL.with(|c| c.borrow().as_ref().map(|q| q.path.clone()));
                close();
                if let Some(p) = p {
                    nacre_platform::launch::open(&p);
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let (x, y) = ((lp.0 as i16) as f32, ((lp.0 >> 16) as i16) as f32);
            let act = QL.with(|c| {
                let g = c.borrow();
                let q = g.as_ref()?;
                if traffic::hit(traffic::Layout::tahoe(q.d), x, y) == Some(traffic::Btn::Close) {
                    Some(None)
                } else if in_rect(q.open_rect, x, y) {
                    Some(Some(q.path.clone()))
                } else {
                    None
                }
            });
            match act {
                Some(None) => close(),
                Some(Some(p)) => {
                    close();
                    nacre_platform::launch::open(&p);
                }
                None => {}
            }
            LRESULT(0)
        }
        WM_ACTIVATE => {
            if (wp.0 & 0xFFFF) == 0 {
                let _ = PostMessageW(Some(hwnd), WM_APP + 95, WPARAM(0), LPARAM(0));
            }
            LRESULT(0)
        }
        m if m == WM_APP + 95 => {
            close();
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
