//! Small GPUI 0.2.2 widget set (gpui ships no input element in this
//! release; the TextInput below mirrors the an earlier internal GPUI codebase GPUI one). Colours come
//! from the shared theme module so light/dark switch everywhere at once.

use gpui::{
    App, AppContext, Context, ElementId, EventEmitter, FocusHandle, InteractiveElement,
    IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, ParentElement, Render, SharedString,
    StatefulInteractiveElement, Styled, Window, div, px,
};

// ---------------------------------------------------------------------------
// TextInput
// ---------------------------------------------------------------------------

/// Emitted when the user presses Enter in the input.
pub struct InputSubmitted {
    pub value: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    Text,
    /// Secret-looking field rendered with dots (reserved for key fields).
    #[allow(dead_code)]
    Password,
}

pub struct TextInput {
    focus: FocusHandle,
    value: String,
    placeholder: SharedString,
    kind: InputKind,
    id: SharedString,
}

impl TextInput {
    pub fn new(
        cx: &mut App,
        id: &str,
        placeholder: impl Into<SharedString>,
        kind: InputKind,
    ) -> gpui::Entity<Self> {
        let id = SharedString::from(id.to_string());
        let placeholder = placeholder.into();
        cx.new(move |cx| Self {
            focus: cx.focus_handle(),
            value: String::new(),
            placeholder,
            kind,
            id,
        })
    }

    #[allow(dead_code)]
    pub fn focus_handle(&self) -> FocusHandle {
        self.focus.clone()
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.value.clear();
        cx.notify();
    }

    pub fn set_value(&mut self, value: String, cx: &mut Context<Self>) {
        if self.value != value {
            self.value = value;
            cx.notify();
        }
    }

    /// Paste from the clipboard (or X11 primary selection): gpui 0.2.2's
    /// `KeyDownEvent::key_char` only carries typed characters, so Ctrl+V and
    /// middle-click would otherwise do nothing - fatal for long claim links.
    fn paste(&mut self, cx: &mut Context<Self>) {
        let text = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .or_else(|| cx.read_from_primary().and_then(|item| item.text()));
        let Some(text) = text else { return };
        self.insert(&text, cx);
    }

    fn insert(&mut self, text: &str, cx: &mut Context<Self>) {
        let clean: String = text
            .chars()
            .filter(|c| !c.is_control() && *c != '\n' && *c != '\r' && *c != '\t')
            .collect();
        if clean.is_empty() {
            return;
        }
        self.value.push_str(&clean);
        cx.notify();
    }

    fn push_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let k = &event.keystroke;
        let paste_key = (k.key.as_str() == "v" && (k.modifiers.control || k.modifiers.platform))
            || (k.key.as_str() == "insert" && k.modifiers.shift);
        if paste_key {
            self.paste(cx);
            return;
        }
        if k.modifiers.control || k.modifiers.platform {
            return;
        }
        match k.key.as_str() {
            "backspace" => {
                self.value.pop();
                cx.notify();
            }
            "enter" => {
                cx.emit(InputSubmitted {
                    value: self.value.clone(),
                });
            }
            "space" => {
                self.value.push(' ');
                cx.notify();
            }
            "tab" | "escape" | "delete" | "left" | "right" | "up" | "down" | "home" | "end" => {}
            _ => {
                if let Some(ch) = k.key_char.as_deref() {
                    if ch.chars().count() == 1 {
                        self.value.push_str(ch);
                        cx.notify();
                    }
                }
            }
        }
    }
}

impl EventEmitter<InputSubmitted> for TextInput {}

impl Render for TextInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = crate::theme::appearance();
        let is_empty = self.value.is_empty();
        let shown: String = match self.kind {
            InputKind::Password => ".".repeat(self.value.chars().count()),
            InputKind::Text => self.value.clone(),
        };
        let focused = self.focus.is_focused(window);
        let border = if focused { t.accent } else { t.border };

        div()
            .id(ElementId::Name(self.id.clone()))
            .track_focus(&self.focus)
            .on_click(cx.listener(|this, _, window: &mut Window, _| {
                this.focus.focus(window);
            }))
            .on_key_down(cx.listener(
                |this, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>| {
                    this.push_key(event, cx);
                },
            ))
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(
                    |this, _event: &MouseDownEvent, _window: &mut Window, cx: &mut Context<Self>| {
                        this.paste(cx);
                    },
                ),
            )
            .flex()
            .items_center()
            .px_3()
            .h(px(36.0))
            .w_full()
            .bg(t.raised)
            .border_1()
            .border_color(border)
            .rounded_md()
            .cursor_text()
            .text_size(crate::theme::text::BODY)
            .text_color(if is_empty { t.muted } else { t.text })
            .child(if is_empty {
                self.placeholder.to_string()
            } else {
                shown
            })
    }
}

// ---------------------------------------------------------------------------
// Buttons
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ButtonRole {
    Primary,
    Secondary,
    Danger,
}

/// A GPUI button primitive. `id` must be unique among the rendered buttons;
/// `on_click` runs against the root view's Context.
pub fn button<V: Render>(
    id: &str,
    label: &str,
    role: ButtonRole,
    enabled: bool,
    cx: &mut Context<V>,
    on_click: impl Fn(&mut V, &gpui::ClickEvent, &mut Window, &mut Context<V>) + 'static,
) -> impl IntoElement {
    button_on(id, label, role, enabled, cx, on_click)
}

fn button_on<V: Render>(
    id: &str,
    label: &str,
    role: ButtonRole,
    enabled: bool,
    cx: &mut Context<V>,
    on_click: impl Fn(&mut V, &gpui::ClickEvent, &mut Window, &mut Context<V>) + 'static,
) -> impl IntoElement {
    let t = crate::theme::appearance();
    let (bg, fg) = match role {
        ButtonRole::Primary => (t.accent, t.on_accent),
        ButtonRole::Secondary => (t.raised, t.text),
        ButtonRole::Danger => (t.raised, t.danger),
    };

    let mut el = div()
        .id(ElementId::Name(SharedString::from(id.to_string())))
        .flex()
        .items_center()
        .justify_center()
        .px_4()
        .h(px(34.0))
        .rounded_md()
        .text_size(crate::theme::text::BODY)
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .bg(bg)
        .border_1()
        .border_color(match role {
            ButtonRole::Secondary | ButtonRole::Danger => t.border,
            ButtonRole::Primary => bg,
        })
        .text_color(fg)
        .child(label.to_string());
    if enabled {
        el = el.cursor_pointer().on_click(cx.listener(on_click));
    } else {
        el = el.opacity(0.5);
    }
    el
}

/// Small state pill (yap-a-thon's status chip): rounded-full, bg-coloured
/// inset so the tone reads from the edge of a screen. Kept SHORT - a pill
/// is a status tone, not a sentence.
pub fn pill(id: &str, label: &str, color: gpui::Hsla) -> impl IntoElement {
    let t = crate::theme::appearance();
    div()
        .id(ElementId::Name(SharedString::from(id.to_string())))
        .flex()
        .items_center()
        .flex_shrink()
        .h(px(24.0))
        .px_2()
        .rounded_full()
        .bg(t.bg)
        .text_size(crate::theme::text::LABEL)
        .font_weight(gpui::FontWeight::BOLD)
        .text_color(color)
        .child(label.to_string())
}

/// Muted supporting text.
pub fn note(content: impl Into<SharedString>) -> impl IntoElement {
    let t = crate::theme::appearance();
    div()
        .text_size(crate::theme::text::LABEL)
        .text_color(t.muted)
        .child(content.into())
}

/// A section card: p_4, rounded_lg, surface + border (the yap-a-thon look).
pub fn card() -> gpui::Div {
    let t = crate::theme::appearance();
    div()
        .p_4()
        .rounded_lg()
        .bg(t.surface)
        .border_1()
        .border_color(t.border)
}
