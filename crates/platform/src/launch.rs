use windows::core::{w, HSTRING};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_ESCAPE,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GetForegroundWindow, GetWindowThreadProcessId, PostMessageW, SC_TASKLIST, SW_SHOWNORMAL,
    WM_SYSCOMMAND,
};

fn off_thread(job: impl FnOnce() + Send + 'static) {
    let spawned = std::thread::Builder::new().name("abra-launch".into()).spawn(move || {
        use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE};
        let com = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) }.is_ok();
        job();
        if com {
            unsafe { CoUninitialize() };
        }
    });
    if let Err(e) = spawned {
        crate::log::line("launch", &format!("thread gagal: {e}"));
    }
}

pub fn open(path: &str) {
    if let Some(w) = crate::wins::identity::parse_webapp(path) {
        let raw = w.cmd.trim().to_string();
        let (file, args) = split_command(&raw);
        if !file.is_empty() {
            off_thread(move || unsafe {
                let p = if args.is_empty() { None } else { Some(HSTRING::from(args.as_str())) };
                let parm = p.as_ref().map(|h| windows::core::PCWSTR(h.as_ptr())).unwrap_or(windows::core::PCWSTR::null());
                ShellExecuteW(None, w!("open"), &HSTRING::from(file.as_str()), parm, None, SW_SHOWNORMAL);
            });
            return;
        }
    }
    let path = path.split('?').next().unwrap_or(path).to_string();
    off_thread(move || unsafe {
        ShellExecuteW(None, w!("open"), &HSTRING::from(path.as_str()), None, None, SW_SHOWNORMAL);
    });
}

fn split_command(cmd: &str) -> (String, String) {
    let t = cmd.trim();
    if let Some(rest) = t.strip_prefix('"') {
        if let Some((exe, tail)) = rest.split_once('"') {
            return (exe.trim().to_string(), tail.trim().to_string());
        }
    }
    match t.split_once(char::is_whitespace) {
        Some((exe, tail)) => (exe.trim().to_string(), tail.trim().to_string()),
        None => (t.to_string(), String::new()),
    }
}

pub fn is_start_open() -> bool {
    let mut pid = 0u32;
    unsafe {
        let fg = GetForegroundWindow();
        if fg.0.is_null() {
            return false;
        }
        GetWindowThreadProcessId(fg, Some(&mut pid));
    }
    let Some(path) = crate::wins::enumerate::exe_path_of_pid(pid) else { return false };
    let file = path.rsplit('\\').next().unwrap_or("").to_lowercase();
    file == "startmenuexperiencehost.exe" || file == "searchhost.exe"
}

fn tap(vk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY) {
    let key = |flags| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 } },
    };
    let seq = [key(Default::default()), key(KEYEVENTF_KEYUP)];
    unsafe {
        SendInput(&seq, std::mem::size_of::<INPUT>() as i32);
    }
}

pub fn toggle_start(was_open_at_press: bool) {
    if is_start_open() {
        tap(VK_ESCAPE);
    } else if !was_open_at_press {
        unsafe {
            if let Ok(tray) = FindWindowW(w!("Shell_TrayWnd"), None) {
                let _ = PostMessageW(Some(tray), WM_SYSCOMMAND, WPARAM(SC_TASKLIST as usize), LPARAM(0));
            }
        }
    }
}

pub fn properties(path: &str) -> bool {
    use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_INVOKEIDLIST, SHELLEXECUTEINFOW};
    let file = HSTRING::from(path);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_INVOKEIDLIST,
        lpVerb: w!("properties"),
        lpFile: windows::core::PCWSTR(file.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    unsafe { ShellExecuteExW(&mut info).is_ok() }
}

pub fn run_visible(file: &str, params: &str) -> bool {
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE};
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
        let r = ShellExecuteW(None, w!("open"), &HSTRING::from(file), &HSTRING::from(params), None, SW_SHOWNORMAL);
        r.0 as isize > 32
    }
}

pub fn show(file: &str, params: &str) {
    let file = system_tool(file);
    let params = params.to_string();
    off_thread(move || unsafe {
        let r = ShellExecuteW(None, w!("open"), &HSTRING::from(file.as_str()), &HSTRING::from(params.as_str()), None, SW_SHOWNORMAL);
        if r.0 as isize <= 32 {
            crate::log::line("launch", &format!("{file} gagal dibuka (kode {})", r.0 as isize));
        }
    });
}

pub fn run(file: &str, params: &str) {
    let file = system_tool(file);
    let params = params.to_string();
    off_thread(move || unsafe {
        ShellExecuteW(None, w!("open"), &HSTRING::from(file.as_str()), &HSTRING::from(params.as_str()), None, windows::Win32::UI::WindowsAndMessaging::SW_HIDE);
    });
}

fn system_tool(file: &str) -> String {
    if file.contains(['\\', '/', ':']) {
        return file.to_string();
    }
    if let Some(root) = std::env::var_os("SystemRoot") {
        let p = std::path::Path::new(&root).join("System32").join(file);
        if p.is_file() {
            return p.to_string_lossy().to_string();
        }
    }
    file.to_string()
}

#[cfg(test)]
mod tests {
    use super::system_tool;

    #[test]
    fn bare_tools_resolve_to_system32() {
        let p = system_tool("rundll32.exe");
        assert!(p.to_lowercase().ends_with("system32\\rundll32.exe"), "{p}");
        assert_eq!(system_tool("C:\\x\\y.exe"), "C:\\x\\y.exe");
        assert_eq!(system_tool("tidak-ada-xyz.exe"), "tidak-ada-xyz.exe");
    }
}
