//! Window popup kecil (tooltip, pratinjau): window ber-Composition sendiri, dibuat satu kali dan
//! disembunyikan (`SW_HIDE`, tanpa biaya compositor) saat tidak dipakai.

use nacre_ui::glass::{GlassPanel, GlassParams};
use nacre_ui::host::{attach, Surface};
use windows::core::PCWSTR;
use windows::UI::Composition::{ContainerVisual, Compositor};
use windows::Win32::Foundation::{HINSTANCE, HWND};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows_core::Result;
use windows_numerics::Vector2;

pub struct Popup {
    pub hwnd: HWND,
    surface: Surface,
    pub panel: GlassPanel,
    pub content: ContainerVisual,
    pub rect: (i32, i32, i32, i32),
}

impl Popup {
    pub fn new(
        comp: &Compositor,
        class: PCWSTR,
        proc: WNDPROC,
        ex_extra: WINDOW_EX_STYLE,
        glass: GlassParams,
    ) -> Result<Popup> {
        Self::new_with(comp, class, proc, ex_extra, glass, false)
    }

    /// `activatable`: jendela bisa menerima fokus keyboard (Spotlight, Launchpad); selain itu tidak pernah mencuri fokus.
    pub fn new_with(
        comp: &Compositor,
        class: PCWSTR,
        proc: WNDPROC,
        ex_extra: WINDOW_EX_STYLE,
        glass: GlassParams,
        activatable: bool,
    ) -> Result<Popup> {
        unsafe {
            let hi: HINSTANCE = GetModuleHandleW(None)?.into();
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: proc,
                hInstance: hi,
                lpszClassName: class,
                hCursor: LoadCursorW(None, IDC_ARROW)?,
                ..Default::default()
            });
            let hwnd = CreateWindowExW(
                WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | if activatable { WINDOW_EX_STYLE(0) } else { WS_EX_NOACTIVATE } | ex_extra,
                class,
                PCWSTR::null(),
                WS_POPUP,
                0, 0, 10, 10,
                None, None, Some(hi), None,
            )?;
            let surface = attach(comp, hwnd, (10.0, 10.0))?;
            let panel = GlassPanel::new(comp, &surface.root, glass)?;
            let content = comp.CreateContainerVisual()?;
            surface.root.Children()?.InsertAtTop(&content)?;
            Ok(Popup { hwnd, surface, panel, content, rect: (0, 0, 10, 10) })
        }
    }

    /// Posisi/ukuran dalam piksel layar. Kartu kaca mengisi seluruh window.
    pub fn set_rect(&mut self, x: i32, y: i32, w: i32, h: i32) -> Result<()> {
        self.rect = (x, y, w, h);
        unsafe {
            let _ = SetWindowPos(self.hwnd, None, x, y, w, h, SWP_NOACTIVATE | SWP_NOZORDER);
        }
        self.surface.root.SetSize(Vector2 { X: w as f32, Y: h as f32 })?;
        self.panel.set_rect(0.0, 0.0, w as f32, h as f32)
    }

    /// Tampilkan dan ambil fokus keyboard (untuk popup `activatable`).
    pub fn show_active(&self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOW);
            let _ = SetForegroundWindow(self.hwnd);
        }
    }

    pub fn show(&self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
        }
    }

    /// Animasi muncul ala macOS: pudar masuk + skala 0,97→1 dari tepi atas (dropdown) atau bawah (menu konteks).
    pub fn fade_in(&self, comp: &Compositor, from_top: bool) {
        use windows::Foundation::TimeSpan;
        use windows::UI::Composition::{CompositionEasingFunction, CompositionObject};
        use windows_core::{Interface, HSTRING};
        let go = || -> Result<()> {
            let root = &self.surface.root;
            let (w, h) = (self.rect.2 as f32, self.rect.3 as f32);
            root.SetCenterPoint(windows_numerics::Vector3 { X: w / 2.0, Y: if from_top { 0.0 } else { h }, Z: 0.0 })?;
            let ease: CompositionEasingFunction =
                comp.CreateCubicBezierEasingFunction(Vector2 { X: 0.2, Y: 0.0 }, Vector2 { X: 0.0, Y: 1.0 })?.cast()?;
            let dur = TimeSpan { Duration: 170 * 10_000 };
            let o = comp.CreateScalarKeyFrameAnimation()?;
            o.InsertKeyFrame(0.0, 0.0)?;
            o.InsertKeyFrameWithEasingFunction(1.0, 1.0, &ease)?;
            o.SetDuration(dur)?;
            root.cast::<CompositionObject>()?.StartAnimation(&HSTRING::from("Opacity"), &o)?;
            let s = comp.CreateVector3KeyFrameAnimation()?;
            s.InsertKeyFrame(0.0, windows_numerics::Vector3 { X: 0.97, Y: 0.94, Z: 1.0 })?;
            s.InsertKeyFrameWithEasingFunction(1.0, windows_numerics::Vector3 { X: 1.0, Y: 1.0, Z: 1.0 }, &ease)?;
            s.SetDuration(dur)?;
            root.cast::<CompositionObject>()?.StartAnimation(&HSTRING::from("Scale"), &s)
        };
        let _ = go();
    }

    /// Animasi muncul layar penuh ala Launchpad: membesar dari 1,12 ke 1 sambil memudar masuk (±240 ms).
    pub fn zoom_in(&self, comp: &Compositor) {
        use windows::Foundation::TimeSpan;
        use windows::UI::Composition::{CompositionEasingFunction, CompositionObject};
        use windows_core::{Interface, HSTRING};
        let go = || -> Result<()> {
            let root = &self.surface.root;
            let (w, h) = (self.rect.2 as f32, self.rect.3 as f32);
            root.SetCenterPoint(windows_numerics::Vector3 { X: w / 2.0, Y: h / 2.0, Z: 0.0 })?;
            let ease: CompositionEasingFunction =
                comp.CreateCubicBezierEasingFunction(Vector2 { X: 0.2, Y: 0.0 }, Vector2 { X: 0.0, Y: 1.0 })?.cast()?;
            let dur = TimeSpan { Duration: 240 * 10_000 };
            let o = comp.CreateScalarKeyFrameAnimation()?;
            o.InsertKeyFrame(0.0, 0.0)?;
            o.InsertKeyFrameWithEasingFunction(1.0, 1.0, &ease)?;
            o.SetDuration(dur)?;
            root.cast::<CompositionObject>()?.StartAnimation(&HSTRING::from("Opacity"), &o)?;
            let s = comp.CreateVector3KeyFrameAnimation()?;
            s.InsertKeyFrame(0.0, windows_numerics::Vector3 { X: 1.12, Y: 1.12, Z: 1.0 })?;
            s.InsertKeyFrameWithEasingFunction(1.0, windows_numerics::Vector3 { X: 1.0, Y: 1.0, Z: 1.0 }, &ease)?;
            s.SetDuration(dur)?;
            root.cast::<CompositionObject>()?.StartAnimation(&HSTRING::from("Scale"), &s)
        };
        let _ = go();
    }

    /// Animasi tutup layar penuh ala Launchpad: membesar halus ke 1,08 sambil memudar (±170 ms).
    pub fn zoom_out(&self, comp: &Compositor) {
        let root = &self.surface.root;
        let (w, h) = (self.rect.2 as f32, self.rect.3 as f32);
        let _ = root.SetCenterPoint(windows_numerics::Vector3 { X: w / 2.0, Y: h / 2.0, Z: 0.0 });
        let _ = crate::motion::vec3(comp, root, "Scale", windows_numerics::Vector3 { X: 1.0, Y: 1.0, Z: 1.0 }, windows_numerics::Vector3 { X: 1.08, Y: 1.08, Z: 1.0 }, 170);
        let _ = crate::motion::scalar(comp, root, "Opacity", 1.0, 0.0, 170);
        let _ = root.SetOpacity(0.0);
    }

    /// Visual akar window (untuk animasi buka/tutup khusus).
    pub fn root(&self) -> &ContainerVisual {
        &self.surface.root
    }

    pub fn hide(&self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
    }

    pub fn is_visible(&self) -> bool {
        unsafe { IsWindowVisible(self.hwnd).as_bool() }
    }
}
