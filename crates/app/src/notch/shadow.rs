//! Bayangan tipis notch. Jendela notch dipotong oleh region kliknya, jadi bayangan digambar di jendela tersendiri yang
//! tembus klik, tepat di bawah notch dalam urutan-Z. Sprite nine-grid digerakkan pegas yang sama dengan badan notch.

use super::spring;
use nacre_core::notch::shade;
use nacre_ui::gfx::Gfx;
use nacre_ui::host::Surface;
use std::collections::HashMap;
use windows::core::w;
use windows::UI::Composition::{CompositionBrush, CompositionObject, CompositionStretch, Compositor, ContainerVisual, SpriteVisual};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows_core::{Interface, HSTRING};
use windows_numerics::{Vector2, Vector3};

pub struct Shadow {
    pub hwnd: HWND,
    comp: Compositor,
    _surface: Surface,
    stage: ContainerVisual,
    sprite: SpriteVisual,
    brushes: HashMap<(i32, i32, i32), (CompositionBrush, f32, f32)>,
    key: (i32, i32, i32),
    /// Jarak tepi kiri jendela bayangan dari tepi kiri jendela notch (px).
    pub pad: f32,
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

impl Shadow {
    pub fn new(comp: &Compositor, size: (i32, i32), pad: f32) -> windows_core::Result<Shadow> {
        unsafe {
            let hi: HINSTANCE = GetModuleHandleW(None)?.into();
            let class = w!("AbraNotchShadow");
            RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(proc), hInstance: hi, lpszClassName: class, ..Default::default() });
            let hwnd = CreateWindowExW(
                // LAYERED + TRANSPARENT = benar-benar tembus klik untuk semua aplikasi. TRANSPARENT saja (atau
                // HTTRANSPARENT) hanya meneruskan klik ke jendela di thread yang sama, sehingga kotak bayangan ini dulu
                // menelan klik di sekitar notch (mis. tab Chrome) dan mengganggu tangkapan layar.
                WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_LAYERED,
                class,
                w!("abra-notch-shadow"),
                WS_POPUP,
                0, 0, size.0, size.1,
                None, None, Some(hi), None,
            )?;
            let _ = SetLayeredWindowAttributes(hwnd, windows::Win32::Foundation::COLORREF(0), 255, LWA_ALPHA);
            let surface = nacre_ui::host::attach(comp, hwnd, (size.0 as f32, size.1 as f32))?;
            let stage = comp.CreateContainerVisual()?;
            surface.root.Children()?.InsertAtTop(&stage)?;
            let sprite = comp.CreateSpriteVisual()?;
            sprite.SetOpacity(0.0)?;
            stage.Children()?.InsertAtTop(&sprite)?;
            Ok(Shadow { hwnd, comp: comp.clone(), _surface: surface, stage, sprite, brushes: HashMap::new(), key: (0, 0, 0), pad })
        }
    }

    /// Letakkan jendela bayangan tepat di bawah `notch` (sama-sama paling atas).
    pub fn place_window(&self, notch: HWND, x: i32, y: i32, w: i32, h: i32) {
        unsafe {
            let _ = SetWindowPos(self.hwnd, Some(notch), x, y, w, h, SWP_NOACTIVATE | SWP_SHOWWINDOW);
        }
        let _ = self._surface.root.SetSize(Vector2 { X: w as f32, Y: h as f32 });
    }

    pub fn set_visible(&self, on: bool) {
        unsafe {
            let _ = ShowWindow(self.hwnd, if on { SW_SHOWNOACTIVATE } else { SW_HIDE });
        }
    }

    fn brush(&mut self, gfx: &Gfx, radius: f32, blur: f32, dy: f32) -> Option<(CompositionBrush, f32, f32)> {
        let key = (radius.round() as i32, blur.round() as i32, dy.round() as i32);
        if let Some(b) = self.brushes.get(&key) {
            return Some(b.clone());
        }
        let s = shade::rounded(radius, blur, dy);
        let make = || -> windows_core::Result<CompositionBrush> {
            let surf = gfx.surface_from_bgra(s.w, s.h, &s.bgra)?;
            let src = self.comp.CreateSurfaceBrushWithSurface(&surf)?;
            // tanpa Fill, sumber digambar seukuran aslinya di tengah dan nine-grid tidak meregang
            src.SetStretch(CompositionStretch::Fill)?;
            let ng = self.comp.CreateNineGridBrush()?;
            ng.SetSource(&src)?;
            let (l, t, r, b) = s.insets;
            ng.SetInsetsWithValues(l, t, r, b)?;
            ng.cast()
        };
        let b = (make().ok()?, s.margin as f32, s.dy as f32);
        if self.brushes.len() > 16 {
            self.brushes.clear();
        }
        self.brushes.insert(key, b.clone());
        Some(b)
    }

    /// Ikuti badan notch: `body` = (x, y, w, h) dalam koordinat jendela notch (termasuk bagian di atas layar),
    /// `look` = (blur, geser y, opasitas) dalam px. `spring_params` = None untuk langsung (tanpa animasi).
    pub fn follow(&mut self, gfx: &Gfx, body: (f32, f32, f32, f32), radius: f32, look: (f32, f32, f32), spring_params: Option<(f32, u32)>) {
        let (blur, dy, opacity) = look;
        let Some((brush, m, dy)) = self.brush(gfx, radius, blur, dy) else { return };
        let key = (radius.round() as i32, blur.round() as i32, dy.round() as i32);
        if key != self.key {
            let _ = self.sprite.SetBrush(&brush);
            self.key = key;
        }
        let (x, y, w, h) = (body.0 + self.pad - m, body.1 - m, body.2 + 2.0 * m, body.3 + 2.0 * m + dy);
        match spring_params {
            None => {
                let _ = self.sprite.SetOffset(Vector3 { X: x, Y: y, Z: 0.0 });
                let _ = self.sprite.SetSize(Vector2 { X: w, Y: h });
                let _ = self.sprite.SetOpacity(opacity);
            }
            Some((ratio, period)) => {
                let o: CompositionObject = self.sprite.cast().unwrap();
                spring(&self.comp, &o, "Offset.X", x, ratio, period);
                spring(&self.comp, &o, "Offset.Y", y, ratio, period);
                spring(&self.comp, &o, "Size.X", w, ratio, period);
                spring(&self.comp, &o, "Size.Y", h, ratio, period);
                let _ = fade(&self.comp, &o, opacity, 160);
            }
        }
    }

    /// Geser seluruh bayangan (mis. notch menyingkir ke atas).
    pub fn stage_y(&self, y: f32, ratio: f32, period: u32) {
        let o: CompositionObject = self.stage.cast().unwrap();
        spring(&self.comp, &o, "Offset.Y", y, ratio, period);
    }
}

fn fade(comp: &Compositor, o: &CompositionObject, to: f32, ms: u32) -> windows_core::Result<()> {
    let a = comp.CreateScalarKeyFrameAnimation()?;
    a.InsertKeyFrame(1.0, to)?;
    a.SetDuration(windows::Foundation::TimeSpan { Duration: ms as i64 * 10_000 })?;
    o.StartAnimation(&HSTRING::from("Opacity"), &a)
}
