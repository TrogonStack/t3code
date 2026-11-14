//! The Keybindings tab: `KeybindingsSettings.tsx`. A search, the rules grouped by command with
//! their source, `when` clause and key, and the writes as `server.upsertKeybinding` and
//! `server.removeKeybinding`; the config stream brings the result back. The web's visual `when`
//! tree is a text expression here, parsed with the same grammar.

use super::components::{
    KeyHandler, SelectTrigger, group, popup_shell, row as settings_row, section, tab_stop,
};
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
use std::rc::Rc;

/// `w-44` and `w-56` (KeybindingsSettings.tsx:1245-1250).
const KEY_INPUT_WIDTH: f32 = 176.;
const COMMAND_WIDTH: f32 = 224.;

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

    /// `Input size="sm" font="mono"` (input.tsx:12-60): a `rounded-lg border` box, `h-7`, `text-sm`,
    /// the placeholder in `--placeholder` (muted-foreground). Focus paints the ring while recording.
    fn key_input(
        &self,
        id: ElementId,
        target: Target,
        key: &str,
        editable: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let theme = Tokens::of(cx);
        let recording = self.recording.as_ref() == Some(&target);
        let (text, color) = if recording {
            ("Press shortcut", theme.muted_foreground)
        } else if key.is_empty() {
            ("Unassigned", theme.muted_foreground)
        } else {
            (key, theme.foreground)
        };
        let background = if theme.dark {
            theme.input.opacity(0.32)
        } else {
            theme.background
        };
        let shadow = if recording {
            vec![gpui::BoxShadow {
                color: theme.ring.opacity(0.24),
                offset: gpui::point(px(0.), px(0.)),
                blur_radius: px(0.),
                spread_radius: px(3.),
                inset: true,
            }]
        } else {
            vec![gpui::BoxShadow {
                color: gpui::black().opacity(0.05),
                offset: gpui::point(px(0.), px(1.)),
                blur_radius: px(2.),
                spread_radius: px(0.),
                inset: false,
            }]
        };
        div()
            .id(id)
            .w(px(KEY_INPUT_WIDTH))
            .h(px(28.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .px(px(9.))
            .rounded(px(8.))
            .border_1()
            .border_color(if recording { theme.ring } else { theme.input })
            .bg(background)
            .shadow(shadow)
            .text_sm()
            .font_family(cx.theme().mono_font_family.clone())
            .text_color(color)
            .truncate()
            .when(editable, |cell| cell.cursor_text())
            .when(!editable, |cell| cell.opacity(0.64))
            .child(text.to_owned())
            .when(editable, |cell| {
                cell.on_click(
                    cx.listener(move |this, _, _, cx| this.start_recording(target.clone(), cx)),
                )
            })
    }

    /// The saved shortcut as a button (`KeybindingKeyControl`, KeybindingsSettings.tsx:826-840):
    /// `h-7 rounded-md px-1.5`, a transparent border that shows on hover with `bg-accent`.
    fn key_pill(
        &self,
        id: ElementId,
        target: Target,
        key: &str,
        editable: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let theme = Tokens::of(cx);
        let (border, accent) = (theme.border.opacity(0.7), theme.accent);
        div()
            .id(id)
            .h(px(28.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .px(px(6.))
            .rounded(px(6.))
            .border_1()
            .border_color(gpui::transparent_black())
            .when(editable, |cell| {
                cell.cursor_pointer()
                    .hover(move |style| style.border_color(border).bg(accent))
            })
            .when(!editable, |cell| cell.opacity(0.64))
            .child(Self::pill(key, cx))
            .when(editable, |cell| {
                cell.on_click(
                    cx.listener(move |this, _, _, cx| this.start_recording(target.clone(), cx)),
                )
            })
    }

    /// `WhenClauseControl`'s trigger (KeybindingsSettings.tsx:1033-1058): `Button size="micro"`
    /// (`h-5 rounded-sm px-[5px] text-[11px] gap-1`), `ghost` once a condition is set and
    /// `ghost-muted` for "Always", the expression in mono, and a 12px chevron at 60% opacity.
    fn when_trigger(
        &self,
        id: ElementId,
        target: Target,
        when: &str,
        editable: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let theme = Tokens::of(cx);
        let current = when.to_owned();
        let set = !when.is_empty();
        let label = if set {
            when.to_owned()
        } else {
            "Always".to_owned()
        };
        let accent = theme.accent;
        let foreground = theme.foreground;
        div()
            .id(id)
            .h(px(20.))
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(4.))
            .px(px(5.))
            .rounded(px(4.))
            .border_1()
            .border_color(gpui::transparent_black())
            .text_size(px(11.))
            .line_height(px(16.))
            .font_weight(FontWeight::MEDIUM)
            .text_color(if set {
                theme.foreground
            } else {
                theme.muted_foreground
            })
            .when(editable, |link| {
                link.cursor_pointer().hover(move |style| {
                    let style = style.bg(accent);
                    if set {
                        style
                    } else {
                        style.text_color(foreground)
                    }
                })
            })
            .when(!editable, |link| link.opacity(0.64))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .font_family(cx.theme().mono_font_family.clone())
                    .child(label),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .opacity(0.6)
                    .child(Icon::new(Icons::ChevronDown).size(px(12.))),
            )
            .when(editable, |link| {
                link.on_click(cx.listener(move |this, _, window, cx| {
                    this.open_when(target.clone(), current.clone(), window, cx)
                }))
            })
    }

    /// The "When" line shared by both row kinds: `flex h-6 gap-1.5`, a `text-xs` label at 70%.
    fn when_line(&self, trigger: gpui::Stateful<gpui::Div>, cx: &App) -> gpui::Div {
        h_flex()
            .h(px(24.))
            .items_center()
            .gap(px(6.))
            .child(
                div()
                    .text_xs()
                    .line_height(px(12.))
                    .text_color(Tokens::of(cx).muted_foreground.opacity(0.7))
                    .child("When"),
            )
            .child(trigger)
    }

    /// `KeybindingConflictWarning`: a warning icon whose tooltip carries the conflict.
    fn conflict_icon(
        id: ElementId,
        conflicts: &[String],
        cx: &App,
    ) -> Option<gpui::Stateful<gpui::Div>> {
        if conflicts.is_empty() {
            return None;
        }
        let mut names = conflicts
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        if conflicts.len() > 3 {
            names.push_str(", and more");
        }
        let text = format!("Conflicts with {names}. {CONFLICT_TIP}");
        Some(
            div()
                .id(id)
                .size(px(20.))
                .flex()
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .text_color(warning(&Tokens::of(cx)))
                .tooltip(tooltip(text))
                .child(Icon::new(Icons::TriangleAlert).size(px(14.))),
        )
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
        let target = Target::Row(row.id.clone());
        let badge = (row.source != Source::Default).then(|| {
            div()
                .px(px(5.))
                .rounded(px(4.))
                .border_1()
                .border_color(theme.border)
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(row.source.label())
        });
        let title = h_flex()
            .gap_2()
            .min_w_0()
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .child(command_label(&row.command)),
            )
            .children(badge);
        let when_line = self.when_line(
            self.when_trigger(
                eid("keybinding-when", &row.id),
                target.clone(),
                &when,
                editable,
                cx,
            ),
            cx,
        );
        let save = dirty.then(|| {
            let saved = row.clone();
            button(
                eid("keybinding-save", &row.id),
                Variant::Primary,
                editable && self.pending.is_empty(),
                cx,
            )
            .child("Save")
            .on_click(cx.listener(move |this, _, _, cx| {
                if this.editable(cx) {
                    this.save_row(&saved, cx)
                }
            }))
        });
        let recording = self.recording.as_ref() == Some(&target);
        let key_id = eid("keybinding-key", &row.id);
        let key_control = if !recording && key == row.key && !key.is_empty() && !dirty {
            self.key_pill(key_id, target, &key, editable, cx)
                .mr(px(-6.))
        } else {
            self.key_input(key_id, target, &key, editable, cx)
        };
        // Hidden until the row is hovered, and kept while its popup is open.
        let group = SharedString::from(format!("keybinding-row-{}", row.id));
        let menu = editable
            .then(|| self.row_menu(row, cx))
            .flatten()
            .map(|menu| {
                let open = self.menu.as_deref() == Some(row.id.as_str());
                div()
                    .opacity(if open { 1. } else { 0. })
                    .group_hover(group.clone(), |style| style.opacity(1.))
                    .child(menu)
            });
        let control = h_flex()
            .flex_wrap()
            .items_center()
            .justify_end()
            .gap(px(6.))
            .children(Self::conflict_icon(
                eid("keybinding-conflict", &row.id),
                &conflicts,
                cx,
            ))
            .children(menu)
            .children(save)
            .child(key_control)
            .into_any_element();
        div()
            .group(group)
            .child(settings_row(title, when_line, None, control, self.wide, cx))
            .into_any_element()
    }

    /// `NewKeybindingCommandSelect`: `Select size="sm"` at `w-56`, its list under the trigger.
    fn command_select(
        &self,
        new: &NewRow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Tokens::of(cx);
        let view = cx.entity();
        let id = ElementId::from("keybinding-command");
        let handle = tab_stop(&id, window, cx);
        let on_key: KeyHandler = Rc::new({
            let view = view.clone();
            move |event, _, cx| {
                if matches!(
                    event.keystroke.key.as_str(),
                    "enter" | "space" | "down" | "up"
                ) {
                    cx.stop_propagation();
                    view.update(cx, |this, cx| {
                        this.picking = true;
                        cx.notify();
                    });
                }
            }
        });
        let value = match new.command.as_deref() {
            Some(command) => div().child(command_label(command)).into_any_element(),
            None => div()
                .text_color(theme.muted_foreground)
                .child("Command")
                .into_any_element(),
        };
        let trigger = SelectTrigger::new(id, "", COMMAND_WIDTH, handle, on_key)
            .content(value)
            .open(self.picking);
        let popup = self.picking.then(|| {
            let options = command_options(&self.keybindings.rules);
            let mut list = popup_shell(COMMAND_WIDTH, cx)
                .id("keybinding-commands")
                .max_h(px(288.))
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
            .flex_shrink_0()
            .child(
                div()
                    .id("keybinding-command-trigger")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.picking = !this.picking;
                        cx.notify();
                    }))
                    .child(trigger),
            )
            .children(popup)
            .into_any_element()
    }

    /// `NewKeybindingSettingsRow` (KeybindingsSettings.tsx:1224-1255): a `bg-muted/15` row with the
    /// title and When line at the left and, at the right, the command select, a conflict
    /// warning, the shortcut input, Save and an icon-only cancel.
    fn new_row_view(
        &self,
        new: &NewRow,
        all: &[Row],
        editable: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Tokens::of(cx);
        let ready = editable
            && self.pending.is_empty()
            && new.command.is_some()
            && !new.key.trim().is_empty();
        let conflicts = conflict_labels(all, "new", &new.key, &new.when, is_mac());
        let when_line = self.when_line(
            self.when_trigger(
                "keybinding-new-when".into(),
                Target::New,
                &new.when,
                editable,
                cx,
            ),
            cx,
        );
        let control = h_flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(self.command_select(new, window, cx))
            .children(Self::conflict_icon(
                "keybinding-new-conflict".into(),
                &conflicts,
                cx,
            ))
            .child(self.key_input(
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
                icon_button(
                    "keybinding-new-cancel",
                    Variant::GhostMuted,
                    self.pending.is_empty(),
                    cx,
                )
                .tooltip(tooltip("Cancel"))
                .child(Icon::new(IconName::Close).size(px(14.)))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.new = None;
                    this.picking = false;
                    this.recording = None;
                    this._keys = None;
                    cx.notify();
                })),
            )
            .into_any_element();
        div()
            .bg(theme.muted.opacity(0.15))
            .child(row(
                "New keybinding",
                when_line,
                None,
                control,
                self.wide,
                cx,
            ))
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
            rows.push(self.new_row_view(new, &all, editable, window, cx));
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
