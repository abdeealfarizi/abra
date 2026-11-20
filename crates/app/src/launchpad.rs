use nacre_core::spotlight::Mode;
use windows::UI::Composition::Compositor;
use windows::Win32::Foundation::RECT;

pub fn init(_comp: &Compositor) -> windows_core::Result<()> {
    Ok(())
}

pub fn shutdown() {}

pub fn toggle() {
    crate::spotlight::toggle_mode(Mode::Apps);
}

pub fn toggle_at(bounds: RECT) {
    crate::spotlight::toggle_mode_at(Mode::Apps, bounds);
}
