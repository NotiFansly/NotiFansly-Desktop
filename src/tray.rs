//! System tray for NotiFansly Desktop.
//!
//! Platform rules (tray-icon 0.21):
//! - Linux: a GTK event loop must run on the thread that created the tray.
//! - Windows: a win32 message pump must run on the tray thread.
//! - macOS: the tray must be created on the main thread (GPUI owns it).
//!
//! All platforms forward menu picks over a generic mpsc channel as
//! [TrayCommand], which the GPUI thread polls (see App::run in main.rs).

use std::sync::mpsc::Sender;
use std::sync::OnceLock;
use tray_icon::{
    TrayIcon, TrayIconBuilder,
    menu::{Menu, MenuEvent, MenuItem},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCommand {
    Show,
    SyncNow,
    Quit,
}

/// Application-wide command channel, owned by the tray module so the macOS
/// main-thread creation path can reach it without 'static gymnastics in
/// main.rs.
static TRAY_TX: OnceLock<Sender<TrayCommand>> = OnceLock::new();

pub const MENU_ID_SHOW: &str = "show";
pub const MENU_ID_SYNC: &str = "sync-now";
pub const MENU_ID_QUIT: &str = "quit";

/// Decode the embedded PNG into the RGBA buffer tray-icon wants.
pub fn load_icon() -> Option<tray_icon::Icon> {
    let bytes: &[u8] = include_bytes!("../assets/icon.png");
    let img = image::load_from_memory(bytes).ok()?.into_rgba8();
    let (w, h) = img.dimensions();
    tray_icon::Icon::from_rgba(img.into_raw(), w, h).ok()
}

/// Build the tray icon + menu. The returned TrayIcon must be kept alive for
/// the process lifetime, and its owning thread must pump events.
fn build_tray() -> TrayIcon {
    let menu = Menu::new();
    let show = MenuItem::with_id(MENU_ID_SHOW, "Show NotiFansly", true, None);
    let sync = MenuItem::with_id(MENU_ID_SYNC, "Sync now", true, None);
    let quit = MenuItem::with_id(MENU_ID_QUIT, "Quit", true, None);
    let _ = menu.append_items(&[&show, &sync, &quit]);

    let builder = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("NotiFansly Desktop")
        .with_title("NotiFansly");
    match load_icon() {
        Some(icon) => builder.with_icon(icon).build().expect("tray icon"),
        None => builder.build().expect("tray icon"),
    }
}

/// Spawn the tray. On Linux/Windows this starts a dedicated thread with the
/// required event pump; on macOS the caller must invoke [main_thread_tray]
/// on the main thread instead (inside GPUI's run callback).
pub fn spawn_tray(tx: Sender<TrayCommand>) {
    let _ = TRAY_TX.set(tx);
    forward_menu_events();

    #[cfg(target_os = "linux")]
    {
        std::thread::Builder::new()
            .name("notifansly-tray".into())
            .spawn(linux_tray_main)
            .map(|_| eprintln!("[tray] linux tray thread spawned"))
            .unwrap_or_else(|e| eprintln!("[tray] failed to spawn tray thread: {e:?}"));
    }

    #[cfg(target_os = "windows")]
    {
        std::thread::Builder::new()
            .name("notifansly-tray".into())
            .spawn(windows_tray_main)
            .map(|_| eprintln!("[tray] windows tray thread spawned"))
            .unwrap_or_else(|e| eprintln!("[tray] failed to spawn tray thread: {e:?}"));
    }

    // macOS: nothing to spawn; main_thread_tray() runs on the main thread.
}

/// macOS: the tray must be created on the main thread, so main.rs calls this
/// from inside GPUI's run callback (GPUI's NSApp pumps the event loop).
/// Keep the returned TrayIcon alive for the process lifetime.
#[cfg(target_os = "macos")]
pub fn main_thread_tray() -> TrayIcon {
    build_tray()
}

fn forward_menu_events() {
    // MenuEvent's global handler is called on whatever thread emits the
    // event; forwarding over the channel is thread-safe. The channel lives
    // in TRAY_TX, set by spawn_tray before any tray exists.
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let cmd = match event.id.0.as_str() {
            MENU_ID_SHOW => Some(TrayCommand::Show),
            MENU_ID_SYNC => Some(TrayCommand::SyncNow),
            MENU_ID_QUIT => Some(TrayCommand::Quit),
            _ => None,
        };
        if let Some(cmd) = cmd {
            if let Some(tx) = TRAY_TX.get() {
                let _ = tx.send(cmd);
            }
        }
    }));
}

#[cfg(target_os = "linux")]
fn linux_tray_main() {
    // Menu events are forwarded by the global handler set in spawn_tray;
    // this thread only exists to own the GTK loop the SNI tray needs.
    if let Err(e) = gtk::init() {
        eprintln!("[tray] gtk init failed ({e:?}) - running without a tray icon");
        return;
    }
    let _tray = build_tray();
    eprintln!("[tray] tray icon created, entering gtk loop");
    gtk::main();
    std::mem::forget(_tray);
    eprintln!("[tray] gtk main loop exited");
}

#[cfg(target_os = "windows")]
fn windows_tray_main() {
    let _tray = build_tray();
    eprintln!("[tray] tray icon created, pumping win32 messages");
    // windows-sys MSG derives nothing; HWND is an isize handle. A zeroed
    // MSG is the canonical "no message yet" state, and handing 0 as the
    // HWND filter means "all windows of this thread".
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            DispatchMessageW, GetMessageW, MSG, TranslateMessage,
        };
        let mut msg: MSG = std::mem::zeroed();
        // GetMessageW returns -1 on error, 0 on WM_QUT, nonzero otherwise.
        while GetMessageW(&mut msg, 0, 0, 0) > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    std::mem::forget(_tray);
}
