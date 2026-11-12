//! Kartu pratinjau window: thumbnail DWM hidup + judul, di atas ikon yang di-hover.
//! Thumbnail digambar DWM di atas visual dan berbentuk persegi (Spike C), jadi judul diletakkan di bawahnya.

use crate::popup::Popup;
use nacre_platform::text::{render, Style};
use nacre_platform::wins::thumbnail;
use nacre_ui::gfx::Gfx;
use nacre_ui::glass::GlassParams;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use windows::core::w;
use windows::UI::Composition::{CompositionDrawingSurface, CompositionStretch, Compositor};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows_core::Result;
use windows_numerics::{Vector2, Vector3};

pub const PREVIEW_CLICK: u32 = WM_APP + 7;
pub const PREVIEW_ENTER: u32 = WM_APP + 8;
pub const PREVIEW_LEAVE: u32 = WM_APP + 9;
const WM_MOUSELEAVE: u32 = 0x02A3;

static TARGET: AtomicIsize = AtomicIsize::new(0);
static INSIDE: AtomicBool = AtomicBool::new(false);

unsafe extern "system" fn proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    let target = HWND(TARGET.load(Ordering::Relaxed) as *mut _);
    match m {
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_MOUSEMOVE => {
            if !INSIDE.swap(true, Ordering::Relaxed) {
                let mut t = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: h,
                    dwHoverTime: 0,
                };
                let _ = TrackMouseEvent(&mut t);
                let _ = PostMessageW(Some(target), PREVIEW_ENTER, WPARAM(0), LPARAM(0));
            }
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            INSIDE.store(false, Ordering::Relaxed);
            let _ = PostMessageW(Some(target), PREVIEW_LEAVE, WPARAM(0), LPARAM(0));
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let _ = PostMessageW(Some(target), PREVIEW_CLICK, WPARAM((l.0 as i16) as isize as usize), LPARAM(0));
            LRESULT(0)
        }
        _ => DefWindowProcW(h, m, w, l),
    }
}

struct Cell {
    left: i32,
    right: i32,
    hwnd: isize,
    thumb: Option<isize>,
}

pub struct Preview {
    popup: Popup,
    cells: Vec<Cell>,
    dpi: f32,
    screen_w: i32,
}

impl Preview {
    pub fn new(comp: &Compositor, dock_hwnd: HWND, dpi: f32, screen_w: i32) -> Result<Preview> {
        TARGET.store(dock_hwnd.0 as isize, Ordering::Relaxed);
        let popup = Popup::new(
            comp,
            w!("NacrePreview"),
            Some(proc),
            WINDOW_EX_STYLE(0),
            crate::prefs::glass((10.0, 10.0), 18.0 * dpi, crate::prefs::Role::Dark),
        )?;
        Ok(Preview { popup, cells: vec![], dpi, screen_w })
    }

    pub fn panel_mut(&mut self) -> &mut nacre_ui::glass::GlassPanel {
        &mut self.popup.panel
    }

    pub fn is_visible(&self) -> bool {
        self.popup.is_visible()
    }

    /// `windows`: (hwnd, judul). `cx`: pusat horizontal; `bottom`: tepi bawah kartu (px layar).
    /// `windows`: (hwnd, judul, diminimize?). Window yang diminimize tidak punya gambar hidup di DWM, jadi
    /// diganti ikon aplikasi (`icon`) yang diredupkan di tengah kotak thumbnail.
    pub fn show(
        &mut self,
        comp: &Compositor,
        gfx: &Gfx,
        windows: &[(isize, String, bool)],
        icon: Option<&CompositionDrawingSurface>,
        cx: i32,
        bottom: i32,
    ) -> Result<()> {
        self.clear();
        let d = self.dpi;
        let n = windows.len().min(6);
        if n == 0 {
            return Ok(());
        }
        let (tw, th) = ((200.0 * d) as i32, (125.0 * d) as i32);
        let (pad, gap, title_h) = ((12.0 * d) as i32, (6.0 * d) as i32, (20.0 * d) as i32);
        let w = pad + n as i32 * (tw + pad);
        let h = pad + th + gap + title_h + pad;
        let x = (cx - w / 2).clamp(4, (self.screen_w - w - 4).max(4));
        self.popup.set_rect(x, bottom - h, w, h)?;
        self.popup.panel.set_radius(18.0 * d)?;
        self.popup.show();

        let style = Style { px: (12.0 * d).round() as i32, weight: 500 };
        for (i, (hwnd, title, minimized)) in windows.iter().take(n).enumerate() {
            let x0 = pad + i as i32 * (tw + pad);
            let thumb = if *minimized { None } else { thumbnail::register(self.popup.hwnd.0 as isize, *hwnd) };
            if *minimized {
                if let Some(s) = icon {
                    let sz = (64.0 * d) as i32;
                    let brush = comp.CreateSurfaceBrushWithSurface(s)?;
                    brush.SetStretch(CompositionStretch::Uniform)?;
                    let sprite = comp.CreateSpriteVisual()?;
                    sprite.SetBrush(&brush)?;
                    sprite.SetSize(Vector2 { X: sz as f32, Y: sz as f32 })?;
                    sprite.SetOffset(Vector3 { X: (x0 + (tw - sz) / 2) as f32, Y: (pad + (th - sz) / 2) as f32, Z: 0.0 })?;
                    sprite.SetOpacity(0.55)?;
                    self.popup.content.Children()?.InsertAtTop(&sprite)?;
                }
            }
            if let Some(t) = thumb {
                let (sw, sh) = thumbnail::source_size(t).filter(|s| s.0 > 0 && s.1 > 0).unwrap_or((tw, th));
                let k = (tw as f32 / sw as f32).min(th as f32 / sh as f32).min(1.0);
                let (dw, dh) = (((sw as f32 * k) as i32).max(1), ((sh as f32 * k) as i32).max(1));
                let (dx, dy) = (x0 + (tw - dw) / 2, pad + (th - dh) / 2);
                thumbnail::place(t, (dx, dy, dx + dw, dy + dh), 255);
            }
            if let Some(bmp) = render(title, &style, tw, title_h) {
                let surface = gfx.surface_from_bgra(bmp.w, bmp.h, &bmp.bgra)?;
                let brush = comp.CreateSurfaceBrushWithSurface(&surface)?;
                brush.SetStretch(CompositionStretch::None)?;
                let sprite = comp.CreateSpriteVisual()?;
                sprite.SetBrush(&brush)?;
                sprite.SetSize(Vector2 { X: tw as f32, Y: title_h as f32 })?;
                sprite.SetOffset(Vector3 { X: x0 as f32, Y: (pad + th + gap) as f32, Z: 0.0 })?;
                self.popup.content.Children()?.InsertAtTop(&sprite)?;
            }
            self.cells.push(Cell { left: x0 - pad / 2, right: x0 + tw + pad / 2, hwnd: *hwnd, thumb });
        }
        Ok(())
    }

    /// Window di bawah koordinat klien `x`.
    pub fn hit(&self, x: i32) -> Option<isize> {
        self.cells.iter().find(|c| x >= c.left && x < c.right).map(|c| c.hwnd)
    }

    fn clear(&mut self) {
        for c in self.cells.drain(..) {
            if let Some(t) = c.thumb {
                thumbnail::unregister(t);
            }
        }
        if let Ok(kids) = self.popup.content.Children() {
            let _ = kids.RemoveAll();
        }
    }

    pub fn hide(&mut self) {
        if self.popup.is_visible() || !self.cells.is_empty() {
            self.clear();
            self.popup.hide();
        }
    }
}

