//! Root view: app model, async flows, and the Welcome / Dashboard screens.
//!
//! Layout and colour roles follow the yap-a-thon dock: surface cards on a
//! calm background, rounded status pills that read from the edge of the
//! screen, named text sizes, and the NotiFansly pink accent.

use std::sync::Arc;
use std::time::Duration;

use gpui::{
    AnyElement, App, AppContext, AsyncApp, ClipboardItem, Context, Entity, FontWeight,
    InteractiveElement, IntoElement, ParentElement, Render, SharedString, StatefulInteractiveElement,
    Styled, Task, Window, div, img, px,
};

use crate::api::{ApiClient, ConnectResponse, StatusResponse};
use crate::state::{self, PersistedState};
use crate::theme::{self, Appearance, Palette, text};
use crate::widgets::{ButtonRole, InputKind, InputSubmitted, button, card, note, pill};

const POLL_ROUNDS: u32 = 10;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Welcome,
    Dashboard,
}

pub struct RootView {
    client: Arc<ApiClient>,
    pub saved: PersistedState,
    screen: Screen,
    appearance: Appearance,
    /// Brand mark, decoded once from the embedded asset.
    brand_icon: Arc<gpui::Image>,

    // async-operation state
    busy: bool,    // connect/connect-like op in flight
    syncing: bool, // server sync in flight (post-trigger polling)
    error: Option<String>,
    notice: Option<String>,

    status: Option<StatusResponse>,
    reveal_key: bool,
    editing_api_url: bool,
    launch_at_login: bool,

    // inputs
    claim_input: gpui::Entity<crate::widgets::TextInput>,
    api_url_input: gpui::Entity<crate::widgets::TextInput>,

    // keep async tasks alive
    _tasks: Vec<Task<()>>,
}

impl RootView {
    pub fn new(cx: &mut App) -> Entity<Self> {
        let claim_input = crate::widgets::TextInput::new(
            cx,
            "claim-input",
            "Paste your Fansly management session link…",
            InputKind::Text,
        );
        let saved = state::load();
        let api_url_input = crate::widgets::TextInput::new(
            cx,
            "api-url-input",
            "API server URL",
            InputKind::Text,
        );
        api_url_input.update(cx, |input, cx| {
            input.set_value(saved.api_url().to_string(), cx);
        });

        let appearance = Appearance::from_id(saved.appearance.as_deref().unwrap_or("dark"));
        theme::set_appearance(appearance);
        let brand_icon = Arc::new(gpui::Image::from_bytes(
            gpui::ImageFormat::Png,
            include_bytes!("../assets/icon.png").to_vec(),
        ));

        let connected = saved.is_connected();
        cx.new(|cx| {
            // Enter in the claim input submits; Enter in the API-URL editor saves.
            cx.subscribe(
                &claim_input,
                |this: &mut Self,
                 _input: gpui::Entity<crate::widgets::TextInput>,
                 event: &InputSubmitted,
                 cx: &mut Context<Self>| {
                    if !event.value.trim().is_empty() {
                        this.connect(cx);
                    }
                },
            )
            .detach();
            cx.subscribe(
                &api_url_input,
                |this: &mut Self,
                 _input: gpui::Entity<crate::widgets::TextInput>,
                 _event: &InputSubmitted,
                 cx: &mut Context<Self>| {
                    this.save_api_url(cx);
                },
            )
            .detach();

            Self {
                client: Arc::new(ApiClient::new(saved.api_url())),
                screen: if connected {
                    Screen::Dashboard
                } else {
                    Screen::Welcome
                },
                saved,
                appearance,
                brand_icon,
                busy: false,
                syncing: false,
                error: None,
                notice: None,
                status: None,
                reveal_key: false,
                editing_api_url: false,
                launch_at_login: crate::autostart::is_enabled(),
                claim_input,
                api_url_input,
                _tasks: Vec::new(),
            }
        })
    }

    fn palette(&self) -> Palette {
        theme::theme_for(self.appearance)
    }

    fn persist(&mut self) {
        self.saved.appearance = Some(self.appearance.id().to_string());
        state::save(&self.saved);
    }

    fn set_notice(&mut self, msg: impl Into<String>, cx: &mut Context<Self>) {
        self.error = None;
        self.notice = Some(msg.into());
        cx.notify();
    }

    fn set_error(&mut self, msg: impl Into<String>, cx: &mut Context<Self>) {
        self.notice = None;
        self.error = Some(msg.into());
        cx.notify();
    }

    fn connected(&self) -> bool {
        self.saved.is_connected()
    }

    // -- async flows ---------------------------------------------------------

    /// Claim a management session link. For an existing creator the server
    /// returns their original sync key, so this doubles as recovery.
    pub fn connect(&mut self, cx: &mut Context<Self>) {
        let token = self.claim_input.read(cx).value().trim().to_string();
        if token.is_empty() {
            self.set_error("Paste your Fansly management session link first.", cx);
            return;
        }
        self.busy = true;
        self.error = None;
        self.notice = None;
        cx.notify();

        let client = self.client.clone();
        let task = cx.spawn(async move |view, cx: &mut AsyncApp| {
            let result = cx
                .background_executor()
                .spawn(async move { client.connect(&token) })
                .await;
            let _ = view.update(cx, |this, cx| {
                this.busy = false;
                this.finish_connect(result, cx);
            });
        });
        self._tasks.push(task);
    }

    fn finish_connect(&mut self, result: Result<ConnectResponse, String>, cx: &mut Context<Self>) {
        match result {
            Ok(resp) => {
                self.saved.fansly_user_id = Some(resp.fansly_user_id.clone());
                let username = if resp.username.is_empty() {
                    self.saved.username.clone().unwrap_or_default()
                } else {
                    resp.username.clone()
                };
                self.saved.username = Some(username);
                self.saved.sync_key = Some(resp.sync_key.clone());
                self.persist();
                self.claim_input.update(cx, |input, cx| input.clear(cx));
                self.screen = Screen::Dashboard;
                self.set_notice(
                    if resp.created {
                        "Account connected - server-side sync is live."
                    } else {
                        "Welcome back - your account was recovered and your sync key restored."
                    },
                    cx,
                );
                self.refresh_status(cx);
            }
            Err(e) => self.set_error(e, cx),
        }
    }

    /// Fetch the server-side sync state for the saved sync key.
    pub fn refresh_status(&mut self, cx: &mut Context<Self>) {
        let Some(sync_key) = self.saved.sync_key.clone() else {
            return;
        };
        let client = self.client.clone();
        let task = cx.spawn(async move |view, cx: &mut AsyncApp| {
            let result = cx
                .background_executor()
                .spawn(async move { client.status(&sync_key) })
                .await;
            let _ = view.update(cx, |this, cx| match result {
                Ok(s) => {
                    this.status = Some(s);
                    this.error = None;
                    cx.notify();
                }
                Err(e) => this.set_error(e, cx),
            });
        });
        self._tasks.push(task);
    }

    /// Ask the server to run a sync now, then poll status briefly so the UI
    /// can confirm it finished.
    pub fn trigger_sync(&mut self, cx: &mut Context<Self>) {
        let Some(sync_key) = self.saved.sync_key.clone() else {
            return;
        };
        if self.syncing {
            return;
        }
        self.syncing = true;
        self.error = None;
        self.set_notice("Server sync started…", cx);

        let client = self.client.clone();
        let task = cx.spawn(async move |view, cx: &mut AsyncApp| {
            let started = cx
                .background_executor()
                .spawn(async move { client.trigger_sync(&sync_key) })
                .await;
            let _ = view.update(cx, |this, cx| match started {
                Ok(()) => this.poll_sync_progress(cx),
                Err(e) => {
                    this.syncing = false;
                    this.set_error(e, cx);
                }
            });
        });
        self._tasks.push(task);
    }

    /// After triggering, poll status a few times until `last_sync_at` moves.
    fn poll_sync_progress(&mut self, cx: &mut Context<Self>) {
        let sync_key = match self.saved.sync_key.clone() {
            Some(k) => k,
            None => {
                self.syncing = false;
                return;
            }
        };
        let client = self.client.clone();
        let before = self.status.as_ref().and_then(|s| s.last_sync_at.clone());
        let task = cx.spawn(async move |view, cx: &mut AsyncApp| {
            for round in 1..=POLL_ROUNDS {
                cx.background_executor()
                    .timer(Duration::from_secs(if round == 1 { 3 } else { 6 }))
                    .await;
                if !view.upgrade().is_some() {
                    return;
                }
                let snapshot = sync_key.clone();
                let client = client.clone();
                let result = cx
                    .background_executor()
                    .spawn(async move { client.status(&snapshot) })
                    .await;
                let done = match &result {
                    Ok(s) => s.last_sync_at.is_some() && s.last_sync_at != before,
                    Err(_) => false,
                };
                let keep_going = view
                    .update(cx, |this, cx| match result {
                        Ok(s) => {
                            this.status = Some(s);
                            if done {
                                this.syncing = false;
                                this.set_notice("Server sync finished.", cx);
                                false
                            } else {
                                cx.notify();
                                true
                            }
                        }
                        Err(e) => {
                            this.syncing = false;
                            this.set_error(e, cx);
                            false
                        }
                    })
                    .unwrap_or(false);
                if !keep_going {
                    return;
                }
            }
            let _ = view.update(cx, |this, cx| {
                this.syncing = false;
                this.set_notice("Sync is still running on the server - check back shortly.", cx);
            });
        });
        self._tasks.push(task);
    }

    /// Periodic status refresh while the app runs (once a minute).
    pub fn schedule_periodic_refresh(&mut self, cx: &mut Context<Self>) {
        let client = self.client.clone();
        let task = cx.spawn(async move |view, cx: &mut AsyncApp| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_secs(60))
                    .await;
                let Some(this) = view.upgrade() else { return };
                let sync_key = cx
                    .update(|cx| {
                        let root = this.read(cx);
                        if root.connected() {
                            root.saved.sync_key.clone()
                        } else {
                            None
                        }
                    })
                    .ok()
                    .flatten();
                let Some(sync_key) = sync_key else { continue };
                let client = client.clone();
                let result = cx
                    .background_executor()
                    .spawn(async move { client.status(&sync_key) })
                    .await;
                let _ = view.update(cx, |this, cx| {
                    if let Ok(s) = result {
                        this.status = Some(s);
                        cx.notify();
                    }
                });
            }
        });
        self._tasks.push(task);
    }

    // -- local actions ------------------------------------------------------

    pub fn copy_sync_key(&mut self, cx: &mut Context<Self>) {
        if let Some(key) = self.saved.sync_key.clone() {
            if !key.is_empty() {
                cx.write_to_clipboard(ClipboardItem::new_string(key));
                self.set_notice("Sync key copied to clipboard.", cx);
            }
        }
    }

    pub fn toggle_reveal_key(&mut self, cx: &mut Context<Self>) {
        self.reveal_key = !self.reveal_key;
        cx.notify();
    }

    pub fn open_website(&mut self, cx: &mut Context<Self>) {
        let url = self.saved.dashboard_url().to_string();
        let _ = webbrowser::open(&url);
        cx.notify();
    }

    pub fn toggle_appearance(&mut self, cx: &mut Context<Self>) {
        self.appearance = self.appearance.toggle();
        theme::set_appearance(self.appearance);
        self.persist();
        cx.notify();
    }

    pub fn toggle_api_url_editor(&mut self, cx: &mut Context<Self>) {
        self.editing_api_url = !self.editing_api_url;
        cx.notify();
    }

    pub fn save_api_url(&mut self, cx: &mut Context<Self>) {
        let url = self.api_url_input.read(cx).value().trim().to_string();
        if url.is_empty() {
            self.set_error("API server URL cannot be empty.", cx);
            return;
        }
        self.saved.api_url = Some(url);
        self.persist();
        self.client = Arc::new(ApiClient::new(self.saved.api_url()));
        self.editing_api_url = false;
        self.set_notice("API server updated.", cx);
    }

    /// Toggle launch-at-login (XDG autostart / LaunchAgent / Run key).
    pub fn toggle_launch_at_login(&mut self, cx: &mut Context<Self>) {
        let want = !self.launch_at_login;
        match crate::autostart::set_enabled(want) {
            Ok(()) => {
                self.launch_at_login = want;
                self.set_notice(
                    if want {
                        "NotiFansly Desktop will launch at login."
                    } else {
                        "Launch at login disabled."
                    },
                    cx,
                );
            }
            Err(e) => self.set_error(e, cx),
        }
    }

    /// Forgotten device / start over: wipe the local sync key. The server
    /// keeps the creator record - a fresh management link recovers it.
    pub fn disconnect(&mut self, cx: &mut Context<Self>) {
        self.saved.fansly_user_id = None;
        self.saved.username = None;
        self.saved.sync_key = None;
        self.persist();
        self.status = None;
        self.syncing = false;
        self.reveal_key = false;
        self.screen = Screen::Welcome;
        self.set_notice(
            "Disconnected on this device. The server keeps your account - paste a fresh management link to reconnect.",
            cx,
        );
    }

    // -- rendering ------------------------------------------------------------

    /// Brand header, shared by both screens: the NotiFansly mark, the app
    /// name, and the light/dark toggle.
    fn header(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.palette();
        let is_dark = self.appearance == Appearance::Dark;
        let other = if is_dark { "☀" } else { "☾" };

        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .child(
                img(gpui::ImageSource::Image(self.brand_icon.clone()))
                    .w(px(30.0))
                    .h(px(30.0))
                    .flex_shrink(),
            )
            .child(
                div()
                    .text_size(text::TITLE)
                    .font_weight(FontWeight::BOLD)
                    .text_color(t.text)
                    .child("NotiFansly"),
            )
            .child(
                div()
                    .text_size(text::LABEL)
                    .text_color(t.muted)
                    .child("Desktop"),
            )
            .child(div().flex_1())
            .child(
                div()
                    .id(gpui::ElementId::Name(SharedString::from("appearance-toggle")))
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(30.0))
                    .h(px(30.0))
                    .rounded_md()
                    .bg(t.surface)
                    .border_1()
                    .border_color(t.border)
                    .cursor_pointer()
                    .text_size(text::BODY)
                    .text_color(t.muted)
                    .on_click(cx.listener(|this, _e, _w, cx| this.toggle_appearance(cx)))
                    .child(other.to_string()),
            )
    }

    fn banner(&self) -> Option<AnyElement> {
        let t = self.palette();
        self.error.clone().map(|msg| {
            div()
                .px_3()
                .py_2()
                .rounded_md()
                .bg(t.surface)
                .border_1()
                .border_color(t.danger)
                .text_color(t.danger)
                .text_size(text::LABEL)
                .child(msg)
                .into_any_element()
        })
    }

    fn welcome_view(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.palette();

        let steps = [
            (
                "1",
                "In your Fansly creator dashboard, generate a management session link.",
            ),
            ("2", "Paste the whole link below and press Connect."),
            (
                "3",
                "NotiFansly syncs your tiers, followers and subscribers - automatically, server-side.",
            ),
        ];

        div()
            .id(gpui::ElementId::Name(SharedString::from("welcome")))
            .flex()
            .flex_col()
            .gap_3()
            .children(self.banner())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(text::VALUE)
                            .font_weight(FontWeight::BOLD)
                            .text_color(t.text)
                            .child("Connect your Fansly account"),
                    )
                    .child(note(
                        "Management link tokens - no passwords, no API keys.",
                    )),
            )
            .child(
                card().flex().flex_col().gap_2().children(steps.into_iter().map(
                    |(n, text_value)| {
                        div()
                            .flex()
                            .flex_row()
                            .items_start()
                            .gap_3()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .flex_shrink()
                                    .w(px(22.0))
                                    .h(px(22.0))
                                    .rounded_full()
                                    .bg(t.accent_dim)
                                    .border_1()
                                    .border_color(t.accent)
                                    .text_size(text::LABEL)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(t.accent)
                                    .child(n.to_string()),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .overflow_hidden()
                                    .text_size(text::BODY)
                                    .text_color(t.muted)
                                    .child(text_value.to_string()),
                            )
                    },
                )),
            )
            .child(self.claim_input.clone())
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .child(button(
                        "connect-btn",
                        if self.busy { "Connecting…" } else { "Connect" },
                        ButtonRole::Primary,
                        !self.busy,
                        cx,
                        |this: &mut Self, _e, _w, cx| this.connect(cx),
                    ))
                    .child(button(
                        "recover-btn",
                        "Recover existing account",
                        ButtonRole::Secondary,
                        !self.busy,
                        cx,
                        |this: &mut Self, _e, _w, cx| {
                            this.set_notice(
                                "Use the same flow: paste a fresh management session link - your existing account and sync key will be found and restored.",
                                cx,
                            );
                        },
                    )),
            )
            .child(self.api_url_row(cx))
    }

    fn api_url_row(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.palette();
        if self.editing_api_url {
            return div()
                .id(gpui::ElementId::Name(SharedString::from("api-url-edit")))
                .flex()
                .flex_col()
                .gap_2()
                .pt_3()
                .border_t_1()
                .border_color(t.border)
                .child(note("Sync API server"))
                .child(self.api_url_input.clone())
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_2()
                        .child(button(
                            "api-url-save",
                            "Save",
                            ButtonRole::Primary,
                            true,
                            cx,
                            |this: &mut Self, _e, _w, cx| this.save_api_url(cx),
                        ))
                        .child(button(
                            "api-url-cancel",
                            "Cancel",
                            ButtonRole::Secondary,
                            true,
                            cx,
                            |this: &mut Self, _e, _w, cx| this.toggle_api_url_editor(cx),
                        )),
                )
                .into_any_element();
        }
        div()
            .id(gpui::ElementId::Name(SharedString::from("api-url-view")))
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .pt_3()
            .border_t_1()
            .border_color(t.border)
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .text_size(text::LABEL)
                    .text_color(t.muted)
                    .child(format!("Sync API: {}", self.client.api_url())),
            )
            .child(
                div()
                    .id(gpui::ElementId::Name(SharedString::from("api-url-toggle")))
                    .cursor_pointer()
                    .text_size(text::LABEL)
                    .text_color(t.accent)
                    .on_click(cx.listener(|this: &mut Self, _e, _w, cx| {
                        this.toggle_api_url_editor(cx)
                    }))
                    .child("edit"),
            )
            .into_any_element()
    }

    fn dashboard_view(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.palette();
        let username = self
            .status
            .as_ref()
            .map(|s| s.username.clone())
            .filter(|u| !u.is_empty())
            .or_else(|| self.saved.username.clone())
            .unwrap_or_else(|| "creator".to_string());

        let token_status = self
            .status
            .as_ref()
            .map(|s| s.token_status.clone())
            .unwrap_or_else(|| "unknown".to_string());
        let (token_label, token_color) = match token_status.as_str() {
            "active" => ("● TOKEN ACTIVE", t.success),
            "invalid" => ("● TOKEN INVALID", t.danger),
            "expired" => ("● TOKEN EXPIRED", t.warning),
            _ => ("○ STATUS UNKNOWN", t.muted),
        };
        // A pill is a tone, not a sentence; the actionable wording goes here.
        let token_note = match token_status.as_str() {
            "invalid" | "expired" => Some("Reconnect with a fresh management session link."),
            _ => None,
        };

        let last_sync = self
            .status
            .as_ref()
            .and_then(|s| s.last_sync_at.clone())
            .map(|v| {
                // Server sends RFC3339; show as-is minus fractional seconds.
                v.split('.').next().map(|s| s.replace('T', " ")).unwrap_or(v)
            })
            .unwrap_or_else(|| "never".to_string());

        let discord_linked = self
            .status
            .as_ref()
            .map(|s| s.discord_linked)
            .unwrap_or(false);

        let sync_method = self
            .status
            .as_ref()
            .map(|s| s.sync_method.clone())
            .unwrap_or_else(|| "management_token".to_string());

        let key = self.saved.sync_key.clone().unwrap_or_default();
        let masked = if key.len() > 10 {
            format!("{}••••{}", &key[..6], &key[key.len() - 4..])
        } else {
            "••••".to_string()
        };
        let key_shown: String = if self.reveal_key { key.clone() } else { masked };
        let has_key = !key.is_empty();

        let fansly_id = self
            .status
            .as_ref()
            .map(|s| s.fansly_user_id.clone())
            .filter(|v| !v.is_empty())
            .or_else(|| self.saved.fansly_user_id.clone())
            .unwrap_or_default();

        let rows: Vec<(&str, String)> = vec![
            ("Fansly ID", fansly_id),
            ("Sync method", sync_method),
            ("Last server sync", last_sync),
            (
                "Discord",
                if discord_linked {
                    "linked".to_string()
                } else {
                    "not linked".to_string()
                },
            ),
        ];

        div()
            .id(gpui::ElementId::Name(SharedString::from("dashboard")))
            .flex()
            .flex_col()
            .gap_3()
            .children(self.banner())
            .child(
                card()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(pill("token-status-pill", token_label, token_color))
                            .child(div().flex_1())
                            .child(
                                div()
                                    .id(gpui::ElementId::Name(SharedString::from(
                                        "open-dashboard",
                                    )))
                                    .cursor_pointer()
                                    .text_size(text::LABEL)
                                    .text_color(t.accent)
                                    .on_click(cx.listener(|this: &mut Self, _e, _w, cx| {
                                        this.open_website(cx)
                                    }))
                                    .child("Open web dashboard ↗"),
                            ),
                    )
                    .child(
                        div()
                            .text_size(text::VALUE)
                            .font_weight(FontWeight::BOLD)
                            .text_color(t.text)
                            .child(format!("@{username}")),
                    )
                    .children(token_note.map(|n| note(n)))
                    .children(rows.into_iter().map(|(label, value)| {
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .pt_2()
                            .child(
                                div()
                                    .flex_shrink()
                                    .text_size(text::LABEL)
                                    .text_color(t.muted)
                                    .child(label.to_string()),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .overflow_hidden()
                                    .text_size(text::LABEL)
                                    .text_color(if label == "Discord" && value == "not linked" {
                                        t.muted
                                    } else {
                                        t.text
                                    })
                                    .child(value.clone()),
                            )
                    })),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .child(button(
                        "sync-now-btn",
                        if self.syncing { "Syncing…" } else { "Sync now" },
                        ButtonRole::Primary,
                        !self.syncing,
                        cx,
                        |this: &mut Self, _e, _w, cx| this.trigger_sync(cx),
                    ))
                    .child(button(
                        "refresh-btn",
                        "Refresh",
                        ButtonRole::Secondary,
                        !self.busy,
                        cx,
                        |this: &mut Self, _e, _w, cx| this.refresh_status(cx),
                    )),
            )
            .child(
                card().flex().flex_col().gap_2()
                    .child(note("Sync key — keep it private. It restores your account on a new install together with a fresh management link."))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .overflow_hidden()
                                    .text_size(text::BODY)
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(t.text)
                                    .child(key_shown),
                            )
                            .child(button(
                                "reveal-key-btn",
                                if self.reveal_key { "Hide" } else { "Reveal" },
                                ButtonRole::Secondary,
                                has_key,
                                cx,
                                |this: &mut Self, _e, _w, cx| this.toggle_reveal_key(cx),
                            ))
                            .child(button(
                                "copy-key-btn",
                                "Copy",
                                ButtonRole::Secondary,
                                has_key,
                                cx,
                                |this: &mut Self, _e, _w, cx| this.copy_sync_key(cx),
                            )),
                    ),
            )
            .child(
                div()
                    .id(gpui::ElementId::Name(SharedString::from("dashboard-footer")))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .pt_3()
                    .border_t_1()
                    .border_color(t.border)
                    .child(button(
                        "autostart-btn",
                        if self.launch_at_login {
                            "Launch at login: ON"
                        } else {
                            "Launch at login: OFF"
                        },
                        ButtonRole::Secondary,
                        true,
                        cx,
                        |this: &mut Self, _e, _w, cx| this.toggle_launch_at_login(cx),
                    ))
                    .child(button(
                        "disconnect-btn",
                        "Disconnect",
                        ButtonRole::Danger,
                        true,
                        cx,
                        |this: &mut Self, _e, _w, cx| this.disconnect(cx),
                    )),
            )
    }
}

impl Render for RootView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.palette();

        let content = div()
            .id(gpui::ElementId::Name(SharedString::from("dock-scroll")))
            .flex()
            .flex_col()
            .bg(t.bg)
            .text_color(t.text)
            .size_full()
            .overflow_y_scroll()
            .p_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .max_w(px(480.0))
                    .mx_auto()
                    .gap_3()
                    .child(self.header(cx))
                    .child(match self.screen {
                        Screen::Welcome => self.welcome_view(window, cx).into_any_element(),
                        Screen::Dashboard => self.dashboard_view(window, cx).into_any_element(),
                    }),
            );

        content
    }
}
