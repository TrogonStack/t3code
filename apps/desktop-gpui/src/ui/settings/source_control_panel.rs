//! The Source control tab: the Repositories rows of `ProjectDefaultsSettings` (440-535), then
//! `SourceControlSettingsPanel` (SourceControlSettings.tsx:337-527) and
//! `SourceControlWritingSettingsSection` (SourceControlWritingSettings.tsx:88-615). The panel owns
//! its menus, text areas and requests; server-settings writes are optimistic and put back when
//! the call fails.

use super::components::{
    HoverHandler, KeyHandler, Menu, PickHandler, SelectTrigger, Toggle, disclosure, group,
    reset_button_labeled, section, section_with_action, select_menu, switch,
};
use super::devices_panel::widgets::{Variant, button, icon_button, note, spinner, warning};
use super::scope_inheritance::{Marker, marker_row_with};
use super::scope_pickers::ScopeBar;
use super::tokens::Tokens;
use super::*;
use crate::connection::{Action, ActionSender, Call};
use crate::settings_labs::{Scope, resolve};
use crate::settings_source_control::{
    self as sc, Badge, Discovery, Dot, Field, Item, MERGE_METHODS, STYLE_MODES,
    SourceControlConfig, Span,
};
use crate::status::CallEvent;
use gpui::{FocusHandle, KeyDownEvent, Subscription};
use gpui_component::input::{InputEvent, Textarea, TextareaState};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

mod hosts;

/// `emerald-500` and `amber-500`, the `--success` and `--warning` tokens (index.css:1162-1165).
const SUCCESS: u32 = 0x10b981;
const WARNING: u32 = 0xf59e0b;
/// `max-w-2xl`.
const TEXT_AREA_MAX: f32 = 672.;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pick {
    Merge,
    Style,
}

impl Pick {
    fn index(self) -> usize {
        match self {
            Self::Merge => 0,
            Self::Style => 1,
        }
    }

    fn count(self) -> usize {
        match self {
            Self::Merge => MERGE_METHODS.len(),
            Self::Style => STYLE_MODES.len(),
        }
    }
}

pub struct SourceControlPanel {
    actions: ActionSender,
    scope: Entity<ScopeBar>,
    wide: bool,
    config: SourceControlConfig,
    projects: crate::settings_devices::ProjectIds,
    discovery: Option<Discovery>,
    scan_error: Option<String>,
    scan_token: Option<u64>,
    requested: bool,
    /// Settings writes in flight, with the config to fall back to.
    pending: HashMap<u64, SourceControlConfig>,
    next_token: u64,
    menu: Option<(Pick, usize)>,
    triggers: [FocusHandle; 2],
    advanced_open: bool,
    /// Providers whose account name is showing.
    revealed: HashSet<usize>,
    custom: Entity<TextareaState>,
    review: Entity<TextareaState>,
    /// Providers whose details are open.
    expanded: HashSet<String>,
    hosts: hosts::HostForms,
    /// Host writes in flight (also keyed in `pending`).
    host_writes: HashSet<u64>,
    /// Secret checks in flight, with the field that asked.
    checks: HashMap<u64, Entity<super::secret_field::SecretField>>,
    /// Providers whose login names show in the details.
    reveal_accounts: HashSet<String>,
    /// The open account select: host and highlighted row.
    account_menu: Option<(String, usize)>,
    _subscriptions: Vec<Subscription>,
}

/// What the scope bar says about the environment the tab inspects.
struct Ctx {
    in_project: bool,
    members: Vec<String>,
    label: String,
    machine: crate::projects::MachineKind,
    connected: bool,
    /// `scope.environmentIds.length !== 1 && connectedEnvironments.length > 1`.
    aggregate: bool,
}

impl SourceControlPanel {
    pub fn new(
        actions: ActionSender,
        scope: Entity<ScopeBar>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let custom = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Keep titles concise. Use short bullet points in descriptions.")
                .auto_grow(4, 4)
        });
        let review = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(
                    "Show service interfaces before implementations. Skip test files unless they change behavior. Keep summaries to two sentences.",
                )
                .auto_grow(4, 4)
        });
        let hosts = hosts::HostForms::new(window, cx);
        let mut subscriptions = vec![
            cx.observe(&scope, |_, _, cx| cx.notify()),
            cx.subscribe_in(&custom, window, |this, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Blur) {
                    this.commit(Field::StyleInstructions, cx);
                }
            }),
            cx.subscribe_in(&review, window, |this, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Blur) {
                    this.commit(Field::ReviewInstructions, cx);
                }
            }),
        ];
        subscriptions.extend(hosts.subscribe(window, cx));
        Self {
            actions,
            scope,
            wide: true,
            config: SourceControlConfig::default(),
            projects: crate::settings_devices::ProjectIds::new(),
            discovery: None,
            scan_error: None,
            scan_token: None,
            requested: false,
            pending: HashMap::new(),
            next_token: sc::TOKEN_BASE,
            menu: None,
            triggers: std::array::from_fn(|_| cx.focus_handle().tab_stop(true)),
            advanced_open: false,
            revealed: HashSet::new(),
            custom,
            review,
            expanded: HashSet::new(),
            hosts,
            host_writes: HashSet::new(),
            checks: HashMap::new(),
            reveal_accounts: HashSet::new(),
            account_menu: None,
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
        self.ensure_scan(cx);
        if changed {
            cx.notify();
        }
    }

    /// Closes menus and scans again, as the settings page opens.
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        self.menu = None;
        self.advanced_open = false;
        self.revealed.clear();
        self.expanded.clear();
        self.reveal_accounts.clear();
        self.account_menu = None;
        self.requested = false;
        self.ensure_scan(cx);
        cx.notify();
    }

    /// Closes an open menu; returns whether there was one, so Escape stops there.
    pub fn close_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.account_menu.take().is_some() {
            cx.notify();
            return true;
        }
        let mut closed = false;
        for (_, _, field) in self.hosts.secrets() {
            closed |= field.update(cx, |field, cx| field.close_menu(cx));
        }
        if closed {
            return true;
        }
        let Some((pick, _)) = self.menu.take() else {
            return false;
        };
        window.focus(&self.triggers[pick.index()], cx);
        cx.notify();
        true
    }

    /// The first scan of an open page, once an environment is connected.
    fn ensure_scan(&mut self, cx: &mut Context<Self>) {
        if !self.requested && self.ctx(cx).connected {
            self.requested = true;
            self.scan(cx);
        }
    }

    fn scan(&mut self, cx: &mut Context<Self>) {
        if self.scan_token.is_some() {
            return;
        }
        let token = self.next_token;
        self.next_token += 1;
        let action = Action::Call {
            token,
            call: Call::DiscoverSourceControl,
            payload: json!({}),
        };
        if self.actions.try_send(action).is_err() {
            return;
        }
        self.scan_token = Some(token);
        self.scan_error = None;
        cx.notify();
    }

    /// The outcome of a scan or of a write.
    pub fn on_call(&mut self, token: u64, event: CallEvent, cx: &mut Context<Self>) {
        if self.checks.contains_key(&token) {
            self.checked(token, event, cx);
            return;
        }
        if matches!(event, CallEvent::Progress(_)) {
            return;
        }
        if self.scan_token == Some(token) {
            self.scan_token = None;
            match event {
                CallEvent::Done(value) => {
                    self.discovery = Some(Discovery::parse(&value));
                    self.scan_error = None;
                }
                CallEvent::Failed(message) => self.scan_error = Some(message),
                CallEvent::Progress(_) => {}
            }
            cx.notify();
            return;
        }
        let Some(previous) = self.pending.remove(&token) else {
            return;
        };
        if matches!(event, CallEvent::Failed(_)) {
            self.host_writes.remove(&token);
            self.config = previous;
            cx.notify();
        } else {
            self.host_saved(token, cx);
            cx.notify();
        }
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
        let connected = data.environments.iter().filter(|e| e.connected).count();
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
            connected: environment.is_some_and(|e| e.connected),
            aggregate: !matches!(resolved, Scope::Environment(_)) && connected > 1,
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

    /// Saves a text area's trimmed content when it lost focus and differs from the setting.
    fn commit(&mut self, field: Field, cx: &mut Context<Self>) {
        let input = if field == Field::StyleInstructions {
            &self.custom
        } else {
            &self.review
        };
        let value = input.read(cx).value().trim().to_owned();
        let ctx = self.ctx(cx);
        if value != self.text(field, &ctx) {
            self.write(vec![(field, Some(json!(value)))], cx);
        }
    }

    /// Puts the setting's text into an unfocused text area, as the web's `key` remount does.
    fn sync_inputs(&self, ctx: &Ctx, window: &mut Window, cx: &mut Context<Self>) {
        for (input, field) in [
            (&self.custom, Field::StyleInstructions),
            (&self.review, Field::ReviewInstructions),
        ] {
            let want = self.text(field, ctx);
            let focused = input.read(cx).focus_handle(cx).is_focused(window);
            if !focused && input.read(cx).value().as_ref() != want.as_str() {
                input.update(cx, |state, cx| state.set_value(want, window, cx));
            }
        }
    }

    fn open_menu(&mut self, pick: Pick, selected: usize, cx: &mut Context<Self>) {
        self.menu = Some((pick, selected));
        cx.notify();
    }

    fn pick(&mut self, pick: Pick, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        match pick {
            Pick::Merge => self.write(vec![(Field::MergeMethod, Some(sc::merge_value(index)))], cx),
            Pick::Style => {
                let ctx = self.ctx(cx);
                let mut changes = vec![(Field::StyleMode, Some(json!(STYLE_MODES[index].0)))];
                if self.text(Field::StyleMode, &ctx) == "custom" {
                    let text = self.custom.read(cx).value().trim().to_owned();
                    changes.push((Field::StyleInstructions, Some(json!(text))));
                }
                self.write(changes, cx);
            }
        }
        self.close_menu(window, cx);
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some((pick, highlighted)) = self.menu else {
            return;
        };
        let count = pick.count();
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

    fn marker(&self, id: &'static str, field: Field, ctx: &Ctx) -> Marker {
        let project = self.config.project_override(field, &ctx.members);
        let (state, summary) = sc::inheritance(
            field,
            ctx.in_project,
            project.is_some(),
            self.config.environment_set(field),
            &ctx.label,
        );
        Marker {
            id,
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
        let dirty = fields.iter().any(|field| {
            if ctx.in_project {
                field.project_scoped()
                    && self.config.project_override(*field, &ctx.members).is_some()
            } else {
                self.config.environment_set(*field)
            }
        });
        if !dirty || self.gate(fields[0], cx).is_some() {
            return None;
        }
        let view = cx.entity();
        let changes: Vec<(Field, Option<Value>)> =
            fields.iter().map(|field| (*field, None)).collect();
        Some(reset_button_labeled(
            id,
            tooltip,
            move |_, _, cx| {
                let changes = changes.clone();
                view.update(cx, |this, cx| this.write(changes, cx))
            },
            window,
            cx,
        ))
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

    #[allow(clippy::too_many_arguments)]
    fn setting(
        &self,
        id: &'static str,
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
            id,
            title,
            description,
            Some(self.marker(marker_id(id), field, ctx)),
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
        let (id, popover, labels): (&'static str, &'static str, Vec<&'static str>) = match pick {
            Pick::Merge => (
                "settings-merge-method",
                "settings-merge-method-select",
                MERGE_METHODS.iter().map(|(_, label)| *label).collect(),
            ),
            Pick::Style => (
                "settings-source-control-writing-style",
                "settings-source-control-writing-style-select",
                STYLE_MODES.iter().map(|(_, label)| *label).collect(),
            ),
        };
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
        let trigger = SelectTrigger::new(
            id,
            labels[selected],
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

    /// `Textarea` (textarea.tsx) under a setting: `mt-3 max-w-2xl pb-3.5`.
    fn text_area(
        &self,
        input: &Entity<TextareaState>,
        label: &'static str,
        disabled: bool,
        window: &Window,
        cx: &App,
    ) -> AnyElement {
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

    fn repositories(&self, ctx: &Ctx, window: &mut Window, cx: &mut Context<Self>) -> gpui::Div {
        let project = ctx.in_project;
        let merge_why = self.gate(Field::MergeMethod, cx);
        let merge = sc::merge_index(&self.value(Field::MergeMethod, ctx));
        let rows = vec![
            self.setting(
                "automatic-pull",
                "Automatically pull",
                if project {
                    "Keeps this project's default branch current when the checkout has no local changes or commits."
                } else {
                    "Keeps the default branch current when the checkout has no local changes or commits. Projects can override it."
                },
                Field::AutoPull,
                self.reset(
                    "settings-automatic-pull-reset",
                    "Reset automatic pull to off",
                    &[Field::AutoPull],
                    ctx,
                    window,
                    cx,
                ),
                self.flag("settings-automatic-pull-switch", Field::AutoPull, ctx, window, cx),
                None,
                ctx,
                window,
                cx,
            ),
            self.setting(
                "remove-agent-credits-on-merge",
                "Remove agent credits when merging",
                "Remove recognized agent credit lines from merge and squash messages on hosts that support it, keeping human co-authors. Includes auto-merge. Excludes merge queues, stack merges, and existing commits.",
                Field::AgentCredits,
                self.reset(
                    "settings-agent-credits-reset",
                    "Keep agent credits",
                    &[Field::AgentCredits],
                    ctx,
                    window,
                    cx,
                ),
                self.flag("settings-agent-credits-switch", Field::AgentCredits, ctx, window, cx),
                None,
                ctx,
                window,
                cx,
            ),
            self.setting(
                "pull-request-merge-method",
                "Default merge method",
                if project {
                    "Pull requests in this project start with this method."
                } else {
                    "Pull requests start with this method. Last selected reuses whatever you chose most recently on this device."
                },
                Field::MergeMethod,
                self.reset(
                    "settings-merge-method-reset",
                    "Reset to last selected",
                    &[Field::MergeMethod],
                    ctx,
                    window,
                    cx,
                ),
                self.select(Pick::Merge, merge, 176., merge_why, cx),
                None,
                ctx,
                window,
                cx,
            ),
        ];
        section("Repositories", group(rows, cx))
    }

    fn scan_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Tokens::of(cx);
        let idle = self.scan_token.is_none();
        let view = cx.entity();
        let glyph = if idle {
            Icon::new(Icons::RefreshCw).size(px(14.)).into_any_element()
        } else {
            spinner(14., theme.muted_foreground)
        };
        icon_button(
            "settings-source-control-rescan",
            Variant::GhostMuted,
            idle,
            cx,
        )
        .size(px(24.))
        .tooltip(tooltip("Rescan Git and hosting integrations"))
        .when(idle, |button| {
            button.on_click(move |_, _, cx| view.update(cx, |this, cx| this.scan(cx)))
        })
        .child(glyph)
        .into_any_element()
    }

    /// `SourceControlItemMark`: the icon with its status dot at its top left corner.
    fn mark(&self, item: &Item, cx: &App) -> AnyElement {
        let theme = Tokens::of(cx);
        let color = match item.dot() {
            Dot::Success => gpui::rgb(SUCCESS).into(),
            Dot::Warning => gpui::rgb(WARNING).into(),
            Dot::Muted => theme.muted_foreground.opacity(0.35),
        };
        let dot = div().size(px(8.)).rounded_full().bg(color);
        let Some(path) = crate::assets::source_control_icon(&item.kind, theme.dark) else {
            return dot.flex_shrink_0().into_any_element();
        };
        div()
            .relative()
            .size(px(20.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .size(px(18.))
                    .opacity(0.8)
                    .child(img(path).size_full()),
            )
            .child(
                div()
                    .absolute()
                    .left(px(-4.))
                    .top(px(-4.))
                    .size(px(12.))
                    .rounded_full()
                    .bg(theme.background)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(dot),
            )
            .into_any_element()
    }

    /// `Badge size="sm"` (badge.tsx:21): `h-4 rounded-[.25rem] px-[3px] text-[.625rem]`.
    fn badge(item: Badge, cx: &App) -> gpui::Div {
        let theme = Tokens::of(cx);
        let (background, color) = match item {
            Badge::ComingSoon => (
                gpui::Hsla::from(gpui::rgb(WARNING)).opacity(if theme.dark { 0.16 } else { 0.08 }),
                warning(&theme),
            ),
            Badge::NotAuthenticated if theme.dark => (
                gpui::white().opacity(0.03),
                gpui::Hsla::from(gpui::rgb(0xf5f5f5)),
            ),
            Badge::NotAuthenticated => (
                gpui::Hsla::from(gpui::rgb(0xfafafa)),
                gpui::Hsla::from(gpui::rgb(0x27272a)),
            ),
        };
        div()
            .h(px(16.))
            .min_w(px(16.))
            .px(px(3.))
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .rounded(px(4.))
            .bg(background)
            .text_color(color)
            .text_size(px(10.))
            .line_height(px(10.))
            .font_weight(FontWeight::MEDIUM)
            .whitespace_nowrap()
            .child(item.label())
    }

    /// One flex child of an item's summary line.
    fn span(&self, index: usize, span: &Span, cx: &mut Context<Self>) -> AnyElement {
        match span {
            Span::Text(text) => div().child(text.clone()).into_any_element(),
            Span::Code(text) => div()
                .px_1()
                .py(px(1.))
                .rounded(px(4.))
                .bg(Tokens::of(cx).muted)
                .font_family(cx.theme().mono_font_family.clone())
                .text_size(px(11.))
                .child(text.clone())
                .into_any_element(),
            Span::Account(account) => {
                let shown = self.revealed.contains(&index);
                let view = cx.entity();
                div()
                    .id(("settings-source-control-account", index))
                    .cursor_pointer()
                    .tooltip(tooltip(if shown {
                        "Click to hide account"
                    } else {
                        "Click to reveal account"
                    }))
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            if !this.revealed.remove(&index) {
                                this.revealed.insert(index);
                            }
                            cx.notify();
                        })
                    })
                    .child(hosts::redacted_login(
                        &if shown {
                            account.clone()
                        } else {
                            sc::redacted(account)
                        },
                        true,
                        cx,
                    ))
                    .into_any_element()
            }
        }
    }

    /// `DiscoveryItemRow` (SourceControlSettings.tsx:208-299).
    fn item_row(
        &self,
        index: usize,
        position: (usize, usize),
        item: &Item,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Tokens::of(cx);
        let hover = theme.muted.opacity(0.2);
        let mono = cx.theme().mono_font_family.clone();
        let title = h_flex()
            .min_w_0()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(self.mark(item, cx))
            .child(
                div()
                    .truncate()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.foreground)
                    .child(item.label.clone()),
            )
            .children(item.version.clone().map(|version| {
                div()
                    .font_family(mono)
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(version)
            }))
            .children(item.badge().map(|badge| Self::badge(badge, cx)));
        let summary = h_flex()
            .min_w_0()
            .flex_wrap()
            .items_center()
            .gap_x(px(4.))
            .text_xs()
            .line_height(px(18.))
            .text_color(theme.muted_foreground.opacity(0.8))
            .children(item.summary().iter().map(|span| self.span(index, span, cx)));
        let enabled = switch(
            ELEMENT_SWITCH[index.min(ELEMENT_SWITCH.len() - 1)],
            item.enabled(),
            Rc::new(|_, _| {}),
            window,
            cx,
        );
        div()
            .id(("settings-source-control-item", index))
            .when(position.0 == 0, |row| row.rounded_t(px(12.)))
            .when(position.0 + 1 == position.1, |row| row.rounded_b(px(12.)))
            .when(item.not_ready(), |row| row.opacity(0.8))
            .hover(move |style| style.bg(hover))
            .px_4()
            .py_3()
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        v_flex()
                            .min_w_0()
                            .flex_1()
                            .gap_1()
                            .child(title)
                            .child(summary),
                    )
                    .child(
                        h_flex()
                            .flex_shrink_0()
                            .items_center()
                            .gap_2()
                            .children(self.details_toggle(index, item, cx))
                            .children(
                                (!item.not_ready())
                                    .then(|| div().opacity(0.64).child(enabled).into_any_element()),
                            ),
                    ),
            )
            .children(self.host_details(item, window, cx))
            .into_any_element()
    }

    fn skeleton(&self, title: String, action: Option<AnyElement>, cx: &App) -> gpui::Div {
        let theme = Tokens::of(cx);
        let bar = |width: f32, height: f32| {
            div()
                .w(px(width))
                .h(px(height))
                .rounded_full()
                .bg(theme.foreground.opacity(0.06))
        };
        let rows = ["primary", "secondary"]
            .iter()
            .map(|_| {
                h_flex()
                    .items_center()
                    .justify_between()
                    .px_4()
                    .py_3()
                    .child(
                        v_flex()
                            .gap_2()
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .size(px(18.))
                                            .rounded(px(4.))
                                            .bg(theme.foreground.opacity(0.06)),
                                    )
                                    .child(bar(112., 16.))
                                    .child(bar(56., 20.)),
                            )
                            .child(bar(240., 12.)),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .size(px(28.))
                                    .rounded(px(4.))
                                    .bg(theme.foreground.opacity(0.06)),
                            )
                            .child(bar(36., 20.)),
                    )
                    .into_any_element()
            })
            .collect();
        section_with_action(title, action, group(rows, cx))
    }

    /// The empty card of a server with nothing detected, or whose scan failed.
    fn empty(&self, cx: &mut Context<Self>) -> gpui::Div {
        let theme = Tokens::of(cx);
        let failed = self.scan_error.is_some();
        let view = cx.entity();
        let idle = self.scan_token.is_none();
        let scan = button("settings-source-control-scan", Variant::Outline, idle, cx)
            .when(idle, |scan| {
                scan.on_click(move |_, _, cx| view.update(cx, |this, cx| this.scan(cx)))
            })
            .child(if idle {
                Icon::new(Icons::RefreshCw).size(px(14.)).into_any_element()
            } else {
                spinner(14., theme.foreground)
            })
            .child("Scan");
        let card = v_flex()
            .items_center()
            .gap_4()
            .px_6()
            .py_8()
            .text_center()
            .child(
                div()
                    .size(px(40.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(8.))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.card)
                    .text_color(theme.muted_foreground)
                    .child(Icon::new(Icons::GitBranch).size(px(20.))),
            )
            .child(
                v_flex()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .text_base()
                            .font_weight(FontWeight::MEDIUM)
                            .child(if failed {
                                "Could not scan the server environment"
                            } else {
                                "Nothing detected yet"
                            }),
                    )
                    .child(
                        div()
                            .max_w(px(384.))
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child(self.scan_error.clone().unwrap_or_else(|| {
                                "Install Git on the server, add optional hosting integrations or credentials your workspace needs, then rescan."
                                    .to_owned()
                            })),
                    ),
            )
            .child(scan);
        section(
            "Server environment",
            group(vec![card.into_any_element()], cx),
        )
    }

    /// The Version Control and Source control providers sections, or what stands in for them.
    fn discovery(&self, ctx: &Ctx, window: &mut Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = Tokens::of(cx);
        if !ctx.connected {
            let message = note(
                "Connect an environment to inspect its version control tools and hosting integrations.",
                cx,
            );
            let message = div()
                .px_4()
                .py_3()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(message.text_sm());
            return vec![
                section(
                    "Server environment",
                    group(vec![message.into_any_element()], cx),
                )
                .into_any_element(),
            ];
        }
        let suffix = if ctx.aggregate {
            format!(" \u{b7} {}", ctx.label)
        } else {
            String::new()
        };
        let Some(discovery) = self.discovery.as_ref() else {
            if self.scan_error.is_some() {
                return vec![self.empty(cx).into_any_element()];
            }
            let action = self.scan_button(cx);
            return vec![
                self.skeleton(format!("Version Control{suffix}"), Some(action), cx)
                    .into_any_element(),
                self.skeleton("Source control providers".to_owned(), None, cx)
                    .into_any_element(),
            ];
        };
        if discovery.is_empty() {
            return vec![self.empty(cx).into_any_element()];
        }
        let has_vcs = !discovery.version_control.is_empty();
        let mut sections = Vec::new();
        let mut next = 0;
        let mut rows =
            |items: &[Item], this: &Self, window: &mut Window, cx: &mut Context<Self>| {
                let rows: Vec<AnyElement> = items
                    .iter()
                    .enumerate()
                    .map(|(position, item)| {
                        this.item_row(next + position, (position, items.len()), item, window, cx)
                    })
                    .collect();
                next += items.len();
                group(rows, cx)
            };
        if has_vcs {
            let body = rows(&discovery.version_control, self, window, cx);
            let action = self.scan_button(cx);
            sections.push(
                section_with_action(format!("Version Control{suffix}"), Some(action), body)
                    .into_any_element(),
            );
        }
        if !discovery.providers.is_empty() {
            let body = rows(&discovery.providers, self, window, cx);
            let (title, action) = if has_vcs {
                ("Source control providers".to_owned(), None)
            } else {
                (
                    format!("Source control providers{suffix}"),
                    Some(self.scan_button(cx)),
                )
            };
            sections.push(section_with_action(title, action, body).into_any_element());
        }
        sections
    }

    /// The model rows: a text for the model in use and a switch that turns a dedicated model on.
    fn model_control(
        &self,
        id: &'static str,
        field: Field,
        none: &'static str,
        ctx: &Ctx,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Tokens::of(cx);
        if !ctx.connected {
            return div()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(none)
                .into_any_element();
        }
        let current = self.value(field, ctx);
        let dedicated = current.is_object();
        let base = self.config.text_generation(&ctx.members, ctx.in_project);
        let why = self.gate(field, cx);
        let enabled = dedicated || base.is_some();
        let view = cx.entity();
        let toggle: Toggle = if why.is_some() || !enabled {
            Rc::new(|_, _| {})
        } else {
            Rc::new(move |_, cx| {
                let next = if dedicated {
                    Some(Value::Null)
                } else {
                    base.clone()
                };
                view.update(cx, |this, cx| this.write(vec![(field, next)], cx))
            })
        };
        let toggle = switch(id, dedicated, toggle, window, cx);
        let toggle = if enabled {
            toggle
        } else {
            div().opacity(0.64).child(toggle).into_any_element()
        };
        h_flex()
            .flex_wrap()
            .items_center()
            .justify_end()
            .gap_2()
            .children(sc::model_label(&current).map(|model| {
                div()
                    .max_w(px(220.))
                    .truncate()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(model)
            }))
            .child(Self::inert(id, why, toggle))
            .into_any_element()
    }

    fn writing(&self, ctx: &Ctx, window: &mut Window, cx: &mut Context<Self>) -> gpui::Div {
        let mode = self.text(Field::StyleMode, ctx);
        let custom_why = self.gate(Field::StyleInstructions, cx);
        let style_select = self.select(
            Pick::Style,
            sc::style_index(&json!(mode)),
            224.,
            self.gate(Field::StyleMode, cx),
            cx,
        );
        let custom = (mode == "custom").then(|| {
            self.text_area(
                &self.custom,
                "Custom source control writing instructions",
                custom_why.is_some(),
                window,
                cx,
            )
        });
        let mut rows = vec![
            self.setting(
                "source-control-writing-style",
                "Source control writing style",
                sc::style_description(&mode),
                Field::StyleMode,
                self.reset(
                    "settings-writing-style-reset",
                    "Reset to default",
                    &[Field::StyleMode, Field::StyleInstructions],
                    ctx,
                    window,
                    cx,
                ),
                style_select,
                custom,
                ctx,
                window,
                cx,
            ),
            self.setting(
                "review-guides",
                "Review guides (beta)",
                "Add a Guide tab to pull requests. An agent explains the changes and points out decisions that need review.",
                Field::ReviewGuides,
                None,
                self.flag("settings-review-guides", Field::ReviewGuides, ctx, window, cx),
                None,
                ctx,
                window,
                cx,
            ),
        ];
        if self.value(Field::ReviewGuides, ctx).as_bool() == Some(true) {
            let model = self.model_control(
                "settings-review-guide-model",
                Field::ReviewModel,
                "Connect an environment to choose its review guide model.",
                ctx,
                window,
                cx,
            );
            rows.push(self.setting(
                "review-guide-model",
                "Review guide model",
                "Model that reads a pull request to write its review guide. It reads the change itself over many steps, so a stronger model helps. Off uses the source control writer model.",
                Field::ReviewModel,
                None,
                model,
                None,
                ctx,
                window,
                cx,
            ));
            let area = self.text_area(
                &self.review,
                "Review guide instructions",
                self.gate(Field::ReviewInstructions, cx).is_some(),
                window,
                cx,
            );
            rows.push(self.setting(
                "review-guide-instructions",
                "Review guide instructions",
                "Your preferences for review guides: what to show first, dwell on, or skip. The guide's format and coverage stay the same.",
                Field::ReviewInstructions,
                self.reset(
                    "settings-review-instructions-reset",
                    "Reset to default",
                    &[Field::ReviewInstructions],
                    ctx,
                    window,
                    cx,
                ),
                div().into_any_element(),
                Some(area),
                ctx,
                window,
                cx,
            ));
        }
        let advanced = vec![
            self.setting(
                "follow-change-request-templates",
                "Follow change request templates",
                "Use the repository's template for change request descriptions when available.",
                Field::FollowTemplates,
                self.reset(
                    "settings-follow-templates-reset",
                    "Reset to default",
                    &[Field::FollowTemplates],
                    ctx,
                    window,
                    cx,
                ),
                self.flag(
                    "settings-follow-templates",
                    Field::FollowTemplates,
                    ctx,
                    window,
                    cx,
                ),
                None,
                ctx,
                window,
                cx,
            ),
            {
                let model = self.model_control(
                    "settings-writer-model",
                    Field::WriterModel,
                    "Connect an environment to choose its source control writer model.",
                    ctx,
                    window,
                    cx,
                );
                self.setting(
                    "source-control-writer-model",
                    "Source control writer model",
                    "Model for source control text and branch or bookmark names. When disabled, use the text generation model configured for this environment.",
                    Field::WriterModel,
                    None,
                    model,
                    None,
                    ctx,
                    window,
                    cx,
                )
            },
        ];
        let view = cx.entity();
        rows.push(disclosure(
            "settings-source-control-advanced",
            self.advanced_open,
            Rc::new(move |_, cx| {
                view.update(cx, |this, cx| {
                    this.advanced_open = !this.advanced_open;
                    cx.notify();
                })
            }),
            advanced,
            window,
            cx,
        ));
        section("Writing & review", group(rows, cx))
    }
}

/// The marker id of a row id (a `'static` string per row).
fn marker_id(row: &'static str) -> &'static str {
    match row {
        "automatic-pull" => "settings-automatic-pull-marker",
        "remove-agent-credits-on-merge" => "settings-agent-credits-marker",
        "pull-request-merge-method" => "settings-merge-method-marker",
        "source-control-writing-style" => "settings-writing-style-marker",
        "review-guides" => "settings-review-guides-marker",
        "review-guide-model" => "settings-review-guide-model-marker",
        "review-guide-instructions" => "settings-review-instructions-marker",
        "follow-change-request-templates" => "settings-follow-templates-marker",
        _ => "settings-writer-model-marker",
    }
}

/// `"{label} availability"` switches need distinct ids; the server sends at most a handful.
const ELEMENT_SWITCH: [&str; 10] = [
    "settings-source-control-availability-0",
    "settings-source-control-availability-1",
    "settings-source-control-availability-2",
    "settings-source-control-availability-3",
    "settings-source-control-availability-4",
    "settings-source-control-availability-5",
    "settings-source-control-availability-6",
    "settings-source-control-availability-7",
    "settings-source-control-availability-8",
    "settings-source-control-availability-9",
];

impl Render for SourceControlPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ctx = self.ctx(cx);
        self.sync_inputs(&ctx, window, cx);
        self.sync_hosts(window, cx);
        let mut page = v_flex()
            .w_full()
            .gap_8()
            .on_key_down(cx.listener(|this, event, window, cx| this.key_down(event, window, cx)))
            .child(self.repositories(&ctx, window, cx));
        for part in self.discovery(&ctx, window, cx) {
            page = page.child(part);
        }
        page.child(self.writing(&ctx, window, cx))
    }
}
