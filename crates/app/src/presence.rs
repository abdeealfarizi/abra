use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::RECT;

type Bounds = (i32, i32, i32, i32);

struct Snapshot {
    updated: Instant,
    screens: Vec<(Bounds, bool)>,
    active: Option<Bounds>,
}

static SNAPSHOT: OnceLock<Mutex<Snapshot>> = OnceLock::new();

fn snapshot() -> &'static Mutex<Snapshot> {
    SNAPSHOT.get_or_init(|| Mutex::new(Snapshot { updated: Instant::now() - Duration::from_secs(1), screens: Vec::new(), active: None }))
}

pub fn refresh() {
    let mut state = snapshot().lock().unwrap_or_else(|e| e.into_inner());
    if state.updated.elapsed() < Duration::from_millis(120) {
        return;
    }
    let windows = nacre_platform::wins::enumerate::list(std::process::id());
    let active_hwnd = nacre_platform::wins::tracker::external_foreground();
    state.active = windows
        .iter()
        .find(|window| window.hwnd == active_hwnd && !nacre_platform::wins::focus::is_minimized(window.hwnd))
        .and_then(|window| crate::display::monitor_bounds_for_window(window.hwnd))
        .map(|r| (r.left, r.top, r.right, r.bottom));
    state.screens = crate::display::monitors()
        .into_iter()
        .map(|monitor| {
            let r = monitor.rect;
            let bounds = (r.left, r.top, r.right, r.bottom);
            let has_app = windows.iter().any(|window| {
                !nacre_platform::wins::focus::is_minimized(window.hwnd) && crate::display::window_is_on(window.hwnd, r)
            });
            (bounds, has_app)
        })
        .collect();
    state.updated = Instant::now();
}

pub fn should_hide_visor_on(bounds: RECT) -> bool {
    let key = (bounds.left, bounds.top, bounds.right, bounds.bottom);
    let state = snapshot().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(active) = state.active {
        active == key
    } else {
        state.screens.iter().find(|(screen, _)| *screen == key).is_some_and(|(_, found)| *found)
    }
}
