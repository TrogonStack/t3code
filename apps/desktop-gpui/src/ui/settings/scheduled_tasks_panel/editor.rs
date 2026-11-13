//! The task editor dialog of `ScheduledTasksSettings.tsx`: text inputs are the source of truth for
//! their fields and are read into the draft when saving; everything else lives in the draft.

use super::*;
use crate::settings_scheduled_task_editor::{
    self as ed, Draft, EditorData, Encoding, Failure, FilterOp, Mode, Model, ProjectChoice,
    Workspace,
};
use gpui::{ClipboardItem, ElementId};
use gpui_component::input::{InputState, Textarea, TextareaState};

use super::super::devices_panel::widgets::{field, labeled};

const EDITOR_WIDTH: f32 = 560.;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Menu {
    Environment,
    Project,
    Destination,
    Thread,
    Workspace,
    Model,
    Op(u64),
    Encoding,
}

pub(super) struct Editor {
    pub environment: String,
    pub draft: Draft,
    pub continue_thread: bool,
    pub has_secret: bool,
    pub editing_once: bool,
    pub webhook_url: Option<String>,
    pub menu: Option<Menu>,
    pub saving: bool,
    pub rotating: bool,
    pub rotate_confirm: bool,
    pub error: Option<Failure>,
}

type FilterInputs = (Entity<InputState>, Entity<InputState>);

pub(super) struct Inputs {
    title: Entity<InputState>,
    base_ref: Entity<InputState>,
    path: Entity<InputState>,
    interval: Entity<InputState>,
    time: Entity<InputState>,
    run_at: Entity<InputState>,
    header: Entity<InputState>,
    prefix: Entity<InputState>,
    secret: Entity<InputState>,
    age: Entity<InputState>,
    replay: Entity<InputState>,
    coalesce_key: Entity<InputState>,
    coalesce_window: Entity<InputState>,
    model: Entity<InputState>,
    prompt: Entity<TextareaState>,
    filters: HashMap<u64, FilterInputs>,
}

fn input(placeholder: &'static str, window: &mut Window, cx: &mut App) -> Entity<InputState> {
    cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
}

fn text_of(input: &Entity<InputState>, cx: &App) -> String {
    input.read(cx).value().to_string()
}

fn set_text(input: &Entity<InputState>, value: &str, window: &mut Window, cx: &mut App) {
    input.update(cx, |state, cx| {
        state.set_value(value.to_owned(), window, cx)
    });
}

impl Inputs {
    pub(super) fn new(window: &mut Window, cx: &mut App) -> Self {
        Self {
            title: input("e.g. Check for Sentry issues", window, cx),
            base_ref: input("main", window, cx),
            path: input("/path/to/checkout", window, cx),
            interval: input("15", window, cx),
            time: input("09:00", window, cx),
            run_at: input("YYYY-MM-DDTHH:MM", window, cx),
            header: input(ed::SIGNATURE_HEADER, window, cx),
            prefix: input(ed::SIGNATURE_PREFIX, window, cx),
            secret: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Shared secret")
                    .masked(true)
            }),
            age: input("Run every request", window, cx),
            replay: input("{{headers.delivery-id}}", window, cx),
            coalesce_key: input("{{body.id}}", window, cx),
            coalesce_window: input("120", window, cx),
            model: input("instanceId:model", window, cx),
            prompt: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder("What should the agent do?")
                    .auto_grow(4, 12)
            }),
            filters: HashMap::new(),
        }
    }
}

fn model_text(model: &Option<Model>) -> String {
    model
        .as_ref()
        .map(|(instance, slug)| format!("{instance}:{slug}"))
        .unwrap_or_default()
}

fn mode_label(mode: Mode) -> &'static str {
    match mode {
        Mode::Fixed => "At a time",
        Mode::Once => "Once",
        Mode::Interval => "Every interval",
        Mode::Webhook => "On webhook",
    }
}

const DAYS: [(i64, &str); 7] = [
    (1, "Mo"),
    (2, "Tu"),
    (3, "We"),
    (4, "Th"),
    (5, "Fr"),
    (6, "Sa"),
    (0, "Su"),
];

type Pick =
    Rc<dyn Fn(&mut ScheduledTasksPanel, usize, &mut Window, &mut Context<ScheduledTasksPanel>)>;

impl ScheduledTasksPanel {
    pub fn set_editor_data(&mut self, data: EditorData, cx: &mut Context<Self>) {
        if self.data != data {
            self.data = data;
            cx.notify();
        }
    }

    fn connected_environments(&self, cx: &App) -> Vec<Environment> {
        self.resolved(cx)
            .1
            .into_iter()
            .filter(|environment| {
                environment.connected
                    && matches!(
                        self.watches.get(&environment.id).map(|w| &w.load),
                        Some(Load::Loaded(_))
                    )
            })
            .collect()
    }

    pub(super) fn can_create(&self, cx: &App) -> bool {
        !self.connected_environments(cx).is_empty()
    }

    fn project_options(&self, environment: &str) -> Vec<ed::ProjectOption> {
        self.data
            .projects
            .get(environment)
            .cloned()
            .unwrap_or_default()
    }

    fn fill_inputs(&mut self, draft: &Draft, window: &mut Window, cx: &mut Context<Self>) {
        let inputs = &self.inputs;
        set_text(&inputs.title, &draft.title, window, cx);
        set_text(&inputs.base_ref, &draft.base_ref, window, cx);
        set_text(&inputs.path, &draft.existing_worktree_path, window, cx);
        set_text(&inputs.interval, &draft.interval_minutes, window, cx);
        set_text(&inputs.time, &draft.time_of_day, window, cx);
        set_text(&inputs.run_at, &draft.run_at, window, cx);
        set_text(&inputs.header, &draft.signature_header, window, cx);
        set_text(&inputs.prefix, &draft.signature_prefix, window, cx);
        set_text(&inputs.secret, &draft.signature_secret, window, cx);
        set_text(&inputs.age, &draft.max_delivery_age_minutes, window, cx);
        set_text(&inputs.replay, &draft.replay_key, window, cx);
        set_text(&inputs.coalesce_key, &draft.coalesce_key, window, cx);
        set_text(
            &inputs.coalesce_window,
            &draft.coalesce_window_seconds,
            window,
            cx,
        );
        set_text(&inputs.model, &model_text(&draft.model), window, cx);
        let prompt = draft.prompt.clone();
        inputs
            .prompt
            .update(cx, |state, cx| state.set_value(prompt, window, cx));
        self.inputs.filters.clear();
        for filter in &draft.filters {
            self.add_filter_inputs(filter.id, &filter.path, &filter.value, window, cx);
        }
    }

    fn add_filter_inputs(
        &mut self,
        id: u64,
        path: &str,
        value: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path_input = input("body.action", window, cx);
        let value_input = input("Value", window, cx);
        set_text(&path_input, path, window, cx);
        set_text(&value_input, value, window, cx);
        self.inputs.filters.insert(id, (path_input, value_input));
    }

    fn start_editor(
        &mut self,
        environment: String,
        draft: Draft,
        task: Option<&Task>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.error = None;
        self.menu = None;
        self.fill_inputs(&draft, window, cx);
        self.editor = Some(Editor {
            environment,
            continue_thread: !draft.thread_id.is_empty(),
            has_secret: task.is_some_and(|task| task.has_secret),
            editing_once: draft.mode == Mode::Once && draft.editing_id.is_some(),
            webhook_url: task.and_then(|task| task.webhook_url.clone()),
            draft,
            menu: None,
            saving: false,
            rotating: false,
            rotate_confirm: false,
            error: None,
        });
        cx.notify();
    }

    pub(super) fn open_new(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let connected = self.connected_environments(cx);
        let Some(environment) = connected
            .iter()
            .find(|environment| environment.id == PRIMARY)
            .or(connected.first())
            .map(|environment| environment.id.clone())
        else {
            return;
        };
        let scoped = self.resolved(cx).2.is_some();
        let draft = Draft {
            project_id: self.default_project(&environment, scoped),
            ..Draft::default()
        };
        self.start_editor(environment, draft, None, window, cx);
    }

    pub(super) fn open_edit(
        &mut self,
        environment: &str,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(Watch {
            load: Load::Loaded(list),
            ..
        }) = self.watches.get(environment)
        else {
            return;
        };
        let Some(task) = list.iter().find(|task| task.id == id).cloned() else {
            return;
        };
        let Some(draft) = ed::task_to_draft(&task.raw, &self.zone) else {
            self.error = Some("This task could not be opened for editing.".to_owned());
            cx.notify();
            return;
        };
        self.start_editor(environment.to_owned(), draft, Some(&task), window, cx);
    }

    fn default_project(&self, environment: &str, scoped: bool) -> String {
        let choices: Vec<ProjectChoice> = self
            .project_options(environment)
            .into_iter()
            .map(|p| ProjectChoice {
                id: p.id,
                pinned: p.pinned,
                archived: p.archived,
            })
            .collect();
        ed::default_project_id(&choices, scoped)
    }

    fn selection(&self, draft: &Draft, environment: &str) -> Option<Model> {
        if let Some(model) = &draft.model {
            return Some(model.clone());
        }
        (environment == PRIMARY)
            .then(|| self.data.default_model.clone())
            .flatten()
    }

    fn collect(&mut self, cx: &App) {
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        let inputs = &self.inputs;
        let draft = &mut editor.draft;
        draft.title = text_of(&inputs.title, cx);
        draft.base_ref = text_of(&inputs.base_ref, cx);
        draft.existing_worktree_path = text_of(&inputs.path, cx);
        draft.interval_minutes = text_of(&inputs.interval, cx);
        draft.time_of_day = text_of(&inputs.time, cx);
        draft.run_at = text_of(&inputs.run_at, cx);
        draft.signature_header = text_of(&inputs.header, cx);
        draft.signature_prefix = text_of(&inputs.prefix, cx);
        draft.signature_secret = text_of(&inputs.secret, cx);
        draft.max_delivery_age_minutes = text_of(&inputs.age, cx);
        draft.replay_key = text_of(&inputs.replay, cx);
        draft.coalesce_key = text_of(&inputs.coalesce_key, cx);
        draft.coalesce_window_seconds = text_of(&inputs.coalesce_window, cx);
        draft.prompt = inputs.prompt.read(cx).value().to_string();
        for filter in &mut draft.filters {
            if let Some((path, value)) = inputs.filters.get(&filter.id) {
                filter.path = text_of(path, cx);
                filter.value = text_of(value, cx);
            }
        }
        if editor.environment != PRIMARY {
            let text = text_of(&inputs.model, cx);
            if let Some((instance, slug)) = text.trim().split_once(':')
                && !instance.is_empty()
                && !slug.is_empty()
            {
                draft.model = Some((instance.to_owned(), slug.to_owned()));
            }
        }
    }

    pub(super) fn save(&mut self, cx: &mut Context<Self>) {
        self.collect(cx);
        let Some(editor) = self.editor.as_ref() else {
            return;
        };
        if editor.saving {
            return;
        }
        let environment = editor.environment.clone();
        let mut draft = editor.draft.clone();
        if !editor.continue_thread {
            draft.thread_id.clear();
        } else if draft.thread_id.is_empty() {
            self.set_editor_error(
                "Pick a thread",
                "Choose the thread each run should continue.",
            );
            cx.notify();
            return;
        }
        let mut project_ids: Vec<String> = self
            .project_options(&environment)
            .into_iter()
            .map(|p| p.id)
            .collect();
        if draft.editing_id.is_some() && !project_ids.contains(&draft.project_id) {
            project_ids.push(draft.project_id.clone());
        }
        let context = ed::Context {
            project_ids: &project_ids,
            zone: &self.zone,
            offers_once: environment == PRIMARY || editor.editing_once,
            has_secret: editor.has_secret,
            selection: self.selection(&draft, &environment),
        };
        match ed::build_input(&draft, &context) {
            Err(failure) => {
                if let Some(editor) = self.editor.as_mut() {
                    editor.error = Some(failure);
                }
            }
            Ok(payload) => {
                let sent = self.send(
                    &environment,
                    Call::UpsertScheduledTask,
                    payload,
                    Pending::Save,
                );
                if sent {
                    if let Some(editor) = self.editor.as_mut() {
                        editor.saving = true;
                        editor.error = None;
                    }
                } else {
                    self.set_editor_error(
                        "Could not save the task",
                        "The environment is not connected.",
                    );
                }
            }
        }
        cx.notify();
    }

    fn set_editor_error(&mut self, title: &str, description: &str) {
        if let Some(editor) = self.editor.as_mut() {
            editor.error = Some(Failure {
                title: title.to_owned(),
                description: description.to_owned(),
            });
        }
    }

    fn rotate(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        let (Some(id), false) = (editor.draft.editing_id.clone(), editor.rotating) else {
            return;
        };
        editor.rotate_confirm = false;
        let environment = editor.environment.clone();
        if self.send(
            &environment,
            Call::RotateScheduledTaskWebhook,
            json!({ "id": id }),
            Pending::Rotate,
        ) && let Some(editor) = self.editor.as_mut()
        {
            editor.rotating = true;
        }
        cx.notify();
    }

    pub(super) fn editor_saved(&mut self, environment: &str, value: &Value) {
        if let Some(task) = value.get("task").and_then(tasks::decode_task)
            && let Some(Watch {
                load: Load::Loaded(list),
                ..
            }) = self.watches.get_mut(environment)
        {
            match list.iter_mut().find(|slot| slot.id == task.id) {
                Some(slot) => *slot = task,
                None => list.push(task),
            }
        }
        self.editor = None;
    }

    pub(super) fn editor_rotated(&mut self, value: &Value) {
        let task = value
            .get("task")
            .and_then(tasks::decode_task)
            .or_else(|| tasks::decode_task(value));
        if let Some(editor) = self.editor.as_mut() {
            editor.rotating = false;
            if let Some(task) = task {
                editor.webhook_url = task.webhook_url;
            }
        }
    }

    pub(super) fn editor_failed(&mut self, title: &str, message: &str) {
        if let Some(editor) = self.editor.as_mut() {
            editor.saving = false;
            editor.rotating = false;
        }
        self.set_editor_error(title, message);
    }

    /// Closes the open select, then the editor; returns whether anything closed.
    pub(super) fn close_editor_layer(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(editor) = self.editor.as_mut() else {
            return false;
        };
        if editor.menu.take().is_some() || std::mem::take(&mut editor.rotate_confirm) {
            cx.notify();
            return true;
        }
        if !editor.saving {
            self.editor = None;
            cx.notify();
            return true;
        }
        false
    }

    fn set_menu(&mut self, menu: Menu, open: bool, cx: &mut Context<Self>) {
        if let Some(editor) = self.editor.as_mut() {
            editor.menu = open.then_some(menu);
            cx.notify();
        }
    }

    fn edit(&mut self, cx: &mut Context<Self>, change: impl FnOnce(&mut Editor)) {
        if let Some(editor) = self.editor.as_mut() {
            change(editor);
            editor.menu = None;
            editor.error = None;
            cx.notify();
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn select(
        view: &Entity<Self>,
        id: impl Into<SharedString>,
        menu: Menu,
        open: bool,
        disabled: bool,
        current: String,
        options: Vec<String>,
        selected: Option<usize>,
        pick: Pick,
        cx: &App,
    ) -> AnyElement {
        let id: SharedString = id.into();
        let theme = Tokens::of(cx);
        let trigger = h_flex()
            .id(id.clone())
            .h(px(32.))
            .w_full()
            .items_center()
            .justify_between()
            .px(px(11.))
            .rounded(px(8.))
            .border_1()
            .border_color(theme.input)
            .bg(if theme.dark {
                theme.input.opacity(0.32)
            } else {
                theme.background
            })
            .text_size(px(14.))
            .when(disabled, |trigger| trigger.opacity(0.64))
            .when(!disabled, |trigger| trigger.cursor_pointer())
            .child(div().min_w_0().truncate().child(current))
            .child(
                div()
                    .opacity(0.7)
                    .child(Icon::new(Icons::ChevronDown).size(px(14.))),
            );
        let opened = view.clone();
        let picked = view.clone();
        Popover::new(SharedString::from(format!("{id}-menu")))
            .anchor(Anchor::TopLeft)
            .offset(px(4.))
            .open(open && !disabled)
            .p_0()
            .appearance(false)
            .on_open_change(move |next, _, cx| {
                opened.update(cx, |this, cx| this.set_menu(menu, *next, cx))
            })
            .trigger(PopTrigger::new(trigger))
            .content(move |_, _, cx| {
                let theme = Tokens::of(cx);
                let accent = theme.accent;
                popup_shell(cx)
                    .w(px(EDITOR_WIDTH - 48.))
                    .max_w_full()
                    .max_h(px(240.))
                    .id(SharedString::from(format!("{id}-list")))
                    .overflow_y_scroll()
                    .p_1()
                    .children(options.iter().enumerate().map(|(index, label)| {
                        let (view, pick) = (picked.clone(), pick.clone());
                        h_flex()
                            .id(SharedString::from(format!("{id}-{index}")))
                            .min_h(px(28.))
                            .items_center()
                            .gap_2()
                            .px_2()
                            .py_1()
                            .rounded(px(4.))
                            .text_sm()
                            .cursor_pointer()
                            .text_color(theme.foreground)
                            .hover(move |style| style.bg(accent))
                            .when(Some(index) == selected, |item| {
                                item.font_weight(FontWeight::MEDIUM)
                            })
                            .child(label.clone())
                            .on_click(move |_, window, cx| {
                                view.update(cx, |this, cx| pick(this, index, window, cx))
                            })
                    }))
            })
            .into_any_element()
    }

    fn chip(
        view: &Entity<Self>,
        id: impl Into<ElementId>,
        label: &'static str,
        active: bool,
        enabled: bool,
        apply: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &App,
    ) -> AnyElement {
        let view = view.clone();
        button(
            id,
            if active {
                Variant::Primary
            } else {
                Variant::Outline
            },
            enabled,
            cx,
        )
        .child(label)
        .on_click(move |_, window, cx| view.update(cx, |this, cx| apply(this, window, cx)))
        .into_any_element()
    }

    fn switch_row(
        view: &Entity<Self>,
        id: &'static str,
        label: &'static str,
        checked: bool,
        apply: impl Fn(&mut Editor) + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = view.clone();
        let toggle: super::super::components::Toggle =
            Rc::new(move |_, cx| view.update(cx, |this, cx| this.edit(cx, &apply)));
        h_flex()
            .gap_3()
            .items_center()
            .child(switch(id, checked, toggle, window, cx))
            .child(div().text_size(px(14.)).child(label))
            .into_any_element()
    }

    fn project_label(options: &[ed::ProjectOption], id: &str) -> String {
        options
            .iter()
            .find(|option| option.id == id)
            .map(|option| option.title.clone())
            .unwrap_or_else(|| {
                if id.is_empty() {
                    "Select a project".to_owned()
                } else {
                    id.to_owned()
                }
            })
    }

    fn model_options(&self, draft: &Draft, environment: &str) -> Vec<(String, Model)> {
        let current = self.selection(draft, environment);
        let mut options = Vec::new();
        for group in &self.data.models {
            for (slug, label, legacy) in &group.models {
                let model = (group.instance_id.clone(), slug.clone());
                if !legacy || current.as_ref() == Some(&model) {
                    options.push((format!("{} / {label}", group.name), model));
                }
            }
        }
        options
    }

    fn fields(
        &mut self,
        view: &Entity<Self>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Some(editor) = self.editor.as_ref() else {
            return Vec::new();
        };
        let theme = Tokens::of(cx);
        let draft = editor.draft.clone();
        let environment = editor.environment.clone();
        let menu = editor.menu;
        let editing = draft.editing_id.is_some();
        let continue_thread = editor.continue_thread;
        let webhook_url = editor.webhook_url.clone();
        let rotate_confirm = editor.rotate_confirm;
        let rotating = editor.rotating;
        let has_secret = editor.has_secret;
        let offers_once = environment == PRIMARY || editor.editing_once;
        let environments = self.connected_environments(cx);
        let projects = self.project_options(&environment);
        let mut out: Vec<AnyElement> = Vec::new();

        if let Some(error) = editor.error.clone() {
            out.push(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.destructive)
                            .child(error.title),
                    )
                    .child(problem(error.description, cx))
                    .into_any_element(),
            );
        }

        let env_labels: Vec<String> = environments.iter().map(|e| e.label.clone()).collect();
        let env_ids: Vec<String> = environments.iter().map(|e| e.id.clone()).collect();
        let env_current = environments
            .iter()
            .find(|e| e.id == environment)
            .map_or(environment.clone(), |e| e.label.clone());
        let env_selected = env_ids.iter().position(|id| *id == environment);
        let data_projects = self.data.projects.clone();
        out.push(
            labeled(
                "Runs on",
                Self::select(
                    view,
                    "scheduled-task-env",
                    Menu::Environment,
                    menu == Some(Menu::Environment),
                    editing || env_ids.len() < 2,
                    env_current,
                    env_labels,
                    env_selected,
                    Rc::new(move |this, index, _, cx| {
                        let Some(next) = env_ids.get(index).cloned() else {
                            return;
                        };
                        let choices: Vec<ProjectChoice> = data_projects
                            .get(&next)
                            .cloned()
                            .unwrap_or_default()
                            .into_iter()
                            .map(|p| ProjectChoice {
                                id: p.id,
                                pinned: p.pinned,
                                archived: p.archived,
                            })
                            .collect();
                        let project = ed::default_project_id(&choices, false);
                        this.edit(cx, |editor| {
                            editor.environment = next;
                            editor.draft.project_id = project;
                            editor.draft.thread_id.clear();
                            editor.draft.model = None;
                            editor.continue_thread = false;
                        });
                    }),
                    cx,
                ),
            )
            .into_any_element(),
        );

        out.push(labeled("Name", field(&self.inputs.title, false, window, cx)).into_any_element());

        let threads: Vec<ed::ThreadOption> = if environment == PRIMARY {
            self.data
                .threads
                .iter()
                .filter(|thread| thread.project_id == draft.project_id)
                .cloned()
                .collect()
        } else {
            Vec::new()
        };
        let destination_disabled = environment != PRIMARY && !continue_thread;
        let mut destination = v_flex().gap_2().child(Self::select(
            view,
            "scheduled-task-destination",
            Menu::Destination,
            menu == Some(Menu::Destination),
            destination_disabled,
            if continue_thread {
                "One thread continuing the conversation".to_owned()
            } else {
                "New thread each run".to_owned()
            },
            vec![
                "New thread each run".to_owned(),
                "One thread continuing the conversation".to_owned(),
            ],
            Some(usize::from(continue_thread)),
            Rc::new(|this, index, _, cx| {
                this.edit(cx, |editor| {
                    editor.continue_thread = index == 1;
                    if index == 0 {
                        editor.draft.thread_id.clear();
                    }
                })
            }),
            cx,
        ));
        if continue_thread {
            let thread_label = threads
                .iter()
                .find(|thread| thread.id == draft.thread_id)
                .map(|thread| thread.title.clone())
                .unwrap_or_else(|| {
                    if draft.thread_id.is_empty() {
                        "Select a thread".to_owned()
                    } else {
                        draft.thread_id.clone()
                    }
                });
            let titles: Vec<String> = threads.iter().map(|t| t.title.clone()).collect();
            let ids: Vec<String> = threads.iter().map(|t| t.id.clone()).collect();
            let selected = ids.iter().position(|id| *id == draft.thread_id);
            destination = destination
                .child(Self::select(
                    view,
                    "scheduled-task-thread",
                    Menu::Thread,
                    menu == Some(Menu::Thread),
                    environment != PRIMARY,
                    thread_label,
                    titles,
                    selected,
                    Rc::new(move |this, index, _, cx| {
                        if let Some(id) = ids.get(index).cloned() {
                            this.edit(cx, |editor| editor.draft.thread_id = id);
                        }
                    }),
                    cx,
                ))
                .child(note(
                    "Each run continues this conversation, in its workspace and with its current model. Deleting the thread also deletes this task.",
                    cx,
                ));
        }
        if draft.thread_mode.is_some() {
            destination = destination.child(note(
                "This task uses a thread mode this build does not know. It is kept as is when you save.",
                cx,
            ));
        }
        out.push(labeled("Destination", destination).into_any_element());

        let project_titles: Vec<String> = projects.iter().map(|p| p.title.clone()).collect();
        let project_ids: Vec<String> = projects.iter().map(|p| p.id.clone()).collect();
        let project_selected = project_ids.iter().position(|id| *id == draft.project_id);
        out.push(
            labeled(
                "Project",
                Self::select(
                    view,
                    "scheduled-task-project",
                    Menu::Project,
                    menu == Some(Menu::Project),
                    false,
                    Self::project_label(&projects, &draft.project_id),
                    project_titles,
                    project_selected,
                    Rc::new(move |this, index, _, cx| {
                        if let Some(id) = project_ids.get(index).cloned() {
                            this.edit(cx, |editor| {
                                editor.draft.project_id = id;
                                editor.draft.thread_id.clear();
                            });
                        }
                    }),
                    cx,
                ),
            )
            .into_any_element(),
        );

        let workspaces = [
            (Workspace::Worktree, "Create a new worktree"),
            (Workspace::Root, "Use the project checkout"),
            (Workspace::Existing, "Use a specific checkout"),
        ];
        let mut workspace = v_flex().gap_2().child(Self::select(
            view,
            "scheduled-task-workspace",
            Menu::Workspace,
            menu == Some(Menu::Workspace),
            false,
            workspaces
                .iter()
                .find(|(kind, _)| *kind == draft.workspace)
                .map_or("", |(_, label)| *label)
                .to_owned(),
            workspaces.iter().map(|(_, l)| (*l).to_owned()).collect(),
            workspaces.iter().position(|(k, _)| *k == draft.workspace),
            Rc::new(move |this, index, _, cx| {
                if let Some((kind, _)) = workspaces.get(index) {
                    let kind = *kind;
                    this.edit(cx, |editor| editor.draft.workspace = kind);
                }
            }),
            cx,
        ));
        match draft.workspace {
            Workspace::Worktree => {
                workspace = workspace
                    .child(labeled(
                        "Base branch",
                        field(&self.inputs.base_ref, false, window, cx),
                    ))
                    .child(Self::switch_row(
                        view,
                        "scheduled-task-origin",
                        "Start from origin",
                        draft.start_from_origin,
                        |editor| editor.draft.start_from_origin = !editor.draft.start_from_origin,
                        window,
                        cx,
                    ));
            }
            Workspace::Existing => {
                workspace = workspace.child(labeled(
                    "Checkout path",
                    field(&self.inputs.path, false, window, cx),
                ));
            }
            Workspace::Root => {}
        }
        out.push(labeled("Workspace", workspace).into_any_element());

        let focused = self
            .inputs
            .prompt
            .read(cx)
            .focus_handle(cx)
            .contains_focused(window, cx);
        out.push(
            labeled(
                "Prompt",
                div()
                    .w_full()
                    .rounded(px(8.))
                    .border_1()
                    .border_color(if focused { theme.ring } else { theme.input })
                    .bg(if theme.dark {
                        theme.input.opacity(0.32)
                    } else {
                        theme.background
                    })
                    .text_size(px(14.))
                    .child(
                        Textarea::new(&self.inputs.prompt)
                            .appearance(false)
                            .bordered(false)
                            .aria_label("Prompt"),
                    ),
            )
            .into_any_element(),
        );

        out.push(self.model_field(view, &draft, &environment, menu, window, cx));

        let tab_modes = [Mode::Fixed, Mode::Once, Mode::Interval, Mode::Webhook];
        let tabs = h_flex()
            .gap_2()
            .flex_wrap()
            .children(tab_modes.into_iter().map(|mode| {
                let enabled = mode != Mode::Once || offers_once;
                Self::chip(
                    view,
                    ("scheduled-task-mode", mode as usize),
                    mode_label(mode),
                    draft.mode == mode,
                    enabled,
                    move |this, window, cx| this.set_mode(mode, window, cx),
                    cx,
                )
            }));
        let mut schedule = v_flex().gap_3().child(tabs);
        schedule = match draft.mode {
            Mode::Fixed => schedule
                .child(h_flex().gap_1().flex_wrap().children(DAYS.into_iter().map(
                    |(day, label)| {
                        Self::chip(
                            view,
                            ("scheduled-task-day", day as usize),
                            label,
                            draft.weekdays.contains(&day),
                            true,
                            move |this, _, cx| {
                                this.edit(cx, |editor| {
                                    let days = &mut editor.draft.weekdays;
                                    if days.contains(&day) {
                                        if days.len() > 1 {
                                            days.remove(&day);
                                        }
                                    } else {
                                        days.insert(day);
                                    }
                                })
                            },
                            cx,
                        )
                    },
                )))
                .child(labeled(
                    "Time (24-hour, local)",
                    field(&self.inputs.time, false, window, cx),
                )),
            Mode::Once => schedule
                .child(labeled(
                    "Run at (local time)",
                    field(&self.inputs.run_at, false, window, cx),
                ))
                .child(note(
                    "Format YYYY-MM-DDTHH:MM, for example 2026-12-01T09:00. The task runs once.",
                    cx,
                )),
            Mode::Interval => schedule
                .child(labeled(
                    "Every (minutes)",
                    field(&self.inputs.interval, false, window, cx),
                ))
                .child(note(
                    "Intervals shorter than a minute from older tasks keep running until you save a new one.",
                    cx,
                )),
            Mode::Webhook => schedule.child(self.webhook_fields(
                view,
                &draft,
                webhook_url,
                rotate_confirm,
                rotating,
                has_secret,
                menu,
                window,
                cx,
            )),
        };
        out.push(labeled("Schedule", schedule).into_any_element());

        out.push(
            v_flex()
                .gap_1()
                .child(Self::switch_row(
                    view,
                    "scheduled-task-draft-enabled",
                    "Enabled",
                    draft.enabled,
                    |editor| editor.draft.enabled = !editor.draft.enabled,
                    window,
                    cx,
                ))
                .child(note("Disabled tasks stay saved but do not run.", cx))
                .into_any_element(),
        );
        out
    }

    fn model_field(
        &self,
        view: &Entity<Self>,
        draft: &Draft,
        environment: &str,
        menu: Option<Menu>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if environment != PRIMARY {
            return v_flex()
                .gap_2()
                .child(labeled(
                    "Model",
                    field(&self.inputs.model, false, window, cx),
                ))
                .child(note(
                    "Enter the provider instance and model as instanceId:model.",
                    cx,
                ))
                .into_any_element();
        }
        let options = self.model_options(draft, environment);
        let current = self.selection(draft, environment);
        let label = options
            .iter()
            .find(|(_, model)| Some(model) == current.as_ref())
            .map(|(label, _)| label.clone())
            .or_else(|| current.as_ref().map(|(i, m)| format!("{i} / {m}")))
            .unwrap_or_else(|| "No model available".to_owned());
        let selected = options
            .iter()
            .position(|(_, model)| Some(model) == current.as_ref());
        let labels: Vec<String> = options.iter().map(|(l, _)| l.clone()).collect();
        let models: Vec<Model> = options.into_iter().map(|(_, m)| m).collect();
        labeled(
            "Model",
            Self::select(
                view,
                "scheduled-task-model",
                Menu::Model,
                menu == Some(Menu::Model),
                false,
                label,
                labels,
                selected,
                Rc::new(move |this, index, _, cx| {
                    if let Some(model) = models.get(index).cloned() {
                        this.edit(cx, |editor| editor.draft.model = Some(model));
                    }
                }),
                cx,
            ),
        )
        .into_any_element()
    }

    fn set_mode(&mut self, mode: Mode, window: &mut Window, cx: &mut Context<Self>) {
        let empty_prompt = self.inputs.prompt.read(cx).value().trim().is_empty();
        let empty_run_at = text_of(&self.inputs.run_at, cx).trim().is_empty();
        if mode == Mode::Webhook && empty_prompt {
            let prompt = ed::DEFAULT_WEBHOOK_PROMPT.to_owned();
            self.inputs
                .prompt
                .update(cx, |state, cx| state.set_value(prompt, window, cx));
        }
        if mode == Mode::Once && empty_run_at {
            let next = ed::next_whole_hour(now_ms(), &self.zone);
            set_text(&self.inputs.run_at, &next, window, cx);
        }
        self.edit(cx, |editor| editor.draft.mode = mode);
    }

    #[allow(clippy::too_many_arguments)]
    fn webhook_fields(
        &mut self,
        view: &Entity<Self>,
        draft: &Draft,
        url: Option<String>,
        rotate_confirm: bool,
        rotating: bool,
        has_secret: bool,
        menu: Option<Menu>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let theme = Tokens::of(cx);
        let mut column = v_flex().gap_3();
        column = column.child(match url {
            None => note("The URL appears after you save.", cx).into_any_element(),
            Some(url) => {
                let (copy_url, rotate_view, confirm_view, cancel_view) =
                    (url.clone(), view.clone(), view.clone(), view.clone());
                let actions = if rotate_confirm {
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(note(
                            "Rotate this webhook URL? The current URL stops working.",
                            cx,
                        ))
                        .child(
                            button("scheduled-task-rotate-confirm", Variant::Primary, true, cx)
                                .child("Rotate")
                                .on_click(move |_, _, cx| {
                                    confirm_view.update(cx, |this, cx| this.rotate(cx))
                                }),
                        )
                        .child(
                            button("scheduled-task-rotate-cancel", Variant::Outline, true, cx)
                                .child("Cancel")
                                .on_click(move |_, _, cx| {
                                    cancel_view.update(cx, |this, cx| {
                                        if let Some(editor) = this.editor.as_mut() {
                                            editor.rotate_confirm = false;
                                        }
                                        cx.notify();
                                    })
                                }),
                        )
                } else {
                    h_flex()
                        .gap_2()
                        .child(
                            button("scheduled-task-copy-url", Variant::Outline, true, cx)
                                .child("Copy")
                                .on_click(move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        copy_url.clone(),
                                    ))
                                }),
                        )
                        .child(
                            button("scheduled-task-rotate", Variant::Outline, !rotating, cx)
                                .child(if rotating { "Rotating" } else { "Rotate" })
                                .on_click(move |_, _, cx| {
                                    rotate_view.update(cx, |this, cx| {
                                        if let Some(editor) = this.editor.as_mut() {
                                            editor.rotate_confirm = true;
                                        }
                                        cx.notify();
                                    })
                                }),
                        )
                };
                v_flex()
                    .gap_2()
                    .child(
                        div()
                            .w_full()
                            .px(px(11.))
                            .py(px(6.))
                            .rounded(px(8.))
                            .border_1()
                            .border_color(theme.input)
                            .text_size(px(13.))
                            .text_color(theme.muted_foreground)
                            .child(url),
                    )
                    .child(actions)
                    .into_any_element()
            }
        });
        column = column.child(note(
            "Use {{body}}, {{body.path}}, {{headers.name}} and {{query.name}} in the prompt to include request data.",
            cx,
        ));
        column = column.child(labeled(
            "Skip requests older than (minutes)",
            field(&self.inputs.age, false, window, cx),
        ));

        let mut filters = v_flex().gap_2();
        for (index, filter) in draft.filters.iter().enumerate() {
            let Some((path, value)) = self.inputs.filters.get(&filter.id).cloned() else {
                continue;
            };
            let id = filter.id;
            let ops: Vec<String> = FilterOp::ALL
                .iter()
                .map(|op| op.label().to_owned())
                .collect();
            let remove = view.clone();
            filters = filters.child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(div().w(px(150.)).child(field(&path, false, window, cx)))
                    .child(div().w(px(120.)).child(Self::select(
                        view,
                        format!("scheduled-task-filter-op-{index}"),
                        Menu::Op(id),
                        menu == Some(Menu::Op(id)),
                        false,
                        filter.op.label().to_owned(),
                        ops,
                        FilterOp::ALL.iter().position(|op| *op == filter.op),
                        Rc::new(move |this, pick, _, cx| {
                            if let Some(op) = FilterOp::ALL.get(pick).copied() {
                                this.edit(cx, |editor| {
                                    if let Some(f) =
                                        editor.draft.filters.iter_mut().find(|f| f.id == id)
                                    {
                                        f.op = op;
                                    }
                                });
                            }
                        }),
                        cx,
                    )))
                    .child(div().flex_1().child(field(
                        &value,
                        filter.op == FilterOp::Exists,
                        window,
                        cx,
                    )))
                    .child(
                        icon_button(
                            ("scheduled-task-filter-remove", index),
                            Variant::GhostMuted,
                            true,
                            cx,
                        )
                        .child("x")
                        .on_click(move |_, _, cx| {
                            remove.update(cx, |this, cx| {
                                this.collect(cx);
                                this.inputs.filters.remove(&id);
                                this.edit(cx, |editor| {
                                    editor.draft.filters =
                                        ed::remove_filter_draft(&editor.draft.filters, id);
                                });
                            })
                        }),
                    ),
            );
        }
        let add = view.clone();
        let can_add = draft.filters.len() < ed::MAX_WEBHOOK_FILTERS;
        column = column.child(labeled(
            "Filters",
            v_flex()
                .gap_2()
                .child(filters)
                .child(
                    button("scheduled-task-filter-add", Variant::Outline, can_add, cx)
                        .child("Add filter")
                        .on_click(move |_, window, cx| {
                            add.update(cx, |this, cx| {
                                this.collect(cx);
                                let filter = ed::empty_filter_draft();
                                this.add_filter_inputs(filter.id, "", "", window, cx);
                                this.edit(cx, |editor| editor.draft.filters.push(filter));
                            })
                        }),
                )
                .child(note(
                    "Only requests matching every filter run the task. Paths start with body, headers or query. For In, separate values with commas.",
                    cx,
                )),
        ));

        column = column
            .child(labeled(
                "Duplicate request key",
                field(&self.inputs.replay, false, window, cx),
            ))
            .child(note(
                "Requests with the same key run only once within 7 days.",
                cx,
            ))
            .child(labeled(
                "Coalesce key",
                field(&self.inputs.coalesce_key, false, window, cx),
            ))
            .child(labeled(
                "Coalesce window (seconds)",
                field(&self.inputs.coalesce_window, false, window, cx),
            ))
            .child(note(
                "Requests sharing a coalesce key within the window run as one task.",
                cx,
            ));

        column = column.child(Self::switch_row(
            view,
            "scheduled-task-signature",
            "Require signature",
            draft.signature_enabled,
            |editor| editor.draft.signature_enabled = !editor.draft.signature_enabled,
            window,
            cx,
        ));
        if draft.signature_enabled {
            let encodings = [Encoding::Hex, Encoding::Base64];
            let secret_note = if has_secret {
                "Leave the secret empty to keep the saved one."
            } else {
                "Requests are verified with this shared secret."
            };
            column = column
                .child(labeled(
                    "Header",
                    field(&self.inputs.header, false, window, cx),
                ))
                .child(labeled(
                    "Prefix",
                    field(&self.inputs.prefix, false, window, cx),
                ))
                .child(labeled(
                    "Encoding",
                    Self::select(
                        view,
                        "scheduled-task-encoding",
                        Menu::Encoding,
                        menu == Some(Menu::Encoding),
                        false,
                        match draft.signature_encoding {
                            Encoding::Hex => "Hex",
                            Encoding::Base64 => "Base64",
                        }
                        .to_owned(),
                        vec!["Hex".to_owned(), "Base64".to_owned()],
                        Some(
                            encodings
                                .iter()
                                .position(|e| *e == draft.signature_encoding)
                                .unwrap_or(0),
                        ),
                        Rc::new(move |this, index, _, cx| {
                            if let Some(encoding) = encodings.get(index).copied() {
                                this.edit(cx, |editor| editor.draft.signature_encoding = encoding);
                            }
                        }),
                        cx,
                    ),
                ))
                .child(labeled(
                    "Secret",
                    field(&self.inputs.secret, false, window, cx),
                ))
                .child(note(secret_note, cx));
        }
        column
    }

    /// The editor dialog over a backdrop.
    pub(super) fn editor_overlay(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let editor = self.editor.as_ref()?;
        let editing = editor.draft.editing_id.is_some();
        let saving = editor.saving;
        let view = cx.entity();
        let body = self.fields(&view, window, cx);
        let (cancel, save, dismiss) = (view.clone(), view.clone(), view.clone());
        let footer = vec![
            button(
                "scheduled-task-editor-cancel",
                Variant::Outline,
                !saving,
                cx,
            )
            .child("Cancel")
            .on_click(move |_, _, cx| {
                cancel.update(cx, |this, cx| {
                    this.editor = None;
                    cx.notify();
                })
            })
            .into_any_element(),
            button("scheduled-task-editor-save", Variant::Primary, !saving, cx)
                .child(match (saving, editing) {
                    (true, _) => "Saving",
                    (false, true) => "Save task",
                    (false, false) => "Create task",
                })
                .on_click(move |_, _, cx| save.update(cx, |this, cx| this.save(cx)))
                .into_any_element(),
        ];
        Some(dialog_of(
            "scheduled-task-editor",
            EDITOR_WIDTH,
            if editing { "Edit task" } else { "New task" },
            "Run a prompt automatically on an interval, at a fixed time, once at a set date, or when a webhook is called.".to_owned(),
            body,
            footer,
            move |_, cx| {
                dismiss.update(cx, |this, cx| {
                    if this.editor.as_ref().is_some_and(|e| !e.saving) {
                        this.editor = None;
                        cx.notify();
                    }
                })
            },
            cx,
        ))
    }
}
