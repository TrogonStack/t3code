//! The Projects tab: `ProjectsSettings.tsx` (the list under "All projects") and the first part of
//! `ProjectSettingsPanel.tsx` (Name, Location, Project icon, Pin, Archive and Danger). The icon
//! picker dialogs are not here: the buttons show and stay off. Writes go out as `project.update`
//! and `project.delete` per member; the shell stream brings the result back.

use super::components::{group, reset_button_labeled, row, section, switch};
use super::devices_panel::widgets::{Variant, button, dialog_of, note, problem};
use super::scope_pickers::ScopeBar;
use super::secret_field::sm_field;
use super::tokens::Tokens;
use super::*;
use crate::connection::{Action, ActionSender, Call, command_id};
use crate::projects::{Member, ProjectEntry};
use crate::settings_projects::{self as model};
use crate::status::CallEvent;
use gpui::{PathPromptOptions, Subscription};
use gpui_component::input::{InputEvent, InputState};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::rc::Rc;

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
    /// The project of every listed thread, for the removal's thread count.
    thread_projects: Vec<String>,
    name: Entity<InputState>,
    /// The group and title the name input shows.
    seeded: Option<(String, String)>,
    /// Writes in flight, with the failure title each reports.
    pending: HashMap<u64, &'static str>,
    removing: bool,
    error: Option<String>,
    confirm: bool,
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
            thread_projects: Vec::new(),
            name,
            seeded: None,
            pending: HashMap::new(),
            removing: false,
            error: None,
            confirm: false,
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

    pub fn set_data(
        &mut self,
        live: Vec<ProjectEntry>,
        archived: Vec<ProjectEntry>,
        can_organize: bool,
        environment: String,
        thread_projects: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        if self.live != live
            || self.archived != archived
            || self.can_organize != can_organize
            || self.environment != environment
            || self.thread_projects != thread_projects
        {
            self.live = live;
            self.archived = archived;
            self.can_organize = can_organize;
            self.environment = environment;
            self.thread_projects = thread_projects;
            cx.notify();
        }
    }

    pub fn reload(&mut self, cx: &mut Context<Self>) {
        self.confirm = false;
        self.error = None;
        self.seeded = None;
        cx.notify();
    }

    /// Closes the removal confirmation; returns whether there was one, so Escape stops there.
    pub fn close_menu(&mut self, cx: &mut Context<Self>) -> bool {
        if std::mem::take(&mut self.confirm) {
            cx.notify();
            return true;
        }
        false
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

    fn send(&mut self, project_id: &str, payload: Value, failure: &'static str) {
        let token = self.next_token;
        self.next_token += 1;
        let action = Action::Call {
            token,
            call: Call::ProjectsMutate,
            payload,
        };
        match self.actions.try_send(action) {
            Ok(()) => {
                self.pending.insert(token, failure);
            }
            Err(_) => {
                self.error = Some(format!("{failure}: could not send the request"));
                eprintln!("[projects] could not send the update for {project_id}");
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
            self.send(&member.id, payload, failure);
        }
        cx.notify();
    }

    pub fn on_call(&mut self, token: u64, event: CallEvent, cx: &mut Context<Self>) {
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
        if self.removing && self.pending.is_empty() {
            self.removing = false;
            if self.error.is_none() {
                self.scope
                    .update(cx, |bar, cx| bar.select_project("all", cx));
            }
        }
        cx.notify();
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

    fn remove(&mut self, cx: &mut Context<Self>) {
        self.confirm = false;
        let Some(entry) = self.selected(cx).cloned() else {
            return;
        };
        self.error = None;
        self.removing = true;
        for member in &entry.members {
            let payload = model::delete_payload(&member.id, command_id());
            self.send(&member.id, payload, "Failed to remove project");
        }
        if self.pending.is_empty() {
            self.removing = false;
        }
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

        if editable {
            let danger = model::danger(false, members.len());
            let remove = button("project-remove", Variant::Outline, !self.removing, cx)
                .text_color(theme.destructive)
                .child(Icon::new(Icons::Trash).size(px(14.)))
                .child(danger.button)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.confirm = true;
                    cx.notify();
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

    fn confirm_view(&self, view: Entity<Self>, entry: &ProjectEntry, cx: &App) -> AnyElement {
        let theme = Tokens::of(cx);
        let threads = self
            .thread_projects
            .iter()
            .filter(|id| entry.members.iter().any(|m| &m.id == *id))
            .count();
        let lines = model::confirmation(
            &entry.members,
            entry.members.len(),
            false,
            &entry.title,
            Some(self.environment.as_str()),
            threads,
        );
        let danger = model::danger(false, entry.members.len());
        let (cancel, go, dismiss) = (view.clone(), view.clone(), view);
        let destructive = theme.destructive;
        let footer = vec![
            button("project-remove-cancel", Variant::Outline, true, cx)
                .child("Cancel")
                .on_click(move |_, _, cx| {
                    cancel.update(cx, |this, cx| {
                        this.confirm = false;
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
                    this.confirm = false;
                    cx.notify();
                })
            },
            cx,
        )
    }

    pub fn overlay(&mut self, _: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.confirm {
            return None;
        }
        let Some(entry) = self.selected(cx).cloned() else {
            self.confirm = false;
            return None;
        };
        Some(self.confirm_view(cx.entity(), &entry, cx))
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
