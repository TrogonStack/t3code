//! The Projects tab: `ProjectsSettings.tsx` (the list under "All projects") and
//! `ProjectSettingsPanel.tsx` (Name, Location, Project icon, Pin, Archive, New threads, Actions,
//! Checkouts and Danger). The icon picker dialogs are not here: the buttons show and stay off.
//! Writes go out as `project.update` and `project.delete` per member, and as settings patches
//! for the New threads and Actions overrides; the streams bring the result back.

use super::components::{group, reset_button_labeled, row, section, switch, tab_stop};
use super::devices_panel::widgets::{Variant, button, dialog_of, note, problem};
use super::scope_pickers::ScopeBar;
use super::secret_field::sm_field;
use super::tokens::Tokens;
use super::*;
use crate::connection::{Action, ActionSender, Call, command_id};
use crate::projects::{MachineKind, Member, ProjectEntry};
use crate::providers::Catalog;
use crate::settings::{DeviceSettings, GroupingMode};
use crate::settings_projects::{self as model};
use crate::settings_source_control::{Field, SourceControlConfig};
use crate::status::CallEvent;
use gpui::{PathPromptOptions, Subscription};
use gpui_component::input::{InputEvent, InputState, TextareaState};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::rc::Rc;

mod sections;

/// What the tab reads from the connection, handed over whenever it changes.
#[derive(Clone, PartialEq)]
pub struct ProjectsData {
    pub live: Vec<ProjectEntry>,
    pub archived: Vec<ProjectEntry>,
    pub can_organize: bool,
    pub environment: String,
    pub environment_id: String,
    pub machine: MachineKind,
    /// The project of every listed thread, for the removal's thread count.
    pub thread_projects: Vec<String>,
    pub config: SourceControlConfig,
    pub catalog: Option<Catalog>,
}

/// The select that is open, and the checkout it belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MenuKind {
    Workspace,
    Grouping(usize),
}

/// The action editor: the switches and icon the form holds besides its three text inputs.
struct Editor {
    /// The id of the action being edited; `None` adds one.
    editing: Option<String>,
    form: model::Script,
    problem: Option<&'static str>,
}

/// Members still to remove, one `project.delete` at a time as the web does.
struct Removal {
    queue: VecDeque<Member>,
    current: Option<(u64, String)>,
    whole_group: bool,
}

const DIALOG_WIDTH: f32 = 512.;
const INFO: &str =
    "Can't find a setting? Keep this project picked above and hop to any other settings page.";
const CHOOSE: &str = "Choose a project to manage its name, icon, checkouts and actions.";

pub struct ProjectsPanel {
    actions: ActionSender,
    scope: Entity<ScopeBar>,
    wide: bool,
    live: Vec<ProjectEntry>,
    archived: Vec<ProjectEntry>,
    can_organize: bool,
    environment: String,
    environment_id: String,
    machine: MachineKind,
    /// The project of every listed thread, for the removal's thread count.
    thread_projects: Vec<String>,
    config: SourceControlConfig,
    catalog: Option<Catalog>,
    /// Client-side grouping rules by physical key, and the global rule they fall back to.
    grouping: BTreeMap<String, GroupingMode>,
    global_grouping: GroupingMode,
    /// Settings writes in flight, with what to put back and the failure title.
    saves: HashMap<u64, (SourceControlConfig, &'static str)>,
    menu: Option<(MenuKind, usize)>,
    editor: Option<Editor>,
    script_name: Entity<InputState>,
    script_preview: Entity<InputState>,
    script_command: Entity<TextareaState>,
    /// The action the delete confirmation asks about: its id and name.
    delete_script: Option<(String, String)>,
    name: Entity<InputState>,
    /// The group and title the name input shows.
    seeded: Option<(String, String)>,
    /// Writes in flight, with the failure title each reports.
    pending: HashMap<u64, &'static str>,
    removal: Option<Removal>,
    error: Option<String>,
    /// The members the removal confirmation asks about.
    confirm: Option<Vec<Member>>,
    next_token: u64,
    _subscriptions: Vec<Subscription>,
}

impl ProjectsPanel {
    pub fn new(
        actions: ActionSender,
        scope: Entity<ScopeBar>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("Project name"));
        let script_name = cx.new(|cx| InputState::new(window, cx).placeholder("Test"));
        let script_preview =
            cx.new(|cx| InputState::new(window, cx).placeholder("http://localhost:5173"));
        let script_command = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("bun test")
                .auto_grow(3, 6)
        });
        let subscriptions = vec![
            cx.observe(&scope, |_, _, cx| cx.notify()),
            cx.subscribe_in(
                &name,
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
            live: Vec::new(),
            archived: Vec::new(),
            can_organize: false,
            environment: String::new(),
            environment_id: String::new(),
            machine: MachineKind::Desktop,
            thread_projects: Vec::new(),
            config: SourceControlConfig::default(),
            catalog: None,
            grouping: DeviceSettings::grouping_overrides(),
            global_grouping: DeviceSettings::load().project_grouping,
            saves: HashMap::new(),
            menu: None,
            editor: None,
            script_name,
            script_preview,
            script_command,
            delete_script: None,
            name,
            seeded: None,
            pending: HashMap::new(),
            removal: None,
            error: None,
            confirm: None,
            next_token: model::TOKEN_BASE,
            _subscriptions: subscriptions,
        }
    }

    pub fn set_wide(&mut self, wide: bool, cx: &mut Context<Self>) {
        if self.wide != wide {
            self.wide = wide;
            cx.notify();
        }
    }

    pub fn set_data(&mut self, data: ProjectsData, cx: &mut Context<Self>) {
        let ProjectsData {
            live,
            archived,
            can_organize,
            environment,
            environment_id,
            machine,
            thread_projects,
            config,
            catalog,
        } = data;
        let mut changed = false;
        macro_rules! take {
            ($field:ident, $value:expr) => {
                if self.$field != $value {
                    self.$field = $value;
                    changed = true;
                }
            };
        }
        take!(live, live);
        take!(archived, archived);
        take!(can_organize, can_organize);
        take!(environment, environment);
        take!(environment_id, environment_id);
        take!(machine, machine);
        take!(thread_projects, thread_projects);
        take!(catalog, catalog);
        if self.saves.is_empty() {
            take!(config, config);
        }
        if changed {
            cx.notify();
        }
    }

    pub fn reload(&mut self, cx: &mut Context<Self>) {
        self.confirm = None;
        self.editor = None;
        self.delete_script = None;
        self.menu = None;
        self.error = None;
        self.seeded = None;
        self.grouping = DeviceSettings::grouping_overrides();
        self.global_grouping = DeviceSettings::load().project_grouping;
        cx.notify();
    }

    /// Closes the innermost open layer (a select, a confirmation, the editor); returns whether
    /// there was one, so Escape stops there.
    pub fn close_menu(&mut self, cx: &mut Context<Self>) -> bool {
        let closed = self.menu.take().is_some()
            || self.confirm.take().is_some()
            || self.delete_script.take().is_some()
            || self.editor.take().is_some();
        if closed {
            cx.notify();
        }
        closed
    }

    /// Every listed project but the scratch one: the live groups, then the archived.
    fn groups(&self) -> impl Iterator<Item = &ProjectEntry> {
        self.live
            .iter()
            .chain(&self.archived)
            .filter(|entry| !entry.scratch)
    }

    fn selected(&self, cx: &App) -> Option<&ProjectEntry> {
        let key = self.scope.read(cx).search().project.clone()?;
        self.groups().find(|entry| entry.key == key)
    }

    fn send(&mut self, project_id: &str, payload: Value, failure: &str) -> Option<u64> {
        let token = self.next_token;
        self.next_token += 1;
        let action = Action::Call {
            token,
            call: Call::ProjectsMutate,
            payload,
        };
        match self.actions.try_send(action) {
            Ok(()) => Some(token),
            Err(_) => {
                self.error = Some(format!("{failure}: could not send the request"));
                eprintln!("[projects] could not send the update for {project_id}");
                None
            }
        }
    }

    /// One `project.update` per member, the web's `updateProjectGroup`.
    fn update(
        &mut self,
        members: &[Member],
        fields: Value,
        failure: &'static str,
        cx: &mut Context<Self>,
    ) {
        self.error = None;
        for member in members {
            let payload = model::update_payload(&member.id, command_id(), fields.clone());
            if let Some(token) = self.send(&member.id, payload, failure) {
                self.pending.insert(token, failure);
            }
        }
        cx.notify();
    }

    pub fn on_call(&mut self, token: u64, event: CallEvent, cx: &mut Context<Self>) {
        if let Some((previous, failure)) = self.saves.remove(&token) {
            if let CallEvent::Failed(message) = event {
                self.config = previous;
                if self.error.is_none() {
                    self.error = Some(format!("{failure}: {message}"));
                }
            } else if matches!(event, CallEvent::Progress(_)) {
                self.saves.insert(token, (previous, failure));
                return;
            }
            cx.notify();
            return;
        }
        if self
            .removal
            .as_ref()
            .and_then(|removal| removal.current.as_ref())
            .is_some_and(|(current, _)| *current == token)
        {
            self.on_removal(event, cx);
            return;
        }
        let Some(failure) = self.pending.get(&token).copied() else {
            return;
        };
        match event {
            CallEvent::Progress(_) => return,
            CallEvent::Done(_) => {}
            CallEvent::Failed(message) => {
                if self.error.is_none() {
                    self.error = Some(format!("{failure}: {message}"));
                }
            }
        }
        self.pending.remove(&token);
        cx.notify();
    }

    /// One removal answered: the next member goes out, or the removal ends (the web stops at the
    /// first failure and leaves the settings page for the list only when the whole group went).
    fn on_removal(&mut self, event: CallEvent, cx: &mut Context<Self>) {
        let title = self
            .removal
            .as_ref()
            .and_then(|removal| removal.current.as_ref())
            .map(|(_, title)| title.clone())
            .unwrap_or_default();
        match event {
            CallEvent::Progress(_) => return,
            CallEvent::Done(_) => self.next_removal(cx),
            CallEvent::Failed(message) => {
                self.removal = None;
                self.error = Some(format!("Failed to remove \"{title}\": {message}"));
            }
        }
        cx.notify();
    }

    fn next_removal(&mut self, cx: &mut Context<Self>) {
        let Some(removal) = self.removal.as_mut() else {
            return;
        };
        let Some(member) = removal.queue.pop_front() else {
            let whole_group = removal.whole_group;
            self.removal = None;
            if whole_group {
                self.scope
                    .update(cx, |bar, cx| bar.select_project("all", cx));
            }
            return;
        };
        let payload = model::delete_payload(&member.id, command_id());
        let failure = format!("Failed to remove \"{}\"", member.title);
        match self.send(&member.id, payload, &failure) {
            Some(token) => {
                if let Some(removal) = self.removal.as_mut() {
                    removal.current = Some((token, member.title));
                }
            }
            None => self.removal = None,
        }
    }

    fn commit_name(&mut self, cx: &mut Context<Self>) {
        let Some(entry) = self.selected(cx).cloned() else {
            return;
        };
        let value = self.name.read(cx).value().to_string();
        match model::title_to_write(&entry.members, &value) {
            Err(message) => {
                self.error = Some(message.to_owned());
                cx.notify();
            }
            Ok(None) => {}
            Ok(Some(title)) => {
                self.update(
                    &entry.members,
                    json!({ "title": title }),
                    "Failed to rename project",
                    cx,
                );
            }
        }
    }

    fn change_location(&mut self, member: Member, window: &mut Window, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Change location".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = picked.await;
            this.update(cx, |this, cx| match result {
                Ok(Ok(Some(paths))) => {
                    if let Some(path) = paths.first() {
                        this.update(
                            std::slice::from_ref(&member),
                            json!({ "workspaceRoot": path.to_string_lossy() }),
                            "Failed to change project location",
                            cx,
                        );
                    }
                }
                Ok(Err(error)) => {
                    this.error = Some(format!("Failed to change project location: {error}"));
                    cx.notify();
                }
                _ => {}
            })
            .ok();
        })
        .detach();
    }

    /// Removes the confirmed members, `removeMembers`: one `project.delete` with `force` each.
    fn remove(&mut self, cx: &mut Context<Self>) {
        let Some(members) = self.confirm.take() else {
            return;
        };
        let Some(entry) = self.selected(cx).cloned() else {
            return;
        };
        self.error = None;
        self.removal = Some(Removal {
            whole_group: members.len() == entry.members.len(),
            queue: members.into(),
            current: None,
        });
        self.next_removal(cx);
        cx.notify();
    }

    fn seed_name(&mut self, entry: &ProjectEntry, window: &mut Window, cx: &mut Context<Self>) {
        let seed = (entry.key.clone(), entry.title.clone());
        if self.seeded.as_ref() == Some(&seed) {
            return;
        }
        let editing = self.name.read(cx).focus_handle(cx).is_focused(window);
        if editing && self.seeded.as_ref().is_some_and(|(key, _)| *key == seed.0) {
            return;
        }
        self.name.update(cx, |state, cx| {
            state.set_value(entry.title.clone(), window, cx)
        });
        self.seeded = Some(seed);
    }

    fn list_row(&self, entry: &ProjectEntry, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let theme = Tokens::of(cx);
        let mono = cx.theme().mono_font_family.clone();
        let color: gpui::Hsla =
            gpui::rgb(crate::settings_labs::avatar_color(entry.color, theme.dark)).into();
        let key = entry.key.clone();
        let hover = theme.accent;
        let root = if entry.archived {
            format!("Archived \u{b7} {}", entry.root)
        } else {
            entry.root.clone()
        };
        h_flex()
            .id(("settings-project-row", index))
            .items_center()
            .gap_3()
            .px_4()
            .py_3()
            .cursor_pointer()
            .hover(move |style| style.bg(hover.opacity(0.5)))
            .child(monogram(&entry.initials, color, 24., &mono))
            .child(
                v_flex()
                    .min_w_0()
                    .flex_1()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(14.))
                            .font_weight(FontWeight::MEDIUM)
                            .child(entry.title.clone()),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(root),
                    ),
            )
            .child(
                Icon::new(Icons::ChevronRight)
                    .size(px(16.))
                    .text_color(theme.muted_foreground),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.scope
                    .update(cx, |bar, cx| bar.select_project(&key, cx));
            }))
            .into_any_element()
    }

    fn callout(text: &'static str, cx: &App) -> AnyElement {
        let theme = Tokens::of(cx);
        h_flex()
            .items_start()
            .gap_3()
            .rounded(px(8.))
            .border_1()
            .border_color(theme.border)
            .px_3()
            .py_2()
            .child(
                Icon::new(Icons::Info)
                    .size(px(16.))
                    .mt(px(2.))
                    .text_color(theme.muted_foreground),
            )
            .child(div().text_size(px(14.)).child(text))
            .into_any_element()
    }

    fn detail(
        &mut self,
        entry: ProjectEntry,
        editable: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.seed_name(&entry, window, cx);
        let theme = Tokens::of(cx);
        let wide = self.wide;
        let members = entry.members.clone();
        let single = (members.len() == 1).then(|| members[0].clone());
        let organize = editable && self.can_organize;
        let pinned = members.iter().any(|m| m.pinned);
        let archived = !members.is_empty() && members.iter().all(|m| m.archived);
        let view = cx.entity();

        let name_field = div()
            .w(px(256.))
            .child(sm_field(&self.name, false, false, !editable, window, cx));
        let mut rows = vec![
            row(
                "Name",
                "The shared name for this project group in the sidebar and thread lists.",
                None,
                name_field.into_any_element(),
                wide,
                cx,
            )
            .into_any_element(),
        ];

        if let Some(member) = single.clone() {
            let change = button("project-location-change", Variant::Outline, editable, cx)
                .child("Change...")
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.change_location(member.clone(), window, cx)
                }));
            rows.push(
                row(
                    "Location",
                    div().child(entry.root.clone()),
                    None,
                    change.into_any_element(),
                    wide,
                    cx,
                )
                .into_any_element(),
            );
        }

        let icon_reset =
            (editable && members.iter().any(|m| !model::has_default_icon(m))).then(|| {
                let (view, members) = (view.clone(), members.clone());
                reset_button_labeled(
                    "project-icon-reset",
                    "Reset to automatic",
                    move |_, _, cx| {
                        let members = members.clone();
                        view.update(cx, |this, cx| {
                            this.update(
                                &members,
                                json!({ "faviconPath": null, "projectIcon": null }),
                                "Failed to update project icon",
                                cx,
                            )
                        })
                    },
                    window,
                    cx,
                )
            });
        let mono = cx.theme().mono_font_family.clone();
        let color: gpui::Hsla =
            gpui::rgb(crate::settings_labs::avatar_color(entry.color, theme.dark)).into();
        let icon_description = members
            .first()
            .map_or_else(|| "Automatic".to_owned(), model::icon_description);
        let icon_controls = h_flex()
            .gap_2()
            .items_center()
            .child(monogram(&entry.initials, color, 24., &mono))
            .child(button("project-icon-choose", Variant::Outline, false, cx).child("Choose icon"))
            .child(button("project-file-choose", Variant::Outline, false, cx).child("Choose file"));
        rows.push(
            row(
                "Project icon",
                icon_description,
                icon_reset,
                icon_controls.into_any_element(),
                wide,
                cx,
            )
            .into_any_element(),
        );

        if organize {
            let toggle = {
                let (view, members) = (view.clone(), members.clone());
                Rc::new(move |_: &mut Window, cx: &mut App| {
                    let members = members.clone();
                    view.update(cx, |this, cx| {
                        let next = !pinned;
                        this.update(
                            &members,
                            json!({ "pinned": next }),
                            if next {
                                "Failed to pin project"
                            } else {
                                "Failed to unpin project"
                            },
                            cx,
                        )
                    })
                })
            };
            rows.push(
                row(
                    "Pin",
                    "Pinned projects come first in project pickers.",
                    None,
                    switch("project-pin", pinned, toggle, window, cx),
                    wide,
                    cx,
                )
                .into_any_element(),
            );
            let members_for_archive = members.clone();
            let archive = button("project-archive", Variant::Outline, true, cx)
                .child(if archived { "Unarchive" } else { "Archive" })
                .on_click(cx.listener(move |this, _, _, cx| {
                    let next = !archived;
                    this.update(
                        &members_for_archive,
                        json!({ "archived": next }),
                        if next {
                            "Failed to archive project"
                        } else {
                            "Failed to unarchive project"
                        },
                        cx,
                    )
                }));
            rows.push(
                row(
                    "Archive",
                    if archived {
                        "Hidden from project pickers. Threads and files are kept."
                    } else {
                        "Hides the project from project pickers. Threads and files are kept."
                    },
                    None,
                    archive.into_any_element(),
                    wide,
                    cx,
                )
                .into_any_element(),
            );
        }

        let mut page = v_flex().gap_8();
        page = page.child(Self::callout(INFO, cx));
        if !editable {
            page = page.child(
                div()
                    .px_4()
                    .child(note("This connection cannot change this project.", cx)),
            );
        }
        if let Some(error) = self.error.clone() {
            page = page.child(div().px_4().child(problem(error, cx)));
        }
        page = page.child(group(rows, cx));

        page = page.child(self.new_threads(&entry, editable, window, cx));
        page = page.child(self.actions_section(&entry, editable, window, cx));
        page = page.child(self.checkouts_section(&entry, editable, window, cx));

        if editable {
            let danger = model::danger(false, members.len());
            let remove = button(
                "project-remove",
                Variant::Outline,
                self.removal.is_none(),
                cx,
            )
            .text_color(theme.destructive)
            .child(Icon::new(Icons::Trash).size(px(14.)))
            .child(danger.button)
            .on_click(cx.listener({
                let members = members.clone();
                move |this, _, _, cx| {
                    this.confirm = Some(members.clone());
                    cx.notify();
                }
            }));
            page = page.child(section(
                "Danger",
                group(
                    vec![
                        row(
                            danger.title,
                            danger.description,
                            None,
                            remove.into_any_element(),
                            wide,
                            cx,
                        )
                        .into_any_element(),
                    ],
                    cx,
                ),
            ));
        }
        page.into_any_element()
    }

    fn confirm_view(
        &self,
        view: Entity<Self>,
        entry: &ProjectEntry,
        targets: &[Member],
        cx: &App,
    ) -> AnyElement {
        let theme = Tokens::of(cx);
        let whole = targets.len() == entry.members.len();
        let threads = self
            .thread_projects
            .iter()
            .filter(|id| targets.iter().any(|m| &m.id == *id))
            .count();
        let lines = model::confirmation(
            targets,
            entry.members.len(),
            false,
            &entry.title,
            Some(self.environment.as_str()),
            threads,
        );
        let danger = model::danger(false, if whole { entry.members.len() } else { 1 });
        let (cancel, go, dismiss) = (view.clone(), view.clone(), view);
        let destructive = theme.destructive;
        let footer = vec![
            button("project-remove-cancel", Variant::Outline, true, cx)
                .child("Cancel")
                .on_click(move |_, _, cx| {
                    cancel.update(cx, |this, cx| {
                        this.confirm = None;
                        cx.notify();
                    })
                })
                .into_any_element(),
            button("project-remove-confirm", Variant::Ghost, true, cx)
                .bg(destructive)
                .border_color(destructive)
                .text_color(gpui::white())
                .hover(move |style| style.bg(destructive.opacity(0.9)))
                .child(danger.button)
                .on_click(move |_, _, cx| go.update(cx, |this, cx| this.remove(cx)))
                .into_any_element(),
        ];
        let body = lines
            .into_iter()
            .map(|line| {
                div()
                    .text_size(px(14.))
                    .text_color(theme.muted_foreground)
                    .child(line)
                    .into_any_element()
            })
            .collect();
        dialog_of(
            "project-remove",
            DIALOG_WIDTH,
            danger.title,
            "",
            body,
            footer,
            move |_, cx| {
                dismiss.update(cx, |this, cx| {
                    this.confirm = None;
                    cx.notify();
                })
            },
            cx,
        )
    }

    pub fn overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if let Some(view) = self.delete_script_view(cx) {
            return Some(view);
        }
        if let Some(view) = self.editor_view(window, cx) {
            return Some(view);
        }
        let targets = self.confirm.clone()?;
        let Some(entry) = self.selected(cx).cloned() else {
            self.confirm = None;
            return None;
        };
        Some(self.confirm_view(cx.entity(), &entry, &targets, cx))
    }
}

impl Render for ProjectsPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let wanted = self.scope.read(cx).search().project.clone();
        let Some(key) = wanted else {
            let theme = Tokens::of(cx);
            let rows: Vec<AnyElement> = self
                .groups()
                .cloned()
                .collect::<Vec<_>>()
                .iter()
                .enumerate()
                .map(|(index, entry)| self.list_row(entry, index, cx))
                .collect();
            let body = if rows.is_empty() {
                note("Add a project from the sidebar to configure it here.", cx).into_any_element()
            } else {
                group(rows, cx).into_any_element()
            };
            return v_flex()
                .gap_4()
                .child(
                    div()
                        .px_4()
                        .text_size(px(14.))
                        .text_color(theme.muted_foreground)
                        .child(CHOOSE),
                )
                .child(body)
                .into_any_element();
        };
        let Some(entry) = self.groups().find(|entry| entry.key == key).cloned() else {
            return div()
                .px_4()
                .child(note("This project is no longer available.", cx))
                .into_any_element();
        };
        let editable = !entry.members.is_empty();
        self.detail(entry, editable, window, cx)
    }
}
