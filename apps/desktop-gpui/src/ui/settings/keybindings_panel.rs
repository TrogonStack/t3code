//! The Keybindings tab: `KeybindingsSettings.tsx`. A search, the rules grouped by command with
//! their source, `when` clause and key, and the writes as `server.upsertKeybinding` and
//! `server.removeKeybinding`; the config stream brings the result back. The `when` popover keeps
//! the web's expression input and condition tree in step, through the same grammar.

use super::components::{
    KeyHandler, SelectTrigger, group, popup_shell, row as settings_row, section, tab_stop,
};
use super::devices_panel::widgets::{
    Variant, button, field, icon_button, menu_item, note, problem, warning,
};
use super::scope_pickers::ScopeBar;
use super::tokens::Tokens;
use super::*;
use crate::connection::{Action, ActionSender, Call};
use crate::keybinding_rules::{
    self as rules, Row, Source, TOKEN_BASE, command_label, command_options, conflict_labels,
    filter_rows, group_rows, is_mac, pill_parts, record_input, target_payload, upsert_payload,
};
use crate::keybindings::{Keybindings, When};
use crate::status::CallEvent;
use gpui::{
    Bounds, ElementId, Keystroke, MouseButton, MouseDownEvent, Subscription, canvas, deferred,
};
use gpui_component::input::{Input, InputEvent, InputState};
use serde_json::Value;
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

/// `w-44` and `w-56` (KeybindingsSettings.tsx:1245-1250).
const KEY_INPUT_WIDTH: f32 = 176.;
const COMMAND_WIDTH: f32 = 224.;
const WHEN_WIDTH: f32 = 830.;
const OPERATOR_WIDTH: f32 = 200.;

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

#[derive(Clone, PartialEq, Eq)]
enum Pick {
    Variable(Vec<usize>),
    Operator(Vec<usize>),
}

/// `WhenExpressionBuilder`: the tree and the text it came from, which is not always a tree.
struct WhenEdit {
    target: Target,
    value: Option<When>,
    text: String,
    problem: Option<&'static str>,
    pick: Option<Pick>,
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
    when_trigger: Rc<Cell<Bounds<Pixels>>>,
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
            cx.subscribe_in(&when_input, window, |this, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    this.typed_when(cx);
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
            when_trigger: Rc::new(Cell::new(Bounds::default())),
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
        let closed = self.close_when_layer()
            || self.menu.take().is_some()
            || std::mem::take(&mut self.picking);
        if closed {
            cx.notify();
        }
        closed
    }

    fn close_when_layer(&mut self) -> bool {
        match &mut self.when_edit {
            Some(edit) if edit.pick.is_some() => {
                edit.pick = None;
                true
            }
            Some(_) => {
                self.when_edit = None;
                true
            }
            None => false,
        }
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
        let value = rules::parse_when_draft(&current).ok().flatten();
        self.when_input
            .update(cx, |input, cx| input.set_value(current.clone(), window, cx));
        let focus = self.when_input.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        self.when_edit = Some(WhenEdit {
            target,
            value,
            text: current,
            problem: None,
            pick: None,
        });
        cx.notify();
    }

    fn store_when(&mut self, target: Target, expression: String) {
        match target {
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

    fn set_tree(&mut self, value: Option<When>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(edit) = &mut self.when_edit else {
            return;
        };
        let expression = rules::when_expression(value.as_ref());
        edit.value = value;
        edit.text = expression.clone();
        edit.problem = None;
        edit.pick = None;
        let target = edit.target.clone();
        self.when_input.update(cx, |input, cx| {
            input.set_value(expression.clone(), window, cx)
        });
        self.store_when(target, expression);
        cx.notify();
    }

    fn edit_tree(
        &mut self,
        path: &[usize],
        change: rules::Edit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(edit) = &self.when_edit else {
            return;
        };
        let next = rules::apply_edit(edit.value.as_ref(), path, &change);
        self.set_tree(next, window, cx);
    }

    fn add_root(&mut self, group: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(edit) = &self.when_edit else {
            return;
        };
        let next = rules::add_root(edit.value.clone(), group);
        self.set_tree(Some(next), window, cx);
    }

    fn typed_when(&mut self, cx: &mut Context<Self>) {
        let text = self.when_input.read(cx).value().to_string();
        let Some(edit) = &mut self.when_edit else {
            return;
        };
        if text == edit.text {
            return;
        }
        edit.text = text.clone();
        match rules::parse_when_draft(&text) {
            Ok(value) => {
                let expression = rules::when_expression(value.as_ref());
                edit.value = value;
                edit.problem = None;
                let target = edit.target.clone();
                self.store_when(target, expression);
            }
            Err(message) => edit.problem = Some(message),
        }
        cx.notify();
    }

    fn when_invalid(&self, target: &Target) -> bool {
        self.when_edit
            .as_ref()
            .is_some_and(|edit| &edit.target == target && edit.problem.is_some())
    }

    fn toggle_pick(&mut self, pick: Pick, cx: &mut Context<Self>) {
        if let Some(edit) = &mut self.when_edit {
            edit.pick = if edit.pick.as_ref() == Some(&pick) {
                None
            } else {
                Some(pick)
            };
            cx.notify();
        }
    }

    fn unknown_icon(id: String, names: &[String], cx: &App) -> gpui::Stateful<gpui::Div> {
        let label = if names.len() == 1 {
            format!("Unknown condition: {}", names[0])
        } else {
            format!("Unknown conditions: {}", names.join(", "))
        };
        div()
            .id(ElementId::from(SharedString::from(id)))
            .size(px(18.))
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .text_color(warning(&Tokens::of(cx)))
            .tooltip(tooltip(format!(
                "{label}. T3 Code does not recognize this condition yet. It can still be saved, but it may not match unless the runtime provides it."
            )))
            .child(Icon::new(Icons::TriangleAlert).size(px(14.)))
    }

    /// `Button size="compact"` (`h-7 gap-1 rounded-md px-[7px] text-xs`).
    fn compact_button(
        id: String,
        primary: bool,
        label: &str,
        cx: &App,
    ) -> gpui::Stateful<gpui::Div> {
        let variant = if primary {
            Variant::Primary
        } else {
            Variant::Outline
        };
        button(ElementId::from(SharedString::from(id)), variant, true, cx)
            .px(px(7.))
            .gap(px(4.))
            .rounded(px(6.))
            .text_size(px(12.))
            .line_height(px(16.))
            .child(Icon::new(Icons::Plus).size(px(14.)))
            .child(label.to_owned())
    }

    fn compact_toggle(id: String, pressed: bool, cx: &App) -> gpui::Stateful<gpui::Div> {
        let theme = Tokens::of(cx);
        let background = if theme.dark {
            theme.input.opacity(0.32)
        } else {
            theme.background
        };
        let on = if theme.dark {
            theme.input
        } else {
            theme.accent
        };
        let hover = if theme.dark {
            theme.input.opacity(0.64)
        } else {
            theme.accent.opacity(0.5)
        };
        div()
            .id(ElementId::from(SharedString::from(id)))
            .h(px(28.))
            .min_w(px(40.))
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .px(px(3.))
            .rounded(px(6.))
            .border_1()
            .border_color(theme.input)
            .bg(if pressed { on } else { background })
            .text_size(px(12.))
            .line_height(px(16.))
            .font_weight(FontWeight::MEDIUM)
            .text_color(theme.foreground)
            .cursor_pointer()
            .hover(move |style| style.bg(if pressed { on } else { hover }))
            .child("Not")
    }

    fn compact_select(
        id: String,
        label: &str,
        open: bool,
        warning_icon: Option<gpui::Stateful<gpui::Div>>,
        cx: &App,
    ) -> gpui::Stateful<gpui::Div> {
        let theme = Tokens::of(cx);
        let background = if theme.dark {
            theme.input.opacity(0.32)
        } else {
            theme.background
        };
        div()
            .id(ElementId::from(SharedString::from(id)))
            .h(px(28.))
            .min_w(px(144.))
            .w_full()
            .flex()
            .items_center()
            .gap(px(4.))
            .px(px(7.))
            .rounded(px(6.))
            .border_1()
            .border_color(if open { theme.ring } else { theme.input })
            .bg(background)
            .text_size(px(12.))
            .text_color(theme.foreground)
            .cursor_pointer()
            .child(div().min_w_0().flex_1().truncate().child(label.to_owned()))
            .children(warning_icon)
            .child(
                div()
                    .mr(px(-4.))
                    .flex_shrink_0()
                    .opacity(0.7)
                    .text_color(theme.muted_foreground)
                    .child(Icon::new(Icons::ChevronDown).size(px(14.))),
            )
    }

    fn pick_list(
        &self,
        options: Vec<(String, bool, rules::Edit)>,
        path: Vec<usize>,
        id: &str,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut list = popup_shell(width, cx)
            .id(ElementId::from(SharedString::from(format!("{id}-list"))))
            .max_h(px(288.))
            .overflow_y_scroll();
        let foreground = Tokens::of(cx).foreground.opacity(0.08);
        for (index, (label, selected, change)) in options.into_iter().enumerate() {
            let path = path.clone();
            list = list.child(
                menu_item(
                    ElementId::from(SharedString::from(format!("{id}-option-{index}"))),
                    label,
                    false,
                    cx,
                )
                .when(selected, |item| item.bg(foreground))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.edit_tree(&path, change.clone(), window, cx)
                    }),
                ),
            );
        }
        deferred(div().absolute().top(px(32.)).left_0().occlude().child(list))
            .with_priority(2)
            .into_any_element()
    }

    fn remove_button(
        &self,
        id: String,
        label: &'static str,
        path: Vec<usize>,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        icon_button(
            ElementId::from(SharedString::from(id)),
            Variant::Ghost,
            true,
            cx,
        )
        .tooltip(tooltip(label))
        .child(Icon::new(Icons::Minus).size(px(14.)))
        .on_click(cx.listener(move |this, _, window, cx| {
            this.edit_tree(&path, rules::Edit::Remove, window, cx)
        }))
    }

    fn guide(depth: usize, node: AnyElement, cx: &App) -> gpui::Div {
        let theme = Tokens::of(cx);
        let color = if depth == 0 {
            theme.border
        } else {
            theme.border.opacity(0.7)
        };
        div()
            .relative()
            .pl(px(16.))
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(6.))
                    .w(px(1.))
                    .bg(color),
            )
            .child(
                div()
                    .absolute()
                    .top(px(16.))
                    .left(px(6.))
                    .h(px(1.))
                    .w(px(10.))
                    .bg(color),
            )
            .child(node)
    }

    fn when_node(
        &self,
        node: &When,
        path: Vec<usize>,
        depth: usize,
        removable: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Tokens::of(cx);
        let key = path
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join("-");
        let remove = removable.then(|| {
            self.remove_button(
                format!("when-remove-{key}-{depth}"),
                rules::remove_label(node, depth),
                path.clone(),
                cx,
            )
        });

        if let Some((name, negated)) = rules::condition_parts(node) {
            let pick = Pick::Variable(path.clone());
            let open = self
                .when_edit
                .as_ref()
                .is_some_and(|edit| edit.pick.as_ref() == Some(&pick));
            let warning_icon = (!rules::is_known_variable(name))
                .then(|| Self::unknown_icon(format!("when-unknown-{key}"), &[name.to_owned()], cx));
            let select_id = format!("when-variable-{key}");
            let list = open.then(|| {
                let mut variables = rules::variable_options();
                if !variables.iter().any(|variable| variable == name) {
                    variables.insert(0, name.to_owned());
                }
                let options = variables
                    .into_iter()
                    .map(|variable| {
                        let selected = variable == name;
                        (
                            variable.clone(),
                            selected,
                            rules::Edit::Identifier(variable),
                        )
                    })
                    .collect();
                self.pick_list(options, path.clone(), &select_id, 160., cx)
            });
            let toggle_path = path.clone();
            let select = div()
                .relative()
                .min_w_0()
                .flex_1()
                .child(
                    Self::compact_select(select_id, name, open, warning_icon, cx).on_click(
                        cx.listener(move |this, _, _, cx| this.toggle_pick(pick.clone(), cx)),
                    ),
                )
                .children(list);
            return h_flex()
                .items_center()
                .gap_2()
                .px_2()
                .py_2()
                .rounded(px(6.))
                .border_1()
                .border_color(theme.border.opacity(0.7))
                .bg(theme.background.opacity(0.6))
                .child(
                    Self::compact_toggle(format!("when-not-{key}"), negated, cx).on_click(
                        cx.listener(move |this, _, window, cx| {
                            this.edit_tree(&toggle_path, rules::Edit::Negate(!negated), window, cx)
                        }),
                    ),
                )
                .child(select)
                .children(remove)
                .into_any_element();
        }

        if let When::Not(inner) = node {
            let toggle_path = path.clone();
            let header = h_flex()
                .items_center()
                .gap_2()
                .child(
                    Self::compact_toggle(format!("when-not-{key}"), true, cx).on_click(
                        cx.listener(move |this, _, window, cx| {
                            this.edit_tree(&toggle_path, rules::Edit::Negate(false), window, cx)
                        }),
                    ),
                )
                .children(remove.map(|remove| div().ml_auto().child(remove)));
            let child = self.when_node(inner, path, depth + 1, false, cx);
            return v_flex()
                .gap_2()
                .p_2()
                .rounded(px(8.))
                .border_1()
                .border_color(theme.border.opacity(if depth > 0 { 0.5 } else { 0.7 }))
                .bg(if depth > 0 {
                    theme.background.opacity(0.5)
                } else {
                    theme.muted.opacity(0.2)
                })
                .child(header)
                .child(Self::guide(1, child, cx))
                .into_any_element();
        }

        let operator = rules::group_operator(node);
        let pick = Pick::Operator(path.clone());
        let open = self
            .when_edit
            .as_ref()
            .is_some_and(|edit| edit.pick.as_ref() == Some(&pick));
        let select_id = format!("when-operator-{key}");
        let list = open.then(|| {
            let options = [rules::Operator::And, rules::Operator::Or]
                .into_iter()
                .map(|choice| {
                    (
                        choice.label().to_owned(),
                        choice == operator,
                        rules::Edit::Operator(choice),
                    )
                })
                .collect();
            self.pick_list(options, path.clone(), &select_id, 96., cx)
        });
        let operator_select = div()
            .relative()
            .w(px(OPERATOR_WIDTH))
            .flex_shrink_0()
            .child(
                Self::compact_select(select_id, operator.label(), open, None, cx).on_click(
                    cx.listener(move |this, _, _, cx| this.toggle_pick(pick.clone(), cx)),
                ),
            )
            .children(list);
        let (condition_path, group_path) = (path.clone(), path.clone());
        let header = h_flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(operator_select)
            .child(
                Self::compact_button(format!("when-add-condition-{key}"), false, "Condition", cx)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.edit_tree(&condition_path, rules::Edit::AddCondition, window, cx)
                    })),
            )
            .child(
                Self::compact_button(format!("when-add-group-{key}"), false, "Group", cx).on_click(
                    cx.listener(move |this, _, window, cx| {
                        this.edit_tree(&group_path, rules::Edit::AddGroup, window, cx)
                    }),
                ),
            )
            .children(remove.map(|remove| div().ml_auto().child(remove)));
        let children = rules::flatten(node, operator)
            .iter()
            .enumerate()
            .map(|(index, child)| {
                let mut child_path = path.clone();
                child_path.push(index);
                let body = self.when_node(child, child_path, depth + 1, true, cx);
                Self::guide(depth, body, cx).into_any_element()
            })
            .collect::<Vec<_>>();
        v_flex()
            .gap_2()
            .p_2()
            .rounded(px(8.))
            .border_1()
            .border_color(theme.border.opacity(if depth > 0 { 0.7 } else { 0.6 }))
            .bg(if depth > 0 {
                theme.background.opacity(0.55)
            } else {
                theme.muted.opacity(0.1)
            })
            .child(header)
            .child(v_flex().gap_2().children(children))
            .into_any_element()
    }

    /// `WhenExpressionBuilder` (KeybindingsSettings.tsx:556-675), anchored under the trigger.
    fn when_popover(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let edit = self.when_edit.as_ref()?;
        let theme = Tokens::of(cx);
        let invalid = edit.problem.is_some();
        let unknown = rules::unknown_variables(edit.value.as_ref());
        let focused = self
            .when_input
            .read(cx)
            .focus_handle(cx)
            .contains_focused(window, cx);
        let background = if theme.dark {
            theme.input.opacity(0.32)
        } else {
            theme.background
        };
        let border = match (invalid, focused) {
            (true, true) => theme.destructive.opacity(0.64),
            (true, false) => theme.destructive.opacity(0.36),
            (false, true) => theme.ring,
            (false, false) => theme.input,
        };
        let header = h_flex()
            .items_start()
            .justify_between()
            .gap_3()
            .child(
                div()
                    .text_size(px(14.))
                    .line_height(px(20.))
                    .font_weight(FontWeight::MEDIUM)
                    .child("When"),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .gap_2()
                    .child(
                        Self::compact_button("when-root-condition".into(), false, "Condition", cx)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.add_root(false, window, cx)),
                            ),
                    )
                    .child(
                        Self::compact_button("when-root-group".into(), false, "Group", cx)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.add_root(true, window, cx)),
                            ),
                    ),
            );
        let input = h_flex()
            .w_full()
            .h(px(28.))
            .items_center()
            .px(px(9.))
            .rounded(px(6.))
            .border_1()
            .border_color(border)
            .bg(background)
            .text_size(px(12.))
            .font_family(cx.theme().mono_font_family.clone())
            .child(
                div().flex_1().min_w_0().child(
                    Input::new(&self.when_input)
                        .appearance(false)
                        .bordered(false)
                        .h(px(26.))
                        .px_0(),
                ),
            )
            .children(
                (!unknown.is_empty())
                    .then(|| Self::unknown_icon("when-unknown-input".into(), &unknown, cx)),
            );
        let message = edit.problem.map(|message| {
            h_flex()
                .items_center()
                .gap(px(6.))
                .text_size(px(11.))
                .line_height(px(16.))
                .text_color(theme.destructive)
                .child(Icon::new(Icons::CircleX).size(px(14.)))
                .child(message)
        });
        let body = match &edit.value {
            Some(value) => self.when_node(value, Vec::new(), 0, true, cx),
            None => div()
                .p_3()
                .rounded(px(6.))
                .border_1()
                .border_dashed()
                .border_color(theme.border.opacity(0.8))
                .bg(theme.muted.opacity(0.15))
                .child(
                    h_flex()
                        .flex_wrap()
                        .gap_2()
                        .child(
                            Self::compact_button(
                                "when-empty-condition".into(),
                                true,
                                "Condition",
                                cx,
                            )
                            .on_click(
                                cx.listener(|this, _, window, cx| this.add_root(false, window, cx)),
                            ),
                        )
                        .child(
                            Self::compact_button("when-empty-group".into(), false, "Group", cx)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.add_root(true, window, cx)
                                })),
                        ),
                )
                .into_any_element(),
        };
        let blocked = invalid.then(|| {
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .p_4()
                .rounded(px(8.))
                .border_1()
                .border_color(theme.destructive.opacity(0.3))
                .bg(theme.background.opacity(0.75))
                .text_size(px(12.))
                .text_color(theme.destructive)
                .child("Fix the expression above to continue editing visually.")
        });
        let card = popup_shell(WHEN_WIDTH, cx)
            .w(px(WHEN_WIDTH))
            .p(px(16.))
            .gap(px(12.))
            .child(header)
            .child(v_flex().gap(px(6.)).child(input).children(message))
            .child(div().relative().child(body).children(blocked));
        Some(
            deferred(
                div()
                    .absolute()
                    .top(px(26.))
                    .left_0()
                    .occlude()
                    .on_mouse_down_out(cx.listener(|this, event: &MouseDownEvent, _, cx| {
                        if let Some(edit) = &mut this.when_edit {
                            if edit.pick.take().is_none()
                                && !this.when_trigger.get().contains(&event.position)
                            {
                                this.when_edit = None;
                            }
                            cx.notify();
                        }
                    }))
                    .child(card),
            )
            .with_priority(1)
            .into_any_element(),
        )
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
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let theme = Tokens::of(cx);
        let open = self
            .when_edit
            .as_ref()
            .is_some_and(|edit| edit.target == target);
        let popover = if open {
            self.when_popover(window, cx)
        } else {
            None
        };
        let bounds = self.when_trigger.clone();
        let current = when.to_owned();
        let set = !when.is_empty();
        let label = if set {
            when.to_owned()
        } else {
            "Always".to_owned()
        };
        let accent = theme.accent;
        let foreground = theme.foreground;
        let trigger = div()
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
            .when(open, |link| {
                link.child(
                    canvas(move |area, _, _| bounds.set(area), |_, _, _, _| {})
                        .absolute()
                        .size_full(),
                )
            })
            .when(editable, |link| {
                link.on_click(cx.listener(move |this, _, window, cx| {
                    if open {
                        this.when_edit = None;
                        cx.notify();
                    } else {
                        this.open_when(target.clone(), current.clone(), window, cx)
                    }
                }))
            });
        div().relative().min_w_0().child(trigger).children(popover)
    }

    /// The "When" line shared by both row kinds: `flex h-6 gap-1.5`, a `text-xs` label at 70%.
    fn when_line(&self, trigger: gpui::Div, cx: &App) -> gpui::Div {
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
        window: &mut Window,
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
                window,
                cx,
            ),
            cx,
        );
        let save = dirty.then(|| {
            let saved = row.clone();
            button(
                eid("keybinding-save", &row.id),
                Variant::Primary,
                editable && self.pending.is_empty() && !self.when_invalid(&target),
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
            && !new.key.trim().is_empty()
            && !self.when_invalid(&Target::New);
        let conflicts = conflict_labels(all, "new", &new.key, &new.when, is_mac());
        let when_line = self.when_line(
            self.when_trigger(
                "keybinding-new-when".into(),
                Target::New,
                &new.when,
                editable,
                window,
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
                rows.push(self.row_view(row, &all, editable, window, cx));
            }
        }
        v_flex().child(section("Keybindings", group(rows, cx)))
    }
}
