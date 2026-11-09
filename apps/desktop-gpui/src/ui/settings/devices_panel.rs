//! The Devices tab: `DeviceIntegrationSettings` (IntegrationsSettings.tsx:804-1030) and
//! `DeviceSourcesSettings` (DeviceSourcesSettings.tsx) with their host editors. The panel owns
//! its popovers, editors and requests, so the other tabs stay untouched. Server-settings writes
//! follow the Labs tab: optimistic, put back when the call fails.

mod editors;
mod widgets;

use super::components::{Toggle, group, reset_button, section, switch};
use super::scope_inheritance::{Marker, PopTrigger, marker_row, popup_shell};
use super::scope_pickers::ScopeBar;
use super::tokens::Tokens;
use super::*;
use crate::connection::{Action, ActionSender, Call};
use crate::settings_devices::{
    self as devices, AddKind, DeviceConfig, DeviceState, Key, Row, Source, StatusAction, Tone, Tool,
};
use crate::settings_labs::{Scope, resolve};
use crate::status::CallEvent;
use editors::Editor;
use gpui::{ElementId, Subscription};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::rc::Rc;
use widgets::{Variant, button, icon_button, menu_item, spinner, warning};

/// A switch write in flight.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Busy {
    Key(Key),
}

/// A version action in flight (`pendingAction`).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Versions {
    Update(Tool),
    Inspect,
}

/// What a request in flight is for.
enum Pending {
    Configure {
        previous: DeviceConfig,
    },
    Override {
        previous: DeviceConfig,
    },
    List {
        versions: Option<Versions>,
    },
    Hosts {
        previous: DeviceConfig,
        title: &'static str,
    },
    Test {
        seq: u64,
    },
    RowTest {
        label: String,
    },
    Account {
        seq: u64,
    },
}

/// A failure or a test result, shown under the sources card in place of the web's toast.
struct Notice {
    title: String,
    description: String,
    error: bool,
}

pub struct DevicesPanel {
    actions: ActionSender,
    scope: Entity<ScopeBar>,
    wide: bool,
    config: DeviceConfig,
    projects: devices::ProjectIds,
    state: Option<DeviceState>,
    busy: Option<Busy>,
    versions: Option<Versions>,
    update_error: Option<(Tool, String)>,
    pending: HashMap<u64, Pending>,
    next_token: u64,
    notice: Option<Notice>,
    add_menu: bool,
    row_menu: Option<String>,
    editor: Option<Editor>,
    _subscriptions: Vec<Subscription>,
}

/// What the scope bar says about the one attached environment.
struct Ctx {
    in_project: bool,
    members: Vec<String>,
    label: String,
    machine: crate::projects::MachineKind,
    connected: bool,
    hub: Result<(), String>,
    agent: Result<(), String>,
}

fn is_state(value: &Value) -> bool {
    value.get("hosts").is_some()
}

impl DevicesPanel {
    pub fn new(actions: ActionSender, scope: Entity<ScopeBar>, cx: &mut Context<Self>) -> Self {
        let observed = cx.observe(&scope, |_, _, cx| cx.notify());
        Self {
            actions,
            scope,
            wide: true,
            config: DeviceConfig::default(),
            projects: devices::ProjectIds::new(),
            state: None,
            busy: None,
            versions: None,
            update_error: None,
            pending: HashMap::new(),
            next_token: devices::TOKEN_BASE,
            notice: None,
            add_menu: false,
            row_menu: None,
            editor: None,
            _subscriptions: vec![observed],
        }
    }

    pub fn set_wide(&mut self, wide: bool, cx: &mut Context<Self>) {
        if self.wide != wide {
            self.wide = wide;
            cx.notify();
        }
    }

    /// The attached environment's device settings and the projects that hold the overrides.
    pub fn set_data(
        &mut self,
        config: DeviceConfig,
        projects: devices::ProjectIds,
        cx: &mut Context<Self>,
    ) {
        let mut changed = self.projects != projects;
        self.projects = projects;
        if self.pending.is_empty() && self.config != config {
            self.config = config;
            changed = true;
        }
        if changed {
            cx.notify();
        }
    }

    /// Closes menus and the dialog as the settings page opens, and asks for the device state.
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        self.add_menu = false;
        self.row_menu = None;
        self.editor = None;
        self.notice = None;
        self.update_error = None;
        self.list(json!({}), None, cx);
        cx.notify();
    }

    /// Closes the innermost open menu or the dialog; returns whether there was one.
    pub fn close_menu(&mut self, _: &mut Window, cx: &mut Context<Self>) -> bool {
        let closed = if self.row_menu.take().is_some() || std::mem::take(&mut self.add_menu) {
            true
        } else if let Some(editor) = &mut self.editor {
            if std::mem::take(&mut editor.app_menu) {
                true
            } else if editor.saving {
                false
            } else {
                self.editor = None;
                true
            }
        } else {
            false
        };
        if closed {
            cx.notify();
        }
        closed
    }

    fn send(&mut self, call: Call, payload: Value, pending: Pending) -> bool {
        let token = self.next_token;
        self.next_token += 1;
        let action = Action::Call {
            token,
            call,
            payload,
        };
        if self.actions.try_send(action).is_err() {
            return false;
        }
        self.pending.insert(token, pending);
        true
    }

    fn list(&mut self, payload: Value, versions: Option<Versions>, cx: &mut Context<Self>) {
        if self.send(Call::DeviceList, payload, Pending::List { versions }) {
            self.versions = versions;
        }
        cx.notify();
    }

    fn ctx(&self, cx: &App) -> Ctx {
        let scope = self.scope.read(cx);
        let (search, data) = (scope.search(), scope.data());
        let environment = data
            .environments
            .iter()
            .find(|e| e.connected && e.writable)
            .or(data.environments.first());
        Ctx {
            in_project: matches!(resolve(search, data), Scope::Project(_)),
            members: search
                .project
                .as_ref()
                .and_then(|key| self.projects.get(key))
                .cloned()
                .unwrap_or_default(),
            label: environment.map_or_else(|| "Environment".to_owned(), |e| e.label.clone()),
            machine: environment.map_or(crate::projects::MachineKind::Desktop, |e| e.machine),
            connected: environment.is_some_and(|e| e.connected && e.writable),
            hub: devices::reach(Key::Hub, search, data),
            agent: devices::reach(Key::Agent, search, data),
        }
    }

    fn project_override(&self, key: Key, ctx: &Ctx) -> Option<bool> {
        ctx.members
            .iter()
            .find_map(|id| self.config.project_override(key, id))
    }

    fn value(&self, key: Key, ctx: &Ctx) -> bool {
        if ctx.in_project && key.project_scoped() {
            self.project_override(key, ctx)
                .unwrap_or_else(|| self.config.environment_value(key))
        } else {
            self.config.environment_value(key)
        }
    }

    fn loaded(&self) -> bool {
        self.state.is_some()
    }

    fn hub_busy(&self) -> bool {
        self.busy.is_some() || self.state.as_ref().is_some_and(DeviceState::busy)
    }

    fn hosts_busy(&self) -> bool {
        self.pending
            .values()
            .any(|p| matches!(p, Pending::Hosts { .. }))
    }

    /// Whether the switch ignores clicks (IntegrationsSettings.tsx:950-1017).
    fn gated(&self, key: Key, ctx: &Ctx) -> bool {
        match key {
            Key::Hub => ctx.hub.is_err() || !self.loaded() || !ctx.connected || self.hub_busy(),
            Key::Agent => {
                ctx.agent.is_err()
                    || !ctx.connected
                    || (!ctx.in_project
                        && (!self.loaded()
                            || !self.config.enable_device_support
                            || self.hub_busy()))
            }
        }
    }

    fn toggle(&mut self, key: Key, cx: &mut Context<Self>) {
        let ctx = self.ctx(cx);
        if self.gated(key, &ctx) {
            return;
        }
        let next = !self.value(key, &ctx);
        self.write(key, Some(next), &ctx, cx);
    }

    fn reset(&mut self, key: Key, cx: &mut Context<Self>) {
        let ctx = self.ctx(cx);
        if self.gated(key, &ctx) {
            return;
        }
        let value = if ctx.in_project && key.project_scoped() {
            None
        } else {
            Some(false)
        };
        self.write(key, value, &ctx, cx);
    }

    /// A project scope writes the override of every project of the group (`None` removes it);
    /// anywhere else the environment's value is written through `device.configure`.
    fn write(&mut self, key: Key, value: Option<bool>, ctx: &Ctx, cx: &mut Context<Self>) {
        let previous = self.config.clone();
        if ctx.in_project && key.project_scoped() {
            let patch = devices::merge_override_patches(
                ctx.members
                    .iter()
                    .map(|id| devices::override_patch(&self.config, id, key, value))
                    .collect(),
            );
            for id in &ctx.members {
                let entry = self
                    .config
                    .project_settings_overrides
                    .entry(id.clone())
                    .or_default();
                match value {
                    Some(value) => {
                        entry.insert(key.wire().to_owned(), json!(value));
                    }
                    None => {
                        entry.remove(key.wire());
                    }
                }
            }
            if !self.send(
                Call::UpdateSettings,
                patch,
                Pending::Override {
                    previous: previous.clone(),
                },
            ) {
                self.config = previous;
            }
        } else {
            let value = value.unwrap_or(false);
            match key {
                Key::Hub => {
                    self.config.enable_device_support = value;
                    if !value {
                        self.config.enable_agent_device_access = false;
                    }
                }
                Key::Agent => self.config.enable_agent_device_access = value,
            }
            if self.send(
                Call::DeviceConfigure,
                devices::configure_payload(key, value),
                Pending::Configure {
                    previous: previous.clone(),
                },
            ) {
                self.busy = Some(Busy::Key(key));
            } else {
                self.config = previous;
            }
        }
        cx.notify();
    }

    fn update_tool(&mut self, tool: Tool, cx: &mut Context<Self>) {
        self.update_error = None;
        self.list(
            json!({"updateTool": tool.wire()}),
            Some(Versions::Update(tool)),
            cx,
        );
    }

    fn check_versions(&mut self, cx: &mut Context<Self>) {
        self.update_error = None;
        self.list(json!({"inspectOnly": true}), Some(Versions::Inspect), cx);
    }

    fn notify_failure(&mut self, title: impl Into<String>, description: impl Into<String>) {
        self.notice = Some(Notice {
            title: title.into(),
            description: description.into(),
            error: true,
        });
    }

    /// The outcome of a request.
    pub fn on_call(&mut self, token: u64, event: CallEvent, cx: &mut Context<Self>) {
        if matches!(event, CallEvent::Progress(_)) {
            return;
        }
        let Some(pending) = self.pending.remove(&token) else {
            return;
        };
        let label = self.ctx(cx).label;
        match (pending, event) {
            (Pending::Configure { previous }, event) => {
                self.busy = None;
                match event {
                    CallEvent::Done(value) if is_state(&value) => {
                        self.state = Some(DeviceState::parse(&value));
                    }
                    CallEvent::Failed(_) => {
                        self.config = previous;
                        self.notify_failure(
                            "Device settings not saved on all environments",
                            format!("Could not update {label}."),
                        );
                    }
                    _ => {}
                }
            }
            (Pending::Override { previous }, CallEvent::Failed(_)) => {
                self.config = previous;
                self.notify_failure(
                    "Device settings not saved on all environments",
                    format!("Could not update {label}."),
                );
            }
            (Pending::List { versions }, event) => {
                if self.versions == versions {
                    self.versions = None;
                }
                match event {
                    CallEvent::Done(value) => {
                        self.state = Some(DeviceState::parse(&value));
                        self.update_error = None;
                    }
                    CallEvent::Failed(message) => match versions {
                        Some(Versions::Update(tool)) => self.update_error = Some((tool, message)),
                        _ => self.notify_failure("Could not load devices", message),
                    },
                    CallEvent::Progress(_) => {}
                }
            }
            (Pending::Hosts { previous, title }, event) => match event {
                CallEvent::Done(_) => {
                    self.editor = None;
                    self.list(json!({}), None, cx);
                }
                CallEvent::Failed(message) => {
                    self.config = previous;
                    if let Some(editor) = &mut self.editor {
                        editor.saving = false;
                        editor.error = Some(message.clone());
                    }
                    self.notify_failure(title, format!("{label}: {message}"));
                }
                CallEvent::Progress(_) => {}
            },
            (Pending::Test { seq }, event) => {
                if let Some(editor) = self.editor.as_mut().filter(|e| e.seq == seq) {
                    editor.finish_test(event);
                }
            }
            (Pending::RowTest { label: host }, event) => {
                self.notice = Some(match event {
                    CallEvent::Failed(message) => Notice {
                        title: format!("{host}: 1 of 1 environments failed"),
                        description: format!("{label}: {message}"),
                        error: true,
                    },
                    _ => Notice {
                        title: format!("{host}: connection checks passed"),
                        description:
                            "Connected or already available locally on each selected environment."
                                .to_owned(),
                        error: false,
                    },
                });
            }
            (Pending::Account { seq }, event) => {
                if let Some(editor) = self.editor.as_mut().filter(|e| e.seq == seq) {
                    editor.finish_account(event);
                }
            }
            _ => {}
        }
        cx.notify();
    }

    fn open_editor(
        &mut self,
        kind: AddKind,
        existing: Option<&Source>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.add_menu = false;
        self.row_menu = None;
        let editor = Editor::open(kind, existing, &self.config, window, cx);
        let account = editor.wants_account();
        self.editor = Some(editor);
        if account {
            self.check_account(cx);
        }
        cx.notify();
    }

    fn save_editor(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = &mut self.editor else {
            return;
        };
        let Some(save) = editor.save(&self.config, cx) else {
            return;
        };
        match save {
            Err(message) => editor.error = Some(message),
            Ok(save) => {
                editor.saving = true;
                editor.error = None;
                let previous = self.config.clone();
                save.apply(&mut self.config);
                let payload = save.patch();
                if !self.send(
                    Call::UpdateSettings,
                    payload,
                    Pending::Hosts {
                        previous: previous.clone(),
                        title: save.title,
                    },
                ) {
                    self.config = previous;
                    if let Some(editor) = &mut self.editor {
                        editor.saving = false;
                    }
                }
            }
        }
        cx.notify();
    }

    fn remove(&mut self, row: &Row, cx: &mut Context<Self>) {
        self.row_menu = None;
        let Some(save) = editors::removal(&row.source, &self.config) else {
            return;
        };
        let previous = self.config.clone();
        save.apply(&mut self.config);
        let patch = save.patch();
        if !self.send(
            Call::UpdateSettings,
            patch,
            Pending::Hosts {
                previous: previous.clone(),
                title: save.title,
            },
        ) {
            self.config = previous;
        }
        cx.notify();
    }

    fn test_row(&mut self, row: &Row, cx: &mut Context<Self>) {
        self.row_menu = None;
        if let Source::Ssh(host) = &row.source {
            self.send(
                Call::DeviceTestHost,
                devices::test_payload(host),
                Pending::RowTest {
                    label: host.label.clone(),
                },
            );
        }
        cx.notify();
    }

    pub(super) fn test_connection(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = &mut self.editor else {
            return;
        };
        let Some(payload) = editor.start_test(cx) else {
            return;
        };
        let seq = editor.seq;
        self.send(Call::DeviceTestHost, payload, Pending::Test { seq });
        cx.notify();
    }

    pub(super) fn check_account(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = &mut self.editor else {
            return;
        };
        let payload = editor.start_account(cx);
        let seq = editor.seq;
        self.send(Call::DeviceEasAccount, payload, Pending::Account { seq });
        cx.notify();
    }

    fn dismiss_notice(&mut self, cx: &mut Context<Self>) {
        self.notice = None;
        cx.notify();
    }

    /// The switch, inert under a tooltip when the scope or a precondition forbids a write.
    fn switch_control(
        &self,
        id: &'static str,
        key: Key,
        ctx: &Ctx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let checked = self.value(key, ctx);
        let why = match key {
            Key::Hub => ctx.hub.clone().err(),
            Key::Agent => ctx.agent.clone().err(),
        };
        let toggle: Toggle = if self.gated(key, ctx) {
            Rc::new(|_, _| {})
        } else {
            Rc::new(move |_, cx| view.update(cx, |this, cx| this.toggle(key, cx)))
        };
        let switch = switch(id, checked, toggle, window, cx);
        if !self.gated(key, ctx) {
            return switch;
        }
        div()
            .id((id, 1usize))
            .opacity(0.5)
            .when_some(why, |inert, why| inert.tooltip(tooltip(why)))
            .child(switch)
            .into_any_element()
    }

    /// The spinner and label beside a switch whose write is in flight.
    fn pending_label(&self, key: Key, cx: &App) -> Option<AnyElement> {
        (self.busy == Some(Busy::Key(key))).then(|| {
            let muted = Tokens::of(cx).muted_foreground;
            h_flex()
                .items_center()
                .gap_1()
                .text_xs()
                .text_color(muted)
                .child(spinner(12., muted))
                .child(
                    self.state
                        .as_ref()
                        .map_or("Updating\u{2026}", DeviceState::setup_label),
                )
                .into_any_element()
        })
    }

    fn reset_control(
        &self,
        id: &'static str,
        key: Key,
        ctx: &Ctx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let dirty = if ctx.in_project && key.project_scoped() {
            self.project_override(key, ctx).is_some()
        } else if ctx.in_project {
            false
        } else {
            self.config.environment_value(key)
        };
        if !dirty || self.gated(key, ctx) {
            return None;
        }
        let view = cx.entity();
        Some(reset_button(
            id,
            move |_, _, cx| view.update(cx, |this, cx| this.reset(key, cx)),
            window,
            cx,
        ))
    }

    fn marker(&self, id: &'static str, key: Key, ctx: &Ctx) -> Marker {
        let project = self.project_override(key, ctx);
        let environment = self.config.environment_value(key);
        let (state, summary) =
            devices::inheritance(key, ctx.in_project, project, environment, &ctx.label);
        Marker {
            id,
            state,
            summary,
            layers: devices::layers(key, &ctx.label, ctx.in_project, project, environment),
            machine: ctx.machine,
            environment: ctx.label.clone(),
        }
    }

    /// `DeviceToolVersions`: the status link and its popover (DeviceToolVersions.tsx).
    fn version_link(
        &self,
        id: &'static str,
        tool: Tool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Tokens::of(cx);
        let (muted, foreground) = (theme.muted_foreground, theme.foreground);
        let key = ElementId::from(id);
        let open = window.use_keyed_state(ElementId::from((key.clone(), "open")), cx, |_, _| false);
        let opened = *open.read(cx);
        let toggled = open.clone();
        let label = self
            .state
            .as_ref()
            .map_or_else(|| "Version unknown".to_owned(), |s| s.tool_label(tool));
        let trigger = div()
            .id(key)
            .font_weight(FontWeight::MEDIUM)
            .text_color(muted)
            .cursor_pointer()
            .hover(move |style| style.text_color(foreground))
            .child(label);
        let snapshot = VersionsView {
            tool,
            version: self.state.as_ref().and_then(|s| s.tool(tool).cloned()),
            known: self
                .state
                .as_ref()
                .is_some_and(|s| s.local().is_some_and(|h| h.tools.is_some())),
            can_update: self.state.as_ref().is_some_and(|s| s.supports_tool_update),
            can_inspect: self
                .state
                .as_ref()
                .is_some_and(|s| s.supports_tool_inspection),
            pending: self.versions,
            error: self
                .update_error
                .as_ref()
                .filter(|(failed, _)| *failed == tool)
                .map(|(_, message)| message.clone()),
            enabled: self.ctx(cx).connected && self.versions.is_none() && !self.hub_busy(),
        };
        let view = cx.entity();
        Popover::new(ElementId::from((id, 2usize)))
            .anchor(Anchor::TopRight)
            .offset(px(4.))
            .open(opened)
            .p_0()
            .appearance(false)
            .on_open_change(move |next, _, cx| toggled.update(cx, |open, _| *open = *next))
            .trigger(PopTrigger::new(trigger))
            .content(move |_, _, cx| popup_shell(cx).child(versions_body(&snapshot, &view, cx)))
            .into_any_element()
    }

    fn devices_card(&self, ctx: &Ctx, window: &mut Window, cx: &mut Context<Self>) -> gpui::Div {
        let wide = self.wide;
        let hub_status = self.version_link("settings-devices-hub-version", Tool::Hub, window, cx);
        let agent_status =
            self.version_link("settings-devices-agent-version", Tool::Agent, window, cx);
        let hub_switch = h_flex()
            .items_center()
            .gap_2()
            .children(self.pending_label(Key::Hub, cx))
            .child(self.switch_control("settings-devices-hub", Key::Hub, ctx, window, cx))
            .into_any_element();
        let agent_switch = h_flex()
            .items_center()
            .gap_2()
            .children(self.pending_label(Key::Agent, cx))
            .child(self.switch_control("settings-devices-agent", Key::Agent, ctx, window, cx))
            .into_any_element();
        let hub = marker_row(
            "settings-devices-hub-row",
            "Use devices",
            h_flex()
                .flex_wrap()
                .items_center()
                .gap(px(4.))
                .child("Open simulators and emulators in the Device panel. Hub:")
                .child(hub_status),
            Some(self.marker("settings-devices-hub-marker", Key::Hub, ctx)),
            self.reset_control("settings-devices-hub-reset", Key::Hub, ctx, window, cx),
            hub_switch,
            wide,
            window,
            cx,
        );
        let agent = marker_row(
            "settings-devices-agent-row",
            "Agents can use devices",
            h_flex()
                .flex_wrap()
                .items_center()
                .gap(px(4.))
                .child("New agent sessions can open and drive these devices. agent-device:")
                .child(agent_status),
            Some(self.marker("settings-devices-agent-marker", Key::Agent, ctx)),
            self.reset_control("settings-devices-agent-reset", Key::Agent, ctx, window, cx),
            agent_switch,
            wide,
            window,
            cx,
        );
        group(vec![hub.into_any_element(), agent.into_any_element()], cx)
    }

    /// The Add source button and its menu (DeviceSourcesSettings.tsx:521-557).
    fn add_source(&self, disabled: bool, cx: &mut Context<Self>) -> AnyElement {
        let enabled = !disabled && self.editor.is_none();
        let theme = Tokens::of(cx);
        let foreground = theme.foreground;
        let trigger = button("settings-device-add", Variant::Outline, enabled, cx)
            .child(Icon::new(Icons::Plus).size(px(14.)))
            .child("Add source")
            .child(
                div()
                    .opacity(0.7)
                    .text_color(foreground)
                    .child(Icon::new(Icons::ChevronDown).size(px(14.))),
            );
        if !enabled {
            return trigger.into_any_element();
        }
        let items = devices::add_menu(
            self.state.as_ref(),
            self.config.simlock_device_hosts_enabled,
        );
        let view = cx.entity();
        let opened = view.clone();
        Popover::new("settings-device-add-menu")
            .anchor(Anchor::TopRight)
            .offset(px(4.))
            .open(self.add_menu)
            .p_0()
            .appearance(false)
            .on_open_change(move |next, _, cx| {
                opened.update(cx, |this, cx| {
                    this.add_menu = *next;
                    cx.notify();
                })
            })
            .trigger(PopTrigger::new(trigger))
            .content(move |_, _, cx| {
                popup_shell(cx)
                    .min_w(px(192.))
                    .p_1()
                    .children(items.iter().enumerate().map(|(index, (kind, label))| {
                        let (view, kind) = (view.clone(), *kind);
                        menu_item(("settings-device-add-item", index), *label, false, cx).on_click(
                            move |_, window, cx| {
                                view.update(cx, |this, cx| this.open_editor(kind, None, window, cx))
                            },
                        )
                    }))
            })
            .into_any_element()
    }

    fn platform_icon(
        &self,
        index: usize,
        row_index: usize,
        availability: &devices::Availability,
        cx: &App,
    ) -> AnyElement {
        let theme = Tokens::of(cx);
        let (name, icon) = match availability.platform {
            devices::Platform::Ios => ("iOS", Icons::Apple),
            devices::Platform::Android => ("Android", Icons::Bot),
        };
        let text = if availability.available {
            format!("{name} available")
        } else {
            format!(
                "{name} not available. {}",
                availability
                    .reason
                    .as_deref()
                    .unwrap_or("Not set up on this machine.")
            )
        };
        div()
            .id(("settings-device-platform", row_index * 2 + index))
            .size(px(24.))
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .rounded(px(6.))
            .text_color(if availability.available {
                theme.foreground
            } else {
                theme.muted_foreground.opacity(0.45)
            })
            .tooltip(tooltip(text))
            .child(Icon::new(icon).size(px(16.)))
            .into_any_element()
    }

    fn row_actions(
        &self,
        index: usize,
        row: &Row,
        locked: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let view = cx.entity();
        let mut actions = Vec::new();
        let status_action = row.status.as_ref().and_then(|s| s.action.clone());
        if let Some(action) = status_action {
            let (label, payload) = match action {
                StatusAction::Refresh => ("Refresh", json!({})),
                StatusAction::Retry(id) => ("Retry", json!({"retryHostId": id})),
            };
            let enabled = self.versions.is_none();
            let view = view.clone();
            actions.push(
                button(
                    ("settings-device-status-action", index),
                    Variant::Outline,
                    enabled,
                    cx,
                )
                .child(label)
                .when(enabled, |b| {
                    b.on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| this.list(payload.clone(), None, cx))
                    })
                })
                .into_any_element(),
            );
        }
        if matches!(row.source, Source::Local) {
            return actions;
        }
        let enabled = !locked;
        let existing = row.source.clone();
        let edit = {
            let view = view.clone();
            button(
                ("settings-device-edit", index),
                Variant::Outline,
                enabled,
                cx,
            )
            .child("Edit")
            .when(enabled, |b| {
                b.on_click(move |_, window, cx| {
                    let kind = match &existing {
                        Source::Eas(_) => AddKind::Eas,
                        Source::Simlock(_) => AddKind::Simlock,
                        _ => AddKind::Ssh,
                    };
                    let existing = existing.clone();
                    view.update(cx, |this, cx| {
                        this.open_editor(kind, Some(&existing), window, cx)
                    })
                })
            })
            .into_any_element()
        };
        actions.push(edit);
        let more = icon_button(
            ("settings-device-more", index),
            Variant::GhostMuted,
            enabled,
            cx,
        )
        .child(
            Icon::new(Icons::Ellipsis)
                .size(px(16.))
                .rotate(gpui::radians(std::f32::consts::FRAC_PI_2)),
        );
        if !enabled {
            actions.push(more.into_any_element());
            return actions;
        }
        let key = row.key.clone();
        let opened = view.clone();
        let row = row.clone();
        let content_view = view;
        actions.push(
            Popover::new(("settings-device-more-menu", index))
                .anchor(Anchor::TopRight)
                .offset(px(4.))
                .open(self.row_menu.as_deref() == Some(key.as_str()))
                .p_0()
                .appearance(false)
                .on_open_change(move |next, _, cx| {
                    let key = key.clone();
                    opened.update(cx, |this, cx| {
                        this.row_menu = next.then_some(key);
                        cx.notify();
                    })
                })
                .trigger(PopTrigger::new(more))
                .content(move |_, _, cx| {
                    let mut menu = popup_shell(cx).min_w(px(160.)).p_1();
                    if matches!(row.source, Source::Ssh(_)) {
                        let (view, row) = (content_view.clone(), row.clone());
                        menu = menu.child(
                            menu_item(
                                ("settings-device-test", index),
                                "Test connection",
                                false,
                                cx,
                            )
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| this.test_row(&row, cx))
                            }),
                        );
                    }
                    let (view, row) = (content_view.clone(), row.clone());
                    menu.child(
                        menu_item(("settings-device-remove", index), "Remove", true, cx).on_click(
                            move |_, _, cx| view.update(cx, |this, cx| this.remove(&row, cx)),
                        ),
                    )
                })
                .into_any_element(),
        );
        actions
    }

    fn source_row(
        &self,
        index: usize,
        row: &Row,
        locked: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Tokens::of(cx);
        let warn = warning(&theme);
        let (foreground, muted, destructive, border) = (
            theme.foreground,
            theme.muted_foreground,
            theme.destructive,
            theme.border,
        );
        let _ = foreground;
        let badge = row.kind.map(|kind| {
            div()
                .flex_shrink_0()
                .rounded(px(4.))
                .border_1()
                .border_color(border)
                .px(px(4.))
                .text_size(px(10.))
                .line_height(px(14.))
                .text_color(muted)
                .child(kind.to_uppercase())
        });
        let status = row.status.as_ref().map(|status| {
            let color = match status.tone {
                Tone::Pending => muted,
                Tone::Warning => warn,
                Tone::Error => destructive,
            };
            let text = match &status.detail {
                Some(detail) => format!("{}: {detail}", status.label),
                None => status.label.clone(),
            };
            h_flex()
                .items_center()
                .gap_1()
                .text_xs()
                .text_color(color)
                .when(status.tone == Tone::Pending, |line| {
                    line.child(spinner(12., color))
                })
                .child(div().min_w_0().truncate().child(text))
        });
        let icons: Vec<AnyElement> = row
            .platforms
            .iter()
            .enumerate()
            .map(|(i, availability)| self.platform_icon(i, index, availability, cx))
            .collect();
        h_flex()
            .items_center()
            .gap_3()
            .px_4()
            .py_3()
            .child(
                v_flex()
                    .min_w_0()
                    .flex_1()
                    .gap(px(2.))
                    .child(
                        h_flex()
                            .min_w_0()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(row.title.clone()),
                            )
                            .children(badge),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_xs()
                            .text_color(muted)
                            .child(row.description.clone()),
                    )
                    .children(status),
            )
            .child(h_flex().items_center().gap_1().children(icons))
            .child(
                h_flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap_2()
                    .children(self.row_actions(index, row, locked, cx)),
            )
            .into_any_element()
    }

    fn notice_row(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let notice = self.notice.as_ref()?;
        let theme = Tokens::of(cx);
        let view = cx.entity();
        let tone = if notice.error {
            theme.destructive
        } else {
            theme.foreground
        };
        Some(
            h_flex()
                .items_start()
                .gap_3()
                .rounded(px(8.))
                .border_1()
                .border_color(if notice.error {
                    theme.destructive.opacity(0.5)
                } else {
                    theme.border
                })
                .px_3()
                .py_2()
                .child(
                    v_flex()
                        .min_w_0()
                        .flex_1()
                        .gap(px(2.))
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(tone)
                                .child(notice.title.clone()),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(notice.description.clone()),
                        ),
                )
                .child(
                    icon_button(
                        "settings-device-notice-close",
                        Variant::GhostMuted,
                        true,
                        cx,
                    )
                    .on_click(move |_, _, cx| view.update(cx, |this, cx| this.dismiss_notice(cx)))
                    .child(Icon::new(Icons::X).size(px(14.))),
                )
                .into_any_element(),
        )
    }

    fn sources_section(&self, ctx: &Ctx, cx: &mut Context<Self>) -> gpui::Div {
        let active = self.config.enable_device_support;
        let rows = devices::rows(&self.config, self.state.as_ref(), 1, active);
        let locked = devices::sources_disabled(ctx.in_project, self.hosts_busy(), ctx.connected);
        let cards: Vec<AnyElement> = rows
            .iter()
            .enumerate()
            .map(|(index, row)| self.source_row(index, row, locked, cx))
            .collect();
        let header = h_flex()
            .min_h(px(28.))
            .items_center()
            .justify_between()
            .px_4()
            .child(
                div()
                    .text_base()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Device sources"),
            )
            .child(self.add_source(locked, cx));
        v_flex()
            .gap(px(10.))
            .child(header)
            .child(group(cards, cx))
            .children(self.notice_row(cx))
    }

    /// The host editor over a backdrop, built by the settings dialog next to its own overlays.
    pub fn overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let label = self.ctx(cx).label;
        let view = cx.entity();
        let editor = self.editor.as_ref()?;
        Some(editor.view(&label, &view, window, cx))
    }
}

/// What the version popover shows, taken when the panel renders.
struct VersionsView {
    tool: Tool,
    version: Option<devices::ToolVersion>,
    known: bool,
    can_update: bool,
    can_inspect: bool,
    pending: Option<Versions>,
    error: Option<String>,
    enabled: bool,
}

fn versions_body(snapshot: &VersionsView, view: &Entity<DevicesPanel>, cx: &App) -> gpui::Div {
    let theme = Tokens::of(cx);
    let mono = cx.theme().mono_font_family.clone();
    let tool = snapshot.tool;
    let mut body = v_flex().w(px(320.)).max_w_full().p_4().child(
        div()
            .text_sm()
            .line_height(px(14.))
            .font_weight(FontWeight::SEMIBOLD)
            .child(tool.name()),
    );
    match (&snapshot.version, snapshot.known) {
        (Some(version), true) => {
            let value = |text: String| {
                div()
                    .text_xs()
                    .font_family(mono.clone())
                    .text_right()
                    .child(text)
            };
            let label = |text: &'static str| {
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(text)
            };
            let installed = if version.installed.is_empty() {
                "None".to_owned()
            } else {
                version
                    .installed
                    .iter()
                    .map(|v| format!("v{v}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let running = version
                .running
                .as_ref()
                .map_or_else(|| "Not running".to_owned(), |v| format!("v{v}"));
            body = body.child(
                div()
                    .mt_4()
                    .grid()
                    .grid_cols(2)
                    .gap_x_6()
                    .gap_y_1()
                    .child(label("Running"))
                    .child(value(running))
                    .child(label("Required"))
                    .child(value(format!("v{}", version.required)))
                    .child(label("Installed"))
                    .child(value(installed)),
            );
        }
        _ => {
            body = body.child(
                div()
                    .mt_3()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("Versions have not been checked."),
            );
        }
    }
    body = body.child(
        div()
            .mt_4()
            .pt_3()
            .border_t_1()
            .border_color(theme.border.opacity(0.5))
            .text_xs()
            .text_color(theme.muted_foreground)
            .child("Tools update automatically on this host when needed."),
    );
    if let Some(error) = &snapshot.error {
        body = body.child(
            div()
                .mt_2()
                .text_xs()
                .text_color(theme.destructive)
                .child(error.clone()),
        );
    }
    let update = snapshot.can_update
        && snapshot
            .version
            .as_ref()
            .is_some_and(devices::ToolVersion::needs_update);
    if update || snapshot.can_inspect {
        let mut actions = h_flex().mt_3().flex_wrap().gap_2();
        if update && let Some(version) = &snapshot.version {
            let updating = snapshot.pending == Some(Versions::Update(tool));
            let label = if updating {
                "Updating\u{2026}".to_owned()
            } else {
                format!("Update to v{}", version.required)
            };
            let view = view.clone();
            actions = actions.child(
                button(
                    ("settings-device-update", tool as usize),
                    Variant::Primary,
                    snapshot.enabled,
                    cx,
                )
                .child(label)
                .when(snapshot.enabled, |b| {
                    b.on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| this.update_tool(tool, cx))
                    })
                }),
            );
        }
        if snapshot.can_inspect {
            let checking = snapshot.pending == Some(Versions::Inspect);
            let view = view.clone();
            actions = actions.child(
                button(
                    ("settings-device-check", tool as usize),
                    Variant::Outline,
                    snapshot.enabled,
                    cx,
                )
                .child(if checking {
                    "Checking\u{2026}"
                } else {
                    "Check versions"
                })
                .when(snapshot.enabled, |b| {
                    b.on_click(move |_, _, cx| view.update(cx, |this, cx| this.check_versions(cx)))
                }),
            );
        }
        body = body.child(actions);
    }
    body
}

impl Render for DevicesPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ctx = self.ctx(cx);
        let card = self.devices_card(&ctx, window, cx);
        v_flex()
            .w_full()
            .gap_8()
            .child(section("Devices", card))
            .child(self.sources_section(&ctx, cx))
    }
}
