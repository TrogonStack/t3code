//! The Providers tab: `ProviderSettingsPanel.tsx` (the instance list beside the selected
//! instance) and `ProviderInstanceCard.tsx` (Status, Display name and the Environment variables).
//! Writes are optimistic and put back when the call fails. The driver Setup and Runtime forms,
//! accent colour, models, usage and adding or removing an instance are not here.

use super::components::{group, row, section, switch};
use super::devices_panel::widgets::{Variant, button, icon_button, note};
use super::scope_pickers::ScopeBar;
use super::secret_field::{Check, SecretEvent, SecretField, sm_field};
use super::tokens::Tokens;
use super::*;
use crate::connection::{Action, ActionSender, Call};
use crate::providers::InstanceRow;
use crate::secret_source::SecretSource;
use crate::settings_labs::{Scope, resolve};
use crate::settings_providers::{self as model, Content, Variable};
use crate::settings_secrets::Config;
use crate::status::CallEvent;
use gpui::Subscription;
use gpui_component::input::{InputEvent, InputState};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::rc::Rc;

const LIST_WIDTH: f32 = 272.;
const SENSITIVE_NOTE: &str =
    "Sensitive values are stored separately and never returned to the app.";
const REDACTED_NOTE: &str = " Enter a new value to rename a saved one or make it plain text.";

struct Row {
    id: u64,
    name: Entity<InputState>,
    field: Entity<SecretField>,
    /// The row as last loaded or stored: edits are measured against it.
    base: Variable,
    _subscriptions: Vec<Subscription>,
}

/// What the scope bar says about the environment the tab manages.
struct Ctx {
    connected: bool,
    editable: bool,
    in_project: bool,
}

pub struct ProvidersPanel {
    actions: ActionSender,
    scope: Entity<ScopeBar>,
    wide: bool,
    settings: Value,
    instances: Vec<InstanceRow>,
    sources: Vec<SecretSource>,
    selected: Option<String>,
    /// Settings writes in flight, with the settings to fall back to.
    pending: HashMap<u64, Value>,
    /// Secret checks in flight, with the row that asked.
    checks: HashMap<u64, u64>,
    next_token: u64,
    next_row: u64,
    rows: Vec<Row>,
    rows_for: Option<String>,
    /// The variables last stored; an echo of them leaves the rows alone.
    last: Vec<Variable>,
    reseed: bool,
    display_name: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
}

impl ProvidersPanel {
    pub fn new(
        actions: ActionSender,
        scope: Entity<ScopeBar>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let display_name = cx.new(|cx| InputState::new(window, cx).placeholder("Instance label"));
        let subscriptions = vec![
            cx.observe(&scope, |_, _, cx| cx.notify()),
            cx.subscribe_in(
                &display_name,
                window,
                |this, _, event: &InputEvent, _, cx| match event {
                    InputEvent::Blur | InputEvent::PressEnter { .. } => this.commit_name(cx),
                    _ => {}
                },
            ),
        ];
        Self {
            actions,
            scope,
            wide: true,
            settings: Value::Null,
            instances: Vec::new(),
            sources: Vec::new(),
            selected: None,
            pending: HashMap::new(),
            checks: HashMap::new(),
            next_token: model::TOKEN_BASE,
            next_row: 0,
            rows: Vec::new(),
            rows_for: None,
            last: Vec::new(),
            reseed: false,
            display_name,
            _subscriptions: subscriptions,
        }
    }

    pub fn set_wide(&mut self, wide: bool, cx: &mut Context<Self>) {
        if self.wide != wide {
            self.wide = wide;
            cx.notify();
        }
    }

    /// The attached environment's instances; ignored while a write is in flight.
    pub fn set_data(
        &mut self,
        config: Config,
        instances: Vec<InstanceRow>,
        sources: Vec<SecretSource>,
        cx: &mut Context<Self>,
    ) {
        if !self.pending.is_empty() {
            return;
        }
        if self.settings != config.settings
            || self.instances != instances
            || self.sources != sources
        {
            self.settings = config.settings;
            self.instances = instances;
            if self.sources != sources {
                for row in &self.rows {
                    row.field
                        .update(cx, |field, cx| field.set_sources(&sources, cx));
                }
                self.sources = sources;
            }
            cx.notify();
        }
    }

    /// Starts from the stored rows, as the settings page opens.
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        self.reseed = true;
        cx.notify();
    }

    /// Closes a source menu; returns whether there was one, so Escape stops there.
    pub fn close_menu(&mut self, cx: &mut Context<Self>) -> bool {
        let mut closed = false;
        for row in &self.rows {
            closed |= row.field.update(cx, |field, cx| field.close_menu(cx));
        }
        closed
    }

    fn ctx(&self, cx: &App) -> Ctx {
        let scope = self.scope.read(cx);
        let (search, data) = (scope.search(), scope.data());
        let resolved = resolve(search, data);
        let selected = match &resolved {
            Scope::Environment(id) => data.environments.iter().find(|e| &e.id == id),
            _ => None,
        };
        let environment = selected
            .or_else(|| data.environments.iter().find(|e| e.connected && e.writable))
            .or_else(|| data.environments.iter().find(|e| e.connected))
            .or(data.environments.first());
        Ctx {
            connected: environment.is_some_and(|e| e.connected),
            editable: environment.is_some_and(|e| e.connected && e.writable),
            in_project: matches!(resolved, Scope::Project(_)),
        }
    }

    fn selected_instance(&self) -> Option<&InstanceRow> {
        self.selected
            .as_ref()
            .and_then(|id| self.instances.iter().find(|row| &row.id == id))
            .or_else(|| self.instances.first())
    }

    fn send(&mut self, call: Call, payload: Value) -> Option<u64> {
        let token = self.next_token;
        self.next_token += 1;
        let action = Action::Call {
            token,
            call,
            payload,
        };
        self.actions.try_send(action).ok().map(|()| token)
    }

    /// Stores `instance`, showing it at once.
    fn write(&mut self, instance_id: &str, instance: Value, cx: &mut Context<Self>) {
        let previous = self.settings.clone();
        model::apply_instance(&mut self.settings, instance_id, &instance);
        match self.send(
            Call::UpdateSettings,
            model::upsert_payload(instance_id, &instance),
        ) {
            Some(token) => {
                self.pending.insert(token, previous);
            }
            None => self.settings = previous,
        }
        cx.notify();
    }

    /// The outcome of a write or of a secret check.
    pub fn on_call(&mut self, token: u64, event: CallEvent, cx: &mut Context<Self>) {
        if matches!(event, CallEvent::Progress(_)) {
            return;
        }
        if let Some(row) = self.checks.remove(&token) {
            let state = match event {
                CallEvent::Done(value) if value["_tag"] == "Resolved" => Check::Resolved,
                CallEvent::Done(value) => Check::Failed(
                    value["detail"]
                        .as_str()
                        .unwrap_or("The secret could not be resolved.")
                        .to_owned(),
                ),
                CallEvent::Failed(message) => Check::Failed(message),
                CallEvent::Progress(_) => return,
            };
            if let Some(row) = self.rows.iter().find(|r| r.id == row) {
                row.field.update(cx, |field, cx| field.set_check(state, cx));
            }
            return;
        }
        let Some(previous) = self.pending.remove(&token) else {
            return;
        };
        if matches!(event, CallEvent::Failed(_)) {
            self.settings = previous;
            self.reseed = true;
            cx.notify();
        }
    }

    fn base_of(&self, instance: &InstanceRow) -> Value {
        model::base_instance(&self.settings, &instance.id, &instance.driver)
    }

    fn set_enabled(&mut self, id: &str, enabled: bool, cx: &mut Context<Self>) {
        let Some(instance) = self.instances.iter().find(|row| row.id == id).cloned() else {
            return;
        };
        let mut stored = self.base_of(&instance);
        if let Some(object) = stored.as_object_mut() {
            object.insert("enabled".into(), json!(enabled));
        }
        self.write(id, stored, cx);
    }

    fn commit_name(&mut self, cx: &mut Context<Self>) {
        let Some(instance) = self.selected_instance().cloned() else {
            return;
        };
        let typed = self.display_name.read(cx).value().trim().to_owned();
        let mut stored = self.base_of(&instance);
        let current = stored["displayName"].as_str().unwrap_or("").to_owned();
        if typed == current {
            return;
        }
        if let Some(object) = stored.as_object_mut() {
            if typed.is_empty() {
                object.remove("displayName");
            } else {
                object.insert("displayName".into(), json!(typed));
            }
        }
        self.write(&instance.id, stored, cx);
    }

    fn variable(&self, row: &Row, cx: &App) -> Variable {
        let mut variable = row.base.clone();
        variable.name = row.name.read(cx).value().to_string();
        let field = row.field.read(cx);
        let content = match field.secret() {
            Some((provider, reference)) => Content::Secret {
                provider,
                reference,
            },
            None => Content::Text(field.typed(cx)),
        };
        if content == variable.content {
            return variable;
        }
        match content {
            Content::Secret {
                provider,
                reference,
            } => variable.with_secret(Some((provider, reference))),
            Content::Text(text)
                if text.is_empty() && matches!(variable.content, Content::Secret { .. }) =>
            {
                variable.with_secret(None)
            }
            Content::Text(text) => variable.with_text(text),
        }
    }

    /// Stores the rows (`publishRows`), unless one holds a bad name or nothing changed.
    fn commit(&mut self, cx: &mut Context<Self>) {
        let Some(instance) = self.selected_instance().cloned() else {
            return;
        };
        let variables: Vec<Variable> = self.rows.iter().map(|row| self.variable(row, cx)).collect();
        let Some(published) = model::publish(&variables) else {
            return;
        };
        if published == self.last {
            return;
        }
        let mut stored = published.iter();
        for (row, variable) in self.rows.iter_mut().zip(&variables) {
            if model::valid_name(&variable.name)
                && let Some(next) = stored.next()
            {
                row.base = next.clone();
            }
        }
        let instance_value = model::with_environment(&self.base_of(&instance), &published);
        self.last = published;
        self.write(&instance.id, instance_value, cx);
    }

    fn make_row(
        &mut self,
        variable: &Variable,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Row {
        let id = self.next_row;
        self.next_row += 1;
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("VARIABLE_NAME"));
        name.update(cx, |state, cx| {
            state.set_value(variable.name.clone(), window, cx)
        });
        let field = cx.new(|cx| SecretField::new(format!("provider-var-{id}"), "", window, cx));
        let sources = self.sources.clone();
        field.update(cx, |field, cx| {
            field.set_sources(&sources, cx);
            field.set_mono(true, cx);
            field.set_plain(!variable.sensitive, window, cx);
            let (secret, typed) = match &variable.content {
                Content::Secret {
                    provider,
                    reference,
                } => (Some((provider.clone(), reference.clone())), String::new()),
                Content::Text(text) => (None, text.clone()),
            };
            let configured = variable.is_redacted() && typed.is_empty() && secret.is_none();
            field.show(secret, configured, &typed, window, cx);
        });
        let subscriptions = vec![
            cx.subscribe_in(
                &name,
                window,
                |this, _, event: &InputEvent, _, cx| match event {
                    InputEvent::Change => cx.notify(),
                    InputEvent::Blur | InputEvent::PressEnter { .. } => this.commit(cx),
                    _ => {}
                },
            ),
            cx.subscribe_in(
                &field,
                window,
                move |this, _, event: &SecretEvent, _, cx| match event {
                    SecretEvent::Commit => this.commit(cx),
                    SecretEvent::Check(provider, reference) => {
                        this.check(id, provider, reference, cx)
                    }
                    SecretEvent::Changed | SecretEvent::Submit => cx.notify(),
                },
            ),
        ];
        Row {
            id,
            name,
            field,
            base: variable.clone(),
            _subscriptions: subscriptions,
        }
    }

    fn check(&mut self, row: u64, provider: &str, reference: &str, cx: &mut Context<Self>) {
        let payload = crate::secret_source::check_payload(provider, reference);
        match self.send(Call::CheckSecretSource, payload) {
            Some(token) => {
                self.checks.insert(token, row);
            }
            None => {
                if let Some(row) = self.rows.iter().find(|r| r.id == row) {
                    row.field.update(cx, |field, cx| {
                        field.set_check(Check::Failed("Could not reach the server.".to_owned()), cx)
                    });
                }
            }
        }
    }

    /// Loads the selected instance's variables when it changed, or the stored ones differ from
    /// what this panel last stored.
    fn ensure_rows(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(instance) = self.selected_instance().cloned() else {
            self.rows.clear();
            self.rows_for = None;
            return;
        };
        let stored_instance = self.base_of(&instance);
        let stored = model::decode(&stored_instance);
        let stale = self.rows_for.as_deref() != Some(instance.id.as_str())
            || self.reseed
            || (self.pending.is_empty() && stored != self.last);
        if !stale {
            return;
        }
        self.reseed = false;
        self.rows_for = Some(instance.id.clone());
        self.checks.clear();
        let rows: Vec<Row> = stored
            .iter()
            .map(|variable| self.make_row(variable, window, cx))
            .collect();
        self.rows = rows;
        self.last = stored;
        let label = stored_instance["displayName"]
            .as_str()
            .unwrap_or("")
            .to_owned();
        self.display_name
            .update(cx, |state, cx| state.set_value(label, window, cx));
    }

    fn select(&mut self, id: String, cx: &mut Context<Self>) {
        self.selected = Some(id);
        cx.notify();
    }

    fn add_variable(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let row = self.make_row(&Variable::blank(), window, cx);
        let focus = row.name.read(cx).focus_handle(cx);
        self.rows.push(row);
        window.focus(&focus, cx);
        cx.notify();
    }

    fn remove_variable(&mut self, id: u64, cx: &mut Context<Self>) {
        self.rows.retain(|row| row.id != id);
        self.commit(cx);
        cx.notify();
    }

    fn toggle_sensitive(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.rows.iter().position(|row| row.id == id) else {
            return;
        };
        let current = self.variable(&self.rows[index], cx);
        if current.is_redacted() {
            return;
        }
        let next = current.clone().with_sensitive(!current.sensitive);
        let plain = !next.sensitive;
        self.rows[index].base = next;
        self.rows[index]
            .field
            .update(cx, |field, cx| field.set_plain(plain, window, cx));
        self.commit(cx);
        cx.notify();
    }

    fn status_line(instance: &InstanceRow) -> String {
        if !instance.enabled {
            return "Disabled".to_owned();
        }
        let mut status = instance.status.clone();
        if let Some(first) = status.get_mut(0..1) {
            first.make_ascii_uppercase();
        }
        match instance
            .message
            .as_deref()
            .map(str::trim)
            .filter(|m| !m.is_empty())
        {
            Some(message) => format!("{status} \u{b7} {message}"),
            None => status,
        }
    }

    fn list_row(
        &self,
        instance: &InstanceRow,
        selected: bool,
        editable: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Tokens::of(cx);
        let view = cx.entity();
        let id = instance.id.clone();
        let name = instance.mark.display_name.clone();
        let enabled = instance.enabled;
        let toggle = {
            let (view, id) = (view.clone(), id.clone());
            Rc::new(move |_: &mut Window, cx: &mut App| {
                let id = id.clone();
                view.update(cx, |this, cx| this.set_enabled(&id, !enabled, cx))
            })
        };
        let control = div()
            .flex_shrink_0()
            .when(!editable, |control| control.opacity(0.64))
            .child(switch(
                SharedString::from(format!("provider-enabled-{id}")),
                enabled,
                if editable {
                    toggle
                } else {
                    Rc::new(|_: &mut Window, _: &mut App| {})
                },
                window,
                cx,
            ));
        h_flex()
            .id(SharedString::from(format!("provider-row-{id}")))
            .min_h(px(72.))
            .items_center()
            .gap_3()
            .px_3()
            .py_3()
            .cursor_pointer()
            .when(selected, |row| row.bg(theme.foreground.opacity(0.06)))
            .hover(move |row| row.bg(theme.foreground.opacity(0.04)))
            .on_click({
                let id = id.clone();
                move |_, _, cx| {
                    let id = id.clone();
                    view.update(cx, |this, cx| this.select(id, cx))
                }
            })
            .child(
                h_flex()
                    .min_w_0()
                    .flex_1()
                    .items_start()
                    .gap_3()
                    .when(!enabled && !selected, |left| left.opacity(0.6))
                    .child(
                        div()
                            .pt(px(2.))
                            .child(provider_mark(&instance.mark, 1., cx)),
                    )
                    .child(
                        v_flex()
                            .min_w_0()
                            .flex_1()
                            .gap(px(2.))
                            .child(
                                div()
                                    .truncate()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(name),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .line_height(px(16.))
                                    .text_color(theme.muted_foreground)
                                    .child(Self::status_line(instance)),
                            ),
                    ),
            )
            .child(control)
            .into_any_element()
    }

    fn variable_row(
        &self,
        row: &Row,
        editable: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Tokens::of(cx);
        let view = cx.entity();
        let variable = self.variable(row, cx);
        let redacted = variable.is_redacted();
        let invalid = !variable.name.trim().is_empty() && !model::valid_name(&variable.name);
        let row_id = row.id;
        let sensitive = variable.sensitive;
        let reference = row.field.read(cx).is_reference();
        let shown = if variable.name.trim().is_empty() {
            "variable".to_owned()
        } else {
            variable.name.trim().to_owned()
        };
        let remove = icon_button(
            SharedString::from(format!("provider-var-remove-{row_id}")),
            Variant::GhostDestructive,
            editable,
            cx,
        )
        .size(px(20.))
        .tooltip(tooltip(format!("Remove environment variable {shown}")))
        .child(Icon::new(Icons::X).size(px(12.)))
        .on_click({
            let view = view.clone();
            move |_, _, cx| view.update(cx, |this, cx| this.remove_variable(row_id, cx))
        });
        let lock = (!reference).then(|| {
            let enabled = editable && !redacted;
            let button = icon_button(
                SharedString::from(format!("provider-var-sensitive-{row_id}")),
                Variant::GhostMuted,
                enabled,
                cx,
            )
            .size(px(20.))
            .tooltip(tooltip(if sensitive {
                "Sensitive, stored separately"
            } else {
                "Plain text"
            }))
            .child(
                Icon::new(if sensitive {
                    Icons::Lock
                } else {
                    Icons::LockOpen
                })
                .size(px(12.)),
            );
            if enabled {
                let view = view.clone();
                button
                    .on_click(move |_, window, cx| {
                        view.update(cx, |this, cx| this.toggle_sensitive(row_id, window, cx))
                    })
                    .into_any_element()
            } else {
                button.into_any_element()
            }
        });
        let select = row
            .field
            .read(cx)
            .has_source()
            .then(|| row.field.update(cx, |field, cx| field.select(cx)));
        let value = row
            .field
            .update(cx, |field, cx| field.value_element(lock, window, cx));
        v_flex()
            .gap_2()
            .p_2()
            .rounded(px(8.))
            .border_1()
            .border_color(theme.border)
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1().min_w_0().child(sm_field(
                        &row.name,
                        true,
                        invalid,
                        !editable || redacted,
                        window,
                        cx,
                    )))
                    .children(select)
                    .child(remove),
            )
            .child(value)
            .into_any_element()
    }

    fn variables(&self, editable: bool, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = Tokens::of(cx);
        let any_redacted = self
            .rows
            .iter()
            .any(|row| self.variable(row, cx).is_redacted());
        let rows: Vec<AnyElement> = self
            .rows
            .iter()
            .map(|row| self.variable_row(row, editable, window, cx))
            .collect();
        let add = button("provider-var-add", Variant::Outline, editable, cx)
            .h(px(24.))
            .px(px(7.))
            .gap(px(4.))
            .text_size(px(12.))
            .line_height(px(16.))
            .child(Icon::new(Icons::Plus).size(px(12.)))
            .child("Add variable")
            .on_click(cx.listener(|this, _, window, cx| this.add_variable(window, cx)));
        v_flex()
            .gap_3()
            .px_4()
            .py_3()
            .child(
                h_flex()
                    .items_start()
                    .justify_between()
                    .gap_4()
                    .child(
                        v_flex()
                            .min_w_0()
                            .gap(px(2.))
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child("Variables"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .line_height(px(16.))
                                    .text_color(theme.muted_foreground)
                                    .child(
                                        "API keys, base URLs, and other per-instance CLI settings.",
                                    ),
                            ),
                    )
                    .child(add),
            )
            .children(rows)
            .child(note(
                if any_redacted {
                    format!("{SENSITIVE_NOTE}{REDACTED_NOTE}")
                } else {
                    SENSITIVE_NOTE.to_owned()
                },
                cx,
            ))
            .into_any_element()
    }

    fn editor(
        &mut self,
        instance: &InstanceRow,
        editable: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let wide = self.wide;
        let display = div().w(px(224.)).child(sm_field(
            &self.display_name,
            false,
            false,
            !editable,
            window,
            cx,
        ));
        let identity = group(
            vec![
                row(
                    "Status",
                    Self::status_line(instance),
                    None,
                    div().into_any_element(),
                    wide,
                    cx,
                )
                .into_any_element(),
                row(
                    "Display name",
                    "",
                    None,
                    display.into_any_element(),
                    wide,
                    cx,
                )
                .into_any_element(),
            ],
            cx,
        );
        let environment = group(vec![self.variables(editable, window, cx)], cx);
        v_flex()
            .gap_8()
            .child(section(instance.mark.display_name.clone(), identity))
            .child(section("Environment", environment))
            .into_any_element()
    }
}

impl Render for ProvidersPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ctx = self.ctx(cx);
        let note_line = |text: &'static str| div().px_4().child(note(text, cx));
        if !ctx.connected {
            return note_line("Connect an environment to manage its providers.").into_any_element();
        }
        if self.instances.is_empty() {
            return note_line("No providers configured.").into_any_element();
        }
        self.ensure_rows(window, cx);
        let editable = ctx.editable && !ctx.in_project;
        let theme = Tokens::of(cx);
        let selected = self.selected_instance().cloned();
        let selected_id = selected.as_ref().map(|instance| instance.id.clone());
        let instances = self.instances.clone();
        let list = v_flex().children(instances.iter().enumerate().map(|(index, instance)| {
            let chosen = selected_id.as_deref() == Some(instance.id.as_str());
            v_flex()
                .when(index > 0, |item| {
                    item.border_t_1().border_color(theme.border.opacity(0.5))
                })
                .child(self.list_row(instance, chosen, editable, window, cx))
        }));
        let editor = selected.map(|instance| self.editor(&instance, editable, window, cx));
        let notice = ctx.in_project.then(|| {
            div().px_4().child(note(
                "Environment-wide setting. Select an environment to change it.",
                cx,
            ))
        });
        let wide = self.wide;
        v_flex()
            .gap_4()
            .children(notice)
            .child(
                div()
                    .flex()
                    .when(!wide, |layout| layout.flex_col())
                    .gap_6()
                    .child(
                        div()
                            .flex_shrink_0()
                            .when(wide, |pane| pane.w(px(LIST_WIDTH)))
                            .rounded(px(12.))
                            .border_1()
                            .border_color(theme.border.opacity(0.6))
                            .bg(theme.card.opacity(0.4))
                            .overflow_hidden()
                            .self_start()
                            .child(list),
                    )
                    .child(div().min_w_0().flex_1().children(editor)),
            )
            .into_any_element()
    }
}
