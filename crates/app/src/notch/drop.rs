//! Target seret-taruh OLE untuk notch: berkas dari Explorer (atau app lain) yang diseret ke notch. Callback OLE
//! hanya menyimpan jalur berkas dan memposting pesan ke jendela notch; logikanya tetap di kontroler (tanpa reentrancy).

use std::cell::RefCell;
use std::rc::Rc;
use windows::core::implement;
use windows::Win32::Foundation::{HWND, LPARAM, POINTL, WPARAM};
use windows::Win32::System::Com::{IDataObject, DVASPECT_CONTENT, FORMATETC, TYMED_HGLOBAL};
use windows::Win32::System::Ole::{
    IDropTarget, IDropTarget_Impl, OleInitialize, RegisterDragDrop, ReleaseStgMedium, CF_HDROP, DROPEFFECT, DROPEFFECT_COPY,
    DROPEFFECT_NONE,
};
use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

/// Jenis kejadian di `wParam` pesan seret-taruh.
pub const ENTER: usize = 1;
pub const OVER: usize = 2;
pub const LEAVE: usize = 3;
pub const DROP: usize = 4;

/// Jalur berkas yang sedang diseret (diisi saat masuk dan saat dilepas).
pub type Shared = Rc<RefCell<Vec<String>>>;

#[implement(IDropTarget)]
struct Target {
    hwnd: isize,
    msg: u32,
    paths: Shared,
    /// Seretan berisi berkas (selain itu diabaikan, mis. teks).
    has_files: RefCell<bool>,
}

fn files(data: &IDataObject) -> Vec<String> {
    let fmt = FORMATETC { cfFormat: CF_HDROP.0, ptd: std::ptr::null_mut(), dwAspect: DVASPECT_CONTENT.0, lindex: -1, tymed: TYMED_HGLOBAL.0 as u32 };
    let mut out = Vec::new();
    unsafe {
        let Ok(mut medium) = data.GetData(&fmt) else { return out };
        let hdrop = HDROP(medium.u.hGlobal.0);
        let n = DragQueryFileW(hdrop, u32::MAX, None);
        for i in 0..n.min(200) {
            let len = DragQueryFileW(hdrop, i, None) as usize;
            let mut buf = vec![0u16; len + 1];
            DragQueryFileW(hdrop, i, Some(&mut buf));
            out.push(String::from_utf16_lossy(&buf[..len]));
        }
        ReleaseStgMedium(&mut medium);
    }
    out
}

fn pack(pt: &POINTL) -> LPARAM {
    LPARAM(((pt.x as i16 as u16 as u32) | ((pt.y as i16 as u16 as u32) << 16)) as isize)
}

impl Target {
    fn post(&self, kind: usize, pt: &POINTL) {
        unsafe {
            let _ = PostMessageW(Some(HWND(self.hwnd as *mut _)), self.msg, WPARAM(kind), pack(pt));
        }
    }
}

impl IDropTarget_Impl for Target_Impl {
    fn DragEnter(&self, data: windows_core::Ref<IDataObject>, _keys: MODIFIERKEYS_FLAGS, pt: &POINTL, effect: *mut DROPEFFECT) -> windows_core::Result<()> {
        let list = data.as_ref().map(files).unwrap_or_default();
        let ok = !list.is_empty();
        *self.has_files.borrow_mut() = ok;
        if ok {
            *self.paths.borrow_mut() = list;
            self.post(ENTER, pt);
        }
        if !effect.is_null() {
            unsafe { *effect = if ok { DROPEFFECT_COPY } else { DROPEFFECT_NONE } };
        }
        Ok(())
    }

    fn DragOver(&self, _keys: MODIFIERKEYS_FLAGS, pt: &POINTL, effect: *mut DROPEFFECT) -> windows_core::Result<()> {
        let ok = *self.has_files.borrow();
        if ok {
            self.post(OVER, pt);
        }
        if !effect.is_null() {
            unsafe { *effect = if ok { DROPEFFECT_COPY } else { DROPEFFECT_NONE } };
        }
        Ok(())
    }

    fn DragLeave(&self) -> windows_core::Result<()> {
        if *self.has_files.borrow() {
            self.post(LEAVE, &POINTL::default());
        }
        Ok(())
    }

    fn Drop(&self, data: windows_core::Ref<IDataObject>, _keys: MODIFIERKEYS_FLAGS, pt: &POINTL, effect: *mut DROPEFFECT) -> windows_core::Result<()> {
        let list = data.as_ref().map(files).unwrap_or_default();
        let ok = !list.is_empty();
        if ok {
            *self.paths.borrow_mut() = list;
            self.post(DROP, pt);
        }
        if !effect.is_null() {
            unsafe { *effect = if ok { DROPEFFECT_COPY } else { DROPEFFECT_NONE } };
        }
        Ok(())
    }
}

/// Daftarkan `hwnd` sebagai target taruh. Objek yang dikembalikan harus tetap hidup selama jendela ada.
pub fn register(hwnd: HWND, msg: u32, paths: Shared) -> Option<IDropTarget> {
    unsafe {
        let _ = OleInitialize(None);
        let t: IDropTarget = Target { hwnd: hwnd.0 as isize, msg, paths, has_files: RefCell::new(false) }.into();
        RegisterDragDrop(hwnd, &t).ok()?;
        Some(t)
    }
}
