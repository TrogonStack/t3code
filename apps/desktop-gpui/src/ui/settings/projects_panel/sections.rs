//! The New threads, Actions and Checkouts sections of a project, and the action editor.

use super::*;
use crate::settings_source_control as sc;
use crate::ui::settings::components::{
    HoverHandler, KeyHandler, Menu, PickHandler, SelectTrigger, popup_shell, select_menu,
};
use crate::ui::settings::scope_inheritance::{Marker, marker_row_with};
use gpui::{ElementId, KeyDownEvent};
use gpui_component::input::Textarea;

const WORKSPACES: [(&str, &str); 2] = [("local", "Local"), ("worktree", "New worktree")];
const GROUPINGS: [GroupingMode; 3] = [
    GroupingMode::Repository,
    GroupingMode::RepositoryPath,
    GroupingMode::Separate,
];

fn default_label(mode: GroupingMode) -> &'static str {
    match mode {
        GroupingMode::Repository => "Default (Group by repository)",
        GroupingMode::RepositoryPath => "Default (Group by repository path)",
        GroupingMode::Separate => "Default (Keep separate)",
    }
}

fn marker_id(row: &'static str) -> &'static str {
    match row {
        "project-default-model" => "settings-project-default-model-marker",
        _ => "settings-project-workspace-marker",
    }
}

impl ProjectsPanel {
    fn member_ids(entry: &ProjectEntry) -> Vec<String> {
        entry.members.iter().map(|m| m.id.clone()).collect()
    }

    /// An optimistic settings write for the group's projects, put back when the call fails.
    pub(super) fn write_settings(
        &mut self,
        entry: &ProjectEntry,
        changes: Vec<(Field, Option<Value>)>,
        failure: &'static str,
        cx: &mut Context<Self>,
    ) {
        let members = Self::member_ids(entry);
        if members.is_empty() {
            return;
        }
        let patch = self.config.patch(&changes, Some(&members));
        let previous = self.config.clone();
        self.config.apply(&changes, Some(&members));
        let token = self.next_token;
        self.next_token += 1;
        let action = Action::Call {
            token,
            call: Call::UpdateSettings,
            payload: patch,
        };
        self.error = None;
        if self.actions.try_send(action).is_ok() {
            self.saves.insert(token, (previous, failure));
        } else {
            self.config = previous;
            self.error = Some(format!("{failure}: could not send the request"));
        }
        cx.notify();
    }

    fn marker(&self, id: &'static str, field: Field, members: &[String]) -> Marker {
        let project = self.config.project_override(field, members);
        let (state, summary) = sc::inheritance(
            field,
            true,
            project.is_some(),
            self.config.environment_set(field),
            &self.environment,
        );
        Marker {
            id: marker_id(id),
            state,
            summary,
            layers: sc::layers(
                field,
                &self.environment,
                true,
                project.as_ref(),
                &self.config,
            ),
            machine: self.machine,
            environment: self.environment.clone(),
            groups: Vec::new(),
        }
    }

    fn reset_for(
        &self,
        id: &'static str,
        tooltip: &'static str,
        field: Field,
        entry: &ProjectEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.config
            .project_override(field, &Self::member_ids(entry))?;
        let (view, entry) = (cx.entity(), entry.clone());
        Some(reset_button_labeled(
            id,
            tooltip,
            move |_, _, cx| {
                let entry = entry.clone();
                view.update(cx, |this, cx| {
                    this.write_settings(&entry, vec![(field, None)], "Failed to reset", cx)
                })
            },
            window,
            cx,
        ))
    }

    /// A `Select` over a `Popover`, the way the other tabs build one.
    #[allow(clippy::too_many_arguments)]
    fn pick(
        &self,
        kind: MenuKind,
        id: ElementId,
        item_id: &'static str,
        labels: Vec<&'static str>,
        selected: usize,
        width: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let handle = tab_stop(&id, window, cx);
        let on_key: KeyHandler = {
            let view = view.clone();
            Rc::new(move |event: &KeyDownEvent, _, cx| {
                if matches!(
                    event.keystroke.key.as_str(),
                    "enter" | "space" | "down" | "up"
                ) {
                    cx.stop_propagation();
                    view.update(cx, |this, cx| {
                        this.menu = Some((kind, selected));
                        cx.notify();
                    });
                }
            })
        };
        let trigger = SelectTrigger::new(id.clone(), labels[selected], width, handle, on_key);
        let open = self.menu.is_some_and(|(current, _)| current == kind);
        let highlighted = self
            .menu
            .filter(|(current, _)| *current == kind)
            .map_or(usize::MAX, |(_, index)| index);
        let on_pick: PickHandler = {
            let view = view.clone();
            Rc::new(move |index, _, cx| view.update(cx, |this, cx| this.choose(kind, index, cx)))
        };
        let on_hover: HoverHandler = {
            let view = view.clone();
            Rc::new(move |index, cx| {
                view.update(cx, |this, cx| {
                    if this
                        .menu
                        .is_some_and(|(k, current)| k == kind && current != index)
                    {
                        this.menu = Some((kind, index));
                        cx.notify();
                    }
                })
            })
        };
        Popover::new(ElementId::from((id, "popover")))
            .anchor(Anchor::TopRight)
            .offset(px(4.))
            .open(open)
            .p_0()
            .appearance(false)
            .on_open_change(move |opened, _, cx| {
                view.update(cx, |this, cx| {
                    if *opened {
                        this.menu = Some((kind, selected));
                    } else {
                        this.menu = None;
                    }
                    cx.notify();
                })
            })
            .trigger(trigger)
            .content(move |_, _, cx| {
                select_menu(
                    &Menu {
                        id: item_id,
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

    /// A choice made in an open select.
    fn choose(&mut self, kind: MenuKind, index: usize, cx: &mut Context<Self>) {
        self.menu = None;
        let Some(entry) = self.selected(cx).cloned() else {
            return;
        };
        match kind {
            MenuKind::Workspace => {
                let Some((mode, _)) = WORKSPACES.get(index) else {
                    return;
                };
                self.write_settings(
                    &entry,
                    vec![(Field::ThreadEnvMode, Some(json!(mode)))],
                    "Failed to update workspace",
                    cx,
                );
            }
            MenuKind::Grouping(member) => {
                let Some(member) = entry.members.get(member) else {
                    return;
                };
                let key = model::grouping_key(&self.environment_id, &member.root);
                let mode = index.checked_sub(1).and_then(|i| GROUPINGS.get(i)).copied();
                DeviceSettings::save_grouping_override(&key, mode);
                self.grouping = DeviceSettings::grouping_overrides();
                if self.grouping.get(&key).copied() != mode {
                    self.error = Some("Could not save grouping rule".to_owned());
                }
            }
        }
        cx.notify();
    }

    pub(super) fn new_threads(
        &mut self,
        entry: &ProjectEntry,
        editable: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let members = Self::member_ids(entry);
        let first = members.first().cloned();
        let wide = self.wide;

        let value = self.config.effective(Field::DefaultModel, &members, true);
        let pick =
            value
                .get("instanceId")
                .and_then(Value::as_str)
                .map(|id| crate::providers::Pick {
                    instance_id: id.to_owned(),
                    model: value
                        .get("model")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                    options: value
                        .get("options")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default(),
                });
        let trigger = self
            .catalog
            .as_ref()
            .and_then(|catalog| catalog.trigger_for(first.as_deref(), pick.as_ref()));
        let theme = Tokens::of(cx);
        let model_label = match trigger {
            Some(t) => match t.effort {
                Some(effort) => format!("{} \u{b7} {effort}", t.model),
                None => t.model,
            },
            None => "Automatic".to_owned(),
        };
        let model_control = div()
            .text_sm()
            .text_color(theme.muted_foreground)
            .child(model_label)
            .into_any_element();
        let model_reset = editable
            .then(|| {
                self.reset_for(
                    "project-default-model-reset",
                    "Reset to default model",
                    Field::DefaultModel,
                    entry,
                    window,
                    cx,
                )
            })
            .flatten();
        let model_marker = self.marker("project-default-model", Field::DefaultModel, &members);
        let model_row = marker_row_with(
            "project-default-model",
            "Model",
            "Model for new threads in this project.",
            Some(model_marker),
            model_reset,
            model_control,
            None,
            wide,
            window,
            cx,
        )
        .into_any_element();

        let mode = self.config.effective(Field::ThreadEnvMode, &members, true);
        let selected = WORKSPACES
            .iter()
            .position(|(wire, _)| Some(*wire) == mode.as_str())
            .unwrap_or(0);
        let labels = WORKSPACES.iter().map(|(_, label)| *label).collect();
        let select = self.pick(
            MenuKind::Workspace,
            "project-workspace".into(),
            "project-workspace-item",
            labels,
            selected,
            176.,
            window,
            cx,
        );
        let control = if editable {
            select
        } else {
            div().opacity(0.64).child(select).into_any_element()
        };
        let workspace_reset = editable
            .then(|| {
                self.reset_for(
                    "project-workspace-reset",
                    "Reset to default workspace",
                    Field::ThreadEnvMode,
                    entry,
                    window,
                    cx,
                )
            })
            .flatten();
        let workspace_marker = self.marker("project-workspace", Field::ThreadEnvMode, &members);
        let workspace_row = marker_row_with(
            "project-workspace",
            "Workspace",
            "Where new threads in this project start.",
            Some(workspace_marker),
            workspace_reset,
            control,
            None,
            wide,
            window,
            cx,
        )
        .into_any_element();
        section("New threads", group(vec![model_row, workspace_row], cx)).into_any_element()
    }

    fn script_row(
        &self,
        script: &model::Script,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Tokens::of(cx);
        let mono = cx.theme().mono_font_family.clone();
        let badge = |text: &'static str| {
            div()
                .px_1()
                .rounded(px(4.))
                .border_1()
                .border_color(theme.border)
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(text)
        };
        let icon = model::SCRIPT_ICONS
            .iter()
            .find(|(wire, _)| *wire == script.icon)
            .map_or("Play", |(_, label)| *label);
        let id = script.id.clone();
        let label: SharedString = format!("Edit {}", script.name).into();
        h_flex()
            .items_start()
            .gap_3()
            .px_4()
            .py_3()
            .child(
                v_flex()
                    .min_w_0()
                    .flex_1()
                    .gap_1()
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(icon),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(script.name.clone()),
                            )
                            .when(script.run_on_worktree_create, |row| {
                                row.child(badge("setup"))
                            })
                            .when(script.run_on_settle, |row| row.child(badge("on settle")))
                            .when(!script.preview_url.is_empty(), |row| {
                                row.child(badge("preview \u{b7} desktop only"))
                            }),
                    )
                    .child(
                        div()
                            .font_family(mono)
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(script.command.clone()),
                    ),
            )
            .child(
                button(("project-action-edit", index), Variant::Outline, true, cx)
                    .child("Edit")
                    .aria_label(label)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_editor(Some(id.clone()), window, cx)
                    })),
            )
            .into_any_element()
    }

    fn effective_scripts(&self, entry: &ProjectEntry) -> Vec<model::Script> {
        model::decode_scripts(&self.config.effective(
            Field::Scripts,
            &Self::member_ids(entry),
            true,
        ))
    }

    pub(super) fn actions_section(
        &mut self,
        entry: &ProjectEntry,
        editable: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let scripts = self.effective_scripts(entry);
        let reset = editable
            .then(|| {
                self.reset_for(
                    "project-actions-reset",
                    "Reset to default actions",
                    Field::Scripts,
                    entry,
                    window,
                    cx,
                )
            })
            .flatten();
        let add = button("project-action-add", Variant::Outline, editable, cx)
            .child("Add action")
            .on_click(cx.listener(|this, _, window, cx| this.open_editor(None, window, cx)));
        let mut rows = vec![
            row(
                "Actions",
                "Commands that run in this project's checkout or its worktree, with optional shortcuts.",
                reset,
                add.into_any_element(),
                self.wide,
                cx,
            )
            .into_any_element(),
        ];
        if scripts.is_empty() {
            rows.push(
                div()
                    .px_4()
                    .child(note("No actions configured.", cx))
                    .into_any_element(),
            );
        }
        for (index, script) in scripts.iter().enumerate() {
            rows.push(self.script_row(script, index, cx));
        }
        section("Actions", group(rows, cx)).into_any_element()
    }

    pub(super) fn checkouts_section(
        &mut self,
        entry: &ProjectEntry,
        editable: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Tokens::of(cx);
        let wide = self.wide;
        let many = entry.members.len() > 1;
        let mut rows = Vec::new();
        for (index, member) in entry.members.iter().enumerate() {
            let key = model::grouping_key(&self.environment_id, &member.root);
            let own = self.grouping.get(&key).copied();
            let mut labels = vec![default_label(self.global_grouping)];
            labels.extend(GROUPINGS.iter().map(|mode| mode.label()));
            let selected = own
                .and_then(|mode| GROUPINGS.iter().position(|m| *m == mode))
                .map_or(0, |i| i + 1);
            let select = self.pick(
                MenuKind::Grouping(index),
                ElementId::from(("project-grouping", index)),
                "project-grouping-item",
                labels,
                selected,
                224.,
                window,
                cx,
            );
            let mut controls = h_flex().gap_2().items_center().child(select);
            if many && editable {
                let target = member.clone();
                let label: SharedString = format!("Remove checkout {}", member.root).into();
                controls = controls.child(
                    button(
                        ("project-checkout-remove", index),
                        Variant::Outline,
                        self.removal.is_none(),
                        cx,
                    )
                    .text_color(theme.destructive)
                    .child("Remove")
                    .aria_label(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.confirm = Some(vec![target.clone()]);
                        cx.notify();
                    })),
                );
            }
            rows.push(
                row(
                    self.environment.clone(),
                    member.root.clone(),
                    None,
                    controls.into_any_element(),
                    wide,
                    cx,
                )
                .into_any_element(),
            );
        }
        let intro = div()
            .px_4()
            .child(note(
                "Grouping rule decides how each checkout joins project groups in the sidebar. Changing it can move a checkout to a different project.",
                cx,
            ))
            .into_any_element();
        let mut all = vec![intro];
        all.extend(rows);
        section("Checkouts", group(all, cx)).into_any_element()
    }

    pub(super) fn open_editor(
        &mut self,
        id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = self.selected(cx).cloned() else {
            return;
        };
        let form = id
            .as_ref()
            .and_then(|id| {
                self.effective_scripts(&entry)
                    .into_iter()
                    .find(|script| &script.id == id)
            })
            .unwrap_or_else(model::Script::blank);
        self.script_name.update(cx, |state, cx| {
            state.set_value(form.name.clone(), window, cx)
        });
        self.script_preview.update(cx, |state, cx| {
            state.set_value(form.preview_url.clone(), window, cx)
        });
        self.script_command.update(cx, |state, cx| {
            state.set_value(form.command.clone(), window, cx)
        });
        self.editor = Some(Editor {
            editing: id,
            form,
            problem: None,
        });
        cx.notify();
    }

    fn flip(&mut self, edit: impl FnOnce(&mut model::Script), cx: &mut Context<Self>) {
        if let Some(editor) = self.editor.as_mut() {
            edit(&mut editor.form);
            if !editor.form.run_on_worktree_create {
                editor.form.wait_for_setup = false;
            }
            cx.notify();
        }
    }

    fn save_script(&mut self, cx: &mut Context<Self>) {
        let Some(entry) = self.selected(cx).cloned() else {
            return;
        };
        let name = self.script_name.read(cx).value().trim().to_owned();
        let command = self.script_command.read(cx).value().trim().to_owned();
        let preview = self.script_preview.read(cx).value().trim().to_owned();
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        if let Some(problem) = model::script_problem(&name, &command) {
            editor.problem = Some(problem);
            cx.notify();
            return;
        }
        let mut form = editor.form.clone();
        form.name = name;
        form.command = command;
        form.auto_open_preview = form.auto_open_preview && !preview.is_empty();
        form.preview_url = preview;
        if form.id.is_empty() {
            form.id = model::next_script_id(&form.name, &self.config.script_ids());
        }
        let updated = model::saved_scripts(&self.effective_scripts(&entry), form);
        let value = Value::Array(updated.iter().map(model::Script::encode).collect());
        self.editor = None;
        self.write_settings(
            &entry,
            vec![(Field::Scripts, Some(value))],
            "Failed to save action",
            cx,
        );
    }

    fn delete_confirmed(&mut self, cx: &mut Context<Self>) {
        let Some((id, _)) = self.delete_script.take() else {
            return;
        };
        let Some(entry) = self.selected(cx).cloned() else {
            return;
        };
        let remaining: Vec<Value> = self
            .effective_scripts(&entry)
            .iter()
            .filter(|script| script.id != id)
            .map(model::Script::encode)
            .collect();
        self.editor = None;
        self.write_settings(
            &entry,
            vec![(Field::Scripts, Some(Value::Array(remaining)))],
            "Failed to delete action",
            cx,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn switch_row(
        &self,
        id: &'static str,
        label: &'static str,
        checked: bool,
        enabled: bool,
        edit: fn(&mut model::Script, bool),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let toggle = Rc::new(move |_: &mut Window, cx: &mut App| {
            if enabled {
                view.update(cx, |this, cx| this.flip(|form| edit(form, !checked), cx))
            }
        });
        h_flex()
            .gap_3()
            .items_center()
            .justify_between()
            .when(!enabled, |row| row.opacity(0.64))
            .child(div().text_sm().child(label))
            .child(switch(id, checked, toggle, window, cx))
            .into_any_element()
    }

    pub(super) fn editor_view(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let editor = self.editor.as_ref()?;
        let (form, editing, problem_text) =
            (editor.form.clone(), editor.editing.clone(), editor.problem);
        let theme = Tokens::of(cx);
        let view = cx.entity();
        let icons =
            h_flex()
                .gap_1()
                .flex_wrap()
                .children(
                    model::SCRIPT_ICONS
                        .iter()
                        .enumerate()
                        .map(|(index, (wire, label))| {
                            let selected = form.icon == *wire;
                            let wire = *wire;
                            button(("project-action-icon", index), Variant::Outline, true, cx)
                                .when(selected, |b| b.bg(theme.accent))
                                .child(*label)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.flip(|form| form.icon = wire.to_owned(), cx)
                                }))
                        }),
                );
        let field = |label: &'static str, control: AnyElement| {
            v_flex()
                .gap_1()
                .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(label))
                .child(control)
                .into_any_element()
        };
        let name = sm_field(&self.script_name, false, false, false, window, cx);
        let focused = self
            .script_command
            .read(cx)
            .focus_handle(cx)
            .contains_focused(window, cx);
        let background = if theme.dark {
            theme.input.opacity(0.32)
        } else {
            theme.background
        };
        let command = div()
            .w_full()
            .rounded(px(8.))
            .border_1()
            .border_color(if focused { theme.ring } else { theme.input })
            .bg(background)
            .text_size(px(14.))
            .child(
                Textarea::new(&self.script_command)
                    .appearance(false)
                    .bordered(false)
                    .aria_label("Command"),
            );
        let preview = v_flex()
            .gap_1()
            .child(sm_field(
                &self.script_preview,
                true,
                false,
                false,
                window,
                cx,
            ))
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("Open this URL in the in-app preview when this action runs."),
            );
        let has_preview = !self.script_preview.read(cx).value().trim().is_empty();
        let mut body = vec![
            field("Name", name.into_any_element()),
            field("Icon", icons.into_any_element()),
            field("Command", command.into_any_element()),
            field("Preview URL (optional)", preview.into_any_element()),
            self.switch_row(
                "project-action-setup",
                "Run automatically on worktree creation",
                form.run_on_worktree_create,
                true,
                |f, v| f.run_on_worktree_create = v,
                window,
                cx,
            ),
            self.switch_row(
                "project-action-wait",
                "Wait for it to finish before the agent starts",
                form.wait_for_setup,
                form.run_on_worktree_create,
                |f, v| f.wait_for_setup = v,
                window,
                cx,
            ),
            self.switch_row(
                "project-action-settle",
                "Run in the thread's worktree when the thread settles",
                form.run_on_settle,
                true,
                |f, v| f.run_on_settle = v,
                window,
                cx,
            ),
            self.switch_row(
                "project-action-stop",
                "Stop this action when the thread settles",
                form.stop_on_settle,
                true,
                |f, v| f.stop_on_settle = v,
                window,
                cx,
            ),
            self.switch_row(
                "project-action-auto-preview",
                "Open preview automatically when this action runs",
                form.auto_open_preview,
                has_preview,
                |f, v| f.auto_open_preview = v,
                window,
                cx,
            ),
        ];
        if let Some(text) = problem_text {
            body.push(problem(text, cx).into_any_element());
        }
        let mut footer: Vec<AnyElement> = Vec::new();
        if let Some(id) = editing.clone() {
            let name = form.name.clone();
            footer.push(
                button("project-action-delete", Variant::Outline, true, cx)
                    .text_color(theme.destructive)
                    .child("Delete")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.delete_script = Some((id.clone(), name.clone()));
                        cx.notify();
                    }))
                    .into_any_element(),
            );
            footer.push(div().flex_1().into_any_element());
        }
        let cancel = view.clone();
        footer.push(
            button("project-action-cancel", Variant::Outline, true, cx)
                .child("Cancel")
                .on_click(move |_, _, cx| {
                    cancel.update(cx, |this, cx| {
                        this.editor = None;
                        cx.notify();
                    })
                })
                .into_any_element(),
        );
        footer.push(
            button("project-action-save", Variant::Primary, true, cx)
                .child(if editing.is_some() {
                    "Save changes"
                } else {
                    "Save action"
                })
                .on_click(cx.listener(|this, _, _, cx| this.save_script(cx)))
                .into_any_element(),
        );
        let dismiss = view;
        Some(dialog_of(
            "project-action-editor",
            DIALOG_WIDTH,
            if editing.is_some() {
                "Edit Action"
            } else {
                "Add Action"
            },
            "Actions are project-scoped commands you can run from the top bar or keybindings.",
            body,
            footer,
            move |_, cx| {
                dismiss.update(cx, |this, cx| {
                    this.editor = None;
                    cx.notify();
                })
            },
            cx,
        ))
    }

    pub(super) fn delete_script_view(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (_, name) = self.delete_script.clone()?;
        let theme = Tokens::of(cx);
        let view = cx.entity();
        let destructive = theme.destructive;
        let (cancel, dismiss) = (view.clone(), view);
        let footer = vec![
            button("project-action-delete-cancel", Variant::Outline, true, cx)
                .child("Cancel")
                .on_click(move |_, _, cx| {
                    cancel.update(cx, |this, cx| {
                        this.delete_script = None;
                        cx.notify();
                    })
                })
                .into_any_element(),
            button("project-action-delete-confirm", Variant::Ghost, true, cx)
                .bg(destructive)
                .border_color(destructive)
                .text_color(gpui::white())
                .hover(move |style| style.bg(destructive.opacity(0.9)))
                .child("Delete action")
                .on_click(cx.listener(|this, _, _, cx| this.delete_confirmed(cx)))
                .into_any_element(),
        ];
        Some(dialog_of(
            "project-action-delete",
            DIALOG_WIDTH,
            format!("Delete action \"{name}\"?"),
            "This action cannot be undone.",
            Vec::new(),
            footer,
            move |_, cx| {
                dismiss.update(cx, |this, cx| {
                    this.delete_script = None;
                    cx.notify();
                })
            },
            cx,
        ))
    }
}

#[allow(dead_code)]
fn _shell(width: f32, cx: &App) -> gpui::Div {
    popup_shell(width, cx)
}
