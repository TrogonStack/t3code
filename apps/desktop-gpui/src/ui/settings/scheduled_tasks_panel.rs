//! The Scheduled tasks tab's list: `ScheduledTasksSettings.tsx`. Each environment in the scope
//! streams its tasks; a row enables, runs or deletes one. The editor is not part of this tab yet.

use super::components::{group, section, switch};
use super::devices_panel::widgets::{
    Variant, button, dialog_of, icon_button, menu_item, note, problem,
};
use super::scope_inheritance::{PopTrigger, popup_shell};
use super::scope_pickers::ScopeBar;
use super::tokens::Tokens;
use super::*;
use crate::connection::{Action, ActionSender, Call};
use crate::settings_labs::{Environment, Scope, resolve};
use crate::settings_scheduled_tasks::{
    self as tasks, Badge, ProjectsByEnvironment, RunStatus, Task, matches_scope,
};
use crate::status::CallEvent;
use crate::usage_data::Zone;
use gpui::{Anchor, Subscription};
use gpui_component::popover::Popover;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

const DIALOG_WIDTH: f32 = 448.;
const PRIMARY: &str = "primary";

enum Load {
    Loading,
    Failed(String),
    Loaded(Vec<Task>),
}

struct Watch {
    token: u64,
    load: Load,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mutation {
    SetEnabled,
    RunNow,
    Delete,
}

impl Mutation {
    fn verb(self) -> &'static str {
        match self {
            Self::SetEnabled => "update the task",
            Self::RunNow => "run the task",
            Self::Delete => "delete the task",
        }
    }

    fn call(self) -> Call {
        match self {
            Self::SetEnabled => Call::SetScheduledTaskEnabled,
            Self::RunNow => Call::RunScheduledTask,
            Self::Delete => Call::DeleteScheduledTask,
        }
    }
}

enum Pending {
    Watch(String),
    Mutation {
        environment: String,
        id: String,
        kind: Mutation,
    },
}

type Key = (String, String);

pub struct ScheduledTasksPanel {
    actions: ActionSender,
    scope: Entity<ScopeBar>,
    wide: bool,
    zone: Zone,
    watches: HashMap<String, Watch>,
    pending: HashMap<u64, Pending>,
    busy: HashSet<Key>,
    next_token: u64,
    projects: ProjectsByEnvironment,
    menu: Option<Key>,
    confirm: Option<Key>,
    error: Option<String>,
    _subscriptions: Vec<Subscription>,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as i64)
}

impl ScheduledTasksPanel {
    pub fn new(actions: ActionSender, scope: Entity<ScopeBar>, cx: &mut Context<Self>) -> Self {
        let subscriptions = vec![cx.observe(&scope, |_, _, cx| cx.notify())];
        Self {
            actions,
            scope,
            wide: true,
            zone: Zone::detect(),
            watches: HashMap::new(),
            pending: HashMap::new(),
            busy: HashSet::new(),
            next_token: tasks::TOKEN_BASE,
            projects: ProjectsByEnvironment::new(),
            menu: None,
            confirm: None,
            error: None,
            _subscriptions: subscriptions,
        }
    }

    /// Starts the streams the scope needs while the tab shows.
    pub fn set_wide(&mut self, wide: bool, cx: &mut Context<Self>) {
        if self.wide != wide {
            self.wide = wide;
            cx.notify();
        }
        self.watch_scope(cx);
    }

    pub fn set_projects(&mut self, projects: ProjectsByEnvironment, cx: &mut Context<Self>) {
        if self.projects != projects {
            self.projects = projects;
            cx.notify();
        }
    }

    /// Drops what the streams and mutations answered, as the page opens or the connection restarts.
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        self.watches.clear();
        self.pending.clear();
        self.busy.clear();
        self.menu = None;
        self.confirm = None;
        self.error = None;
        cx.notify();
    }

    /// Closes the menu, then the confirmation; returns whether there was one, so Escape stops there.
    pub fn close_menu(&mut self, cx: &mut Context<Self>) -> bool {
        if self.menu.take().is_some() || self.confirm.take().is_some() {
            cx.notify();
            return true;
        }
        false
    }

    fn resolved(&self, cx: &App) -> (Scope, Vec<Environment>, Option<String>) {
        let bar = self.scope.read(cx);
        let scope = resolve(bar.search(), bar.data());
        let environments = tasks::environments_in_scope(&scope, bar.data())
            .into_iter()
            .cloned()
            .collect();
        (scope, environments, bar.search().project.clone())
    }

    fn sender(&self, environment: &str) -> Option<ActionSender> {
        if environment == PRIMARY {
            Some(self.actions.clone())
        } else {
            crate::environments::sender(environment)
        }
    }

    fn send(&mut self, environment: &str, call: Call, payload: Value, pending: Pending) -> bool {
        let Some(sender) = self.sender(environment) else {
            return false;
        };
        let token = self.next_token;
        self.next_token += 1;
        let action = Action::Call {
            token,
            call,
            payload,
        };
        if sender.try_send(action).is_err() {
            return false;
        }
        self.pending.insert(token, pending);
        true
    }

    fn watch_scope(&mut self, cx: &mut Context<Self>) {
        let (_, environments, _) = self.resolved(cx);
        let mut changed = false;
        for environment in &environments {
            if !environment.connected {
                changed |= self.watches.remove(&environment.id).is_some();
                continue;
            }
            if self.watches.contains_key(&environment.id) {
                continue;
            }
            let token = self.next_token;
            let started = self.send(
                &environment.id,
                Call::WatchScheduledTasks,
                json!({}),
                Pending::Watch(environment.id.clone()),
            );
            if started {
                self.watches.insert(
                    environment.id.clone(),
                    Watch {
                        token,
                        load: Load::Loading,
                    },
                );
                changed = true;
            }
        }
        if changed {
            cx.notify();
        }
    }

    pub fn on_call(&mut self, token: u64, event: CallEvent, cx: &mut Context<Self>) {
        let Some(pending) = self.pending.get(&token) else {
            return;
        };
        match pending {
            Pending::Watch(environment) => {
                let environment = environment.clone();
                match event {
                    CallEvent::Progress(value) => {
                        self.set_load(
                            &environment,
                            token,
                            Load::Loaded(tasks::decode_tasks(&value)),
                        );
                    }
                    CallEvent::Failed(message) => {
                        self.pending.remove(&token);
                        self.set_load(&environment, token, Load::Failed(message));
                    }
                    CallEvent::Done(_) => {
                        self.pending.remove(&token);
                        if !matches!(
                            self.watches.get(&environment).map(|w| &w.load),
                            Some(Load::Loaded(_))
                        ) {
                            self.set_load(
                                &environment,
                                token,
                                Load::Failed("The connection closed.".to_owned()),
                            );
                        }
                    }
                }
            }
            Pending::Mutation {
                environment,
                id,
                kind,
            } => {
                let (environment, id, kind) = (environment.clone(), id.clone(), *kind);
                if matches!(event, CallEvent::Progress(_)) {
                    return;
                }
                self.pending.remove(&token);
                self.busy.remove(&(environment.clone(), id.clone()));
                match event {
                    CallEvent::Failed(message) => {
                        self.error = Some(format!("Could not {}: {message}", kind.verb()));
                    }
                    CallEvent::Done(value) => self.apply(&environment, &id, kind, &value),
                    CallEvent::Progress(_) => {}
                }
            }
        }
        cx.notify();
    }

    fn set_load(&mut self, environment: &str, token: u64, load: Load) {
        if let Some(watch) = self.watches.get_mut(environment)
            && watch.token == token
        {
            watch.load = load;
        }
    }

    /// Shows a mutation's outcome at once; the stream confirms it.
    fn apply(&mut self, environment: &str, id: &str, kind: Mutation, value: &Value) {
        let Some(Watch {
            load: Load::Loaded(list),
            ..
        }) = self.watches.get_mut(environment)
        else {
            return;
        };
        match kind {
            Mutation::Delete => list.retain(|task| task.id != id),
            Mutation::SetEnabled | Mutation::RunNow => {
                let updated = value.get("task").and_then(tasks::decode_task);
                if let (Some(updated), Some(slot)) =
                    (updated, list.iter_mut().find(|task| task.id == id))
                {
                    *slot = updated;
                }
            }
        }
    }

    fn mutate(
        &mut self,
        environment: &str,
        id: &str,
        kind: Mutation,
        payload: Value,
        cx: &mut Context<Self>,
    ) {
        let key = (environment.to_owned(), id.to_owned());
        if self.busy.contains(&key) {
            return;
        }
        self.error = None;
        let pending = Pending::Mutation {
            environment: environment.to_owned(),
            id: id.to_owned(),
            kind,
        };
        if self.send(environment, kind.call(), payload, pending) {
            self.busy.insert(key);
        } else {
            self.error = Some(format!(
                "Could not {}: the environment is not connected.",
                kind.verb()
            ));
        }
        cx.notify();
    }

    fn toggle(&mut self, environment: &str, task: &Task, cx: &mut Context<Self>) {
        let payload = json!({ "id": task.id, "enabled": !task.enabled });
        self.mutate(environment, &task.id, Mutation::SetEnabled, payload, cx);
    }

    fn run_now(&mut self, environment: &str, id: &str, cx: &mut Context<Self>) {
        self.mutate(environment, id, Mutation::RunNow, json!({ "id": id }), cx);
    }

    fn delete(&mut self, environment: &str, id: &str, cx: &mut Context<Self>) {
        self.confirm = None;
        self.mutate(environment, id, Mutation::Delete, json!({ "id": id }), cx);
    }

    fn message_row(title: String, description: Option<String>, cx: &App) -> AnyElement {
        let theme = Tokens::of(cx);
        v_flex()
            .px_4()
            .py_3()
            .gap_1()
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.foreground)
                    .child(title),
            )
            .children(description.map(|text| note(text, cx).max_w(px(672.)).line_height(px(18.))))
            .into_any_element()
    }

    fn line_row(text: String, destructive: bool, cx: &App) -> AnyElement {
        let line = if destructive {
            problem(text, cx)
        } else {
            note(text, cx)
        };
        div().px_4().py_2().child(line).into_any_element()
    }

    fn environment_header(environment: &Environment, cx: &App) -> AnyElement {
        let theme = Tokens::of(cx);
        let (dot, status) = if environment.connected {
            (gpui::rgb(0x22c55e).into(), "Connected")
        } else {
            (theme.muted_foreground.opacity(0.5), "Offline")
        };
        let subtitle = if environment.id == PRIMARY {
            "This machine".to_owned()
        } else {
            environment.address.clone()
        };
        h_flex()
            .px_4()
            .py_2()
            .gap_2()
            .items_center()
            .bg(theme.muted.opacity(0.3))
            .child(
                Icon::new(crate::ui::machine_icon(environment.machine))
                    .size(px(14.))
                    .text_color(theme.muted_foreground),
            )
            .child(
                h_flex()
                    .min_w_0()
                    .flex_1()
                    .gap_2()
                    .items_baseline()
                    .child(
                        div()
                            .truncate()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.foreground)
                            .child(environment.label.clone()),
                    )
                    .child(div().truncate().child(note(subtitle, cx))),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .gap_1()
                    .items_center()
                    .child(div().size(px(6.)).rounded_full().bg(dot))
                    .child(note(status, cx)),
            )
            .into_any_element()
    }

    fn badge(status: RunStatus, kind: Badge, cx: &App) -> AnyElement {
        let theme = Tokens::of(cx);
        let color = match kind {
            Badge::Error => theme.destructive,
            Badge::Success => gpui::rgb(if theme.dark { 0x4ade80 } else { 0x15803d }).into(),
            Badge::Info => gpui::rgb(if theme.dark { 0x60a5fa } else { 0x1d4ed8 }).into(),
            Badge::Outline => theme.muted_foreground,
        };
        div()
            .flex_shrink_0()
            .px(px(6.))
            .h(px(18.))
            .flex()
            .items_center()
            .rounded(px(6.))
            .border_1()
            .border_color(if kind == Badge::Outline {
                theme.border
            } else {
                color.opacity(0.4)
            })
            .bg(if kind == Badge::Outline {
                gpui::transparent_black()
            } else {
                color.opacity(0.1)
            })
            .text_size(px(11.))
            .text_color(color)
            .child(status.label())
            .into_any_element()
    }

    fn task_row(
        &self,
        environment: &str,
        task: &Task,
        index: usize,
        now: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Tokens::of(cx);
        let key: Key = (environment.to_owned(), task.id.clone());
        let busy = self.busy.contains(&key);
        let view = cx.entity();
        let badge = tasks::badge(task.last_run_status)
            .map(|kind| Self::badge(task.last_run_status, kind, cx));
        let text = v_flex()
            .min_w_0()
            .flex_1()
            .gap_1()
            .child(
                div()
                    .min_h(px(20.))
                    .truncate()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.foreground)
                    .child(task.title.clone()),
            )
            .when(!task.prompt.is_empty(), |column| {
                column.child(
                    div()
                        .max_w(px(672.))
                        .line_clamp(2)
                        .text_xs()
                        .line_height(px(18.))
                        .text_color(theme.muted_foreground)
                        .child(task.prompt.clone()),
                )
            })
            .child(
                div()
                    .max_w(px(672.))
                    .text_xs()
                    .line_height(px(18.))
                    .text_color(theme.muted_foreground)
                    .child(tasks::summary_line(task, now, &self.zone)),
            )
            .when(badge.is_some() || task.last_run_error.is_some(), |column| {
                column.child(
                    h_flex()
                        .min_w_0()
                        .gap_2()
                        .items_center()
                        .children(badge)
                        .children(
                            task.last_run_error
                                .clone()
                                .map(|error| div().min_w_0().truncate().child(problem(error, cx))),
                        ),
                )
            });
        let toggle = {
            let (view, environment, toggled) = (view.clone(), environment.to_owned(), task.clone());
            let on_toggle = Rc::new(move |_: &mut Window, cx: &mut App| {
                view.update(cx, |this, cx| this.toggle(&environment, &toggled, cx))
            });
            div()
                .when(busy, |control| control.opacity(0.64))
                .child(switch(
                    ("scheduled-task-enabled", index),
                    task.enabled,
                    on_toggle,
                    window,
                    cx,
                ))
        };
        let more = icon_button(
            ("scheduled-task-more", index),
            Variant::GhostMuted,
            !busy,
            cx,
        )
        .child(
            Icon::new(Icons::Ellipsis)
                .size(px(16.))
                .rotate(gpui::radians(std::f32::consts::FRAC_PI_2)),
        );
        let menu = {
            let opened = view.clone();
            let (open_key, change_key) = (key.clone(), key.clone());
            let (environment, id, webhook) =
                (environment.to_owned(), task.id.clone(), task.is_webhook());
            Popover::new(("scheduled-task-menu", index))
                .anchor(Anchor::TopRight)
                .offset(px(4.))
                .open(self.menu.as_ref() == Some(&open_key) && !busy)
                .p_0()
                .appearance(false)
                .on_open_change(move |next, _, cx| {
                    let key = change_key.clone();
                    opened.update(cx, |this, cx| {
                        this.menu = next.then_some(key);
                        cx.notify();
                    })
                })
                .trigger(PopTrigger::new(more))
                .content(move |_, _, cx| {
                    let mut menu = popup_shell(cx).min_w(px(160.)).p_1();
                    if !webhook {
                        let (view, environment, id) =
                            (view.clone(), environment.clone(), id.clone());
                        menu = menu.child(
                            menu_item(("scheduled-task-run", index), "Run now", false, cx)
                                .on_click(move |_, _, cx| {
                                    view.update(cx, |this, cx| {
                                        this.menu = None;
                                        this.run_now(&environment, &id, cx)
                                    })
                                }),
                        );
                    }
                    let (view, key) = (view.clone(), (environment.clone(), id.clone()));
                    menu.child(
                        menu_item(("scheduled-task-delete", index), "Delete", true, cx).on_click(
                            move |_, _, cx| {
                                let key = key.clone();
                                view.update(cx, |this, cx| {
                                    this.menu = None;
                                    this.confirm = Some(key);
                                    cx.notify();
                                })
                            },
                        ),
                    )
                    .into_any_element()
                })
        };
        let controls = h_flex()
            .flex_shrink_0()
            .gap_2()
            .items_center()
            .child(toggle)
            .child(menu);
        let inner = if self.wide {
            h_flex()
                .items_start()
                .gap(px(32.))
                .child(text)
                .child(controls)
        } else {
            h_flex().items_start().gap_3().child(text).child(controls)
        };
        div().px_4().py_3().child(inner).into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn environment_rows(
        &self,
        environment: &Environment,
        scope: &Scope,
        project: Option<&str>,
        multiple: bool,
        index: &mut usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let label = environment.label.clone();
        let mut rows = Vec::new();
        if multiple {
            rows.push(Self::environment_header(environment, cx));
        }
        let message = |text: String, destructive: bool, cx: &App| {
            if multiple {
                Self::line_row(text, destructive, cx)
            } else {
                Self::message_row(text, None, cx)
            }
        };
        if !environment.connected {
            if multiple {
                rows.push(message(
                    format!("Not connected: {label} (Offline)"),
                    false,
                    cx,
                ));
            } else {
                rows.push(Self::message_row(
                    "Environment disconnected".to_owned(),
                    Some(format!("Reconnect {label} to view its scheduled tasks.")),
                    cx,
                ));
            }
            return rows;
        }
        match self.watches.get(&environment.id).map(|watch| &watch.load) {
            None | Some(Load::Loading) => {
                rows.push(message(
                    format!("Loading scheduled tasks on {label}"),
                    false,
                    cx,
                ));
            }
            Some(Load::Failed(error)) => {
                rows.push(message(
                    format!("Could not load scheduled tasks: {error}"),
                    true,
                    cx,
                ));
            }
            Some(Load::Loaded(list)) => {
                let now = now_ms();
                let shown: Vec<&Task> = list
                    .iter()
                    .filter(|task| {
                        matches_scope(scope, project, &environment.id, task, &self.projects)
                    })
                    .collect();
                if shown.is_empty() {
                    rows.push(if multiple {
                        message(format!("No tasks on {label}"), false, cx)
                    } else if list.is_empty() {
                        Self::message_row("No scheduled tasks".to_owned(), None, cx)
                    } else {
                        Self::message_row(
                            "No tasks match this environment and project selection.".to_owned(),
                            None,
                            cx,
                        )
                    });
                }
                for task in shown {
                    *index += 1;
                    rows.push(self.task_row(&environment.id, task, *index, now, window, cx));
                }
            }
        }
        rows
    }

    fn confirm_view(&self, view: &Entity<Self>, key: &Key, cx: &App) -> Option<AnyElement> {
        let Some(Watch {
            load: Load::Loaded(list),
            ..
        }) = self.watches.get(&key.0)
        else {
            return None;
        };
        let task = list.iter().find(|task| task.id == key.1)?;
        let theme = Tokens::of(cx);
        let (environment, id) = key.clone();
        let (cancel, confirm, dismiss) = (view.clone(), view.clone(), view.clone());
        let destructive = theme.destructive;
        let footer = vec![
            button("scheduled-task-delete-cancel", Variant::Outline, true, cx)
                .child("Cancel")
                .on_click(move |_, _, cx| {
                    cancel.update(cx, |this, cx| {
                        this.confirm = None;
                        cx.notify();
                    })
                })
                .into_any_element(),
            button("scheduled-task-delete-confirm", Variant::Ghost, true, cx)
                .bg(destructive)
                .border_color(destructive)
                .text_color(gpui::white())
                .hover(move |style| style.bg(destructive.opacity(0.9)))
                .child("Delete task")
                .on_click(move |_, _, cx| {
                    let (environment, id) = (environment.clone(), id.clone());
                    confirm.update(cx, |this, cx| this.delete(&environment, &id, cx))
                })
                .into_any_element(),
        ];
        Some(dialog_of(
            "scheduled-task-delete",
            DIALOG_WIDTH,
            format!("Delete {}?", task.title),
            "This removes the task and its schedule. Threads from earlier runs stay.".to_owned(),
            Vec::new(),
            footer,
            move |_, cx| {
                dismiss.update(cx, |this, cx| {
                    this.confirm = None;
                    cx.notify();
                })
            },
            cx,
        ))
    }

    /// The delete confirmation, over a backdrop.
    pub fn overlay(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let key = self.confirm.clone()?;
        let view = cx.entity();
        let element = self.confirm_view(&view, &key, cx);
        if element.is_none() {
            self.confirm = None;
        }
        element
    }
}

impl Render for ScheduledTasksPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (scope, environments, project) = self.resolved(cx);
        let mut rows: Vec<AnyElement> = Vec::new();
        if let Some(error) = self.error.clone() {
            rows.push(Self::line_row(error, true, cx));
        }
        if let Scope::Unavailable(message) = &scope {
            rows.push(Self::message_row((*message).to_owned(), None, cx));
        } else if environments.is_empty() {
            rows.push(Self::message_row(
                "No environments available".to_owned(),
                Some("Connect an environment to manage scheduled tasks.".to_owned()),
                cx,
            ));
        } else {
            let multiple = environments.len() > 1;
            let mut index = 0;
            for environment in &environments {
                rows.extend(self.environment_rows(
                    environment,
                    &scope,
                    project.as_deref(),
                    multiple,
                    &mut index,
                    window,
                    cx,
                ));
            }
        }
        section("Scheduled tasks", group(rows, cx))
    }
}
