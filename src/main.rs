//! NotiFansly Desktop - GPUI entry point.
//!
//! Boots the GPUI window, starts the system tray, and polls tray commands
//! (Show / Sync now / Quit). Closing the window hides it to tray; quit via
//! the tray menu - sync runs server-side, so a parked window changes nothing.

mod api;
mod app;
mod autostart;
mod state;
mod theme;
mod tray;
mod widgets;

use std::sync::mpsc;
use std::sync::OnceLock;
use std::time::Duration;

use gpui::{App, AsyncApp, WindowOptions};
use tray::TrayCommand;

/// Demo mode (`--demo` or `NOTIFANSLY_DEMO=1`): canned data instead of the
/// real API - for previews and screenshots without touching a server.
static DEMO: OnceLock<bool> = OnceLock::new();

pub fn demo() -> bool {
    DEMO.get().copied().unwrap_or(false)
}

fn main() {
    let demo = std::env::args().any(|a| a == "--demo")
        || std::env::var("NOTIFANSLY_DEMO").ok().as_deref() == Some("1");
    let _ = DEMO.set(demo);
    if demo {
        eprintln!("[boot] DEMO MODE - canned data, no network");
    }
    eprintln!("[boot] NotiFansly Desktop starting");

    // Tray commands flow from the tray threads to the GPUI thread.
    let (tray_tx, tray_rx) = mpsc::channel::<TrayCommand>();

    // Linux: GTK loop thread. Windows: win32 pump thread. macOS: no-op here;
    // the tray is created on the main thread inside the GPUI run closure.
    tray::spawn_tray(tray_tx);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        gpui::Application::new().run(|cx: &mut App| {
            eprintln!("[boot] GPUI running");

            // macOS: the tray must be created on the main thread (GPUI's
            // NSApp pumps its event loop). Keep it alive for the process.
            #[cfg(target_os = "macos")]
            {
                // Creates + parks the TrayIcon in the thread-local,
                // lived in by this (main) thread via the poller below.
                tray::main_thread_tray();
            }

            let options = WindowOptions {
                window_bounds: Some(gpui::WindowBounds::centered(
                    gpui::Size {
                        width: gpui::px(520.0),
                        height: gpui::px(700.0),
                    },
                    cx,
                )),
                ..Default::default()
            };

            let root = app::RootView::new(cx);
            // Initial status fetch (no-op when not connected) + the 1-minute
            // background refresh loop.
            root.update(cx, |root, cx| {
                // Grey the tray icon out until a status fetch says otherwise
                // (connected startups flip it to normal on the first refresh).
                root.apply_tray_tone();
                root.refresh_status(cx);
                root.check_for_update(cx);
                root.schedule_periodic_refresh(cx);
            });

            let built = cx.open_window(options, |window, cx| {
                window.set_window_title("NotiFansly Desktop");
                window.set_app_id("notifansly-desktop");

                // Closing the window parks the app in the tray instead of
                // exiting - the server keeps syncing either way.
                window.on_window_should_close(cx, |w, _| {
                    eprintln!("[window] close requested -> hiding to tray");
                    w.minimize_window();
                    false
                });

                root.clone()
            });

            match built {
                Ok(handle) => {
                    eprintln!("[boot] window opened");
                    // Poll tray commands and forward them to the window.
                    std::mem::forget(cx.spawn(async move |cx: &mut AsyncApp| {
                        loop {
                            // macOS: applies pending tray tone changes on
                            // this (main) thread. No-op elsewhere.
                            tray::apply_tone_if_changed();
                            while let Ok(cmd) = tray_rx.try_recv() {
                                match cmd {
                                    TrayCommand::Show => {
                                        let _ = cx.update(|cx| {
                                            let _ = handle
                                                .update(cx, |_, window, _| window.activate_window());
                                        });
                                    }
                                    TrayCommand::SyncNow => {
                                        let _ = cx.update(|cx| {
                                            let _ = root.update(cx, |this, cx| this.trigger_sync(cx));
                                        });
                                    }
                                    TrayCommand::Quit => {
                                        eprintln!("[tray] Quit");
                                        let _ = cx.update(|cx| cx.quit());
                                    }
                                }
                            }
                            cx.background_executor()
                                .timer(Duration::from_millis(150))
                                .await;
                        }
                    }));
                }
                Err(e) => eprintln!("[boot] ERROR opening window: {e:?}"),
            }
        });
    }));

    match result {
        Ok(_) => eprintln!("[boot] GPUI event loop exited"),
        Err(payload) => {
            let msg = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "<non-string panic payload>".to_string());
            eprintln!("[boot] GPUI failed to start (GPU/platform init?): {msg}");
        }
    }
}
