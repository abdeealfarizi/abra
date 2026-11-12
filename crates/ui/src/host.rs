//! Host Composition: DispatcherQueue + Compositor, dibagi semua window.

use windows::System::DispatcherQueueController;
use windows::UI::Composition::Desktop::DesktopWindowTarget;
use windows::UI::Composition::{Compositor, ContainerVisual};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::WinRT::Composition::ICompositorDesktopInterop;
use windows::Win32::System::WinRT::{
    CreateDispatcherQueueController, DispatcherQueueOptions, DQTAT_COM_NONE, DQTYPE_THREAD_CURRENT,
};
use windows_core::{Interface, Result};
use windows_numerics::Vector2;

pub struct Host {
    pub compositor: Compositor,
    _queue: DispatcherQueueController,
}

/// Satu window yang dihubungkan ke compositor. Window harus `WS_EX_NOREDIRECTIONBITMAP`.
pub struct Surface {
    pub target: DesktopWindowTarget,
    pub root: ContainerVisual,
}

impl Host {
    /// Harus dipanggil di thread UI sebelum membuat window ber-Composition.
    pub fn new() -> Result<Host> {
        let queue = unsafe {
            CreateDispatcherQueueController(DispatcherQueueOptions {
                dwSize: std::mem::size_of::<DispatcherQueueOptions>() as u32,
                threadType: DQTYPE_THREAD_CURRENT,
                apartmentType: DQTAT_COM_NONE,
            })?
        };
        Ok(Host { compositor: Compositor::new()?, _queue: queue })
    }

    pub fn attach(&self, hwnd: HWND, size: (f32, f32)) -> Result<Surface> {
        attach(&self.compositor, hwnd, size)
    }
}

/// Hubungkan `hwnd` (harus `WS_EX_NOREDIRECTIONBITMAP`) ke compositor. Bisa dipanggil kapan saja
/// dengan `Compositor` yang sudah ada, mis. untuk popup yang dibuat belakangan.
pub fn attach(compositor: &Compositor, hwnd: HWND, size: (f32, f32)) -> Result<Surface> {
    let interop: ICompositorDesktopInterop = compositor.cast()?;
    let target = unsafe { interop.CreateDesktopWindowTarget(hwnd, true)? };
    let root = compositor.CreateContainerVisual()?;
    root.SetSize(Vector2 { X: size.0, Y: size.1 })?;
    target.SetRoot(&root)?;
    Ok(Surface { target, root })
}

