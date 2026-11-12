//! The Keybindings tab: `KeybindingsSettings.tsx`. A search, the rules grouped by command with
//! their source, `when` clause and key, and the writes as `server.upsertKeybinding` and
//! `server.removeKeybinding`; the config stream brings the result back. The web's visual `when`
//! tree is a text expression here, parsed with the same grammar.

use super::components::{group, popup_shell, section};
use super::devices_panel::widgets::{
    Variant, button, dialog_of, field, icon_button, menu_item, note, problem, warning,
};
use super::scope_pickers::ScopeBar;
use super::tokens::Tokens;
use super::*;
use crate::connection::{Action, ActionSender, Call};
use crate::keybinding_rules::{
    self as rules, Row, Source, TOKEN_BASE, command_label, command_options, conflict_labels,
    filter_rows, group_rows, is_mac, pill_parts, record_input, target_payload, upsert_payload,
};
use crate::keybindings::Keybindings;
use crate::status::CallEvent;
use gpui::{ElementId, Keystroke, MouseDownEvent, Subscription, deferred};
use gpui_component::input::{InputEvent, InputState};
use serde_json::Value;
use std::collections::HashMap;

fn eid(prefix: &str, id: &str) -> ElementId {
    ElementId::from(SharedString::from(format!("{prefix}-{id}")))
}

/// A pressed key as the input text a rule stores; `None` for a modifier on its own.
pub(in crate::ui::settings) fn capture(keystroke: &Keystroke) -> Option<String> {
    let modifiers = keystroke.modifiers;
    let pressed = (
        modifiers.control,
        modifiers.alt,
        modifiers.shift,
        modifiers.platform,
    );
    record_input(&keystroke.key, pressed, is_mac())
}

const CONFLICT_TIP: &str = "The most recent matching binding wins when both conditions can apply.";

/// Where a recorded shortcut or an edited `when` goes.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Target {
    Row(String),
    New,
}

struct NewRow {
    command: Option<String>,
    key: String,
    when: String,
}

struct WhenEdit {
    target: Target,
    problem: Option<&'static str>,
}

enum Pending {
    Save { row: Option<String>, adding: bool },
    Remove,
}

pub struct KeybindingsPanel {
    actions: ActionSender,
    scope: Entity<ScopeBar>,
    wide: bool,
    keybindings: Keybindings,
    search: Entity<InputState>,
    when_input: Entity<InputState>,
    key_drafts: HashMap<String, String>,
    when_drafts: HashMap<String, String>,
    new: Option<NewRow>,
    recording: Option<Target>,
    /// The row whose "..." menu is open.
    menu: Option<String>,
    picking: bool,
    when_edit: Option<WhenEdit>,
    pending: HashMap<u64, Pending>,
    error: Option<String>,
    next_token: u64,
    _keys: Option<Subscription>,
    _subscriptions: Vec<Subscription>,
}

impl KeybindingsPanel {
    pub fn new(
        actions: ActionSender,
        scope: Entity<ScopeBar>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search keybindings"));
        let when_input = cx.new(|cx| InputState::new(window, cx).placeholder("Always"));
        let subscriptions = vec![
            cx.observe(&scope, |_, _, cx| cx.notify()),
            cx.subscribe_in(&search, window, |_, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
            cx.subscribe_in(&when_input, window, |_, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
        ];
        Self {
            actions,
            scope,
            wide: true,
            keybindings: Keybindings::default(),
            search,
            when_input,
            key_drafts: HashMap::new(),
            when_drafts: HashMap::new(),
            new: None,
            recording: None,
            menu: None,
            picking: false,
            when_edit: None,
            pending: HashMap::new(),
            error: None,
            next_token: TOKEN_BASE,
            _keys: None,
            _subscriptions: subscriptions,
        }
    }

    pub fn set_wide(&mut self, wide: bool, cx: &mut Context<Self>) {
        if self.wide != wide {
            self.wide = wide;
            cx.notify();
        }
    }

    pub fn set_data(&mut self, keybindings: Keybindings, cx: &mut Context<Self>) {
        if self.keybindings == keybindings {
            return;
        }
        self.keybindings = keybindings;
        let ids: Vec<String> = rules::build_rows(&self.keybindings.rules, is_mac())
            .into_iter()
            .map(|row| row.id)
            .collect();
        self.key_drafts.retain(|id, _| ids.contains(id));
        self.when_drafts.retain(|id, _| ids.contains(id));
        cx.notify();
    }

    pub fn reload(&mut self, cx: &mut Context<Self>) {
        self.key_drafts.clear();
        self.when_drafts.clear();
        self.new = None;
        self.recording = None;
        self._keys = None;
        self.menu = None;
        self.picking = false;
        self.when_edit = None;
        self.error = None;
        cx.notify();
    }

    /// Closes the innermost open layer; returns whether there was one, so Escape stops there.
    pub fn close_menu(&mut self, cx: &mut Context<Self>) -> bool {
        let closed = self.when_edit.take().is_some()
            || self.menu.take().is_some()
            || std::mem::take(&mut self.picking);
        if closed {
            cx.notify();
        }
        closed
    }

    /// Whether a connected environment lets this app write its keybindings.
    fn editable(&self, cx: &App) -> bool {
        self.scope
            .read(cx)
            .data()
            .environments
            .iter()
            .any(|e| e.connected && e.writable)
    }

    fn connected(&self, cx: &App) -> bool {
        self.scope
            .read(cx)
            .data()
            .environments
            .iter()
            .any(|e| e.connected)
    }

    fn send(&mut self, call: Call, payload: Value, pending: Pending, cx: &mut Context<Self>) {
        self.error = None;
        let token = self.next_token;
        self.next_token += 1;
        let action = Action::Call {
            token,
            call,
            payload,
        };
        if self.actions.try_send(action).is_ok() {
            self.pending.insert(token, pending);
        } else {
            self.error = Some("Could not send the request".to_owned());
            eprintln!("[keybindings] could not send the request");
        }
        cx.notify();
    }

    pub fn on_call(&mut self, token: u64, event: CallEvent, cx: &mut Context<Self>) {
        if !self.pending.contains_key(&token) {
            return;
        }
        match event {
            CallEvent::Progress(_) => return,
            CallEvent::Done(_) => {
                if let Some(Pending::Save { row, adding }) = self.pending.remove(&token) {
                    if let Some(id) = row {
                        self.key_drafts.remove(&id);
                        self.when_drafts.remove(&id);
                    }
                    if adding {
                        self.new = None;
                    }
                }
            }
            CallEvent::Failed(message) => {
                let (title, fallback) = match self.pending.remove(&token) {
                    Some(Pending::Remove) => (
                        "Unable to remove keybinding",
                        "The keybinding was not removed.",
                    ),
                    _ => ("Unable to save keybinding", "The keybinding was not saved."),
                };
                let message = if message.is_empty() {
                    fallback.to_owned()
                } else {
                    message
                };
                self.error = Some(format!("{title}: {message}"));
            }
        }
        cx.notify();
    }

    fn save_row(&mut self, row: &Row, cx: &mut Context<Self>) {
        let key = self.key_drafts.get(&row.id).unwrap_or(&row.key).clone();
        let when = self.when_drafts.get(&row.id).unwrap_or(&row.when).clone();
        let payload = upsert_payload(
            &row.command,
            &key,
            &when,
            Some(target_payload(&row.command, &row.key, &row.when)),
        );
        self.send(
            Call::UpsertKeybinding,
            payload,
            Pending::Save {
                row: Some(row.id.clone()),
                adding: false,
            },
            cx,
        );
    }

    fn reset_row(&mut self, row: &Row, cx: &mut Context<Self>) {
        self.menu = None;
        let Some(default_key) = row.default_key.as_deref() else {
            return;
        };
        let payload = upsert_payload(
            &row.command,
            default_key,
            &row.default_when,
            Some(target_payload(&row.command, &row.key, &row.when)),
        );
        self.send(
            Call::UpsertKeybinding,
            payload,
            Pending::Save {
                row: Some(row.id.clone()),
                adding: false,
            },
            cx,
        );
    }

    fn remove_row(&mut self, row: &Row, cx: &mut Context<Self>) {
        self.menu = None;
        let payload = target_payload(&row.command, &row.key, &row.when);
        self.send(Call::RemoveKeybinding, payload, Pending::Remove, cx);
    }

    fn save_new(&mut self, cx: &mut Context<Self>) {
        let Some(new) = &self.new else {
            return;
        };
        let Some(command) = new.command.clone() else {
            return;
        };
        if new.key.trim().is_empty() {
            return;
        }
        let payload = upsert_payload(&command, &new.key, &new.when, None);
        self.send(
            Call::UpsertKeybinding,
            payload,
            Pending::Save {
                row: None,
                adding: true,
            },
            cx,
        );
    }

    fn start_recording(&mut self, target: Target, cx: &mut Context<Self>) {
        let view = cx.weak_entity();
        self.recording = Some(target);
        self._keys = Some(cx.intercept_keystrokes(move |event, _, cx| {
            let keystroke = event.keystroke.clone();
            let _ = view.update(cx, |this, cx| this.record(&keystroke, cx));
            cx.stop_propagation();
        }));
        cx.notify();
    }

    fn record(&mut self, keystroke: &Keystroke, cx: &mut Context<Self>) {
        let input = if keystroke.key == "escape" {
            None
        } else {
            match capture(keystroke) {
                Some(input) => Some(input),
                None => return,
            }
        };
        if let (Some(input), Some(target)) = (input, self.recording.clone()) {
            match target {
                Target::Row(id) => {
                    self.key_drafts.insert(id, input);
                }
                Target::New => {
                    if let Some(new) = &mut self.new {
                        new.key = input;
                    }
                }
            }
        }
        self.recording = None;
        self._keys = None;
        cx.notify();
    }

    fn open_when(
        &mut self,
        target: Target,
        current: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.when_input
            .update(cx, |input, cx| input.set_value(current, window, cx));
        let focus = self.when_input.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        self.when_edit = Some(WhenEdit {
            target,
            problem: None,
        });
        cx.notify();
    }

    fn apply_when(&mut self, cx: &mut Context<Self>) {
        let text = self.when_input.read(cx).value().to_string();
        let expression = match rules::parse_when_draft(&text) {
            Ok(node) => rules::when_expression(node.as_ref()),
            Err(message) => {
                if let Some(edit) = &mut self.when_edit {
                    edit.problem = Some(message);
                }
                cx.notify();
                return;
            }
        };
        if let Some(edit) = self.when_edit.take() {
            match edit.target {
                Target::Row(id) => {
                    self.when_drafts.insert(id, expression);
                }
                Target::New => {
                    if let Some(new) = &mut self.new {
                        new.when = expression;
                    }
                }
            }
        }
        cx.notify();
    }

    pub fn overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let edit = self.when_edit.as_ref()?;
        let theme = Tokens::of(cx);
        let text = self.when_input.read(cx).value().to_string();
        let unknown = rules::unknown_variables(rules::parse_when(&text).as_ref());
        let mut body: Vec<AnyElement> =
            vec![field(&self.when_input, false, window, cx).into_any_element()];
        body.push(
            note(
                "Use variables with !, &&, ||, and parentheses. Leave it empty to always apply.",
                cx,
            )
            .into_any_element(),
        );
        if !unknown.is_empty() {
            let label = if unknown.len() == 1 {
                format!("Unknown condition: {}", unknown[0])
            } else {
                format!("Unknown conditions: {}", unknown.join(", "))
            };
            body.push(
                div()
                    .text_xs()
                    .text_color(warning(&theme))
                    .child(label)
                    .into_any_element(),
            );
        }
        if let Some(message) = edit.problem {
            body.push(problem(message, cx).into_any_element());
        }
        let view = cx.entity();
        let footer = vec![
            button("keybinding-when-cancel", Variant::Outline, true, cx)
                .child("Cancel")
                .on_click({
                    let view = view.clone();
                    move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.when_edit = None;
                            cx.notify();
                        })
                    }
                })
                .into_any_element(),
            button("keybinding-when-apply", Variant::Primary, true, cx)
                .child("Apply")
                .on_click({
                    let view = view.clone();
                    move |_, _, cx| view.update(cx, |this, cx| this.apply_when(cx))
                })
                .into_any_element(),
        ];
        Some(dialog_of(
            "keybinding-when",
            512.,
            "When",
            "Limit this shortcut to a condition.",
            body,
            footer,
            move |_, cx| {
                view.update(cx, |this, cx| {
                    this.when_edit = None;
                    cx.notify();
                })
            },
            cx,
        ))
    }

    fn pill(input: &str, cx: &App) -> gpui::Div {
        let theme = Tokens::of(cx);
        h_flex()
            .gap_1()
            .children(pill_parts(input, is_mac()).into_iter().map(|part| {
                div()
                    .h(px(20.))
                    .min_w(px(20.))
                    .px_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.muted)
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .child(part)
            }))
    }

    fn key_cell(
        &self,
        id: ElementId,
        target: Target,
        key: &str,
        editable: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let theme = Tokens::of(cx);
        let recording = self.recording.as_ref() == Some(&target);
        let content = if recording {
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child("Press shortcut")
                .into_any_element()
        } else if key.is_empty() {
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child("Unassigned")
                .into_any_element()
        } else {
            Self::pill(key, cx).into_any_element()
        };
        div()
            .id(id)
            .h(px(28.))
            .min_w(px(112.))
            .px_2()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(8.))
            .border_1()
            .border_color(if recording { theme.ring } else { theme.input })
            .when(editable, |cell| cell.cursor_pointer())
            .when(!editable, |cell| cell.opacity(0.64))
            .child(content)
            .when(editable, |cell| {
                cell.on_click(
                    cx.listener(move |this, _, _, cx| this.start_recording(target.clone(), cx)),
                )
            })
    }

    fn when_button(
        &self,
        id: ElementId,
        target: Target,
        when: &str,
        editable: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let theme = Tokens::of(cx);
        let current = when.to_owned();
        let label = if when.is_empty() {
            "Always".to_owned()
        } else {
            when.to_owned()
        };
        let accent = theme.accent;
        div()
            .id(id)
            .max_w_full()
            .truncate()
            .text_xs()
            .text_color(theme.muted_foreground)
            .when(editable, |link| {
                link.cursor_pointer().hover(move |style| style.bg(accent))
            })
            .child(format!("When: {label}"))
            .when(editable, |link| {
                link.on_click(cx.listener(move |this, _, window, cx| {
                    this.open_when(target.clone(), current.clone(), window, cx)
                }))
            })
    }

    fn row_menu(&self, row: &Row, cx: &mut Context<Self>) -> Option<AnyElement> {
        let can_reset = row.source == Source::Custom && row.default_key.is_some();
        let can_remove = row.source != Source::Default;
        if !can_reset && !can_remove {
            return None;
        }
        let open = self.menu.as_deref() == Some(row.id.as_str());
        let id = row.id.clone();
        let trigger = icon_button(eid("keybinding-menu", &row.id), Variant::Ghost, true, cx)
            .child(Icon::new(Icons::Ellipsis).size(px(16.)))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.menu = if this.menu.as_deref() == Some(id.as_str()) {
                    None
                } else {
                    Some(id.clone())
                };
                cx.notify();
            }));
        let popup = open.then(|| {
            let view = cx.entity();
            let mut items = popup_shell(176., cx);
            if can_reset {
                let (view, row) = (view.clone(), row.clone());
                items = items.child(
                    menu_item(
                        eid("keybinding-reset", &row.id),
                        "Reset to default",
                        false,
                        cx,
                    )
                    .on_click(move |_, _, cx| view.update(cx, |this, cx| this.reset_row(&row, cx))),
                );
            }
            if can_remove {
                let (view, row) = (view.clone(), row.clone());
                items = items.child(
                    menu_item(eid("keybinding-remove", &row.id), "Remove", true, cx).on_click(
                        move |_, _, cx| view.update(cx, |this, cx| this.remove_row(&row, cx)),
                    ),
                );
            }
            deferred(
                div()
                    .absolute()
                    .top(px(32.))
                    .right_0()
                    .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                        if this.menu.take().is_some() {
                            cx.notify();
                        }
                    }))
                    .child(items),
            )
            .with_priority(1)
        });
        Some(
            div()
                .relative()
                .child(trigger)
                .children(popup)
                .into_any_element(),
        )
    }

    fn row_view(
        &self,
        row: &Row,
        all: &[Row],
        editable: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Tokens::of(cx);
        let key = self.key_drafts.get(&row.id).unwrap_or(&row.key).clone();
        let when = self.when_drafts.get(&row.id).unwrap_or(&row.when).clone();
        let dirty = key != row.key || when != row.when;
        let conflicts = conflict_labels(all, &row.id, &key, &when, is_mac());
        let badge = (row.source != Source::Default).then(|| {
            div()
                .px_1p5()
                .rounded(px(4.))
                .border_1()
                .border_color(theme.border)
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(row.source.label())
        });
        let conflict = (!conflicts.is_empty()).then(|| {
            let mut names = conflicts
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");
            if conflicts.len() > 3 {
                names.push_str(", and more");
            }
            div()
                .id(eid("keybinding-conflict", &row.id))
                .text_xs()
                .text_color(warning(&theme))
                .tooltip(tooltip(CONFLICT_TIP))
                .child(format!("Conflicts with {names}."))
        });
        let save = dirty.then(|| {
            let target = row.clone();
            button(
                eid("keybinding-save", &row.id),
                Variant::Primary,
                editable && self.pending.is_empty(),
                cx,
            )
            .child("Save")
            .on_click(cx.listener(move |this, _, _, cx| {
                if this.editable(cx) {
                    this.save_row(&target, cx)
                }
            }))
        });
        h_flex()
            .px_4()
            .py_3()
            .gap_3()
            .items_start()
            .justify_between()
            .child(
                v_flex()
                    .min_w_0()
                    .flex_1()
                    .gap_1()
                    .child(
                        h_flex()
                            .gap_2()
                            .min_w_0()
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_sm()
                                    .child(command_label(&row.command)),
                            )
                            .children(badge),
                    )
                    .child(self.when_button(
                        eid("keybinding-when", &row.id),
                        Target::Row(row.id.clone()),
                        &when,
                        editable,
                        cx,
                    ))
                    .children(conflict),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .gap_2()
                    .child(self.key_cell(
                        eid("keybinding-key", &row.id),
                        Target::Row(row.id.clone()),
                        &key,
                        editable,
                        cx,
                    ))
                    .children(save)
                    .children(editable.then(|| self.row_menu(row, cx)).flatten()),
            )
            .into_any_element()
    }

    fn command_select(&self, new: &NewRow, cx: &mut Context<Self>) -> AnyElement {
        let theme = Tokens::of(cx);
        let label = new
            .command
            .as_deref()
            .map(command_label)
            .unwrap_or_else(|| "Command".to_owned());
        let trigger = button("keybinding-command", Variant::Outline, true, cx)
            .w(px(220.))
            .justify_between()
            .child(
                div()
                    .truncate()
                    .text_color(if new.command.is_some() {
                        theme.foreground
                    } else {
                        theme.muted_foreground
                    })
                    .child(label),
            )
            .child(Icon::new(Icons::ChevronDown).size(px(14.)))
            .on_click(cx.listener(|this, _, _, cx| {
                this.picking = !this.picking;
                cx.notify();
            }));
        let popup = self.picking.then(|| {
            let view = cx.entity();
            let options = command_options(&self.keybindings.rules);
            let mut list = popup_shell(220., cx)
                .id("keybinding-commands")
                .max_h(px(320.))
                .overflow_y_scroll();
            for (index, command) in options.into_iter().enumerate() {
                let view = view.clone();
                let choice = command.clone();
                list = list.child(
                    menu_item(
                        ElementId::from(("keybinding-command-option", index)),
                        command_label(&command),
                        false,
                        cx,
                    )
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            if let Some(new) = &mut this.new {
                                new.command = Some(choice.clone());
                            }
                            this.picking = false;
                            cx.notify();
                        })
                    }),
                );
            }
            deferred(
                div()
                    .absolute()
                    .top(px(32.))
                    .left_0()
                    .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                        if std::mem::take(&mut this.picking) {
                            cx.notify();
                        }
                    }))
                    .child(list),
            )
            .with_priority(1)
        });
        div()
            .relative()
            .child(trigger)
            .children(popup)
            .into_any_element()
    }

    fn new_row_view(&self, new: &NewRow, editable: bool, cx: &mut Context<Self>) -> AnyElement {
        let ready = editable
            && self.pending.is_empty()
            && new.command.is_some()
            && !new.key.trim().is_empty();
        h_flex()
            .px_4()
            .py_3()
            .gap_3()
            .items_start()
            .justify_between()
            .child(
                v_flex()
                    .min_w_0()
                    .flex_1()
                    .gap_1()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child("New keybinding"),
                    )
                    .child(self.command_select(new, cx))
                    .child(self.when_button(
                        "keybinding-new-when".into(),
                        Target::New,
                        &new.when,
                        editable,
                        cx,
                    )),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .gap_2()
                    .child(self.key_cell(
                        "keybinding-new-key".into(),
                        Target::New,
                        &new.key,
                        editable,
                        cx,
                    ))
                    .child(
                        button("keybinding-new-save", Variant::Primary, ready, cx)
                            .child("Save")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if this.editable(cx) {
                                    this.save_new(cx)
                                }
                            })),
                    )
                    .child(
                        button("keybinding-new-cancel", Variant::Outline, true, cx)
                            .child("Cancel")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.new = None;
                                this.picking = false;
                                this.recording = None;
                                this._keys = None;
                                cx.notify();
                            })),
                    ),
            )
            .into_any_element()
    }
}

impl Render for KeybindingsPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Tokens::of(cx);
        let editable = self.editable(cx);
        let all = rules::build_rows(&self.keybindings.rules, is_mac());
        let query = self.search.read(cx).value().to_string();
        let shown = filter_rows(&all, &query);
        let sections = group_rows(&shown);

        let mut rows: Vec<AnyElement> = Vec::new();
        let open_file = self.keybindings.path.clone().map(|path| {
            icon_button("keybindings-open-file", Variant::Ghost, true, cx)
                .tooltip(tooltip("Open keybindings.json"))
                .child(Icon::new(Icons::ExternalLink).size(px(16.)))
                .on_click(move |_, _, cx| cx.open_url(&format!("file://{path}")))
        });
        rows.push(
            h_flex()
                .px_4()
                .py_3()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(field(&self.search, false, window, cx)),
                )
                .child(
                    button("keybindings-add", Variant::Outline, editable, cx)
                        .child(Icon::new(Icons::Plus).size(px(14.)))
                        .child("Add keybinding")
                        .on_click(cx.listener(|this, _, _, cx| {
                            if this.editable(cx) && this.new.is_none() {
                                this.new = Some(NewRow {
                                    command: None,
                                    key: String::new(),
                                    when: String::new(),
                                });
                                cx.notify();
                            }
                        })),
                )
                .children(open_file)
                .into_any_element(),
        );
        if !editable {
            let text = if self.connected(cx) {
                "This connection can view keybindings but cannot change them."
            } else {
                "Connect an environment to change keybindings."
            };
            rows.push(div().px_4().py_2().child(note(text, cx)).into_any_element());
        }
        if let Some(error) = &self.error {
            rows.push(
                div()
                    .px_4()
                    .py_2()
                    .child(problem(error.clone(), cx))
                    .into_any_element(),
            );
        }
        if let Some(new) = &self.new {
            rows.push(self.new_row_view(new, editable, cx));
        }
        if sections.is_empty() {
            rows.push(
                div()
                    .px_4()
                    .py_6()
                    .flex()
                    .justify_center()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child("No keybindings match your search.")
                    .into_any_element(),
            );
        }
        for (title, members) in sections {
            rows.push(
                div()
                    .px_4()
                    .py_2()
                    .bg(theme.muted.opacity(0.4))
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.muted_foreground)
                    .child(title)
                    .into_any_element(),
            );
            for row in members {
                rows.push(self.row_view(row, &all, editable, cx));
            }
        }
        v_flex().child(section("Keybindings", group(rows, cx)))
    }
}
