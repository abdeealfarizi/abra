use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, FlashWindow, GetAncestor, GetForegroundWindow, IsIconic, IsWindow, IsWindowVisible,
    PostMessageW, SetForegroundWindow, ShowWindow, GA_ROOTOWNER, SW_MINIMIZE, SW_RESTORE, SW_SHOW, WM_CLOSE,
};

pub fn is_flyout_host(exe: &str) -> bool {
    exe.rsplit(['\\', '/']).next().is_some_and(|n| n.eq_ignore_ascii_case("ShellExperienceHost.exe"))
}

pub fn shell_flyout_in_front() -> bool {
    unsafe {
        let fg = GetForegroundWindow();
        if fg.0.is_null() {
            return false;
        }
        let mut pid = 0u32;
        windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(fg, Some(&mut pid));
        pid != 0 && super::enumerate::exe_path_of_pid(pid).is_some_and(|p| is_flyout_host(&p))
    }
}

pub fn dismiss_shell_flyout() -> bool {
    if shell_flyout_in_front() {
        crate::keys::chord(&[0x1B]);
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_windows_flyout_host_counts() {
        assert!(is_flyout_host(r"C:\Windows\SystemApps\ShellExperienceHost_cw5n1h2txyewy\ShellExperienceHost.exe"));
        assert!(is_flyout_host("shellexperiencehost.exe"));
        assert!(!is_flyout_host(r"C:\Windows\explorer.exe"));
        assert!(!is_flyout_host(r"C:\Tools\NotShellExperienceHost.exe"));
        assert!(!is_flyout_host(""));
    }
}

fn h(x: isize) -> HWND {
    HWND(x as *mut _)
}

pub fn is_alive(hwnd: isize) -> bool {
    unsafe { IsWindow(Some(h(hwnd))).as_bool() }
}

pub fn title(hwnd: isize) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { windows::Win32::UI::WindowsAndMessaging::GetWindowTextW(h(hwnd), &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

pub fn is_minimized(hwnd: isize) -> bool {
    unsafe { IsIconic(h(hwnd)).as_bool() }
}

pub fn foreground() -> isize {
    unsafe { GetForegroundWindow().0 as isize }
}

pub fn activate(target: isize, dock: isize) -> bool {
    unsafe {
        let hwnd = h(target);
        if !IsWindow(Some(hwnd)).as_bool() {
            return false;
        }

        let mut fg = GetForegroundWindow();
        if fg.0 as isize == dock {
            if let Some(last) = super::tracker::last_focused_except(dock, is_alive) {
                fg = h(last);
            }
        }
        let is_front = fg == hwnd || GetAncestor(fg, GA_ROOTOWNER) == hwnd;
        if is_front && IsWindowVisible(hwnd).as_bool() && !IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_MINIMIZE);
            let min_ok = IsIconic(hwnd).as_bool();
            super::tracker::bump();
            return min_ok;
        }

        if !IsWindowVisible(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_SHOW);
        }
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        if SetForegroundWindow(hwnd).as_bool() && GetForegroundWindow() == hwnd {
            super::tracker::bump();
            return true;
        }
        let _ = BringWindowToTop(hwnd);
        if GetForegroundWindow() == hwnd || (SetForegroundWindow(hwnd).as_bool() && GetForegroundWindow() == hwnd) {
            super::tracker::bump();
            return true;
        }
        let _ = FlashWindow(hwnd, true);
        false
    }
}

pub fn request_close(hwnd: isize) {
    unsafe {
        let _ = PostMessageW(Some(h(hwnd)), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
}

pub fn force_foreground(target: isize) -> bool {
    use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows::Win32::UI::Input::KeyboardAndMouse::{keybd_event, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, VK_MENU};
    use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
    unsafe {
        let hwnd = h(target);
        if !IsWindow(Some(hwnd)).as_bool() {
            return false;
        }
        if !IsWindowVisible(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_SHOW);
        }
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        if GetForegroundWindow() == hwnd {
            return true;
        }
        let fg = GetForegroundWindow();
        let (me, other) = (GetCurrentThreadId(), GetWindowThreadProcessId(fg, None));
        let attached = other != 0 && other != me && AttachThreadInput(me, other, true).as_bool();
        let _ = BringWindowToTop(hwnd);
        let mut ok = SetForegroundWindow(hwnd).as_bool() && GetForegroundWindow() == hwnd;
        if attached {
            let _ = AttachThreadInput(me, other, false);
        }
        if !ok {
            keybd_event(VK_MENU.0 as u8, 0, KEYBD_EVENT_FLAGS(0), 0);
            keybd_event(VK_MENU.0 as u8, 0, KEYEVENTF_KEYUP, 0);
            let _ = BringWindowToTop(hwnd);
            ok = SetForegroundWindow(hwnd).as_bool() && GetForegroundWindow() == hwnd;
        }
        if !ok {
            let _ = FlashWindow(hwnd, true);
        }
        if ok {
            super::tracker::bump();
        }
        ok
    }
}
