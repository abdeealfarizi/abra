use windows::UI::Color;
use windows::UI::Composition::{Compositor, ContainerVisual};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{IsZoomed, PostMessageW, ShowWindow, SW_MAXIMIZE, SW_MINIMIZE, SW_RESTORE, WM_CLOSE};
use windows_numerics::{Vector2, Vector3};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Btn {
    Close,
    Minimize,
    Zoom,
}

#[derive(Clone, Copy, Debug)]
pub struct Opts {
    pub active: bool,
    pub hover: bool,
    pub dark: bool,
    pub can_min: bool,
    pub can_zoom: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub cx0: f32,
    pub cy: f32,
    pub dia: f32,
    pub step: f32,
}

impl Layout {
    /// Concentric with macOS Tahoe's 26dp window corner radius.
    pub fn tahoe(d: f32) -> Self {
        Self { cx0: 26.0 * d, cy: 26.0 * d, dia: 13.0 * d, step: 21.0 * d }
    }

    pub fn pill(d: f32) -> Self {
        Self { cx0: 14.0 * d, cy: 12.0 * d, dia: 12.0 * d, step: 19.0 * d }
    }

    pub fn center(self, i: usize) -> (f32, f32) {
        (self.cx0 + i as f32 * self.step, self.cy)
    }
}

pub fn in_cluster(l: Layout, x: f32, y: f32) -> bool {
    let pad = (l.step - l.dia).max(4.0) * 0.65;
    let r = l.dia / 2.0 + pad;
    let x0 = l.cx0 - r;
    let x1 = l.cx0 + 2.0 * l.step + r;
    x >= x0 && x <= x1 && y >= l.cy - r && y <= l.cy + r
}

pub fn hit(l: Layout, x: f32, y: f32) -> Option<Btn> {
    let r = (l.dia / 2.0 + 3.0).max(l.step * 0.45);
    let r2 = r * r;
    for (i, btn) in [Btn::Close, Btn::Minimize, Btn::Zoom].into_iter().enumerate() {
        let (cx, cy) = l.center(i);
        let dx = x - cx;
        let dy = y - cy;
        if dx * dx + dy * dy <= r2 {
            return Some(btn);
        }
    }
    None
}

fn put_circle(comp: &Compositor, parent: &ContainerVisual, cx: f32, cy: f32, dia: f32, color: Color) {
    let go = || -> windows_core::Result<()> {
        let v = comp.CreateSpriteVisual()?;
        v.SetSize(Vector2 { X: dia, Y: dia })?;
        v.SetOffset(Vector3 { X: cx - dia / 2.0, Y: cy - dia / 2.0, Z: 0.0 })?;
        v.SetBrush(&comp.CreateColorBrushWithColor(color)?)?;
        let g = comp.CreateRoundedRectangleGeometry()?;
        g.SetSize(Vector2 { X: dia, Y: dia })?;
        g.SetCornerRadius(Vector2 { X: dia / 2.0, Y: dia / 2.0 })?;
        let clip = comp.CreateGeometricClip()?;
        clip.SetGeometry(&g)?;
        v.SetClip(&clip)?;
        parent.Children()?.InsertAtTop(&v)
    };
    let _ = go();
}

fn put_bar(comp: &Compositor, parent: &ContainerVisual, cx: f32, cy: f32, w: f32, h: f32, deg: f32, color: Color) {
    let go = || -> windows_core::Result<()> {
        let v = comp.CreateSpriteVisual()?;
        v.SetSize(Vector2 { X: w, Y: h })?;
        v.SetOffset(Vector3 { X: cx - w / 2.0, Y: cy - h / 2.0, Z: 0.0 })?;
        v.SetCenterPoint(Vector3 { X: w / 2.0, Y: h / 2.0, Z: 0.0 })?;
        if deg != 0.0 {
            v.SetRotationAngleInDegrees(deg)?;
        }
        v.SetBrush(&comp.CreateColorBrushWithColor(color)?)?;
        let g = comp.CreateRoundedRectangleGeometry()?;
        g.SetSize(Vector2 { X: w, Y: h })?;
        g.SetCornerRadius(Vector2 { X: h / 2.0, Y: h / 2.0 })?;
        let clip = comp.CreateGeometricClip()?;
        clip.SetGeometry(&g)?;
        v.SetClip(&clip)?;
        parent.Children()?.InsertAtTop(&v)
    };
    let _ = go();
}

pub fn render(comp: &Compositor, parent: &ContainerVisual, l: Layout, opts: Opts, d: f32) {
    let lit = opts.active || opts.hover;
    let specs: [(Btn, bool, [u8; 3], [u8; 3]); 3] = [
        (Btn::Close, true, [255, 95, 87], [78, 12, 10]),
        (Btn::Minimize, opts.can_min, [254, 188, 46], [104, 62, 5]),
        (Btn::Zoom, opts.can_zoom, [40, 200, 64], [8, 76, 18]),
    ];
    for (i, (btn, enabled, rgb, ink)) in specs.into_iter().enumerate() {
        let (cx, cy) = l.center(i);
        if lit && enabled {
            let rim = if opts.dark { Color { A: 42, R: 255, G: 255, B: 255 } } else { Color { A: 34, R: 0, G: 0, B: 0 } };
            put_circle(comp, parent, cx, cy, l.dia + 1.0 * d, rim);
            put_circle(comp, parent, cx, cy, l.dia, Color { A: 255, R: rgb[0], G: rgb[1], B: rgb[2] });
            // Subtle top specular highlight matching Tahoe Liquid Glass buttons.
            put_bar(comp, parent, cx, cy - l.dia * 0.24, l.dia * 0.46, (1.8 * d).max(1.2), 0.0, Color { A: 54, R: 255, G: 255, B: 255 });
            if opts.hover {
                let c = Color { A: 215, R: ink[0], G: ink[1], B: ink[2] };
                let bar = (l.dia * 0.56).max(5.0);
                let thick = (1.35 * d).max(1.1);
                match btn {
                    Btn::Close => {
                        put_bar(comp, parent, cx, cy, bar, thick, 45.0, c);
                        put_bar(comp, parent, cx, cy, bar, thick, -45.0, c);
                    }
                    Btn::Minimize => {
                        put_bar(comp, parent, cx, cy, bar, thick, 0.0, c);
                    }
                    Btn::Zoom => {
                        put_bar(comp, parent, cx, cy, bar, thick, 0.0, c);
                        put_bar(comp, parent, cx, cy, bar, thick, 90.0, c);
                    }
                }
            }
        } else {
            let muted = if opts.dark {
                Color { A: if enabled { 72 } else { 36 }, R: 255, G: 255, B: 255 }
            } else {
                Color { A: if enabled { 52 } else { 26 }, R: 0, G: 0, B: 0 }
            };
            put_circle(comp, parent, cx, cy, l.dia, muted);
        }
    }
}

pub fn apply_external(target: isize, btn: Btn) {
    if target == 0 {
        return;
    }
    unsafe {
        let hw = HWND(target as *mut _);
        match btn {
            Btn::Close => {
                let _ = PostMessageW(Some(hw), WM_CLOSE, windows::Win32::Foundation::WPARAM(0), windows::Win32::Foundation::LPARAM(0));
            }
            Btn::Minimize => {
                let _ = ShowWindow(hw, SW_MINIMIZE);
            }
            Btn::Zoom => {
                let cmd = if IsZoomed(hw).as_bool() { SW_RESTORE } else { SW_MAXIMIZE };
                let _ = ShowWindow(hw, cmd);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tahoe_hit_matches_each_button_and_cluster() {
        let l = Layout::tahoe(1.0);
        assert_eq!(hit(l, l.cx0, l.cy), Some(Btn::Close));
        assert_eq!(hit(l, l.cx0 + l.step, l.cy), Some(Btn::Minimize));
        assert_eq!(hit(l, l.cx0 + 2.0 * l.step, l.cy), Some(Btn::Zoom));
        assert!(in_cluster(l, l.cx0 + l.step, l.cy));
        assert!(!in_cluster(l, 200.0, 200.0));
        assert_eq!(hit(l, 200.0, 200.0), None);
    }
}
