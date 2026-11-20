use std::sync::atomic::{AtomicBool, Ordering};

static HAS_VISIBLE_APP: AtomicBool = AtomicBool::new(false);

pub fn refresh() -> bool {
    let visible = nacre_platform::wins::enumerate::list(std::process::id())
        .iter()
        .any(|window| !nacre_platform::wins::focus::is_minimized(window.hwnd));
    HAS_VISIBLE_APP.store(visible, Ordering::Relaxed);
    visible
}

pub fn has_visible_app() -> bool {
    HAS_VISIBLE_APP.load(Ordering::Relaxed)
}

pub fn has_visible_app_on(bounds: windows::Win32::Foundation::RECT) -> bool {
    nacre_platform::wins::enumerate::list(std::process::id()).iter().any(|window| {
        !nacre_platform::wins::focus::is_minimized(window.hwnd) && crate::display::window_is_on(window.hwnd, bounds)
    })
}
