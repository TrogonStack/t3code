//! The secret source control (`SecretSourceInput.tsx`): a select of Value or one of the server's
//! secret providers, then a masked input for a value or a mono input for a reference, with an
//! optional check of the committed reference. The Source control and Devices tabs share it.

use super::devices_panel::widgets::{Variant, icon_button, spinner};
use super::scope_inheritance::PopTrigger;
use super::tokens::Tokens;
use super::*;
use crate::secret_source::{self as secret, SecretSource};
use gpui::{Anchor, EventEmitter, FocusHandle, KeyDownEvent, Subscription};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::popover::Popover;
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui::settings) enum SecretEvent {
    /// The value, the source or the committed reference changed.
    Changed,
    /// The check button asked to resolve `(provider, reference)` on the server.
    Check(String, String),
    /// The user finished editing: a typed value lost focus, or a reference passed validation.
    Commit,
    /// Enter was pressed in the input.
    Submit,
}

/// `SecretCheck` (SecretSourceInput.tsx:60-64).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui::settings) enum Check {
    Idle,
    Checking,
    Resolved,
    Failed(String),
}

pub(in crate::ui::settings) struct SecretField {
    id: SharedString,
    sources: Vec<SecretSource>,
    /// The selected source; `None` is Value.
    provider: Option<String>,
    /// The last reference that passed validation.
    committed: Option<(String, String)>,
    /// A literal is stored on the server; the input only replaces it.
    configured: bool,
    rejected: Option<String>,
    value: Entity<InputState>,
    reference: Entity<InputState>,
    menu: Option<usize>,
    trigger: FocusHandle,
    check: Check,
    disabled: bool,
    /// A plain literal: no source select and a visible value.
    plain: bool,
    mono: bool,
    _subscriptions: Vec<Subscription>,
}

/// `Input size="sm"` (input.tsx:24): a 28px box with a 9px gutter, `font-mono` for a reference.
pub(in crate::ui::settings) fn sm_field(
    input: &Entity<InputState>,
    mono: bool,
    invalid: bool,
    disabled: bool,
    window: &Window,
    cx: &App,
) -> gpui::Div {
    let theme = Tokens::of(cx);
    let focused = input.read(cx).focus_handle(cx).contains_focused(window, cx);
    let background = if theme.dark {
        theme.input.opacity(0.32)
    } else {
        theme.background
    };
    let border = if invalid {
        theme.destructive.opacity(if focused { 0.64 } else { 0.36 })
    } else if focused {
        theme.ring
    } else {
        theme.input
    };
    div()
        .w_full()
        .min_w_0()
        .h(px(28.))
        .px(px(9.))
        .rounded(px(8.))
        .border_1()
        .border_color(border)
        .bg(background)
        .text_size(px(14.))
        .when(mono, |field| {
            field.font_family(cx.theme().mono_font_family.clone())
        })
        .when(disabled, |field| field.opacity(0.64))
        .child(
            Input::new(input)
                .appearance(false)
                .bordered(false)
                .disabled(disabled)
                .h(px(26.))
                .px_0(),
        )
}

impl EventEmitter<SecretEvent> for SecretField {}

impl SecretField {
    pub(in crate::ui::settings) fn new(
        id: impl Into<SharedString>,
        _label: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let value = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Not set")
                .masked(true)
        });
        let reference = cx.new(|cx| InputState::new(window, cx));
        let subscriptions = vec![
            cx.subscribe_in(
                &value,
                window,
                |this, _, event: &InputEvent, _, cx| match event {
                    InputEvent::Change => {
                        cx.emit(SecretEvent::Changed);
                        cx.notify();
                    }
                    InputEvent::Blur | InputEvent::PressEnter { .. } => {
                        let blank = this.value.read(cx).value().trim().is_empty();
                        let blurred = matches!(event, InputEvent::Blur);
                        if !blank || (this.plain && blurred) {
                            cx.emit(SecretEvent::Commit);
                        }
                        if matches!(event, InputEvent::PressEnter { .. }) {
                            cx.emit(SecretEvent::Submit);
                        }
                    }
                    _ => {}
                },
            ),
            cx.subscribe_in(&reference, window, |this, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                    this.commit_reference(cx);
                }
                if matches!(event, InputEvent::PressEnter { .. }) {
                    cx.emit(SecretEvent::Submit);
                }
            }),
        ];
        Self {
            id: id.into(),
            sources: Vec::new(),
            provider: None,
            committed: None,
            configured: false,
            rejected: None,
            value,
            reference,
            menu: None,
            trigger: cx.focus_handle().tab_stop(true),
            check: Check::Idle,
            disabled: false,
            plain: false,
            mono: false,
            _subscriptions: subscriptions,
        }
    }

    pub(in crate::ui::settings) fn set_sources(
        &mut self,
        sources: &[SecretSource],
        cx: &mut Context<Self>,
    ) {
        if self.sources != sources {
            self.sources = sources.to_vec();
            cx.notify();
        }
    }

    pub(in crate::ui::settings) fn set_disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
        if self.disabled != disabled {
            self.disabled = disabled;
            cx.notify();
        }
    }

    pub(in crate::ui::settings) fn set_check(&mut self, check: Check, cx: &mut Context<Self>) {
        self.check = check;
        cx.notify();
    }

    /// A plain literal shows its text and offers no source; a sensitive one masks it.
    pub(in crate::ui::settings) fn set_plain(
        &mut self,
        plain: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.plain == plain {
            return;
        }
        self.plain = plain;
        if plain {
            self.provider = None;
            self.committed = None;
            self.rejected = None;
            self.check = Check::Idle;
        }
        self.value
            .update(cx, |state, cx| state.set_masked(!plain, window, cx));
        self.sync_inputs(window, cx);
        cx.notify();
    }

    /// Mono type for the value, as a variable's.
    pub(in crate::ui::settings) fn set_mono(&mut self, mono: bool, cx: &mut Context<Self>) {
        if self.mono != mono {
            self.mono = mono;
            cx.notify();
        }
    }

    /// The text in the value input, untrimmed.
    pub(in crate::ui::settings) fn typed(&self, cx: &App) -> String {
        self.value.read(cx).value().to_string()
    }

    pub(in crate::ui::settings) fn set_typed(
        &mut self,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = text.to_owned();
        self.value
            .update(cx, |state, cx| state.set_value(text, window, cx));
    }

    /// The committed reference of the chosen source.
    pub(in crate::ui::settings) fn secret(&self) -> Option<(String, String)> {
        self.committed
            .clone()
            .filter(|(provider, _)| Some(provider) == self.provider.as_ref())
    }

    /// Shows a reference, a stored literal that is not sent back, or `typed` text, dropping any draft.
    pub(in crate::ui::settings) fn show(
        &mut self,
        secret: Option<(String, String)>,
        configured: bool,
        typed: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.provider = secret.as_ref().map(|(provider, _)| provider.clone());
        self.committed = secret;
        self.configured = configured;
        self.rejected = None;
        self.check = Check::Idle;
        self.menu = None;
        self.sync_inputs(window, cx);
        self.set_typed(typed, window, cx);
        cx.notify();
    }

    /// Shows what the server stores: a reference, a stored literal or nothing, and drops the draft.
    pub(in crate::ui::settings) fn load(
        &mut self,
        saved: &Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.committed = secret::reference_of(saved);
        self.configured = self.committed.is_none() && secret::is_set(saved);
        self.provider = self
            .committed
            .as_ref()
            .map(|(provider, _)| provider.clone());
        self.rejected = None;
        self.check = Check::Idle;
        self.menu = None;
        self.sync_inputs(window, cx);
        cx.notify();
    }

    /// Empties the field, as a cleared token.
    pub(in crate::ui::settings) fn clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.load(&Value::Null, window, cx);
    }

    /// What a save sends: the typed value, or the committed reference; `None` while empty or while
    /// the chosen source has no valid reference yet.
    pub(in crate::ui::settings) fn submission(&self, cx: &App) -> Option<Value> {
        match &self.provider {
            None => {
                let typed = self.value.read(cx).value().trim().to_owned();
                (!typed.is_empty()).then(|| Value::String(typed))
            }
            Some(provider) => self
                .committed
                .as_ref()
                .filter(|(committed, _)| committed == provider)
                .map(|(provider, reference)| secret::reference_value(provider, reference)),
        }
    }

    /// Whether a replacement is waiting: anything typed, or a committed reference.
    pub(in crate::ui::settings) fn has_draft(&self, cx: &App) -> bool {
        self.submission(cx).is_some()
    }

    /// Closes the source menu; returns whether there was one, so Escape stops there.
    pub(in crate::ui::settings) fn close_menu(&mut self, cx: &mut Context<Self>) -> bool {
        let open = self.menu.take().is_some();
        if open {
            cx.notify();
        }
        open
    }

    fn sync_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let placeholder = if self.plain {
            "value"
        } else if self.configured {
            "Stored secret, enter a new value to replace"
        } else {
            "Not set"
        };
        let text = self
            .committed
            .as_ref()
            .filter(|(committed, _)| Some(committed) == self.provider.as_ref())
            .map(|(_, reference)| reference.clone())
            .unwrap_or_default();
        let placeholder_reference = self
            .source_kind()
            .map(secret::placeholder)
            .unwrap_or_default();
        self.value.update(cx, |state, cx| {
            state.set_value("", window, cx);
            state.set_placeholder(placeholder, window, cx);
        });
        self.reference.update(cx, |state, cx| {
            state.set_value(text, window, cx);
            state.set_placeholder(placeholder_reference, window, cx);
        });
    }

    fn source_kind(&self) -> Option<&str> {
        let provider = self.provider.as_deref()?;
        self.sources
            .iter()
            .find(|source| source.id == provider)
            .map(|source| source.kind.as_str())
    }

    fn commit_reference(&mut self, cx: &mut Context<Self>) {
        let Some(provider) = self.provider.clone() else {
            return;
        };
        let text = self.reference.read(cx).value().to_string();
        if text.trim().is_empty() {
            self.rejected = None;
            if self.committed.take().is_some() {
                self.check = Check::Idle;
                cx.emit(SecretEvent::Changed);
                cx.emit(SecretEvent::Commit);
            }
            cx.notify();
            return;
        }
        let kind = self.source_kind().unwrap_or_default().to_owned();
        if !secret::is_reference(&kind, &text) {
            self.rejected = Some(text);
            cx.notify();
            return;
        }
        self.rejected = None;
        if self.committed.as_ref() != Some(&(provider.clone(), text.clone())) {
            self.committed = Some((provider, text));
            self.check = Check::Idle;
            cx.emit(SecretEvent::Changed);
            cx.emit(SecretEvent::Commit);
        }
        cx.notify();
    }

    /// Picks Value (`None`) or a source (`onValueChange`).
    fn pick(&mut self, next: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        self.menu = None;
        self.rejected = None;
        let dropped = next.is_none() && self.committed.take().is_some();
        self.provider = next;
        self.check = Check::Idle;
        self.sync_inputs(window, cx);
        cx.emit(SecretEvent::Changed);
        if dropped {
            cx.emit(SecretEvent::Commit);
        }
        cx.notify();
    }

    fn open_menu(&mut self, cx: &mut Context<Self>) {
        let at = self
            .provider
            .as_ref()
            .and_then(|p| self.sources.iter().position(|s| &s.id == p))
            .map_or(0, |index| index + 1);
        self.menu = Some(at);
        cx.notify();
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.sources.len() + 1;
        match (event.keystroke.key.as_str(), self.menu) {
            ("enter" | "space" | "down" | "up", None) => {
                cx.stop_propagation();
                self.open_menu(cx);
            }
            ("down", Some(at)) => {
                cx.stop_propagation();
                self.menu = Some((at + 1) % count);
                cx.notify();
            }
            ("up", Some(at)) => {
                cx.stop_propagation();
                self.menu = Some((at + count - 1) % count);
                cx.notify();
            }
            ("enter", Some(at)) => {
                cx.stop_propagation();
                let next = at.checked_sub(1).map(|i| self.sources[i].id.clone());
                self.pick(next, window, cx);
            }
            _ => {}
        }
    }

    fn icon(kind: Option<&str>, color: gpui::Hsla) -> AnyElement {
        match kind {
            None => Icon::new(Icons::Type)
                .size(px(14.))
                .text_color(color)
                .into_any_element(),
            Some(kind) => match crate::assets::secret_provider_icon(kind) {
                Some(path) => gpui::svg()
                    .path(path)
                    .size(px(14.))
                    .flex_shrink_0()
                    .text_color(color)
                    .into_any_element(),
                None => Icon::new(Icons::KeyRound)
                    .size(px(14.))
                    .text_color(color)
                    .into_any_element(),
            },
        }
    }

    pub(in crate::ui::settings) fn select(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Tokens::of(cx);
        let view = cx.entity();
        let selected = self.provider.as_ref().map(|p| {
            self.sources
                .iter()
                .find(|s| &s.id == p)
                .map(|s| s.kind.clone())
        });
        let kind = match &selected {
            None => None,
            Some(Some(kind)) => Some(kind.as_str()),
            Some(None) => Some(""),
        };
        let title = match &self.provider {
            None => "Value".to_owned(),
            Some(p) => self
                .sources
                .iter()
                .find(|s| &s.id == p)
                .map_or_else(|| p.clone(), |s| s.label.clone()),
        };
        let on_key = {
            let view = view.clone();
            move |event: &KeyDownEvent, window: &mut Window, cx: &mut App| {
                view.update(cx, |this, cx| this.key_down(event, window, cx))
            }
        };
        let background = if theme.dark {
            theme.input.opacity(0.32)
        } else {
            theme.background
        };
        let trigger = h_flex()
            .id(SharedString::from(format!("{}-source", self.id)))
            .track_focus(&self.trigger)
            .min_h(px(28.))
            .items_center()
            .gap(px(6.))
            .px(px(9.))
            .rounded(px(8.))
            .border_1()
            .border_color(theme.input)
            .bg(background)
            .cursor_pointer()
            .on_key_down(move |event, window, cx| on_key(event, window, cx))
            .tooltip(tooltip(title))
            .child(Self::icon(kind, theme.foreground))
            .child(
                div()
                    .mr(px(-4.))
                    .opacity(0.7)
                    .text_color(theme.muted_foreground)
                    .child(Icon::new(Icons::ChevronDown).size(px(14.))),
            );
        if self.disabled {
            return div().opacity(0.64).child(trigger).into_any_element();
        }
        let sources = self.sources.clone();
        let at = self.menu;
        let chosen = self.provider.clone();
        let open = self.menu.is_some();
        let id = self.id.clone();
        Popover::new(SharedString::from(format!("{}-source-menu", self.id)))
            .anchor(Anchor::TopLeft)
            .offset(px(4.))
            .open(open)
            .p_0()
            .appearance(false)
            .on_open_change({
                let view = view.clone();
                move |opened, _, cx| {
                    view.update(cx, |this, cx| {
                        if *opened {
                            this.open_menu(cx);
                        } else {
                            this.close_menu(cx);
                        }
                    })
                }
            })
            .trigger(PopTrigger::new(trigger))
            .content(move |_, _, cx| {
                let theme = Tokens::of(cx);
                let mut rows: Vec<(Option<String>, Option<String>, String)> =
                    vec![(None, None, "Value".to_owned())];
                rows.extend(
                    sources
                        .iter()
                        .map(|s| (Some(s.id.clone()), Some(s.kind.clone()), s.label.clone())),
                );
                if let Some(unknown) = chosen
                    .as_ref()
                    .filter(|p| !sources.iter().any(|s| &s.id == *p))
                {
                    rows.push((Some(unknown.clone()), Some(String::new()), unknown.clone()));
                }
                super::components::popup_shell(180., cx).children(rows.into_iter().enumerate().map(
                    |(index, (provider, kind, label))| {
                        let view = view.clone();
                        let chosen_row = provider == chosen;
                        div()
                            .id(SharedString::from(format!("{id}-source-{index}")))
                            .min_h(px(28.))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .px_2()
                            .py_1()
                            .rounded(px(4.))
                            .text_sm()
                            .cursor_pointer()
                            .when(chosen_row, |item| item.bg(theme.foreground.opacity(0.08)))
                            .when(at == Some(index), |item| {
                                item.bg(theme.accent).text_color(theme.accent_foreground)
                            })
                            .on_click(move |_, window, cx| {
                                let provider = provider.clone();
                                view.update(cx, |this, cx| this.pick(provider, window, cx))
                            })
                            .child(Self::icon(kind.as_deref(), theme.foreground))
                            .child(div().min_w_0().truncate().child(label))
                    },
                ))
            })
            .into_any_element()
    }

    fn check_button(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (provider, reference) = self.committed.clone()?;
        if self.provider.as_ref() != Some(&provider) {
            return None;
        }
        let theme = Tokens::of(cx);
        let view = cx.entity();
        let enabled = self.check != Check::Checking;
        let tip = match &self.check {
            Check::Idle => "Check that the secret resolves".to_owned(),
            Check::Checking => "Checking the secret".to_owned(),
            Check::Resolved => "Secret resolved".to_owned(),
            Check::Failed(detail) => detail.clone(),
        };
        let glyph: AnyElement = match &self.check {
            Check::Checking => spinner(12., theme.muted_foreground),
            Check::Resolved => Icon::new(Icons::Check)
                .size(px(12.))
                .text_color(gpui::rgb(0x10b981))
                .into_any_element(),
            Check::Failed(_) => Icon::new(Icons::TriangleAlert)
                .size(px(12.))
                .text_color(theme.destructive)
                .into_any_element(),
            Check::Idle => Icon::new(Icons::RefreshCw)
                .size(px(12.))
                .text_color(theme.muted_foreground)
                .into_any_element(),
        };
        let button = icon_button(
            SharedString::from(format!("{}-check", self.id)),
            Variant::GhostMuted,
            enabled,
            cx,
        )
        .size(px(20.))
        .tooltip(tooltip(tip))
        .child(glyph);
        Some(if enabled {
            button
                .on_click(move |_, _, cx| {
                    let (provider, reference) = (provider.clone(), reference.clone());
                    view.update(cx, |this, cx| {
                        this.check = Check::Checking;
                        cx.emit(SecretEvent::Check(provider, reference));
                        cx.notify();
                    })
                })
                .into_any_element()
        } else {
            button.into_any_element()
        })
    }
}

impl SecretField {
    /// The value input, or the reference input with its check button, then `trailing`.
    pub(in crate::ui::settings) fn value_element(
        &self,
        trailing: Option<AnyElement>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Tokens::of(cx);
        match &self.provider {
            None => h_flex()
                .min_w_0()
                .items_center()
                .gap(px(6.))
                .child(div().flex_1().min_w_0().child(sm_field(
                    &self.value,
                    self.mono,
                    false,
                    self.disabled,
                    window,
                    cx,
                )))
                .children(trailing)
                .into_any_element(),
            Some(_) => {
                let check = self.check_button(cx);
                let message = self
                    .rejected
                    .is_some()
                    .then(|| secret::invalid_message(self.source_kind().unwrap_or_default()));
                v_flex()
                    .min_w_0()
                    .flex_1()
                    .gap(px(4.))
                    .child(
                        h_flex()
                            .min_w_0()
                            .items_center()
                            .gap(px(6.))
                            .child(div().flex_1().min_w_0().child(sm_field(
                                &self.reference,
                                true,
                                self.rejected.is_some(),
                                self.disabled,
                                window,
                                cx,
                            )))
                            .children(check)
                            .children(trailing),
                    )
                    .children(message.map(|text| {
                        div()
                            .text_size(px(11.))
                            .line_height(px(14.))
                            .text_color(theme.destructive)
                            .child(text)
                    }))
                    .into_any_element()
            }
        }
    }

    /// Whether a secret provider, not Value, is chosen.
    pub(in crate::ui::settings) fn is_reference(&self) -> bool {
        self.provider.is_some()
    }

    /// Whether the source select shows.
    pub(in crate::ui::settings) fn has_source(&self) -> bool {
        !self.plain && !(self.sources.is_empty() && self.provider.is_none())
    }
}

impl Render for SecretField {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let value = self.value_element(None, window, cx);
        h_flex()
            .min_w_0()
            .flex_1()
            .items_start()
            .gap(px(6.))
            .when(self.has_source(), |row| row.child(self.select(cx)))
            .child(div().min_w_0().flex_1().child(value))
    }
}
