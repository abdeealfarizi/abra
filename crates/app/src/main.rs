//! Nacre â€” titik masuk. M1: cangkang, keselamatan taskbar, host Composition.
#![windows_subsystem = "windows"]

mod controlcenter;
mod dock;
mod fx;
mod hud;
mod prefs;
mod settings_win;
mod services;
mod notch;
mod hotcorners;
mod launchpad;
mod menubar;
mod netmenu;
mod mission;
mod motion;
mod spotlight;
mod stage;
mod switcher;
mod mainwin;
mod paths;
mod popup;
mod strip;
mod topmost;
mod macshortcuts;
mod single_instance;

use nacre_core::visibility::policy::Mode;
use nacre_platform::{log, shell::taskbar};
use nacre_ui::glass::Tier;
use windows::core::{w, BOOL};
use windows::Win32::Foundation::HINSTANCE;
use windows::Win32::System::Console::{SetConsoleCtrlHandler, CTRL_BREAK_EVENT, CTRL_CLOSE_EVENT, CTRL_C_EVENT};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows::Win32::UI::WindowsAndMessaging::*;

struct Args {
    hide_taskbar: bool,
    demo_surface: bool,
    no_dock: bool,
    no_island: bool,
    no_hud: bool,
    appbar: bool,
    dock_mode: Option<Mode>,
    island_mode: Option<Mode>,
    /// Uji: jangan menyembunyikan taskbar meski pengaturan meminta.
    keep_taskbar: bool,
    open_settings: bool,
    add_app: Option<String>,
    set_icon: Option<(String, String)>,
    glass: Option<Tier>,
    exit_after: Option<u32>,
}

fn flag_value(v: &[String], name: &str) -> Option<String> {
    v.iter().position(|a| a == name).and_then(|i| v.get(i + 1)).cloned()
}

fn parse_args() -> Args {
    let v: Vec<String> = std::env::args().skip(1).collect();
    Args {
        hide_taskbar: v.iter().any(|a| a == "--hide-taskbar"),
        demo_surface: v.iter().any(|a| a == "--demo-surface"),
        no_dock: v.iter().any(|a| a == "--no-dock"),
        no_island: v.iter().any(|a| a == "--no-island"),
        no_hud: v.iter().any(|a| a == "--no-hud"),
        appbar: v.iter().any(|a| a == "--appbar"),
        island_mode: flag_value(&v, "--island-mode").and_then(|s| Mode::parse(&s)),
        dock_mode: flag_value(&v, "--dock-mode").and_then(|s| Mode::parse(&s)),
        add_app: flag_value(&v, "--add-app"),
        set_icon: v.iter().position(|a| a == "--set-icon").and_then(|i| Some((v.get(i + 1)?.clone(), v.get(i + 2)?.clone()))),
        open_settings: v.iter().any(|a| a == "--open-settings"),
        keep_taskbar: v.iter().any(|a| a == "--keep-taskbar"),
        glass: flag_value(&v, "--glass").and_then(|s| match s.as_str() {
            "full" => Some(Tier::Full),
            "light" => Some(Tier::Light),
            "solid" => Some(Tier::Solid),
            _ => None, // "auto" atau tak dikenal: otomatis
        }),
        exit_after: v.iter().position(|a| a == "--exit-after").and_then(|i| v.get(i + 1)).and_then(|s| s.parse().ok()),
    }
}

/// Dipanggil pada setiap jalur keluar tak normal.
fn release_all(why: &str) {
    log::line("exit", why);
    nacre_platform::shell::appbar::release();
    log::line("exit", "appbar ok");
    fx::restore_all();
    log::line("exit", "fx ok");
    taskbar::restore();
    log::line("exit", "taskbar ok");
}

unsafe extern "system" fn ctrl_handler(t: u32) -> BOOL {
    if t == CTRL_C_EVENT || t == CTRL_BREAK_EVENT || t == CTRL_CLOSE_EVENT {
        release_all("ctrl-handler");
    }
    BOOL(0)
}

/// Pastikan taskbar kembali walau main() keluar lewat jalur lain.
struct ExitGuard;
impl Drop for ExitGuard {
    fn drop(&mut self) {
        release_all("drop");
    }
}

fn main() {
    let args = parse_args();
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    log::init(paths::log());

    let (mutex, event) = match single_instance::acquire() {
        single_instance::Role::Secondary => return,
        single_instance::Role::Primary(m, e) => (m, e),
    };
    let _keep = mutex;
    log::line("start", &format!("hide={} demo={}", args.hide_taskbar, args.demo_surface));

    // Keselamatan lebih dulu: sisa sesi sebelumnya (crash/dibunuh) dipulihkan sebelum apa pun.
    taskbar::recover_leftover(&paths::marker());

    std::panic::set_hook(Box::new(|info| {
        log::line("panic", &info.to_string());
        release_all("panic");
    }));
    unsafe {
        let _ = SetConsoleCtrlHandler(Some(ctrl_handler), true);
    }
    let _exit = ExitGuard;

    // COM STA di thread UI: properti shell (AUMID), dialog dan menu membutuhkannya.
    unsafe {
        let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED);
    }

    prefs::init();
    let pref = prefs::get();
    let host = match nacre_ui::host::Host::new() {
        Ok(h) => h,
        Err(e) => {
            log::line("host", &format!("gagal: {e}"));
            return;
        }
    };

    let Ok(main_hwnd) = mainwin::create() else { return };
    single_instance::listen(event, main_hwnd);
    mainwin::add_tray(main_hwnd);
    if let Some(s) = args.exit_after {
        mainwin::set_exit_timer(main_hwnd, s);
    }

    if !args.no_dock {
        let created = dock::Dock::create(&host, main_hwnd, dock::Options {
            appbar: args.appbar || (args.dock_mode.is_none() && pref.dock.reserves()),
            mode: args.dock_mode.unwrap_or(pref.dock.mode()),
            glass: args.glass,
        });
        match created {
            Ok(h) => dock::cli_commands(h, args.add_app.clone(), args.set_icon.clone()),
            Err(e) => log::line("dock", &format!("gagal dibuat: {e}")),
        }
    }

    if !args.no_island {
        if let Err(e) = notch::Notch::create(&host, notch::Options {
                glass: args.glass,
                mode: args.island_mode.unwrap_or(pref.notch.mode()),
                reserve: args.island_mode.is_none() && pref.notch.reserves(),
            }) {
            log::line("notch", &format!("gagal dibuat: {e}"));
        }
    }

    if args.open_settings {
        settings_win::open(&host.compositor);
    }
    if let Err(e) = mission::init(&host.compositor) {
        log::line("mission", &format!("gagal: {e}"));
    }
    if let Err(e) = launchpad::init(&host.compositor) {
        log::line("launchpad", &format!("gagal: {e}"));
    }
    // sisa kursor bulat dari ABRA versi lama yang mati mendadak
    if nacre_platform::cursors::recover_leftover(&paths::config_dir().join("cursors_replaced.flag")) {
        log::line("cursor", "kursor sistem dipulihkan");
    }
    if let Err(e) = macshortcuts::init() {
        log::line("macshortcuts", &format!("gagal: {e}"));
    }
    if let Err(e) = nacre_platform::clipboard::init() {
        log::line("clipboard", &format!("gagal: {e}"));
    }
    if let Err(e) = spotlight::init(&host.compositor) {
        log::line("spotlight", &format!("gagal: {e}"));
    }
    if let Err(e) = stage::init(&host.compositor) {
        log::line("stage", &format!("gagal: {e}"));
    }
    if let Err(e) = controlcenter::init(&host.compositor) {
        log::line("controlcenter", &format!("gagal: {e}"));
    }
    if let Err(e) = switcher::init(&host.compositor) {
        log::line("switcher", &format!("gagal: {e}"));
    }
    if let Err(e) = menubar::init(&host.compositor) {
        log::line("menubar", &format!("gagal: {e}"));
    }
    if let Err(e) = topmost::init() {
        log::line("topmost", &format!("gagal: {e}"));
    }
    if let Err(e) = hotcorners::init() {
        log::line("hotcorner", &format!("gagal: {e}"));
    }
    if let Err(e) = fx::init(&host) {
        log::line("fx", &format!("gagal: {e}"));
    }
    services::init();
    let _hud = if args.no_hud { None } else { hud::Hud::create(&host, args.glass).ok() };

    let _demo = if args.demo_surface { demo_surface(&host).ok() } else { None };

    // Mode penuh: dock aktif ⇒ taskbar bawaan disembunyikan permanen (dipulihkan saat keluar/crash oleh guard).
    mainwin::TAKEOVER_ALLOWED.store(!args.no_dock && !args.keep_taskbar, std::sync::atomic::Ordering::Relaxed);
    prefs::listen(main_hwnd);
    if pref.autostart {
        mainwin::apply_autostart(); // perbarui jalur bila exe dipindah
    }
    if args.hide_taskbar || (pref.takeover && pref.dock_enabled && !args.no_dock && !args.keep_taskbar) {
        mainwin::set_taskbar_hidden(true);
    }

    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    switcher::shutdown();
    stage::shutdown();
    nacre_platform::clipboard::shutdown();
    macshortcuts::shutdown();
    // Semua pemulihan (taskbar, appbar, jendela, kursor) sudah selesai. Membongkar objek Composition/WinRT satu per satu
    // saat main() berakhir (dan pembongkaran proses oleh runtime) memicu crash ntdll 0xC000000D, jadi akhiri proses
    // langsung dan biarkan sistem operasi membersihkan sisanya.
    release_all("exit");
    unsafe {
        let _ = windows::Win32::System::Threading::TerminateProcess(windows::Win32::System::Threading::GetCurrentProcess(), 0);
    }
    std::process::exit(0);
}

/// Permukaan kaca sementara untuk membuktikan host Composition berjalan (dibuang di M2).
fn demo_surface(host: &nacre_ui::host::Host) -> windows::core::Result<nacre_ui::host::Surface> {
    unsafe {
        let hi: HINSTANCE = GetModuleHandleW(None)?.into();
        let class = w!("NacreDemo");
        unsafe extern "system" fn dp(h: windows::Win32::Foundation::HWND, m: u32, w: windows::Win32::Foundation::WPARAM, l: windows::Win32::Foundation::LPARAM) -> windows::Win32::Foundation::LRESULT {
            DefWindowProcW(h, m, w, l)
        }
        RegisterClassW(&WNDCLASSW { lpfnWndProc: Some(dp), hInstance: hi, lpszClassName: class, ..Default::default() });
        let (sw, sh) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
        let (w_, h_) = (520, 72);
        let hwnd = CreateWindowExW(
            WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class,
            w!("nacre-demo"),
            WS_POPUP | WS_VISIBLE,
            sw / 2 - w_ / 2, sh - 12 - h_, w_, h_,
            None, None, Some(hi), None,
        )?;
        let surface = host.attach(hwnd, (w_ as f32, h_ as f32))?;
        nacre_ui::glass::add_panel(
            &host.compositor,
            &surface.root,
            nacre_ui::glass::GlassParams::light((w_ as f32, h_ as f32), 24.0),
        )?;
        Ok(surface)
    }
}






