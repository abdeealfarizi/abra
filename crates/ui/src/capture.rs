//! Tangkapan jendela hidup lewat Windows.Graphics.Capture: tiap jendela menjadi `CompositionDrawingSurface` yang
//! diperbarui terus (video, animasi, kursor teks) dan bisa dipakai sebagai kuas sprite (miring, bersudut bulat, dsb.).
//! Frame diambil dengan polling dari thread UI (tanpa thread tambahan); salinan frame ke permukaan berjalan di GPU.

use crate::gfx::Gfx;
use windows::Graphics::Capture::{Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Graphics::SizeInt32;
use windows::UI::Composition::CompositionDrawingSurface;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct3D11::ID3D11Texture2D;
use windows::Win32::System::WinRT::Direct3D11::IDirect3DDxgiInterfaceAccess;
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows_core::{Interface, Result};

pub fn supported() -> bool {
    GraphicsCaptureSession::IsSupported().unwrap_or(false)
}

pub struct Capture {
    pool: Direct3D11CaptureFramePool,
    session: GraphicsCaptureSession,
    device: IDirect3DDevice,
    size: SizeInt32,
    pub surface: CompositionDrawingSurface,
    /// Sudah menerima minimal satu frame (permukaan berisi gambar).
    pub has_frame: bool,
}

impl Capture {
    pub fn start(gfx: &Gfx, hwnd: isize) -> Result<Capture> {
        let interop = windows_core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
        let item: GraphicsCaptureItem = unsafe { interop.CreateForWindow(HWND(hwnd as *mut _))? };
        let size = item.Size()?;
        if size.Width <= 0 || size.Height <= 0 {
            return Err(windows_core::Error::empty());
        }
        let device = gfx.d3d_device()?;
        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(&device, DirectXPixelFormat::B8G8R8A8UIntNormalized, 2, size)?;
        let session = pool.CreateCaptureSession(&item)?;
        let _ = session.SetIsCursorCaptureEnabled(false);
        let _ = session.SetIsBorderRequired(false);
        session.StartCapture()?;
        let surface = gfx.new_surface(size.Width as u32, size.Height as u32)?;
        Ok(Capture { pool, session, device, size, surface, has_frame: false })
    }

    pub fn size(&self) -> (i32, i32) {
        (self.size.Width, self.size.Height)
    }

    /// Ambil frame terbaru (bila ada) dan salin ke `surface`. Mengembalikan true bila ada frame baru.
    pub fn poll(&mut self, gfx: &Gfx) -> bool {
        let mut last = None;
        while let Ok(f) = self.pool.TryGetNextFrame() {
            last = Some(f);
        }
        let Some(frame) = last else { return false };
        let Ok(cs) = frame.ContentSize() else { return false };
        if cs.Width <= 0 || cs.Height <= 0 {
            return false;
        }
        if cs.Width != self.size.Width || cs.Height != self.size.Height {
            // ukuran jendela berubah: sesuaikan kolam frame untuk frame berikutnya
            let _ = self.pool.Recreate(&self.device, DirectXPixelFormat::B8G8R8A8UIntNormalized, 2, cs);
            self.size = cs;
        }
        let ok = (|| -> Result<()> {
            let surf = frame.Surface()?;
            let access: IDirect3DDxgiInterfaceAccess = surf.cast()?;
            let tex: ID3D11Texture2D = unsafe { access.GetInterface()? };
            gfx.copy_into(&self.surface, &tex, cs.Width as u32, cs.Height as u32)
        })()
        .is_ok();
        let _ = frame.Close();
        if ok {
            self.has_frame = true;
        }
        ok
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        let _ = self.session.Close();
        let _ = self.pool.Close();
    }
}
