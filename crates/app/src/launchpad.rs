//! "Launchpad" di Tahoe sudah menjadi mode **Apps** milik Spotlight. Modul ini hanya meneruskan pemanggil lama
//! (tombol dock, hot corner, menu, Control Center) ke `spotlight::toggle_mode(Mode::Apps)`.

use nacre_core::spotlight::Mode;
use windows::UI::Composition::Compositor;

pub fn init(_comp: &Compositor) -> windows_core::Result<()> {
    Ok(())
}

pub fn shutdown() {}

pub fn toggle() {
    crate::spotlight::toggle_mode(Mode::Apps);
}
