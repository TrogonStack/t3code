//! The Threads and Worktrees tabs. Both edit server settings that `settings_source_control::Field`
//! resolves per scope (a project's override over the environment's value), so one panel type
//! owns the scope, the optimistic writes and the controls, and each tab draws its own rows.
//! Server-settings writes are optimistic and put back when the call fails.

use super::components::{
    HoverHandler, KeyHandler, Menu, PickHandler, SelectTrigger, Toggle, group,
    reset_button_labeled, section, select_menu, switch,
};
use super::scope_inheritance::{Marker, marker_row_with};
use super::scope_pickers::ScopeBar;
use super::secret_field::sm_field;
use super::tokens::Tokens;
use super::*;
use crate::connection::{Action, ActionSender, Call};
use crate::settings_labs::{Scope, resolve};
use crate::settings_source_control::{self as sc, Field, SourceControlConfig};
use crate::status::CallEvent;
use gpui::{FocusHandle, KeyDownEvent, Subscription};
use gpui_component::input::{InputEvent, InputState, Textarea, TextareaState};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::rc::Rc;

mod threads;
mod worktrees;

/// `max-w-2xl`.
const TEXT_AREA_MAX: f32 = 672.;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Threads,
    Worktrees,
}

/// The select of a row, with its menu.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pick {
    Permissions,
    Workspace,
    Streaming,
    Submodules,
    BranchMode,
    KeepWhen,
    CleanupMode,
}

impl Pick {
    const ALL: [Self; 7] = [
        Self::Permissions,
        Self::Workspace,
        Self::Streaming,
        Self::Submodules,
        Self::BranchMode,
        Self::KeepWhen,
        Self::CleanupMode,
    ];

    /// The `selected` of a select whose setting is unset: no menu row, and the trigger says why.
    const UNSET: usize = usize::MAX;

    fn index(self) -> usize {
        Self::ALL.iter().position(|pick| *pick == self).unwrap_or(0)
    }

    /// The trigger's id and the popover's id.
    fn ids(self) -> (&'static str, &'static str) {
        match self {
            Self::Permissions => (
                "settings-default-permissions",
                "settings-default-permissions-select",
            ),
            Self::Workspace => (
                "settings-default-workspace",
                "settings-default-workspace-select",
            ),
            Self::Streaming => (
                "settings-response-streaming",
                "settings-response-streaming-select",
            ),
            Self::Submodules => (
                "settings-worktree-submodules",
                "settings-worktree-submodules-select",
            ),
            Self::BranchMode => (
                "settings-worktree-branch-naming",
                "settings-worktree-branch-naming-select",
            ),
            Self::KeepWhen => (
                "settings-worktree-keep-when",
                "settings-worktree-keep-when-select",
            ),
            Self::CleanupMode => (
                "settings-worktree-cleanup-mode",
                "settings-worktree-cleanup-mode-select",
            ),
        }
    }

    fn labels(self) -> Vec<&'static str> {
        match self {
            Self::Permissions => crate::settings_threads::RUNTIME_MODES
                .iter()
                .map(|mode| mode.label)
                .collect(),
            Self::Workspace => crate::settings_threads::WORKSPACES
                .iter()
                .map(|mode| sc::env_mode_label(mode))
                .collect(),
            Self::Streaming => crate::settings_threads::STREAMING_MODES
                .iter()
                .map(|(_, label, _)| *label)
                .collect(),
            Self::Submodules => Self::labels_of(&crate::settings_worktrees::SUBMODULES),
            Self::BranchMode => Self::labels_of(&crate::settings_worktrees::BRANCH_MODES),
            Self::KeepWhen => Self::labels_of(&crate::settings_worktrees::KEEP_WHEN),
            Self::CleanupMode => Self::labels_of(&crate::settings_worktrees::CLEANUP_MODES),
        }
    }

    fn labels_of(options: &[(&'static str, &'static str)]) -> Vec<&'static str> {
        options.iter().map(|(_, label)| *label).collect()
    }

    /// The setting a menu row writes, which also decides whether the control is inert.
    fn field(self, in_project: bool) -> Field {
        match self {
            Self::Permissions => Field::RuntimeMode,
            Self::Workspace => Field::ThreadEnvMode,
            Self::Streaming => Field::Streaming,
            Self::Submodules => Field::Submodules,
            Self::BranchMode => Field::BranchMode,
            Self::KeepWhen if in_project => Field::WorktreeCleanup,
            Self::KeepWhen => Field::StorageCleanup,
            Self::CleanupMode => Field::WorktreeCleanup,
        }
    }

    /// What a menu row writes; `None` removes the setting.
    fn value(self, index: usize) -> Option<Value> {
        Some(match self {
            Self::Permissions => json!(crate::settings_threads::RUNTIME_MODES[index].value),
            Self::Workspace => json!(crate::settings_threads::WORKSPACES[index]),
            Self::Streaming => json!(crate::settings_threads::STREAMING_MODES[index].0),
            Self::Submodules => json!(crate::settings_worktrees::SUBMODULES[index].0),
            Self::BranchMode => json!(crate::settings_worktrees::BRANCH_MODES[index].0),
            Self::KeepWhen => json!(crate::settings_worktrees::KEEP_WHEN[index].0),
            Self::CleanupMode => match crate::settings_worktrees::CLEANUP_MODES[index].0 {
                "off" => json!({"mode": "off"}),
                "custom" => json!({"mode": "custom", "rules": {}}),
                _ => return None,
            },
        })
    }

    fn selected(self, value: &Value) -> usize {
        let text = value.as_str().unwrap_or_default();
        match self {
            Self::Permissions => crate::settings_threads::runtime_mode_index(text),
            Self::Workspace if value.is_null() => Self::UNSET,
            Self::Workspace => crate::settings_threads::workspace_index(text),
            Self::Streaming => crate::settings_threads::streaming_index(text),
            Self::Submodules if value.is_null() => Self::UNSET,
            Self::Submodules => crate::settings_worktrees::submodules_index(value),
            Self::BranchMode => crate::settings_worktrees::branch_mode_index(value),
            Self::KeepWhen => crate::settings_worktrees::keep_when_index(text),
            Self::CleanupMode => crate::settings_worktrees::cleanup_mode_index(value),
        }
    }
}

/// A retention field of the cleanup rules.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Rule {
    AfterDays,
    SettledDays,
}

impl Rule {
    fn wire(self) -> &'static str {
        match self {
            Self::AfterDays => "worktreeAfterDays",
            Self::SettledDays => "worktreeSettledAfterDays",
        }
    }

    fn minimum(self) -> u32 {
        match self {
            Self::AfterDays => 1,
            Self::SettledDays => 0,
        }
    }
}

/// What a single line field holds: a setting, or one of the cleanup rules.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Key {
    Field(Field),
    Rule(Rule),
}

impl From<Field> for Key {
    fn from(field: Field) -> Self {
        Self::Field(field)
    }
}

impl From<Rule> for Key {
    fn from(rule: Rule) -> Self {
        Self::Rule(rule)
    }
}

/// The ids a row needs: the row, its inheritance marker and its control.
#[derive(Clone, Copy)]
struct Ids {
    row: &'static str,
    marker: &'static str,
}

macro_rules! ids {
    ($row:literal) => {
        Ids {
            row: $row,
            marker: concat!("settings-", $row, "-marker"),
        }
    };
}
use ids;

/// What the scope bar says about the environment the tab inspects.
struct Ctx {
    in_project: bool,
    members: Vec<String>,
    label: String,
    machine: crate::projects::MachineKind,
    connected: bool,
}

/// A single line field of the tab and the setting it commits to.
struct Line {
    key: Key,
    input: Entity<InputState>,
}

pub struct ScopedPanel {
    actions: ActionSender,
    scope: Entity<ScopeBar>,
    wide: bool,
    page: Page,
    config: SourceControlConfig,
    projects: crate::settings_devices::ProjectIds,
    /// Settings writes in flight, with the config to fall back to.
    pending: HashMap<u64, SourceControlConfig>,
    next_token: u64,
    menu: Option<(Pick, usize)>,
    triggers: [FocusHandle; Pick::ALL.len()],
    advanced_open: bool,
    cleanup_open: bool,
    lines: Vec<Line>,
    areas: Vec<(Field, Entity<TextareaState>)>,
    _subscriptions: Vec<Subscription>,
}

impl ScopedPanel {
    pub fn new(
        page: Page,
        token_base: u64,
        actions: ActionSender,
        scope: Entity<ScopeBar>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut subscriptions = vec![cx.observe(&scope, |_, _, cx| cx.notify())];
        let mut lines = Vec::new();
        let mut areas = Vec::new();
        for (key, placeholder) in Self::line_fields(page) {
            let input = cx.new(|cx| InputState::new(window, cx).placeholder(*placeholder));
            subscriptions.push(cx.subscribe_in(
                &input,
                window,
                move |this, _, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                        this.commit_line(*key, window, cx);
                    }
                },
            ));
            lines.push(Line { key: *key, input });
        }
        for (field, placeholder) in Self::area_fields(page) {
            let area = cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder(*placeholder)
                    .auto_grow(4, 4)
            });
            subscriptions.push(cx.subscribe_in(
                &area,
                window,
                move |this, _, event: &InputEvent, _, cx| {
                    if matches!(event, InputEvent::Blur) {
                        this.commit_area(*field, cx);
                    }
                },
            ));
            areas.push((*field, area));
        }
        Self {
            actions,
            scope,
            wide: true,
            page,
            config: SourceControlConfig::default(),
            projects: crate::settings_devices::ProjectIds::new(),
            pending: HashMap::new(),
            next_token: token_base,
            menu: None,
            triggers: std::array::from_fn(|_| cx.focus_handle().tab_stop(true)),
            advanced_open: false,
            cleanup_open: false,
            lines,
            areas,
            _subscriptions: subscriptions,
        }
    }

    pub fn set_wide(&mut self, wide: bool, cx: &mut Context<Self>) {
        if self.wide != wide {
            self.wide = wide;
            cx.notify();
        }
    }

    /// The attached environment's settings and the projects that hold the overrides.
    pub fn set_data(
        &mut self,
        config: SourceControlConfig,
        projects: crate::settings_devices::ProjectIds,
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

    /// Closes menus and folds, as the settings page opens.
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        self.menu = None;
        self.advanced_open = false;
        self.cleanup_open = false;
        cx.notify();
    }

    /// Closes an open menu; returns whether there was one, so Escape stops there.
    pub fn close_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some((pick, _)) = self.menu.take() else {
            return false;
        };
        window.focus(&self.triggers[pick.index()], cx);
        cx.notify();
        true
    }

    pub fn on_call(&mut self, token: u64, event: CallEvent, cx: &mut Context<Self>) {
        if matches!(event, CallEvent::Progress(_)) {
            return;
        }
        let Some(previous) = self.pending.remove(&token) else {
            return;
        };
        if matches!(event, CallEvent::Failed(_)) {
            self.config = previous;
        }
        cx.notify();
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
            in_project: matches!(resolved, Scope::Project(_)),
            members: search
                .project
                .as_ref()
                .and_then(|key| self.projects.get(key))
                .cloned()
                .unwrap_or_default(),
            label: environment.map_or_else(|| "Environment".to_owned(), |e| e.label.clone()),
            machine: environment.map_or(crate::projects::MachineKind::Desktop, |e| e.machine),
            connected: data.environments.iter().any(|e| e.connected),
        }
    }

    /// Why a control is inert, or `None` when its write reaches the environment.
    fn gate(&self, field: Field, cx: &App) -> Option<String> {
        let scope = self.scope.read(cx);
        sc::reach(field, scope.search(), scope.data()).err()
    }

    fn value(&self, field: Field, ctx: &Ctx) -> Value {
        self.config.effective(field, &ctx.members, ctx.in_project)
    }

    fn text(&self, field: Field, ctx: &Ctx) -> String {
        self.value(field, ctx)
            .as_str()
            .unwrap_or_default()
            .to_owned()
    }

    /// A project scope writes the entry of every project of the group; anywhere else the
    /// environment's value is written (`None` removes an override, or restores the default).
    fn write(&mut self, changes: Vec<(Field, Option<Value>)>, cx: &mut Context<Self>) {
        let ctx = self.ctx(cx);
        if changes
            .iter()
            .any(|(field, _)| self.gate(*field, cx).is_some())
            || (ctx.in_project && ctx.members.is_empty())
        {
            return;
        }
        let members = ctx.in_project.then_some(ctx.members.as_slice());
        let patch = self.config.patch(&changes, members);
        let previous = self.config.clone();
        self.config.apply(&changes, members);
        let token = self.next_token;
        self.next_token += 1;
        let action = Action::Call {
            token,
            call: Call::UpdateSettings,
            payload: patch,
        };
        if self.actions.try_send(action).is_ok() {
            self.pending.insert(token, previous);
        } else {
            self.config = previous;
        }
        cx.notify();
    }

    /// Saves a single line field when it lost focus or Enter was pressed and it differs from the
    /// setting. A number that does not parse is dropped, and the field shows the setting again.
    fn commit_line(&mut self, key: Key, window: &mut Window, cx: &mut Context<Self>) {
        let Some(line) = self.lines.iter().find(|line| line.key == key) else {
            return;
        };
        let text = line.input.read(cx).value().trim().to_owned();
        let ctx = self.ctx(cx);
        match key {
            Key::Rule(rule) => {
                let days = crate::settings_worktrees::clamp_days(&text, rule.minimum());
                if days.is_some() && days.map(u64::from) != self.rules(&ctx)[rule.wire()].as_u64() {
                    return self.write_rules(rule.wire(), json!(days), &ctx, cx);
                }
                self.sync_lines(&ctx, window, cx);
            }
            Key::Field(field) => {
                let change = match field {
                    Field::SettleDays => match crate::settings_threads::parse_settle_days(&text) {
                        Some(days) if self.value(field, &ctx).as_u64() != Some(u64::from(days)) => {
                            Some(json!(days))
                        }
                        _ => None,
                    },
                    _ => (text != self.text(field, &ctx)).then(|| json!(text)),
                };
                match change {
                    Some(value) => self.write(vec![(field, Some(value))], cx),
                    None => self.sync_lines(&ctx, window, cx),
                }
            }
        }
    }

    /// The cleanup rules the scope shows: a project's policy over the machine's rules.
    fn rules(&self, ctx: &Ctx) -> serde_json::Map<String, Value> {
        let storage = self.config.environment_value(Field::StorageCleanup);
        let policy = if ctx.in_project {
            self.value(Field::WorktreeCleanup, ctx)
        } else {
            Value::Null
        };
        crate::settings_worktrees::resolve_rules(&storage, &policy)
    }

    /// Saves one cleanup rule: a project's custom policy, or the machine's `storageCleanup`
    /// (written whole, so the artifact and log retention it also holds is carried along).
    fn write_rules(&mut self, key: &'static str, value: Value, ctx: &Ctx, cx: &mut Context<Self>) {
        if ctx.in_project {
            let mut rules = self.rules(ctx);
            rules.insert(key.to_owned(), value);
            let policy = json!({"mode": "custom", "rules": rules});
            return self.write(vec![(Field::WorktreeCleanup, Some(policy))], cx);
        }
        let mut storage = self.config.environment_value(Field::StorageCleanup);
        let mut merged = crate::settings_worktrees::default_storage_cleanup();
        for (name, value) in storage.as_object_mut().into_iter().flatten() {
            merged[name.as_str()] = value.clone();
        }
        merged[key] = value;
        self.write(vec![(Field::StorageCleanup, Some(merged))], cx);
    }

    /// Saves a text area's trimmed content when it lost focus and differs from the setting.
    fn commit_area(&mut self, field: Field, cx: &mut Context<Self>) {
        let Some((_, area)) = self.areas.iter().find(|(f, _)| *f == field) else {
            return;
        };
        let value = area.read(cx).value().trim().to_owned();
        let ctx = self.ctx(cx);
        if value != self.text(field, &ctx) {
            self.write(vec![(field, Some(json!(value)))], cx);
        }
    }

    /// The text a single line field shows for its setting.
    fn line_text(&self, key: Key, ctx: &Ctx) -> String {
        match key {
            Key::Rule(rule) => self.rules(ctx)[rule.wire()]
                .as_u64()
                .map(|days| days.to_string())
                .unwrap_or_default(),
            Key::Field(Field::SettleDays) => self
                .value(Field::SettleDays, ctx)
                .as_u64()
                .map(|days| days.to_string())
                .unwrap_or_default(),
            Key::Field(field) => self.text(field, ctx),
        }
    }

    /// Puts the setting's text into the unfocused fields, as the web's `key` remount does.
    fn sync_lines(&self, ctx: &Ctx, window: &mut Window, cx: &mut Context<Self>) {
        for line in &self.lines {
            let want = self.line_text(line.key, ctx);
            let focused = line.input.read(cx).focus_handle(cx).is_focused(window);
            if !focused && line.input.read(cx).value().as_ref() != want.as_str() {
                line.input
                    .update(cx, |state, cx| state.set_value(want, window, cx));
            }
        }
        for (field, area) in &self.areas {
            let want = self.text(*field, ctx);
            let focused = area.read(cx).focus_handle(cx).is_focused(window);
            if !focused && area.read(cx).value().as_ref() != want.as_str() {
                area.update(cx, |state, cx| state.set_value(want, window, cx));
            }
        }
    }

    fn open_menu(&mut self, pick: Pick, selected: usize, cx: &mut Context<Self>) {
        let selected = if selected < pick.labels().len() {
            selected
        } else {
            0
        };
        self.menu = Some((pick, selected));
        cx.notify();
    }

    fn pick(&mut self, pick: Pick, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let ctx = self.ctx(cx);
        match pick {
            Pick::KeepWhen => {
                if let Some(value) = pick.value(index) {
                    self.write_rules("worktreeKeepWhen", value, &ctx, cx);
                }
            }
            // A project's entry stores whole rules, so Custom starts from the ones in effect, as
            // the web completes its `rules: {}` from `resolveWorktreeCleanup`.
            Pick::CleanupMode if pick.value(index).is_some_and(|v| v["mode"] == "custom") => {
                let policy = json!({"mode": "custom", "rules": self.rules(&ctx)});
                self.write(vec![(Field::WorktreeCleanup, Some(policy))], cx)
            }
            _ => self.write(vec![(pick.field(ctx.in_project), pick.value(index))], cx),
        }
        self.close_menu(window, cx);
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some((pick, highlighted)) = self.menu else {
            return;
        };
        let count = pick.labels().len();
        let next = match event.keystroke.key.as_str() {
            "down" => (highlighted + 1) % count,
            "up" => (highlighted + count - 1) % count,
            "home" => 0,
            "end" => count - 1,
            "enter" | "space" => {
                self.pick(pick, highlighted, window, cx);
                cx.stop_propagation();
                return;
            }
            _ => return,
        };
        self.menu = Some((pick, next));
        cx.stop_propagation();
        cx.notify();
    }

    /// A control that ignores the pointer, half transparent under the reason as its tooltip.
    fn inert(id: &'static str, why: Option<String>, control: AnyElement) -> AnyElement {
        match why {
            None => control,
            Some(why) => div()
                .id((id, 1usize))
                .opacity(0.5)
                .tooltip(tooltip(why))
                .child(control)
                .into_any_element(),
        }
    }

    fn marker(&self, ids: Ids, field: Field, ctx: &Ctx) -> Marker {
        let project = self.config.project_override(field, &ctx.members);
        let (state, summary) = sc::inheritance(
            field,
            ctx.in_project,
            project.is_some(),
            self.config.environment_set(field),
            &ctx.label,
        );
        Marker {
            id: ids.marker,
            state,
            summary,
            layers: sc::layers(
                field,
                &ctx.label,
                ctx.in_project,
                project.as_ref(),
                &self.config,
            ),
            machine: ctx.machine,
            environment: ctx.label.clone(),
            groups: Vec::new(),
        }
    }

    /// Whether any of the fields holds a value of its own in the scope.
    fn dirty(&self, fields: &[Field], ctx: &Ctx) -> bool {
        fields.iter().any(|field| {
            if ctx.in_project {
                field.project_scoped()
                    && self.config.project_override(*field, &ctx.members).is_some()
            } else {
                self.config.environment_set(*field)
            }
        })
    }

    /// `SettingResetButton`: shown while any of the fields holds a value of its own.
    fn reset(
        &self,
        id: &'static str,
        tooltip: &'static str,
        fields: &[Field],
        ctx: &Ctx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.dirty(fields, ctx) || self.gate(fields[0], cx).is_some() {
            return None;
        }
        let changes: Vec<(Field, Option<Value>)> =
            fields.iter().map(|field| (*field, None)).collect();
        Some(self.reset_with(id, tooltip, changes, window, cx))
    }

    /// A reset button that writes exactly `changes`.
    fn reset_with(
        &self,
        id: &'static str,
        tooltip: &'static str,
        changes: Vec<(Field, Option<Value>)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        reset_button_labeled(
            id,
            tooltip,
            move |_, _, cx| {
                let changes = changes.clone();
                view.update(cx, |this, cx| this.write(changes, cx))
            },
            window,
            cx,
        )
    }

    fn flag(
        &self,
        id: &'static str,
        field: Field,
        ctx: &Ctx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let checked = self.value(field, ctx).as_bool().unwrap_or(false);
        let why = self.gate(field, cx);
        let view = cx.entity();
        let toggle: Toggle = if why.is_some() {
            Rc::new(|_, _| {})
        } else {
            Rc::new(move |_, cx| {
                view.update(cx, |this, cx| {
                    this.write(vec![(field, Some(json!(!checked)))], cx)
                })
            })
        };
        Self::inert(id, why, switch(id, checked, toggle, window, cx))
    }

    /// A switch whose on state writes `on` and whose off state writes `off`.
    #[allow(clippy::too_many_arguments)]
    fn toggle(
        &self,
        id: &'static str,
        field: Field,
        checked: bool,
        on: Value,
        off: Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let why = self.gate(field, cx);
        let view = cx.entity();
        let toggle: Toggle = if why.is_some() {
            Rc::new(|_, _| {})
        } else {
            Rc::new(move |_, cx| {
                let next = if checked { off.clone() } else { on.clone() };
                view.update(cx, |this, cx| this.write(vec![(field, Some(next))], cx))
            })
        };
        Self::inert(id, why, switch(id, checked, toggle, window, cx))
    }

    #[allow(clippy::too_many_arguments)]
    fn setting(
        &self,
        ids: Ids,
        title: &'static str,
        description: impl IntoElement,
        field: Field,
        reset: Option<AnyElement>,
        control: AnyElement,
        preview: Option<AnyElement>,
        ctx: &Ctx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        marker_row_with(
            ids.row,
            title,
            description,
            Some(self.marker(ids, field, ctx)),
            reset,
            control,
            preview,
            self.wide,
            window,
            cx,
        )
        .into_any_element()
    }

    /// `Select` (select.tsx) over a `Popover`, as the Labs tab builds it.
    fn select(
        &self,
        pick: Pick,
        selected: usize,
        width: f32,
        why: Option<String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (id, popover) = pick.ids();
        let labels = pick.labels();
        let view = cx.entity();
        let on_key: KeyHandler = {
            let view = view.clone();
            Rc::new(move |event, _, cx| {
                if matches!(
                    event.keystroke.key.as_str(),
                    "enter" | "space" | "down" | "up"
                ) {
                    cx.stop_propagation();
                    view.update(cx, |this, cx| this.open_menu(pick, selected, cx));
                }
            })
        };
        // The web's trigger reads "Mixed" for a value it cannot name, "Unavailable" offline.
        let unset = if self.ctx(cx).connected {
            "Mixed"
        } else {
            "Unavailable"
        };
        let trigger = SelectTrigger::new(
            id,
            labels.get(selected).copied().unwrap_or(unset),
            width,
            self.triggers[pick.index()].clone(),
            on_key,
        );
        if why.is_some() {
            return Self::inert(id, why, trigger.into_any_element());
        }
        let open = self.menu.is_some_and(|(current, _)| current == pick);
        let highlighted = self
            .menu
            .filter(|(current, _)| *current == pick)
            .map_or(usize::MAX, |(_, index)| index);
        let on_pick: PickHandler = {
            let view = view.clone();
            Rc::new(move |index, window, cx| {
                view.update(cx, |this, cx| this.pick(pick, index, window, cx))
            })
        };
        let on_hover: HoverHandler = {
            let view = view.clone();
            Rc::new(move |index, cx| {
                view.update(cx, |this, cx| {
                    if this
                        .menu
                        .is_some_and(|(p, current)| p == pick && current != index)
                    {
                        this.menu = Some((pick, index));
                        cx.notify();
                    }
                })
            })
        };
        Popover::new(popover)
            .anchor(Anchor::TopRight)
            .offset(px(4.))
            .open(open)
            .p_0()
            .appearance(false)
            .on_open_change(move |opened, window, cx| {
                view.update(cx, |this, cx| {
                    if *opened {
                        this.open_menu(pick, selected, cx);
                    } else if this.menu.is_some() {
                        this.close_menu(window, cx);
                    }
                })
            })
            .trigger(trigger)
            .content(move |_, _, cx| {
                select_menu(
                    &Menu {
                        id,
                        labels: labels.clone(),
                        selected,
                        highlighted,
                        width,
                        on_pick: on_pick.clone(),
                        on_hover: on_hover.clone(),
                    },
                    cx,
                )
            })
            .into_any_element()
    }

    /// `Input size="sm"` of a single line field, `w-full sm:w-56`.
    fn line(
        &self,
        key: impl Into<Key>,
        width: f32,
        disabled: bool,
        window: &Window,
        cx: &App,
    ) -> AnyElement {
        let key = key.into();
        let Some(line) = self.lines.iter().find(|line| line.key == key) else {
            return div().into_any_element();
        };
        let frame = sm_field(&line.input, false, false, disabled, window, cx);
        div().w(px(width)).child(frame).into_any_element()
    }

    /// `Textarea` (textarea.tsx) under a setting: `mt-3 max-w-2xl pb-3.5`.
    fn text_area(
        &self,
        field: Field,
        label: &'static str,
        disabled: bool,
        window: &Window,
        cx: &App,
    ) -> AnyElement {
        let Some((_, input)) = self.areas.iter().find(|(f, _)| *f == field) else {
            return div().into_any_element();
        };
        let theme = Tokens::of(cx);
        let focused = input.read(cx).focus_handle(cx).contains_focused(window, cx);
        let background = if theme.dark {
            theme.input.opacity(0.32)
        } else {
            theme.background
        };
        div()
            .mt_3()
            .pb(px(14.))
            .max_w(px(TEXT_AREA_MAX))
            .child(
                div()
                    .w_full()
                    .rounded(px(8.))
                    .border_1()
                    .border_color(if focused { theme.ring } else { theme.input })
                    .bg(background)
                    .text_size(px(14.))
                    .when(disabled, |frame| frame.opacity(0.64))
                    .child(
                        Textarea::new(input)
                            .appearance(false)
                            .bordered(false)
                            .disabled(disabled)
                            .aria_label(label),
                    ),
            )
            .into_any_element()
    }

    /// The row's value where the web has a picker this port does not: the model's name.
    fn label_control(text: impl Into<SharedString>, cx: &App) -> AnyElement {
        let theme = Tokens::of(cx);
        div()
            .text_sm()
            .text_color(theme.muted_foreground)
            .child(text.into())
            .into_any_element()
    }

    fn line_fields(page: Page) -> &'static [(Key, &'static str)] {
        match page {
            Page::Threads => &[
                (Key::Field(Field::BaseDirectory), "~/"),
                (Key::Field(Field::SettleDays), ""),
            ],
            Page::Worktrees => &[
                (Key::Field(Field::WorktreesDir), "Default"),
                (Key::Field(Field::BranchPrefix), "No prefix"),
                (Key::Rule(Rule::AfterDays), ""),
                (Key::Rule(Rule::SettledDays), ""),
            ],
        }
    }

    fn area_fields(page: Page) -> &'static [(Field, &'static str)] {
        match page {
            Page::Threads => &[(
                Field::TitleInstructions,
                "Keep titles to a few words. Lead with the main subject.",
            )],
            Page::Worktrees => &[(
                Field::BranchInstructions,
                "Use julius/ followed by the issue ID and a short description.",
            )],
        }
    }
}

impl Render for ScopedPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ctx = self.ctx(cx);
        self.sync_lines(&ctx, window, cx);
        let page = v_flex()
            .w_full()
            .gap_8()
            .on_key_down(cx.listener(|this, event, window, cx| this.key_down(event, window, cx)));
        match self.page {
            Page::Threads => page.children(self.threads(&ctx, window, cx)),
            Page::Worktrees => page.children(self.worktrees(&ctx, window, cx)),
        }
    }
}
