use nacre_core::spotlight::Mode;
use windows::UI::Composition::Compositor;

pub fn init(_comp: &Compositor) -> windows_core::Result<()> {
    Ok(())
}

pub fn shutdown() {}

pub fn toggle() {
    crate::spotlight::toggle_mode_at_cursor(Mode::Apps);
}
