//! Window-manager integration for "close parks the app in the tray".
//!
//! GPUI 0.2.2 only exposes minimize; on Windows SW_MINIMIZE sends the
//! window to the taskbar, which is not tray semantics. So on Windows the
//! close veto hides the window instead: find our HWND by title and
//! SW_HIDE it (hidden windows leave the alt-tab and taskbar lists) and
//! restore with SW_SHOW + foreground on tray Show.
//!
//! Everything else keeps the plain minimize fallback - compositors that
//! ignore minimize simply leave the window where it is after the vetoed
//! close, and Quit from the tray always works.

// -- generic no-op hooks (Windows overrides) ---------------------------------

#[cfg(not(target_os = "windows"))]
pub fn hide_to_tray() {}

#[cfg(not(target_os = "windows"))]
pub fn restore_from_tray() {}

// -- Windows -----------------------------------------------------------------

#[cfg(target_os = "windows")]
pub fn hide_to_tray() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, ShowWindow, SW_HIDE};
    unsafe {
        let title: Vec<u16> = "NotiFansly Desktop\0".encode_utf16().collect();
        let hwnd = FindWindowW(std::ptr::null(), title.as_ptr());
        if hwnd != 0 {
            ShowWindow(hwnd, SW_HIDE);
        }
    }
}

#[cfg(target_os = "windows")]
pub fn restore_from_tray() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FindWindowW, SetForegroundWindow, ShowWindow, SW_SHOW,
    };
    unsafe {
        let title: Vec<u16> = "NotiFansly Desktop\0".encode_utf16().collect();
        let hwnd = FindWindowW(std::ptr::null(), title.as_ptr());
        if hwnd != 0 {
            ShowWindow(hwnd, SW_SHOW);
            SetForegroundWindow(hwnd);
        }
    }
}
