//! Window-manager integration for the "closing the window parks it in the
//! tray" behavior.
//!
//! GPUI only exposes minimize, which is the wrong primitive everywhere:
//! - Hyprland ignores xdg_toplevel.set_minimized entirely, so the vetoed
//!   close leaves the window sitting on screen.
//! - Windows SW_MINIMIZE sends the window to the taskbar, not the tray.
//!
//! So each platform gets the real thing:
//! - Hyprland: park the window on a hidden special workspace (moved back
//!   on tray Show). All commands go through the Hyprland IPC socket with
//!   the current-generation dispatcher grammar - the CLI's argument rules
//!   drift across versions, the socket has not.
//! - Windows: find our HWND by title and SW_HIDE it (hidden windows leave
//!   the alt-tab and taskbar lists, which is exactly tray semantics);
//!   show + foreground-restore on tray Show.
//! - Everything else (and non-Hyprland Wayland/X11, macOS): the minimize
//!   fallback in main.rs remains the behavior.

// -- generic no-op hooks (overridden below per platform) --------------------

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn hide_to_tray() {}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn restore_from_tray() {}

// -- Windows ---------------------------------------------------------------

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

// -- Linux + Hyprland --------------------------------------------------------

#[cfg(target_os = "linux")]
pub fn hide_to_tray() {
    hypr_hide();
}

#[cfg(target_os = "linux")]
pub fn restore_from_tray() {
    hypr_show();
}

#[cfg(target_os = "linux")]
const APP_IDS: [&str; 2] = ["notifansly-desktop", "NotiFansly Desktop"];
#[cfg(target_os = "linux")]
const SPECIAL_WS: &str = "special:notifansly";

#[cfg(target_os = "linux")]
fn hypr_log(msg: &str) {
    eprintln!("[wm] {msg}");
}

#[cfg(target_os = "linux")]
pub fn is_hyprland() -> bool {
    std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some()
}

/// Send a raw command to the Hyprland IPC socket. Bypasses the `hyprctl
/// dispatch` CLI, whose argument grammar (commas/spaces) changed across
/// versions; the socket still speaks the classic syntax.
#[cfg(target_os = "linux")]
fn hypr_socket_cmd(cmd: &str) -> bool {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    let Some(sig) = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE") else {
        return false;
    };
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| format!("/run/user/{}", unsafe { libc::getuid() }));
    let path = format!("{}/hypr/{}/.socket.sock", runtime, sig.to_string_lossy());
    let Ok(mut sock) = UnixStream::connect(&path) else {
        hypr_log(&format!("socket connect failed ({path})"));
        return false;
    };
    if sock.write_all(cmd.as_bytes()).is_err() {
        return false;
    }
    let _ = sock.set_read_timeout(Some(std::time::Duration::from_secs(3)));
    let mut reply = Vec::new();
    let _ = sock.read_to_end(&mut reply);
    let text: String = String::from_utf8_lossy(&reply).chars().take(200).collect();
    let ok = text.trim() == "ok";
    hypr_log(&format!("socket `{cmd}` -> {text}"));
    ok
}

/// Run a dispatcher command through the socket. This generation routes
/// everything through the `hl.*` Lua table; classic comma-grammar dispatch
/// strings no longer parse reliably.
#[cfg(target_os = "linux")]
fn hypr_dispatch(code: &str) -> bool {
    hypr_socket_cmd(&format!("dispatch {code}"))
}

/// Find our window via client list JSON. Returns (address, workspace name).
#[cfg(target_os = "linux")]
fn hypr_find_self() -> Option<(String, String)> {
    let out = std::process::Command::new("hyprctl")
        .args(["clients", "-j"])
        .output()
        .ok()?;
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    v.as_array()?.iter().find_map(|c| {
        let class = c.get("class").and_then(|x| x.as_str()).unwrap_or("");
        if APP_IDS.contains(&class) {
            let addr = c.get("address").and_then(|a| a.as_str())?.to_string();
            let ws = c
                .get("workspace")
                .and_then(|w| w.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string();
            Some((addr, ws))
        } else {
            None
        }
    })
}

/// The currently focused regular workspace id (for restoring on Show).
#[cfg(target_os = "linux")]
fn hypr_active_workspace() -> Option<i64> {
    let out = std::process::Command::new("hyprctl")
        .args(["activeworkspace", "-j"])
        .output()
        .ok()?;
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    v.get("id")?.as_i64()
}

/// Park the window on the hidden special workspace (called on the vetoed
/// close). On non-Hyprland the caller's minimize fallback still applies.
#[cfg(target_os = "linux")]
fn hypr_hide() {
    if !is_hyprland() {
        return;
    }
    match hypr_find_self() {
        Some((addr, ws)) => {
            hypr_log(&format!("hiding {addr} from workspace {ws}"));
            hypr_dispatch(&format!(
                "hl.dsp.window.move({{ window = \"address:{addr}\", workspace = \"{SPECIAL_WS}\" }})"
            ));
        }
        None => hypr_log("self window not found; hiding skipped"),
    }
}

/// Bring the window back from the special workspace and focus it (called
/// on tray Show). Deliberately avoids the special-workspace *overlay*
/// toggle - the window is moved onto the currently active regular
/// workspace instead, so it lands wherever the user is looking.
#[cfg(target_os = "linux")]
fn hypr_show() {
    if !is_hyprland() {
        return;
    }
    match hypr_find_self() {
        Some((addr, ws)) if ws.starts_with("special") => {
            let target = hypr_active_workspace().unwrap_or(1);
            hypr_log(&format!("restoring {addr} from {ws} to workspace {target}"));
            hypr_dispatch(&format!(
                "hl.dsp.window.move({{ window = \"address:{addr}\", workspace = \"{target}\" }})"
            ));
            hypr_dispatch(&format!(
                "hl.dsp.focus({{ window = \"address:{addr}\" }})"
            ));
        }
        Some((addr, ws)) => {
            hypr_log(&format!("already visible on {ws} ({addr}); just focusing"));
            hypr_dispatch(&format!("hl.dsp.focus({{ window = \"address:{addr}\" }})"));
        }
        None => hypr_log("self window not found on show; nothing to restore"),
    }
}
