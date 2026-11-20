use crate::popup::Popup;
use crate::prefs;
use nacre_platform::launch;
use nacre_platform::text::{measure, render, Style};
use nacre_ui::gfx::Gfx;
use std::cell::RefCell;
use windows::core::w;
use windows::UI::Color;
use windows::UI::Composition::{Compositor, ContainerVisual};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_ESCAPE, VK_RETURN};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows_numerics::{Vector2, Vector3};

type Rect = (f32, f32, f32, f32);

struct Confirm {
    popup: Popup,
    action: Option<(&'static str, &'static str)>,
    callback: Option<Box<dyn FnOnce()>>,
    yes: Rect,
    cancel: Rect,
}

thread_local! {
    static ACTIVE: RefCell<Option<Confirm>> = const { RefCell::new(None) };
}

pub fn show(comp: &Compositor, bounds: RECT, dpi: f32, question: &'static str, file: &'static str, args: &'static str) {
    show_inner(comp, bounds, dpi, question, Some((file, args)), None);
}

pub fn show_action(comp: &Compositor, bounds: RECT, dpi: f32, question: &'static str, action: impl FnOnce() + 'static) {
    show_inner(comp, bounds, dpi, question, None, Some(Box::new(action)));
}

fn show_inner(
    comp: &Compositor,
    bounds: RECT,
    dpi: f32,
    question: &'static str,
    action: Option<(&'static str, &'static str)>,
    callback: Option<Box<dyn FnOnce()>>,
) {
    close();
    let d = dpi.max(0.8);
    let width = (420.0 * d).round() as i32;
    let height = (220.0 * d).round() as i32;
    let x = bounds.left + ((bounds.right - bounds.left - width) / 2).max(0);
    let y = bounds.top + ((bounds.bottom - bounds.top - height) / 2).max(0);
    let mut popup = match Popup::new_with(
        comp,
        w!("AbraConfirmDialog"),
        Some(confirm_proc),
        WINDOW_EX_STYLE(0),
        prefs::glass((10.0, 10.0), 22.0 * d, prefs::Role::Dock),
        true,
    ) {
        Ok(p) => p,
        Err(e) => {
            nacre_platform::log::line("confirm", &format!("gagal membuat dialog: {e}"));
            return;
        }
    };
    let conditions = nacre_platform::sys::conditions();
    let tier = if !conditions.transparency || conditions.battery_saver || conditions.remote_session {
        nacre_ui::glass::Tier::Solid
    } else {
        nacre_ui::glass::Tier::Full
    };
    let _ = popup.panel.set_tier(tier);
    if popup.set_rect(x, y, width, height).is_err() {
        return;
    }
    let yes = (width as f32 - 130.0 * d, height as f32 - 60.0 * d, 106.0 * d, 38.0 * d);
    let cancel = (width as f32 - 250.0 * d, yes.1, 106.0 * d, 38.0 * d);
    paint(comp, &popup.content, &question, d, width as f32, height as f32, yes, cancel);
    let hwnd = popup.hwnd;
    ACTIVE.with(|slot| *slot.borrow_mut() = Some(Confirm { popup, action, callback, yes, cancel }));
    ACTIVE.with(|slot| {
        if let Some(dialog) = slot.borrow().as_ref() {
            dialog.popup.show_active();
            dialog.popup.fade_in(comp, false);
        }
    });
    unsafe { let _ = SetForegroundWindow(hwnd); }
}

fn paint(comp: &Compositor, parent: &ContainerVisual, question: &'static str, d: f32, w: f32, h: f32, yes: Rect, cancel: Rect) {
    let fg = prefs::look(prefs::Role::Dock).fg;
    text(comp, parent, "ABRA", (24.0 * d, 27.0 * d), (16.0 * d) as i32, 600, fg, 0.9);
    text(comp, parent, &prefs::tr(question), (24.0 * d, 86.0 * d), (20.0 * d) as i32, 500, fg, 1.0);
    rect(comp, parent, (24.0 * d, h - 79.0 * d, w - 48.0 * d, 1.0), 0.0, Color { A: 34, R: fg[0], G: fg[1], B: fg[2] });
    rect(comp, parent, cancel, 12.0 * d, Color { A: 32, R: fg[0], G: fg[1], B: fg[2] });
    rect(comp, parent, yes, 12.0 * d, prefs::accent_c(225));
    text_center(comp, parent, prefs::tr("Cancelar"), cancel, d, fg, 0.82);
    text_center(comp, parent, prefs::tr("Ya"), yes, d, [255, 255, 255], 1.0);
}

fn text_center(comp: &Compositor, parent: &ContainerVisual, label: &str, r: Rect, d: f32, rgb: [u8; 3], opacity: f32) {
    let px = (13.0 * d).round() as i32;
    let style = Style { px, weight: 600 };
    let (tw, th) = measure(label, &style);
    text(comp, parent, label, (r.0 + (r.2 - tw as f32) / 2.0, r.1 + (r.3 - th as f32) / 2.0), px, 600, rgb, opacity);
}

fn text(comp: &Compositor, parent: &ContainerVisual, label: &str, at: (f32, f32), px: i32, weight: i32, rgb: [u8; 3], opacity: f32) {
    let style = Style { px, weight };
    let (w, h) = measure(label, &style);
    let Some(bitmap) = render(label, &style, w + 3, h + 2) else { return };
    let mut data = bitmap.bgra;
    for p in data.chunks_exact_mut(4) {
        let a = p[3] as u32;
        p[0] = (rgb[2] as u32 * a / 255) as u8;
        p[1] = (rgb[1] as u32 * a / 255) as u8;
        p[2] = (rgb[0] as u32 * a / 255) as u8;
    }
    let Ok(gfx) = Gfx::shared(comp) else { return };
    let Ok(surface) = gfx.surface_from_bgra(bitmap.w, bitmap.h, &data) else { return };
    let go = || -> windows::core::Result<()> {
        let brush = comp.CreateSurfaceBrushWithSurface(&surface)?;
        let visual = comp.CreateSpriteVisual()?;
        visual.SetBrush(&brush)?;
        visual.SetSize(Vector2 { X: bitmap.w as f32, Y: bitmap.h as f32 })?;
        visual.SetOffset(Vector3 { X: at.0, Y: at.1, Z: 0.0 })?;
        visual.SetOpacity(opacity)?;
        parent.Children()?.InsertAtTop(&visual)
    };
    let _ = go();
}

fn rect(comp: &Compositor, parent: &ContainerVisual, r: Rect, radius: f32, color: Color) {
    let go = || -> windows::core::Result<()> {
        let visual = comp.CreateSpriteVisual()?;
        visual.SetSize(Vector2 { X: r.2, Y: r.3 })?;
        visual.SetOffset(Vector3 { X: r.0, Y: r.1, Z: 0.0 })?;
        visual.SetBrush(&comp.CreateColorBrushWithColor(color)?)?;
        if radius > 0.0 {
            let geometry = comp.CreateRoundedRectangleGeometry()?;
            geometry.SetSize(Vector2 { X: r.2, Y: r.3 })?;
            geometry.SetCornerRadius(Vector2 { X: radius, Y: radius })?;
            let clip = comp.CreateGeometricClip()?;
            clip.SetGeometry(&geometry)?;
            visual.SetClip(&clip)?;
        }
        parent.Children()?.InsertAtTop(&visual)
    };
    let _ = go();
}

fn close() {
    let hwnd = ACTIVE.with(|slot| slot.borrow_mut().take().map(|dialog| dialog.popup.hwnd));
    if let Some(hwnd) = hwnd {
        unsafe { let _ = DestroyWindow(hwnd); }
    }
}

fn accept() {
    let selected = ACTIVE.with(|slot| slot.borrow_mut().take().map(|mut dialog| {
        let hwnd = dialog.popup.hwnd;
        let action = dialog.action;
        let callback = dialog.callback.take();
        unsafe { let _ = DestroyWindow(hwnd); }
        (action, callback)
    }));
    match selected {
        Some((Some((file, args)), _)) => launch::run(file, args),
        Some((_, Some(callback))) => callback(),
        _ => {}
    }
}

fn inside(r: Rect, x: f32, y: f32) -> bool {
    x >= r.0 && x < r.0 + r.2 && y >= r.1 && y < r.1 + r.3
}

unsafe extern "system" fn confirm_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_LBUTTONUP => {
            let (x, y) = ((lp.0 as i16) as f32, ((lp.0 >> 16) as i16) as f32);
            let choice = ACTIVE.with(|slot| slot.borrow().as_ref().and_then(|dialog| {
                if inside(dialog.yes, x, y) { Some(true) }
                else if inside(dialog.cancel, x, y) { Some(false) }
                else { None }
            }));
            if let Some(choice) = choice {
                if choice {
                    accept();
                } else {
                    close();
                }
            }
            LRESULT(0)
        }
        WM_KEYDOWN if wp.0 == VK_ESCAPE.0 as usize => { close(); LRESULT(0) }
        WM_KEYDOWN if wp.0 == VK_RETURN.0 as usize => {
            accept();
            LRESULT(0)
        }
        WM_CLOSE => { close(); LRESULT(0) }
        WM_MOUSEACTIVATE => LRESULT(MA_ACTIVATE as isize),
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
