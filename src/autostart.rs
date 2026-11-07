//! Optional "launch at login" support (off by default): XDG autostart on
//! Linux, a LaunchAgent plist on macOS, the Run registry key on Windows.
//! Uses the current executable's path, so it follows wherever the binary
//! actually lives (dev target dir, /usr/bin, an app bundle…).

use auto_launch::{AutoLaunch, AutoLaunchBuilder, MacOSLaunchMode};

fn builder() -> Option<AutoLaunch> {
    let exe = std::env::current_exe().ok()?;
    let exe = exe.to_string_lossy().to_string();
    AutoLaunchBuilder::new()
        .set_app_name("notifansly-desktop")
        .set_app_path(&exe)
        .set_macos_launch_mode(MacOSLaunchMode::LaunchAgent)
        .build()
        .ok()
}

pub fn is_enabled() -> bool {
    builder()
        .and_then(|al| al.is_enabled().ok())
        .unwrap_or(false)
}

pub fn set_enabled(enabled: bool) -> Result<(), String> {
    let al = builder().ok_or("could not resolve this app's executable path")?;
    if enabled {
        al.enable()
            .map_err(|e| format!("could not enable launch at login: {e}"))
    } else {
        al.disable()
            .map_err(|e| format!("could not disable launch at login: {e}"))
    }
}
