//! The task editor dialog of `ScheduledTasksSettings.tsx`: text inputs are the source of truth for
//! their fields and are read into the draft when saving; everything else lives in the draft.

mod pickers;
mod widgets;

use super::*;
use crate::settings_scheduled_task_editor::{
    self as ed, Draft, EditorData, Encoding, Failure, FilterOp, Mode, Model, ProjectChoice,
    Workspace,
};
use gpui::ClipboardItem;
use gpui_component::input::{InputState, Textarea, TextareaState};

use super::super::devices_panel::widgets::field;
pub(super) use pickers::Pickers;
use widgets::{Opt, Sel, field_of, segment, switch_row_of, weekday_group};

const EDITOR_WIDTH: f32 = 560.;
/// A select's popup spans the dialog body; a grid cell's spans half of it.
const FULL_WIDTH: f32 = EDITOR_WIDTH - 48.;
const HALF_WIDTH: f32 = (FULL_WIDTH - 12.) / 2.;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Menu {
    Environment,
    Project,
    Destination,
    Thread,
    Workspace,
    Model,
    BaseRef,
    Op(u64),
    Encoding,
}

pub(super) struct Editor {
    pub environment: String,
    pub draft: Draft,
    pub has_secret: bool,
    pub editing_once: bool,
    /// The saved task is an interval below the one-minute floor.
    pub legacy_interval: bool,
    pub webhook_url: Option<String>,
    pub menu: Option<Menu>,
    pub saving: bool,
    pub rotating: bool,
    pub rotate_confirm: bool,
    pub copied: bool,
    /// Whether the base ref is a remote one, once the picker or a lookup knows.
    pub base_remote: Option<bool>,
    pub error: Option<Failure>,
}

type FilterInputs = (Entity<InputState>, Entity<InputState>);

pub(super) struct Inputs {
    title: Entity<InputState>,
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
    webhook_url: Entity<InputState>,
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
            path: input("/path/to/checkout", window, cx),
            interval: input("15", window, cx),
            time: input("09:00", window, cx),
            run_at: input("YYYY-MM-DDTHH:MM", window, cx),
            header: input(ed::SIGNATURE_HEADER, window, cx),
            prefix: input("None", window, cx),
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
                    .placeholder("What should the agent do each time this runs?")
                    .auto_grow(4, 12)
            }),
            filters: HashMap::new(),
            webhook_url: input("", window, cx),
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

/// What the editor can save right now, as the web's `connected`, `tasksQuery` and
/// `editingTaskMissing`.
struct Readiness {
    connected: bool,
    loaded: bool,
    missing: bool,
    watch_error: Option<String>,
}

impl ScheduledTasksPanel {
    pub fn set_editor_data(&mut self, data: EditorData, cx: &mut Context<Self>) {
        if self.data != data {
            self.data = data;
            let in_flight = self
                .pending
                .values()
                .any(|pending| matches!(pending, Pending::Favorite(_)));
            self.pickers.settle_favorites(&self.data.favorites, in_flight);
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

    fn project_root(&self, environment: &str, project: &str) -> String {
        self.project_options(environment)
            .into_iter()
            .find(|option| option.id == project)
            .map(|option| option.root)
            .unwrap_or_default()
    }

    fn readiness(&self, editor: &Editor, cx: &App) -> Readiness {
        let connected = self
            .resolved(cx)
            .1
            .iter()
            .any(|environment| environment.id == editor.environment && environment.connected);
        let watch = self.watches.get(&editor.environment);
        let list = match watch.map(|watch| &watch.load) {
            Some(Load::Loaded(list)) => Some(list),
            _ => None,
        };
        Readiness {
            connected,
            loaded: list.is_some(),
            missing: match (&editor.draft.editing_id, list) {
                (Some(id), Some(list)) => !list.iter().any(|task| task.id == *id),
                _ => false,
            },
            watch_error: match watch.map(|watch| &watch.load) {
                Some(Load::Failed(message)) => Some(message.clone()),
                _ => None,
            },
        }
    }

    fn fill_inputs(&mut self, draft: &Draft, window: &mut Window, cx: &mut Context<Self>) {
        let inputs = &self.inputs;
        set_text(&inputs.title, &draft.title, window, cx);
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
        let value_input = input("opened", window, cx);
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
        self.pickers.reset();
        self.fill_inputs(&draft, window, cx);
        let legacy_interval = draft.editing_id.is_some()
            && draft.mode == Mode::Interval
            && draft
                .interval_minutes
                .trim()
                .parse::<f64>()
                .is_ok_and(|minutes| minutes < 1.);
        self.editor = Some(Editor {
            environment,
            has_secret: task.is_some_and(|task| task.has_secret),
            editing_once: draft.mode == Mode::Once && draft.editing_id.is_some(),
            legacy_interval,
            webhook_url: task.and_then(|task| task.webhook_url.clone()),
            draft,
            menu: None,
            saving: false,
            rotating: false,
            rotate_confirm: false,
            copied: false,
            base_remote: None,
            error: None,
        });
        self.lookup_base(cx);
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

    /// Thread modes exist on this build's own server, and on a task that already has one.
    fn offers_thread_modes(editor: &Editor) -> bool {
        editor.environment == PRIMARY || editor.draft.thread_mode.is_some()
    }

    fn collect(&mut self, cx: &App) {
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        let inputs = &self.inputs;
        let draft = &mut editor.draft;
        draft.title = text_of(&inputs.title, cx);
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
        let readiness = self.readiness(editor, cx);
        if editor.saving || !readiness.connected || !readiness.loaded || readiness.missing {
            return;
        }
        let environment = editor.environment.clone();
        let mut draft = editor.draft.clone();
        if Self::offers_thread_modes(editor) {
            draft.thread_mode = Some(json!(ed::thread_mode(&draft)));
        } else {
            draft.thread_mode = None;
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
        self.pickers.reset();
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
            self.pickers.keys = None;
            cx.notify();
            return true;
        }
        if !editor.saving {
            self.editor = None;
            self.pickers.reset();
            cx.notify();
            return true;
        }
        false
    }

    fn set_menu(&mut self, menu: Menu, open: bool, cx: &mut Context<Self>) {
        if let Some(editor) = self.editor.as_mut() {
            if !open && editor.menu != Some(menu) {
                return;
            }
            editor.menu = open.then_some(menu);
            if !open || menu != Menu::Model {
                self.pickers.keys = None;
            }
            cx.notify();
        }
    }

    fn edit(&mut self, cx: &mut Context<Self>, change: impl FnOnce(&mut Editor)) {
        if let Some(editor) = self.editor.as_mut() {
            change(editor);
            editor.menu = None;
            editor.error = None;
            self.pickers.keys = None;
            cx.notify();
        }
    }

    fn select(view: &Entity<Self>, spec: Sel, pick: Pick, cx: &App) -> AnyElement {
        let menu = spec.menu;
        let (opened, picked) = (view.clone(), view.clone());
        widgets::select(
            spec,
            move |next, _, cx| opened.update(cx, |this, cx| this.set_menu(menu, next, cx)),
            Rc::new(move |index, window, cx| {
                let pick = pick.clone();
                picked.update(cx, |this, cx| pick(this, index, window, cx))
            }),
            cx,
        )
    }

    fn project_label(options: &[ed::ProjectOption], id: &str) -> (String, bool) {
        match options.iter().find(|option| option.id == id) {
            Some(option) => (option.title.clone(), false),
            None if id.is_empty() => ("Select a project".to_owned(), true),
            None => (id.to_owned(), false),
        }
    }

    fn change_environment(&mut self, next: String, cx: &mut Context<Self>) {
        let choices: Vec<ProjectChoice> = self
            .project_options(&next)
            .into_iter()
            .map(|p| ProjectChoice {
                id: p.id,
                pinned: p.pinned,
                archived: p.archived,
            })
            .collect();
        let project = ed::default_project_id(&choices, false);
        self.edit(cx, |editor| {
            editor.environment = next;
            let draft = &mut editor.draft;
            ed::unbind_from_thread(draft);
            draft.project_id = project;
            draft.model = None;
            draft.base_model_selection = None;
            draft.base_ref = "main".to_owned();
            draft.start_from_origin = true;
            draft.existing_worktree_path.clear();
            editor.base_remote = None;
        });
        self.inputs.path.update(cx, |_, _| {});
        self.lookup_base(cx);
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
        let webhook_url = editor.webhook_url.clone();
        let rotate_confirm = editor.rotate_confirm;
        let rotating = editor.rotating;
        let copied = editor.copied;
        let has_secret = editor.has_secret;
        let legacy_interval = editor.legacy_interval;
        let base_remote = editor.base_remote.unwrap_or(false);
        let offers_modes = Self::offers_thread_modes(editor);
        let offers_once = environment == PRIMARY || editor.editing_once;
        let readiness = self.readiness(editor, cx);
        let error = editor.error.clone();
        let environments = self.connected_environments(cx);
        let projects = self.project_options(&environment);
        let mut out: Vec<AnyElement> = Vec::new();

        if let Some(error) = error {
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
        if !readiness.connected {
            out.push(problem("Reconnect this environment before saving.", cx).into_any_element());
        }

        let options: Vec<Opt> = environments
            .iter()
            .map(|e| Opt {
                label: e.label.clone(),
                secondary: Some(e.address.clone()),
                lead: Some(crate::ui::machine_icon(e.machine)),
            })
            .collect();
        let env_ids: Vec<String> = environments.iter().map(|e| e.id.clone()).collect();
        let current_env = environments.iter().find(|e| e.id == environment);
        out.push(
            field_of(
                "Runs on",
                None,
                Self::select(
                    view,
                    Sel {
                        id: "scheduled-task-env".into(),
                        menu: Menu::Environment,
                        open: menu == Some(Menu::Environment),
                        disabled: editing || env_ids.len() < 2,
                        lead: current_env.map(|e| crate::ui::machine_icon(e.machine)),
                        current: current_env.map_or(environment.clone(), |e| e.label.clone()),
                        placeholder: false,
                        selected: env_ids.iter().position(|id| *id == environment),
                        options,
                        width: FULL_WIDTH,
                    },
                    Rc::new(move |this, index, _, cx| {
                        if let Some(next) = env_ids.get(index).cloned() {
                            this.change_environment(next, cx);
                        }
                    }),
                    cx,
                ),
                cx,
            )
            .into_any_element(),
        );

        if let Some(message) = readiness.watch_error.clone() {
            out.push(
                div()
                    .text_sm()
                    .text_color(theme.destructive)
                    .child(message)
                    .into_any_element(),
            );
        }
        if readiness.missing {
            out.push(problem("This scheduled task no longer exists.", cx).into_any_element());
        }

        out.push(
            field_of(
                "Name",
                None,
                field(&self.inputs.title, false, window, cx),
                cx,
            )
            .into_any_element(),
        );

        out.push(self.destination_field(
            view,
            &draft,
            &environment,
            offers_modes,
            menu,
            window,
            cx,
        ));

        let project_ids: Vec<String> = projects.iter().map(|p| p.id.clone()).collect();
        let project_options: Vec<Opt> = projects
            .iter()
            .map(|project| {
                let duplicated = projects
                    .iter()
                    .filter(|other| other.title == project.title)
                    .count()
                    > 1;
                Opt {
                    label: project.title.clone(),
                    secondary: duplicated.then(|| project.root.clone()),
                    lead: None,
                }
            })
            .collect();
        let (project_label, project_placeholder) =
            Self::project_label(&projects, &draft.project_id);
        let project = field_of(
            "Project",
            None,
            Self::select(
                view,
                Sel {
                    id: "scheduled-task-project".into(),
                    menu: Menu::Project,
                    open: menu == Some(Menu::Project),
                    disabled: false,
                    lead: None,
                    current: project_label,
                    placeholder: project_placeholder,
                    selected: project_ids.iter().position(|id| *id == draft.project_id),
                    options: project_options,
                    width: HALF_WIDTH,
                },
                Rc::new(move |this, index, _, cx| {
                    if let Some(id) = project_ids.get(index).cloned() {
                        this.edit(cx, |editor| {
                            ed::change_project(&mut editor.draft, &id);
                            editor.base_remote = None;
                        });
                        this.lookup_base(cx);
                    }
                }),
                cx,
            ),
            cx,
        );

        let workspaces = [
            (Workspace::Worktree, "Create a new worktree"),
            (Workspace::Root, "Use the project checkout"),
            (Workspace::Existing, "Use a specific checkout"),
        ];
        let workspace = field_of(
            "Workspace",
            None,
            Self::select(
                view,
                Sel {
                    id: "scheduled-task-workspace".into(),
                    menu: Menu::Workspace,
                    open: menu == Some(Menu::Workspace),
                    disabled: false,
                    lead: None,
                    current: workspaces
                        .iter()
                        .find(|(kind, _)| *kind == draft.workspace)
                        .map_or("", |(_, label)| *label)
                        .to_owned(),
                    placeholder: false,
                    selected: workspaces.iter().position(|(k, _)| *k == draft.workspace),
                    options: workspaces.iter().map(|(_, l)| Opt::plain(*l)).collect(),
                    width: HALF_WIDTH,
                },
                Rc::new(move |this, index, _, cx| {
                    if let Some((kind, _)) = workspaces.get(index) {
                        let kind = *kind;
                        this.edit(cx, |editor| editor.draft.workspace = kind);
                    }
                }),
                cx,
            ),
            cx,
        );
        out.push(
            h_flex()
                .gap_3()
                .items_start()
                .child(div().flex_1().min_w_0().child(project))
                .child(div().flex_1().min_w_0().child(workspace))
                .into_any_element(),
        );

        match draft.workspace {
            Workspace::Worktree => out.push(
                field_of(
                    "Base branch",
                    None,
                    self.base_branch(view, &draft, &environment, base_remote, menu, cx),
                    cx,
                )
                .into_any_element(),
            ),
            Workspace::Existing => out.push(
                field_of(
                    "Checkout path",
                    None,
                    field(&self.inputs.path, false, window, cx),
                    cx,
                )
                .into_any_element(),
            ),
            Workspace::Root => {}
        }

        let focused = self
            .inputs
            .prompt
            .read(cx)
            .focus_handle(cx)
            .contains_focused(window, cx);
        out.push(
            field_of(
                "Prompt",
                None,
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
                cx,
            )
            .into_any_element(),
        );

        out.push(self.model_field(view, &draft, &environment, menu, window, cx));

        let tab_modes = [Mode::Fixed, Mode::Once, Mode::Interval, Mode::Webhook];
        let tabs = widgets::segments(cx).children(tab_modes.into_iter().filter_map(|mode| {
            let visible = mode != Mode::Once || offers_once || draft.mode == Mode::Once;
            let view = view.clone();
            visible.then(|| {
                segment(
                    ("scheduled-task-mode", mode as usize),
                    mode_label(mode),
                    draft.mode == mode,
                    move |_, window, cx| {
                        view.update(cx, |this, cx| this.set_mode(mode, window, cx))
                    },
                    cx,
                )
            })
        }));
        let mut schedule = v_flex().gap_3();
        if legacy_interval {
            schedule = schedule.child(
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child("This task uses a legacy interval below one minute. Saving updates it to at least one minute."),
            );
        }
        schedule = schedule.child(
            h_flex()
                .flex_wrap()
                .items_center()
                .justify_between()
                .gap_2()
                .child(div().text_size(px(14.)).child("Schedule"))
                .child(tabs),
        );
        let inline = |label: &'static str| {
            div()
                .text_size(px(14.))
                .flex_shrink_0()
                .child(label)
                .into_any_element()
        };
        let small = |text: String| {
            div()
                .text_xs()
                .flex_shrink_0()
                .text_color(theme.muted_foreground)
                .child(text)
                .into_any_element()
        };
        schedule = match draft.mode {
            Mode::Fixed => {
                let toggles: Vec<_> = DAYS
                    .into_iter()
                    .map(|(day, label)| {
                        let view = view.clone();
                        (
                            ("scheduled-task-day", day as usize),
                            label,
                            draft.weekdays.contains(&day),
                            Rc::new(move |_: &mut Window, cx: &mut App| {
                                view.update(cx, |this, cx| {
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
                                })
                            }) as Rc<dyn Fn(&mut Window, &mut App)>,
                        )
                    })
                    .collect();
                schedule.child(
                    h_flex()
                        .flex_wrap()
                        .items_center()
                        .gap_3()
                        .child(
                            h_flex()
                                .gap_2()
                                .items_center()
                                .child(inline("Run at"))
                                .child(
                                    div()
                                        .w(px(128.))
                                        .child(field(&self.inputs.time, false, window, cx)),
                                )
                                .child(small(self.zone.name().to_owned()))
                                .child(small("on".to_owned())),
                        )
                        .child(weekday_group(toggles, cx)),
                )
            }
            Mode::Once => schedule.child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(inline("Run on"))
                    .child(
                        div()
                            .w(px(224.))
                            .child(field(&self.inputs.run_at, false, window, cx)),
                    ),
            ),
            Mode::Interval => schedule.child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(inline("Run every"))
                    .child(
                        div()
                            .w(px(96.))
                            .child(field(&self.inputs.interval, false, window, cx)),
                    )
                    .child(small("minutes".to_owned())),
            ),
            Mode::Webhook => schedule.child(self.webhook_fields(
                view,
                &draft,
                webhook_url,
                (rotate_confirm, rotating, copied),
                has_secret,
                menu,
                window,
                cx,
            )),
        };
        out.push(schedule.into_any_element());

        out.push(switch_row_of(
            "Enabled",
            "Disabled tasks stay saved but do not run.",
            "scheduled-task-draft-enabled",
            draft.enabled,
            Self::toggle_with(view, |editor| {
                editor.draft.enabled = !editor.draft.enabled
            }),
            window,
            cx,
        ));
        out
    }

    fn toggle_with(
        view: &Entity<Self>,
        apply: impl Fn(&mut Editor) + 'static,
    ) -> super::super::components::Toggle {
        let view = view.clone();
        Rc::new(move |_, cx| view.update(cx, |this, cx| this.edit(cx, &apply)))
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
        (rotate_confirm, rotating, copied): (bool, bool, bool),
        has_secret: bool,
        menu: Option<Menu>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let mut column = v_flex().gap_4();
        column = column.child(match url {
            None => div()
                .text_sm()
                .text_color(Tokens::of(cx).muted_foreground)
                .child("The URL appears after you save.")
                .into_any_element(),
            Some(url) => {
                set_text(&self.inputs.webhook_url, &url, window, cx);
                let (copy_url, copy_view, rotate_view, confirm_view, cancel_view) = (
                    url.clone(),
                    view.clone(),
                    view.clone(),
                    view.clone(),
                    view.clone(),
                );
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
                        .into_any_element()
                } else {
                    h_flex()
                        .gap_2()
                        .child(
                            button("scheduled-task-copy-url", Variant::Outline, true, cx)
                                .child(Icon::new(Icons::Copy).size(px(14.)))
                                .child(if copied { "Copied" } else { "Copy" })
                                .on_click(move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        copy_url.clone(),
                                    ));
                                    copy_view.update(cx, |this, cx| this.mark_copied(cx));
                                }),
                        )
                        .child(
                            button("scheduled-task-rotate", Variant::Outline, !rotating, cx)
                                .child("Rotate")
                                .on_click(move |_, _, cx| {
                                    rotate_view.update(cx, |this, cx| {
                                        if let Some(editor) = this.editor.as_mut() {
                                            editor.rotate_confirm = true;
                                        }
                                        cx.notify();
                                    })
                                }),
                        )
                        .into_any_element()
                };
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(field(&self.inputs.webhook_url, true, window, cx)),
                    )
                    .child(actions)
                    .into_any_element()
            }
        });
        column = column.child(note(
            "Each request runs the prompt. Use {{body.path}}, {{headers.name}}, {{query.name}}, {{body}} or {{request}} in the prompt; only what it names reaches the agent.",
            cx,
        ));
        column = column.child(field_of(
            "Skip requests older than",
            Some("minutes, optional"),
            field(&self.inputs.age, false, window, cx),
            cx,
        ));

        let theme = Tokens::of(cx);
        let mut filters = v_flex().gap_2();
        for (index, filter) in draft.filters.iter().enumerate() {
            let Some((path, value)) = self.inputs.filters.get(&filter.id).cloned() else {
                continue;
            };
            let id = filter.id;
            let ops: Vec<Opt> = FilterOp::ALL.iter().map(|op| Opt::plain(op.label())).collect();
            let remove = view.clone();
            value.update(cx, |state, cx| {
                let placeholder = if filter.op == FilterOp::In {
                    "opened, reopened"
                } else {
                    "opened"
                };
                state.set_placeholder(placeholder, window, cx);
            });
            filters = filters.child(
                v_flex()
                    .gap_2()
                    .pt_1()
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(div().flex_1().min_w_0().child(field(&path, false, window, cx)))
                            .child(
                                icon_button(
                                    ("scheduled-task-filter-remove", index),
                                    Variant::GhostMuted,
                                    true,
                                    cx,
                                )
                                .child(Icon::new(Icons::Trash).size(px(14.)))
                                .on_click(move |_, _, cx| {
                                    remove.update(cx, |this, cx| {
                                        this.collect(cx);
                                        this.inputs.filters.remove(&id);
                                        this.edit(cx, |editor| {
                                            editor.draft.filters = ed::remove_filter_draft(
                                                &editor.draft.filters,
                                                id,
                                            );
                                        });
                                    })
                                }),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(div().w(px(176.)).flex_shrink_0().child(Self::select(
                                view,
                                Sel {
                                    id: format!("scheduled-task-filter-op-{index}").into(),
                                    menu: Menu::Op(id),
                                    open: menu == Some(Menu::Op(id)),
                                    disabled: false,
                                    lead: None,
                                    current: filter.op.label().to_owned(),
                                    placeholder: false,
                                    selected: FilterOp::ALL.iter().position(|op| *op == filter.op),
                                    options: ops,
                                    width: 176.,
                                },
                                Rc::new(move |this, pick, _, cx| {
                                    if let Some(op) = FilterOp::ALL.get(pick).copied() {
                                        this.edit(cx, |editor| {
                                            if let Some(f) = editor
                                                .draft
                                                .filters
                                                .iter_mut()
                                                .find(|f| f.id == id)
                                            {
                                                f.op = op;
                                            }
                                        });
                                    }
                                }),
                                cx,
                            )))
                            .when(filter.op != FilterOp::Exists, |row| {
                                row.child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .child(field(&value, false, window, cx)),
                                )
                            }),
                    ),
            );
        }
        let add = view.clone();
        let can_add = draft.filters.len() < ed::MAX_WEBHOOK_FILTERS;
        column = column.child(
            v_flex()
                .gap_2()
                .child(
                    h_flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(
                            v_flex()
                                .min_w_0()
                                .gap_1()
                                .child(div().text_size(px(14.)).child("Filters"))
                                .child(
                                    div()
                                        .text_size(px(14.))
                                        .text_color(theme.muted_foreground)
                                        .child("Run only when every filter matches. Other requests are logged as filtered. Separate \u{201c}is one of\u{201d} values with commas; a value can't contain a comma."),
                                ),
                        )
                        .child(
                            button("scheduled-task-filter-add", Variant::Outline, can_add, cx)
                                .child(Icon::new(Icons::Plus).size(px(14.)))
                                .child("Add filter")
                                .on_click(move |_, window, cx| {
                                    add.update(cx, |this, cx| {
                                        if !can_add {
                                            return;
                                        }
                                        this.collect(cx);
                                        let filter = ed::empty_filter_draft();
                                        this.add_filter_inputs(filter.id, "", "", window, cx);
                                        this.edit(cx, |editor| editor.draft.filters.push(filter));
                                    })
                                }),
                        ),
                )
                .child(filters),
        );

        column = column
            .child(field_of(
                "Duplicate request key",
                Some("template, optional"),
                field(&self.inputs.replay, false, window, cx),
                cx,
            ))
            .child(note(
                "Requests with the same key run only once within 7 days.",
                cx,
            ))
            .child(field_of(
                "Coalesce key",
                Some("template, optional"),
                field(&self.inputs.coalesce_key, false, window, cx),
                cx,
            ))
            .child(field_of(
                "Coalesce window",
                Some("seconds, 10\u{2013}3600"),
                field(&self.inputs.coalesce_window, false, window, cx),
                cx,
            ))
            .child(note(
                "Coalescing runs the latest payload once when the window from the first delivery closes. Empty keys run normally.",
                cx,
            ));

        column = column.child(switch_row_of(
            "Require signature",
            "Reject requests without a valid HMAC-SHA256 signature of the body.",
            "scheduled-task-signature",
            draft.signature_enabled,
            Self::toggle_with(view, |editor| {
                editor.draft.signature_enabled = !editor.draft.signature_enabled
            }),
            window,
            cx,
        ));
        if draft.signature_enabled {
            let encodings = [Encoding::Hex, Encoding::Base64];
            self.inputs.secret.update(cx, |state, cx| {
                state.set_placeholder(
                    if has_secret {
                        "Unchanged"
                    } else {
                        "Shared secret"
                    },
                    window,
                    cx,
                );
            });
            let encoding_label = |encoding: Encoding| match encoding {
                Encoding::Hex => "hex",
                Encoding::Base64 => "base64",
            };
            let grid_cell = |child: gpui::Div| div().flex_1().min_w_0().child(child);
            column = column.child(
                v_flex()
                    .gap_3()
                    .child(
                        h_flex()
                            .gap_3()
                            .items_start()
                            .child(grid_cell(field_of(
                                "Header",
                                None,
                                field(&self.inputs.header, false, window, cx),
                                cx,
                            )))
                            .child(grid_cell(field_of(
                                "Prefix",
                                None,
                                field(&self.inputs.prefix, false, window, cx),
                                cx,
                            ))),
                    )
                    .child(
                        h_flex()
                            .gap_3()
                            .items_start()
                            .child(grid_cell(field_of(
                                "Encoding",
                                None,
                                Self::select(
                                    view,
                                    Sel {
                                        id: "scheduled-task-encoding".into(),
                                        menu: Menu::Encoding,
                                        open: menu == Some(Menu::Encoding),
                                        disabled: false,
                                        lead: None,
                                        current: encoding_label(draft.signature_encoding)
                                            .to_owned(),
                                        placeholder: false,
                                        selected: encodings
                                            .iter()
                                            .position(|e| *e == draft.signature_encoding),
                                        options: encodings
                                            .iter()
                                            .map(|e| Opt::plain(encoding_label(*e)))
                                            .collect(),
                                        width: HALF_WIDTH,
                                    },
                                    Rc::new(move |this, index, _, cx| {
                                        if let Some(encoding) = encodings.get(index).copied() {
                                            this.edit(cx, |editor| {
                                                editor.draft.signature_encoding = encoding
                                            });
                                        }
                                    }),
                                    cx,
                                ),
                                cx,
                            )))
                            .child(grid_cell(field_of(
                                "Secret",
                                None,
                                field(&self.inputs.secret, false, window, cx),
                                cx,
                            ))),
                    ),
            );
        }
        column
    }

    fn mark_copied(&mut self, cx: &mut Context<Self>) {
        if let Some(editor) = self.editor.as_mut() {
            editor.copied = true;
        }
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(2))
                .await;
            this.update(cx, |this, cx| {
                if let Some(editor) = this.editor.as_mut() {
                    editor.copied = false;
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
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
        let readiness = self.readiness(editor, cx);
        let can_save = !saving && readiness.connected && readiness.loaded && !readiness.missing;
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
                    if this.editor.as_ref().is_some_and(|e| !e.saving) {
                        this.editor = None;
                        this.pickers.reset();
                        cx.notify();
                    }
                })
            })
            .into_any_element(),
            button("scheduled-task-editor-save", Variant::Primary, can_save, cx)
                .child(if editing { "Save task" } else { "Create task" })
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
                        this.pickers.reset();
                        cx.notify();
                    }
                })
            },
            cx,
        ))
    }
}

