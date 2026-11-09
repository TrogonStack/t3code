//! The web's `AddProjectToMachineDialog` and `addProjectToMachine`: clone the open project onto
//! another connected machine, then bring the chosen ignored files across.
//!
//! The source's calls (`server.getConfig`, `projects.listIgnoredFiles`, `projects.readFile`) go
//! through the window's own connection. The target's (`server.getConfig`, `projectClone.start`,
//! the clone stream, `projects.writeFile`) go through the saved-environment connection
//! `environments::sender` hands out. Answers of both come back as `Status::Call` under this
//! dialog's token range. The window attaches to one environment, so the web's last step, landing
//! on the cloned project, has no equivalent: the dialog closes and a toast says where it went.

use super::composer_menu::{self, OnClick, OnHover, Row, RowState};
use super::settings::tokens::Tokens;
use super::*;
use crate::clone_flow::{self as flow, IgnoredFile, IgnoredFiles, Protocol, ServerSettings};
use crate::environments;
use crate::projects::RepositoryInfo;
use crate::status::CallEvent;
use crate::usage_data;
use gpui::{EventEmitter, FocusHandle, KeyBinding, SharedString};
use gpui_component::{Disableable as _, checkbox::Checkbox};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

/// Call tokens, above every other range.
pub const TOKEN_BASE: u64 = 1 << 47;
const CONTEXT: &str = "AddToMachine";
/// `sm:max-w-xl`.
const WIDTH: f32 = 576.;
/// The middle truncation budget of a path in the source or target column, in characters.
const PATH_BUDGET: usize = 26;

actions!(add_to_machine, [CloseAddToMachine]);

pub fn bindings() -> Vec<KeyBinding> {
    vec![KeyBinding::new("escape", CloseAddToMachine, Some(CONTEXT))]
}

/// A connected machine without the project (`AddProjectTarget`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub id: String,
    pub label: String,
    pub machine: MachineKind,
}

/// What the dialog needs of the project it adds.
#[derive(Clone, Debug)]
pub struct Request {
    pub title: String,
    pub root: String,
    pub repository: RepositoryInfo,
    pub source_label: String,
    pub source_machine: MachineKind,
    pub targets: Vec<Target>,
}

/// A toast for the window to show: a failure, or where the clone went.
pub struct Notice {
    pub title: String,
    pub description: Option<String>,
    pub error: bool,
}

impl EventEmitter<Notice> for AddToMachine {}

enum Listing {
    Waiting,
    Loaded(IgnoredFiles),
    Failed(String),
}

struct Form {
    request: Request,
    target: usize,
    protocol: Option<Protocol>,
    path_override: Option<String>,
    selected: Option<HashSet<String>>,
    /// `environment.capabilities.ignoredFileListing` of the source, once known.
    can_list: Option<bool>,
    listing: Listing,
    configs: HashMap<String, ServerSettings>,
    target_open: bool,
    error: Option<String>,
    submitting: bool,
    session: u64,
}

impl Form {
    fn target(&self) -> Option<&Target> {
        let targets = &self.request.targets;
        targets.get(self.target).or_else(|| targets.first())
    }

    fn source_url(&self) -> &str {
        &self.request.repository.remote_url
    }

    fn ssh_url(&self) -> Option<String> {
        flow::url_for_protocol(self.source_url(), Protocol::Ssh)
    }

    fn https_url(&self) -> Option<String> {
        flow::url_for_protocol(self.source_url(), Protocol::Https)
    }

    fn remote_url(&self) -> String {
        match self.protocol {
            Some(Protocol::Ssh) => self.ssh_url(),
            Some(Protocol::Https) => self.https_url(),
            None => None,
        }
        .unwrap_or_else(|| self.source_url().to_owned())
    }

    fn default_path(&self) -> String {
        let settings = self
            .target()
            .and_then(|target| self.configs.get(&target.id))
            .cloned()
            .unwrap_or_default();
        flow::destination_path(
            &flow::initial_query(settings.base_directory.as_deref()),
            &flow::directory_path(self.source_url(), settings.ghq_layout),
        )
    }

    fn destination(&self) -> String {
        self.path_override
            .clone()
            .unwrap_or_else(|| self.default_path())
    }

    fn files(&self) -> &[IgnoredFile] {
        match &self.listing {
            Listing::Loaded(listed) => &listed.files,
            _ => &[],
        }
    }

    fn chosen(&self) -> HashSet<String> {
        self.selected
            .clone()
            .unwrap_or_else(|| flow::default_selection(self.files()))
    }

    /// Submitting before the file list loads would silently skip the default `.env` copies.
    fn unavailable(&self) -> bool {
        let listed = match self.can_list {
            None => false,
            Some(false) => true,
            Some(true) => !matches!(self.listing, Listing::Waiting),
        };
        self.target().is_none() || !listed
    }
}

enum Pending {
    SourceConfig { session: u64 },
    Listing { session: u64 },
    TargetConfig { session: u64, id: String },
    Read { path: String },
    Start,
    Watch { job: u64 },
    Write { job: u64, path: String },
}

/// A submit in flight: the reads of the files, then the clone.
struct Submit {
    session: u64,
    project_id: String,
    target_id: String,
    target_label: String,
    title: String,
    remote_url: String,
    destination: String,
    subdirectory: String,
    files: Vec<String>,
    reads: HashMap<String, Result<String, String>>,
}

/// What is left after the clone starts: wait for it, then write the files.
struct Job {
    project_id: String,
    target_id: String,
    target_label: String,
    cwd: String,
    subdirectory: String,
    seen: bool,
    reads: Vec<(String, Result<String, String>)>,
    failed: Vec<String>,
}

pub struct AddToMachine {
    actions: ActionSender,
    focus: FocusHandle,
    window: gpui::AnyWindowHandle,
    previous_focus: Option<FocusHandle>,
    folder: Entity<InputState>,
    scroll: gpui::ScrollHandle,
    form: Option<Form>,
    session: u64,
    next_token: u64,
    next_job: u64,
    pending: HashMap<u64, Pending>,
    submit: Option<Submit>,
    jobs: HashMap<u64, Job>,
}

impl AddToMachine {
    pub fn new(actions: ActionSender, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let folder = cx.new(|cx| InputState::new(window, cx));
        cx.subscribe_in(
            &folder,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::Change => this.folder_changed(window, cx),
                InputEvent::PressEnter { .. } => this.submit(cx),
                _ => {}
            },
        )
        .detach();
        Self {
            actions,
            focus: cx.focus_handle(),
            window: window.window_handle(),
            previous_focus: None,
            folder,
            scroll: gpui::ScrollHandle::new(),
            form: None,
            session: 0,
            next_token: TOKEN_BASE,
            next_job: 0,
            pending: HashMap::new(),
            submit: None,
            jobs: HashMap::new(),
        }
    }

    pub fn open(&mut self, request: Request, window: &mut Window, cx: &mut Context<Self>) {
        if request.targets.is_empty() {
            return;
        }
        self.session += 1;
        self.previous_focus = window.focused(cx);
        let protocol = flow::url_protocol(&request.repository.remote_url);
        let first = request.targets[0].id.clone();
        self.form = Some(Form {
            request,
            target: 0,
            protocol,
            path_override: None,
            selected: None,
            can_list: None,
            listing: Listing::Waiting,
            configs: HashMap::new(),
            target_open: false,
            error: None,
            submitting: false,
            session: self.session,
        });
        let session = self.session;
        self.call_primary(
            Call::GetConfig,
            serde_json::json!({}),
            Pending::SourceConfig { session },
            cx,
        );
        self.request_target_config(&first, cx);
        self.set_folder(window, cx);
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.form.take().is_none() {
            return;
        }
        if let Some(previous) = self.previous_focus.take() {
            window.focus(&previous, cx);
        }
        cx.notify();
    }

    fn send(
        &mut self,
        sender: Option<ActionSender>,
        call: Call,
        payload: Value,
        pending: Pending,
        cx: &mut Context<Self>,
    ) {
        self.next_token += 1;
        let token = self.next_token;
        let sent = sender.is_some_and(|sender| {
            sender
                .try_send(Action::Call {
                    token,
                    call,
                    payload,
                })
                .is_ok()
        });
        if sent {
            self.pending.insert(token, pending);
            return;
        }
        let failure = CallEvent::Failed("Not connected to that machine.".to_owned());
        self.settled(pending, failure, cx);
    }

    fn call_primary(
        &mut self,
        call: Call,
        payload: Value,
        pending: Pending,
        cx: &mut Context<Self>,
    ) {
        let sender = Some(self.actions.clone());
        self.send(sender, call, payload, pending, cx);
    }

    fn call_target(
        &mut self,
        id: &str,
        call: Call,
        payload: Value,
        pending: Pending,
        cx: &mut Context<Self>,
    ) {
        self.send(environments::sender(id), call, payload, pending, cx);
    }

    fn request_target_config(&mut self, id: &str, cx: &mut Context<Self>) {
        let session = self.session;
        self.call_target(
            id,
            Call::GetConfig,
            serde_json::json!({}),
            Pending::TargetConfig {
                session,
                id: id.to_owned(),
            },
            cx,
        );
    }

    /// Puts the folder the form means into the input.
    fn set_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = &self.form else { return };
        let text = form.destination();
        self.folder.update(cx, |input, cx| {
            if input.value() != text {
                input.set_value(text, window, cx);
            }
        });
    }

    /// The same, from where there is no window: the next frame sets it.
    fn set_folder_later(&mut self, cx: &mut Context<Self>) {
        let Some(form) = &self.form else { return };
        let text = form.destination();
        let (folder, window) = (self.folder.clone(), self.window);
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, window, cx| {
                folder.update(cx, |input, cx| {
                    if input.value() != text {
                        input.set_value(text, window, cx);
                    }
                });
            });
        });
    }

    /// `setPathOverride(resolvePastedPath(destinationPath, value))`.
    fn folder_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = self.form.as_mut() else {
            return;
        };
        let typed = self.folder.read(cx).value().to_string();
        let previous = form.destination();
        if typed == previous {
            return;
        }
        let resolved = flow::resolve_pasted_path(&previous, &typed);
        form.path_override = Some(resolved.clone());
        if resolved != typed {
            self.folder
                .update(cx, |input, cx| input.set_value(resolved, window, cx));
        }
        cx.notify();
    }

    fn pick_target(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = self.form.as_mut() else {
            return;
        };
        form.target_open = false;
        form.target = index;
        let missing = form
            .target()
            .filter(|target| !form.configs.contains_key(&target.id))
            .map(|target| target.id.clone());
        if let Some(id) = missing {
            self.request_target_config(&id, cx);
        }
        self.set_folder(window, cx);
        cx.notify();
    }

    fn set_target_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if let Some(form) = self.form.as_mut() {
            form.target_open = open;
            cx.notify();
        }
    }

    fn set_protocol(&mut self, protocol: Protocol, cx: &mut Context<Self>) {
        if let Some(form) = self.form.as_mut() {
            form.protocol = Some(protocol);
            cx.notify();
        }
    }

    fn toggle_file(&mut self, path: &str, cx: &mut Context<Self>) {
        let Some(form) = self.form.as_mut() else {
            return;
        };
        let mut next = form.chosen();
        if !next.remove(path) {
            next.insert(path.to_owned());
        }
        form.selected = Some(next);
        cx.notify();
    }

    fn toggle_all(&mut self, cx: &mut Context<Self>) {
        let Some(form) = self.form.as_mut() else {
            return;
        };
        form.selected = Some(flow::toggled_all(&form.chosen(), form.files()));
        cx.notify();
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        let Some(form) = self.form.as_mut() else {
            return;
        };
        if form.submitting || form.unavailable() {
            return;
        }
        let destination = form.destination().trim().to_owned();
        if destination.is_empty() {
            form.error = Some("Choose a folder on the other machine.".to_owned());
            return cx.notify();
        }
        let Some(target) = form.target().cloned() else {
            return;
        };
        form.submitting = true;
        form.error = None;
        let chosen = form.chosen();
        let files: Vec<String> = form
            .files()
            .iter()
            .filter(|file| chosen.contains(&file.path))
            .map(|file| file.path.clone())
            .collect();
        let request = &form.request;
        let submit = Submit {
            session: form.session,
            project_id: new_project_id(),
            target_id: target.id,
            target_label: target.label,
            title: request.title.clone(),
            remote_url: form.remote_url(),
            destination,
            subdirectory: flow::project_subdirectory(
                request.repository.root_path.as_deref(),
                &request.root,
            ),
            files: files.clone(),
            reads: HashMap::new(),
        };
        let root = request.root.clone();
        self.submit = Some(submit);
        cx.notify();
        if files.is_empty() {
            return self.start_clone(cx);
        }
        // Read first, so the copy reflects the files as they were when the user asked.
        for path in files {
            self.call_primary(
                Call::ReadFile,
                flow::read_payload(&root, &path),
                Pending::Read { path },
                cx,
            );
        }
    }

    fn start_clone(&mut self, cx: &mut Context<Self>) {
        let Some(submit) = &self.submit else { return };
        let payload = flow::start_payload(
            &submit.project_id,
            &submit.title,
            &usage_data::iso_from_ms(now_ms()),
            &submit.remote_url,
            &submit.destination,
        );
        let target = submit.target_id.clone();
        self.call_target(
            &target,
            Call::StartProjectClone,
            payload,
            Pending::Start,
            cx,
        );
    }

    /// `setError` while the dialog is up, else a toast: the user has gone on.
    fn fail_submit(&mut self, message: String, cx: &mut Context<Self>) {
        let Some(submit) = self.submit.take() else {
            return;
        };
        match self
            .form
            .as_mut()
            .filter(|form| form.session == submit.session)
        {
            Some(form) => {
                form.error = Some(message);
                form.submitting = false;
            }
            None => cx.emit(Notice {
                title: "Could not start the clone".to_owned(),
                description: Some(message),
                error: true,
            }),
        }
        cx.notify();
    }

    pub fn on_call(&mut self, token: u64, event: CallEvent, cx: &mut Context<Self>) {
        if matches!(event, CallEvent::Progress(_)) {
            let Some(Pending::Watch { job }) = self.pending.get(&token) else {
                return;
            };
            let job = *job;
            return self.clones_changed(job, token, event, cx);
        }
        if let Some(pending) = self.pending.remove(&token) {
            self.settled(pending, event, cx);
        }
    }

    fn settled(&mut self, pending: Pending, event: CallEvent, cx: &mut Context<Self>) {
        match pending {
            Pending::SourceConfig { session } => {
                let current = self.form.as_mut().filter(|form| form.session == session);
                let Some(form) = current else { return };
                let listing = match &event {
                    CallEvent::Done(value) => flow::decode_config(value).ignored_file_listing,
                    _ => false,
                };
                form.can_list = Some(listing);
                if listing {
                    let cwd = form.request.root.clone();
                    self.call_primary(
                        Call::ListIgnoredFiles,
                        flow::list_payload(&cwd),
                        Pending::Listing { session },
                        cx,
                    );
                }
            }
            Pending::Listing { session } => {
                let current = self.form.as_mut().filter(|form| form.session == session);
                let Some(form) = current else { return };
                form.listing = match event {
                    CallEvent::Done(value) => flow::decode_ignored_files(&value).map_or_else(
                        || Listing::Failed("The server sent an unreadable list.".to_owned()),
                        Listing::Loaded,
                    ),
                    CallEvent::Failed(message) => Listing::Failed(message),
                    CallEvent::Progress(_) => return,
                };
            }
            Pending::TargetConfig { session, id } => {
                let current = self.form.as_mut().filter(|form| form.session == session);
                let Some(form) = current else { return };
                let settings = match &event {
                    CallEvent::Done(value) => flow::decode_config(value),
                    _ => ServerSettings::default(),
                };
                form.configs.insert(id, settings);
                self.set_folder_later(cx);
            }
            Pending::Read { path } => {
                let Some(submit) = self.submit.as_mut() else {
                    return;
                };
                let read = match event {
                    CallEvent::Done(value) => flow::decode_read(&value),
                    CallEvent::Failed(message) => Err(message),
                    CallEvent::Progress(_) => return,
                };
                submit.reads.insert(path, read);
                if submit.reads.len() == submit.files.len() {
                    self.start_clone(cx);
                }
            }
            Pending::Start => match event {
                CallEvent::Done(value) => self.clone_started(&value, cx),
                CallEvent::Failed(message) => self.fail_submit(message, cx),
                CallEvent::Progress(_) => {}
            },
            Pending::Watch { job } => {
                // The stream ended before the clone did.
                if self.jobs.remove(&job).is_some() {
                    let message = match event {
                        CallEvent::Failed(message) => message,
                        _ => "The clone list closed.".to_owned(),
                    };
                    cx.emit(Notice {
                        title: "Lost track of the clone".to_owned(),
                        description: Some(message),
                        error: true,
                    });
                }
            }
            Pending::Write { job, path } => {
                if let (CallEvent::Failed(message), Some(job)) = (event, self.jobs.get_mut(&job)) {
                    job.failed.push(format!("{path}: {message}"));
                }
                self.write_next(job, cx);
            }
        }
        cx.notify();
    }

    /// `projectClone.start` answered: the dialog is done, and the files wait for the clone.
    fn clone_started(&mut self, value: &Value, cx: &mut Context<Self>) {
        let Some(cwd) = flow::decode_started(value) else {
            return self.fail_submit("The server sent no folder for the clone.".to_owned(), cx);
        };
        let Some(submit) = self.submit.take() else {
            return;
        };
        if self
            .form
            .as_ref()
            .is_some_and(|form| form.session == submit.session)
        {
            self.form = None;
            if let Some(previous) = self.previous_focus.take() {
                let window = self.window;
                cx.defer(move |cx| {
                    let _ = window.update(cx, |_, window, cx| window.focus(&previous, cx));
                });
            }
        }
        cx.emit(Notice {
            title: format!("Cloning {} to {}", submit.title, submit.target_label),
            description: None,
            error: false,
        });
        if submit.files.is_empty() {
            return;
        }
        self.next_job += 1;
        let id = self.next_job;
        let mut reads = Vec::new();
        for path in &submit.files {
            if let Some(read) = submit.reads.get(path) {
                reads.push((path.clone(), read.clone()));
            }
        }
        let target = submit.target_id.clone();
        self.jobs.insert(
            id,
            Job {
                project_id: submit.project_id,
                target_id: submit.target_id,
                target_label: submit.target_label,
                cwd,
                subdirectory: submit.subdirectory,
                seen: false,
                reads,
                failed: Vec::new(),
            },
        );
        self.call_target(
            &target,
            Call::WatchProjectClones,
            serde_json::json!({}),
            Pending::Watch { job: id },
            cx,
        );
    }

    /// `waitForClone`: done once the clone is, gone once it was seen and then vanished.
    fn clones_changed(&mut self, id: u64, token: u64, event: CallEvent, cx: &mut Context<Self>) {
        let CallEvent::Progress(list) = event else {
            return;
        };
        let Some(job) = self.jobs.get_mut(&id) else {
            return;
        };
        let wait = flow::clone_wait(&list, &job.project_id, &mut job.seen);
        if wait == flow::CloneWait::Pending {
            return;
        }
        self.pending.remove(&token);
        let target = job.target_id.clone();
        if let Some(sender) = environments::sender(&target) {
            let _ = sender.try_send(Action::EndCall { token });
        }
        match wait {
            flow::CloneWait::Done => self.write_next(id, cx),
            _ => {
                self.jobs.remove(&id);
            }
        }
    }

    /// Writes the next file, or reports what failed once none is left.
    fn write_next(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(job) = self.jobs.get_mut(&id) else {
            return;
        };
        while !job.reads.is_empty() {
            let (path, read) = job.reads.remove(0);
            match read {
                Err(message) => job.failed.push(format!("{path}: {message}")),
                Ok(contents) => {
                    let destination = flow::copy_target(&job.subdirectory, &path);
                    let payload = flow::write_payload(&job.cwd, &destination, &contents);
                    let target = job.target_id.clone();
                    return self.call_target(
                        &target,
                        Call::WriteFile,
                        payload,
                        Pending::Write { job: id, path },
                        cx,
                    );
                }
            }
        }
        if let Some(job) = self.jobs.remove(&id)
            && !job.failed.is_empty()
        {
            cx.emit(Notice {
                title: flow::failure_title(job.failed.len(), &job.target_label),
                description: Some(job.failed.join("\n")),
                error: true,
            });
        }
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as i64)
}

/// `Button` at the default size: `h-8 px-3 text-sm`, in the outline or primary variant.
fn button(id: &'static str, primary: bool, enabled: bool, t: &Tokens) -> gpui::Stateful<gpui::Div> {
    let (accent, input, primary_color) = (t.accent, t.input, t.primary);
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .h(px(32.))
        .px(px(12.))
        .rounded(px(8.))
        .border_1()
        .text_size(px(14.))
        .line_height(px(20.))
        .font_weight(FontWeight::MEDIUM)
        .when(primary, |button| {
            button
                .border_color(primary_color)
                .bg(primary_color)
                .text_color(gpui::white())
        })
        .when(!primary, |button| {
            button
                .border_color(input)
                .text_color(t.foreground)
                .bg(if t.dark {
                    t.input.opacity(0.32)
                } else {
                    t.popover
                })
        })
        .when(enabled, |button| {
            button.cursor_pointer().hover(move |style| {
                if primary {
                    style.opacity(0.9)
                } else {
                    style.bg(accent.opacity(0.5))
                }
            })
        })
        .when(!enabled, |button| button.opacity(0.64))
}

fn label(text: impl Into<SharedString>, t: &Tokens) -> gpui::Div {
    div()
        .text_size(px(14.))
        .line_height(px(16.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(t.foreground)
        .child(text.into())
}

fn note(text: impl Into<SharedString>, t: &Tokens) -> gpui::Div {
    div()
        .text_xs()
        .line_height(px(16.))
        .text_color(t.muted_foreground)
        .child(text.into())
}

/// The target select's trigger: `SelectTrigger size="sm"`.
#[derive(IntoElement)]
struct SelectTrigger {
    icon: Icons,
    label: SharedString,
    open: bool,
}

impl Selectable for SelectTrigger {
    fn selected(self, _: bool) -> Self {
        self
    }

    fn is_selected(&self) -> bool {
        false
    }

    fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    fn is_open(&self) -> bool {
        self.open
    }
}

impl RenderOnce for SelectTrigger {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = Tokens::of(cx);
        h_flex()
            .id("add-machine-target")
            .w_full()
            .min_w_0()
            .h(px(32.))
            .px(px(10.))
            .gap(px(6.))
            .items_center()
            .rounded(px(8.))
            .border_1()
            .border_color(t.input)
            .bg(if t.dark {
                t.input.opacity(0.32)
            } else {
                t.popover
            })
            .text_size(px(14.))
            .text_color(t.foreground)
            .cursor_pointer()
            .when(self.open, |trigger| trigger.border_color(t.ring))
            .child(Icon::new(self.icon).size(px(14.)))
            .child(div().flex_1().min_w_0().truncate().child(self.label))
            .child(
                div()
                    .text_color(t.muted_foreground)
                    .child(Icon::new(IconName::ChevronDown).size(px(14.))),
            )
    }
}

impl Render for AddToMachine {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(form) = &self.form else {
            return gpui::Empty.into_any_element();
        };
        let t = Tokens::of(cx);
        let mono = cx.theme().mono_font_family.clone();
        let viewport = window.viewport_size();
        let width = (f32::from(viewport.width) - 32.).min(WIDTH);
        let column = ((width - 48. - 112. - 24.) / 2.).max(80.);
        let target = form.target().cloned();
        let target_label = target.as_ref().map(|target| target.label.clone());
        let destination = form.destination();
        let chosen = form.chosen();
        let selected_count = chosen.len();
        let entity = cx.entity();

        let machine = |icon: Icons, name: String| {
            h_flex()
                .min_w_0()
                .gap(px(6.))
                .items_center()
                .text_size(px(14.))
                .line_height(px(20.))
                .font_weight(FontWeight::MEDIUM)
                .child(div().flex_shrink_0().child(Icon::new(icon).size(px(14.))))
                .child(div().min_w_0().truncate().child(name))
        };
        let path_line = |path: &str| {
            div()
                .min_w_0()
                .font_family(mono.clone())
                .text_xs()
                .line_height(px(16.))
                .text_color(t.muted_foreground)
                .child(super::middle_truncate(path, PATH_BUDGET))
        };

        let source = v_flex()
            .w(px(column))
            .gap_1()
            .child(machine(
                machine_icon(form.request.source_machine),
                form.request.source_label.clone(),
            ))
            .child(path_line(&form.request.root));
        let arrow = v_flex()
            .w(px(112.))
            .items_center()
            .gap(px(2.))
            .text_xs()
            .line_height(px(16.))
            .text_color(t.muted_foreground)
            .child("git clone")
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .child(div().h(px(1.)).flex_1().bg(t.border))
                    .child(
                        div()
                            .ml(px(-4.))
                            .child(Icon::new(IconName::ArrowRight).size(px(14.))),
                    ),
            )
            .child(flow::file_count_label(selected_count));
        let target_control = if form.request.targets.len() > 1 {
            self.target_select(form, column, entity.clone())
        } else {
            machine(
                target
                    .as_ref()
                    .map_or(Icons::Server, |target| machine_icon(target.machine)),
                target_label
                    .clone()
                    .unwrap_or_else(|| "No machine available".to_owned()),
            )
            .into_any_element()
        };
        let route = h_flex()
            .items_center()
            .gap_3()
            .child(source)
            .child(arrow)
            .child(
                v_flex()
                    .w(px(column))
                    .gap_1()
                    .child(target_control)
                    .child(path_line(&destination)),
            );

        let protocol = (form.ssh_url().is_some() && form.https_url().is_some()).then(|| {
            let segment = |id: &'static str, text: &'static str, which: Protocol| {
                let on = form.protocol == Some(which);
                let view = entity.clone();
                div()
                    .id(id)
                    .h(px(30.))
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .text_size(px(14.))
                    .font_weight(FontWeight::MEDIUM)
                    .cursor_pointer()
                    .text_color(if on {
                        t.accent_foreground
                    } else {
                        t.muted_foreground
                    })
                    .when(on, |segment| segment.bg(t.accent))
                    .hover(move |style| style.bg(t.accent))
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| this.set_protocol(which, cx))
                    })
                    .child(text)
            };
            h_flex()
                .self_start()
                .overflow_hidden()
                .rounded(px(8.))
                .border_1()
                .border_color(t.input)
                .child(segment("add-machine-ssh", "SSH", Protocol::Ssh))
                .child(div().w(px(1.)).bg(t.input))
                .child(segment("add-machine-https", "HTTPS", Protocol::Https))
        });
        let clone_over = v_flex()
            .gap(px(6.))
            .child(label("Clone over", &t))
            .children(protocol)
            .child(
                div()
                    .truncate()
                    .font_family(mono.clone())
                    .text_xs()
                    .line_height(px(16.))
                    .text_color(t.muted_foreground)
                    .child(form.remote_url()),
            );
        let folder = v_flex()
            .gap(px(6.))
            .child(label(
                format!(
                    "Folder on {}",
                    target_label.as_deref().unwrap_or("the other machine")
                ),
                &t,
            ))
            .child(Input::new(&self.folder));
        let ignored = (form.can_list == Some(true))
            .then(|| self.ignored_section(form, &chosen, &t, &mono, entity.clone()));
        let error = form.error.clone().map(|message| {
            div()
                .text_size(px(14.))
                .line_height(px(20.))
                .text_color(t.destructive)
                .child(message)
        });

        let unavailable = form.unavailable();
        let submitting = form.submitting;
        let cancel = button("add-machine-cancel", false, true, &t)
            .on_click(cx.listener(|this, _, window, cx| this.close(window, cx)))
            .child("Cancel");
        let enabled = !unavailable && !submitting;
        let clone = button("add-machine-submit", true, enabled, &t)
            .when(enabled, |clone| {
                clone.on_click(cx.listener(|this, _, _, cx| this.submit(cx)))
            })
            .child(flow::submit_label(submitting, target_label.as_deref()));

        let title = form
            .request
            .repository
            .display_name
            .clone()
            .unwrap_or_else(|| form.request.title.clone());
        let header = v_flex()
            .gap_2()
            .px_6()
            .pt_6()
            .pb_3()
            .child(
                div()
                    .text_size(px(20.))
                    .line_height(px(27.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Add to another machine"),
            )
            .child(
                div()
                    .text_size(px(14.))
                    .line_height(px(20.))
                    .text_color(t.muted_foreground)
                    .child(format!(
                        "Clone {title} and bring the local files git does not track."
                    )),
            );
        let panel = v_flex()
            .id("add-machine-panel")
            .track_scroll(&self.scroll)
            .overflow_y_scroll()
            .min_h_0()
            .gap_4()
            .px_6()
            .pt_1()
            .pb_6()
            .child(route)
            .child(clone_over)
            .child(folder)
            .children(ignored)
            .children(error);
        let footer = h_flex()
            .justify_end()
            .gap_2()
            .px_6()
            .py_4()
            .border_t_1()
            .border_color(t.border)
            .bg(t.muted)
            .child(cancel)
            .child(clone);
        let close = div()
            .id("add-machine-close")
            .absolute()
            .right(px(8.))
            .top(px(8.))
            .size(px(28.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(8.))
            .cursor_pointer()
            .text_color(t.muted_foreground)
            .hover(move |style| style.bg(t.accent))
            .on_click(cx.listener(|this, _, window, cx| this.close(window, cx)))
            .child(Icon::new(IconName::Close).size(px(14.)));

        div()
            .absolute()
            .inset_0()
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .on_action(
                cx.listener(|this, _: &CloseAddToMachine, window, cx| this.close(window, cx)),
            )
            .child(
                div()
                    .id("add-machine-backdrop")
                    .absolute()
                    .inset_0()
                    .occlude()
                    .bg(t.backdrop())
                    .on_click(cx.listener(|this, _, window, cx| this.close(window, cx))),
            )
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        v_flex()
                            .relative()
                            .occlude()
                            .w(px(width))
                            .max_h(viewport.height - px(32.))
                            .overflow_hidden()
                            .rounded(px(16.))
                            .border_1()
                            .border_color(t.border)
                            .bg(t.popover)
                            .text_color(t.foreground)
                            .shadow_lg()
                            .child(header)
                            .child(panel)
                            .child(footer)
                            .child(close),
                    ),
            )
            .into_any_element()
    }
}

impl AddToMachine {
    /// `Select size="sm"` over the targets, in the shared composer popup.
    fn target_select(&self, form: &Form, column: f32, entity: Entity<Self>) -> AnyElement {
        let current = form.target().cloned();
        let targets = form.request.targets.clone();
        let selected = current.as_ref().map(|target| target.id.clone());
        let open_view = entity.clone();
        let width = px(column);
        Popover::new("add-machine-select")
            .anchor(Anchor::TopLeft)
            .offset(px(4.))
            .open(form.target_open)
            .p_0()
            .appearance(false)
            .on_open_change(move |open, _, cx| {
                open_view.update(cx, |this, cx| this.set_target_open(*open, cx));
            })
            .trigger(SelectTrigger {
                icon: current
                    .as_ref()
                    .map_or(Icons::Server, |target| machine_icon(target.machine)),
                label: current
                    .as_ref()
                    .map_or_else(SharedString::default, |target| target.label.clone().into()),
                open: form.target_open,
            })
            .content(move |_, _, cx| {
                let rows: Vec<Row> = targets
                    .iter()
                    .enumerate()
                    .map(|(index, target)| {
                        let pick = entity.clone();
                        Row {
                            id: ("add-machine-row", index),
                            state: RowState {
                                selected: selected.as_deref() == Some(target.id.as_str()),
                                ..RowState::default()
                            },
                            body: h_flex()
                                .gap(px(6.))
                                .items_center()
                                .child(Icon::new(machine_icon(target.machine)).size(px(12.)))
                                .child(target.label.clone())
                                .into_any_element(),
                            on_hover: Box::new(|_: &mut App| {}) as OnHover,
                            on_click: Box::new(move |window: &mut Window, cx: &mut App| {
                                pick.update(cx, |this, cx| this.pick_target(index, window, cx));
                            }) as OnClick,
                        }
                    })
                    .collect();
                composer_menu::popup(None, Some(width), rows, cx)
            })
            .into_any_element()
    }

    /// "Copy ignored files": the header, the list and the footnote.
    fn ignored_section(
        &self,
        form: &Form,
        chosen: &HashSet<String>,
        t: &Tokens,
        mono: &SharedString,
        entity: Entity<Self>,
    ) -> gpui::Div {
        let files = form.files();
        let copyable = files.iter().any(IgnoredFile::copyable);
        let bytes: u64 = files
            .iter()
            .filter(|file| chosen.contains(&file.path))
            .map(|file| file.byte_length)
            .sum();
        let count = format!(
            "{} of {}{}",
            chosen.len(),
            files.len(),
            if bytes > 0 {
                format!(" \u{b7} {}", flow::format_bytes(bytes))
            } else {
                String::new()
            }
        );
        let all = flow::all_selected(chosen, files);
        let toggle_view = entity.clone();
        let accent = t.accent;
        let header = h_flex()
            .items_center()
            .gap_2()
            .child(div().flex_1().child(label("Copy ignored files", t)))
            .when(copyable, |header| {
                header.child(note(count, t)).child(
                    div()
                        .id("add-machine-all")
                        .flex()
                        .items_center()
                        .h(px(24.))
                        .px(px(7.))
                        .rounded(px(8.))
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .cursor_pointer()
                        .hover(move |style| style.bg(accent))
                        .on_click(move |_, _, cx| {
                            toggle_view.update(cx, |this, cx| this.toggle_all(cx))
                        })
                        .child(if all { "None" } else { "All" }),
                )
            });
        let body = match &form.listing {
            Listing::Failed(message) => Some(
                div()
                    .text_xs()
                    .line_height(px(16.))
                    .text_color(t.destructive)
                    .child(message.clone()),
            ),
            Listing::Waiting => Some(note("Finding ignored files\u{2026}", t)),
            Listing::Loaded(listed) if listed.files.is_empty() => {
                Some(note("No ignored files to copy.", t))
            }
            Listing::Loaded(_) => None,
        };
        let rows = (!files.is_empty()).then(|| {
            let last = files.len() - 1;
            v_flex()
                .id("add-machine-files")
                .max_h(px(208.))
                .overflow_y_scroll()
                .rounded(px(6.))
                .border_1()
                .border_color(t.border)
                .children(files.iter().enumerate().map(|(index, file)| {
                    let too_large = !file.copyable();
                    let path = file.path.clone();
                    let view = entity.clone();
                    h_flex()
                        .id(("add-machine-file", index))
                        .items_center()
                        .gap_2()
                        .px(px(10.))
                        .py(px(6.))
                        .text_size(px(14.))
                        .when(index != last, |row| row.border_b_1().border_color(t.border))
                        .when(too_large, |row| row.opacity(0.64))
                        .when(!too_large, |row| {
                            row.cursor_pointer().on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| this.toggle_file(&path, cx))
                            })
                        })
                        .child(
                            Checkbox::new(("add-machine-check", index))
                                .checked(chosen.contains(&file.path))
                                .disabled(too_large),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .font_family(mono.clone())
                                .text_xs()
                                .child(file.path.clone()),
                        )
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_xs()
                                .text_color(t.muted_foreground)
                                .child(flow::size_label(file)),
                        )
                }))
        });
        let truncated = match &form.listing {
            Listing::Loaded(listed) if listed.truncated => Some(note(
                format!("Showing the first {} files.", listed.files.len()),
                t,
            )),
            _ => None,
        };
        v_flex()
            .gap(px(6.))
            .child(header)
            .children(body)
            .children(rows)
            .children(truncated)
            .child(note(
                "Files are copied after the clone finishes. Folders git ignores whole, like node_modules, are left out.",
                t,
            ))
    }
}
