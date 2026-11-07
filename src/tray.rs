//! System tray for NotiFansly Desktop.
//!
//! Platform rules (tray-icon 0.21):
//! - Linux: a GTK event loop must run on the thread that created the tray.
//! - Windows: a win32 message pump must run on the tray thread.
//! - macOS: the tray must be created on the main thread (GPUI owns it).
//!
//! All platforms forward menu picks over a generic mpsc channel as
//! [TrayCommand], which the GPUI thread polls (see App::run in main.rs).

use std::sync::atomic::{AtomicU8, Ordering};
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

/// The tray icon's colour state, mirroring the creator's token health:
/// the normal brand mark while the token is live, red when the server
/// reports it invalid/expired, greyed out when unknown or not connected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Tone {
    /// Token active: the normal blue brand icon.
    Ok = 0,
    /// Token invalid/expired: red - fix me, reconnect.
    Alert = 1,
    /// Not connected / status unknown: greyed out.
    Muted = 2,
}

impl Tone {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Alert,
            2 => Self::Muted,
            _ => Self::Ok,
        }
    }
}

/// The tone the app wants right now (set from the GPUI thread).
static TONE: AtomicU8 = AtomicU8::new(Tone::Ok as u8);
/// The tone each tray thread last painted. 255 = "nothing painted yet"
/// so the first poll always applies.
static APPLIED: AtomicU8 = AtomicU8::new(255);

/// Set the desired tray tone. Cheap and idempotent - the tray threads pick
/// it up on their next tick, whichever thread owns the icon per platform.
pub fn set_tone(tone: Tone) {
    TONE.store(tone as u8, Ordering::SeqCst);
}

fn current_tone(tone: Tone) -> Option<Tone> {
    let applied = APPLIED.load(Ordering::SeqCst);
    if applied == tone as u8 {
        return None; // no change
    }
    APPLIED.store(tone as u8, Ordering::SeqCst);
    Some(tone)
}

/// The pending tone, if it differs from what this thread last painted.
fn pending_tone() -> Option<Tone> {
    current_tone(Tone::from_u8(TONE.load(Ordering::SeqCst)))
}

/// Application-wide command channel, owned by the tray module so the macOS
/// main-thread creation path can reach it without 'static gymnastics in
/// main.rs.
static TRAY_TX: OnceLock<Sender<TrayCommand>> = OnceLock::new();

pub const MENU_ID_SHOW: &str = "show";
pub const MENU_ID_SYNC: &str = "sync-now";
pub const MENU_ID_QUIT: &str = "quit";

fn icon_from(bytes: &[u8]) -> Option<tray_icon::Icon> {
    let img = image::load_from_memory(bytes).ok()?.into_rgba8();
    let (w, h) = img.dimensions();
    tray_icon::Icon::from_rgba(img.into_raw(), w, h).ok()
}

/// Decode the embedded brand PNG (normal state) for tray-icon.
pub fn load_icon() -> Option<tray_icon::Icon> {
    icon_from(include_bytes!("../assets/icon.png"))
}

fn icon_for(tone: Tone) -> Option<tray_icon::Icon> {
    match tone {
        Tone::Ok => icon_from(include_bytes!("../assets/icon.png")),
        Tone::Alert => icon_from(include_bytes!("../assets/icon-alert.png")),
        Tone::Muted => icon_from(include_bytes!("../assets/icon-muted.png")),
    }
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
thread_local! {
    static MAC_TRAY: std::cell::RefCell<Option<TrayIcon>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(target_os = "macos")]
pub fn main_thread_tray() {
    let tray = build_tray();
    MAC_TRAY.with(|cell| *cell.borrow_mut() = Some(tray));
    eprintln!("[tray] macos tray created on main thread");
}

/// Apply the pending tone, if any. macOS: called from the GPUI poller, which
/// runs on the main thread that owns the tray. Linux/Windows: no-op, their
/// tray threads poll inside their own event loops.
pub fn apply_tone_if_changed() {
    #[cfg(target_os = "macos")]
    if let Some(tone) = pending_tone() {
        if let Some(icon) = icon_for(tone) {
            MAC_TRAY.with(|cell| {
                if let Some(tray) = cell.borrow().as_ref() {
                    let _ = tray.set_icon(Some(icon));
                }
            });
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        // Linux/Windows: the tray thread polls in its own loop.
    }
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
    use std::time::Duration;
    // Menu events are forwarded by the global handler set in spawn_tray;
    // this thread only exists to own the GTK loop the SNI tray needs.
    if let Err(e) = gtk::init() {
        eprintln!("[tray] gtk init failed ({e:?}) - running without a tray icon");
        return;
    }
    let tray = std::rc::Rc::new(build_tray());
    eprintln!("[tray] tray icon created, entering gtk loop");

    // set_icon must happen on this thread (it owns the SNI icon), so a
    // glib timeout inside the GTK loop picks up tone changes.
    let poll = tray.clone();
    glib::timeout_add_local(Duration::from_millis(1500), move || {
        if let Some(tone) = pending_tone() {
            if let Some(icon) = icon_for(tone) {
                let _ = poll.set_icon(Some(icon));
            }
        }
        glib::ControlFlow::Continue
    });

    gtk::main();
    std::mem::forget(tray);
    eprintln!("[tray] gtk main loop exited");
}

#[cfg(target_os = "windows")]
fn windows_tray_main() {
    let _tray = std::rc::Rc::new(build_tray());
    eprintln!("[tray] tray icon created, pumping win32 messages");
    // MSG must be zeroed (windows-sys derives nothing); HWND is an isize.
    // A MsgWait-pump loop keeps the thread responsive both to messages and
    // to tone changes (set_icon must run on this thread).
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            DispatchMessageW, MSG, MsgWaitForMultipleObjects, PeekMessageW, PM_REMOVE,
            QS_ALLINPUT, TranslateMessage, WM_QUIT,
        };
        let mut msg: MSG = std::mem::zeroed();
        'pump: loop {
            let _ = MsgWaitForMultipleObjects(0, std::ptr::null(), 0, 1500, QS_ALLINPUT);
            while PeekMessageW(&mut msg, 0, 0, 0, PM_REMOVE) != 0 {
                if msg.message == WM_QUIT {
                    break 'pump;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            if let Some(tone) = pending_tone() {
                if let Some(icon) = icon_for(tone) {
                    let _ = _tray.set_icon(Some(icon));
                }
            }
        }
    }
    std::mem::forget(_tray);
}
