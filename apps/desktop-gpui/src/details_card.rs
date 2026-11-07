//! The thread details card (`ThreadDetailsCard.tsx`): the editor split button
//! (`OpenInPicker.tsx`), the branch picker (`BranchToolbarBranchSelector.tsx`), the git action
//! split button (`GitActionsControl.tsx`), the changes row and the usage limits
//! (`ThreadDetailsUsage.tsx`). The entity owns the open popovers, the calls in flight and the
//! progress of a running action; `MainView` feeds it the thread's state through `sync`.

use crate::assets::editor_icon;
use crate::connection::{Action, ActionSender, Call};
use crate::details::{self, ChangedFile, Density, Inputs, Show, VcsStatus};
use crate::git_actions::{
    self, Action as GitAction, MenuId, Outcome, Progress, QuickKind, Terminology,
};
use crate::refs::{self, Page, Tag};
use crate::status::CallEvent;
use crate::usage::{self, Limits, Tone};
use gpui::{
    Anchor, AnyElement, App, Context, Entity, EventEmitter, Focusable as _, FontWeight, Hsla,
    InteractiveElement as _, IntoElement, ParentElement, Render, RenderOnce, ScrollHandle,
    StatefulInteractiveElement as _, Styled, WeakEntity, Window, div, img, prelude::*, px,
    relative,
};
use gpui_component::{
    ActiveTheme, Disableable, Icon, IconName, Selectable, Sizable, WindowExt as _,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    h_flex,
    input::{Input, InputEvent, InputState, Textarea, TextareaState},
    popover::Popover,
    tooltip::Tooltip,
    v_flex,
};
use gpui_kit_assets::IconName as Icons;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// `GIT_ACTION_SUCCESS_VISIBLE_MS`.
const SUCCESS_VISIBLE: Duration = Duration::from_secs(10);
/// The ref picker's page size (`REF_PAGE_SIZE`).
const REF_PAGE: u64 = 100;

/// What `MainView` knows about the thread the card is for.
#[derive(Clone)]
pub struct Props {
    pub cwd: String,
    /// The project's own checkout, which decides what the worktree tag means.
    pub project_cwd: String,
    pub status: Option<VcsStatus>,
    pub editors: Vec<&'static str>,
    pub limits: Option<Limits>,
    pub thread_id: Option<String>,
    pub project_id: Option<String>,
    pub has_run: bool,
    pub max_height: f32,
}

enum Pending {
    Refs { query: String, append: bool },
    Switch,
    Create,
    Pull,
    Stacked,
    Refresh,
}

/// A git action in flight: the primary button shows its progress.
struct Run {
    started: Instant,
    progress: Progress,
}

enum Notice {
    Success {
        title: String,
        description: Option<String>,
    },
    Error(String),
}

/// The Changes row asks the view to open the right panel's Diff, like the web's.
pub struct OpenDiff;

impl EventEmitter<OpenDiff> for DetailsCard {}

pub struct DetailsCard {
    actions: ActionSender,
    props: Option<Props>,
    /// The editor last picked or launched, in place of the web's `t3code:last-editor`.
    pick: Option<String>,
    editor_open: bool,
    ref_open: bool,
    git_open: bool,
    search: Entity<InputState>,
    ref_scroll: ScrollHandle,
    refs: Page,
    refs_query: String,
    refs_loading: bool,
    refs_error: Option<String>,
    next_token: u64,
    pending: HashMap<u64, Pending>,
    run: Option<Run>,
    notice: Option<Notice>,
    notice_epoch: u64,
}

impl DetailsCard {
    pub fn new(actions: ActionSender, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search refs..."));
        cx.subscribe(&search, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.query_changed(cx);
            }
        })
        .detach();
        Self {
            actions,
            props: None,
            pick: None,
            editor_open: false,
            ref_open: false,
            git_open: false,
            search,
            ref_scroll: ScrollHandle::new(),
            refs: Page::default(),
            refs_query: String::new(),
            refs_loading: false,
            refs_error: None,
            next_token: 0,
            pending: HashMap::new(),
            run: None,
            notice: None,
            notice_epoch: 0,
        }
    }

    /// Takes the thread's current state; a new directory drops what was loaded for the last one.
    pub fn sync(&mut self, props: Props) {
        if self.props.as_ref().is_none_or(|old| old.cwd != props.cwd) {
            self.refs = Page::default();
            self.refs_query.clear();
            self.refs_error = None;
            self.notice = None;
            self.close_popovers();
        }
        self.props = Some(props);
    }

    pub fn close_popovers(&mut self) {
        self.editor_open = false;
        self.ref_open = false;
        self.git_open = false;
    }

    fn preferred(&self) -> Option<&'static str> {
        let props = self.props.as_ref()?;
        details::preferred_editor(self.pick.as_deref(), &props.editors)
    }

    fn open_in_editor(&mut self, editor: &'static str, cx: &mut Context<Self>) {
        let Some(props) = &self.props else { return };
        self.pick = Some(editor.to_owned());
        let action = Action::OpenInEditor {
            cwd: props.cwd.clone(),
            editor: editor.to_owned(),
        };
        if self.actions.try_send(action).is_err() {
            eprintln!("[details] could not ask to open the editor");
        }
        cx.notify();
    }

    /// The web's `editor.openFavorite`.
    pub fn open_favorite(&mut self, cx: &mut Context<Self>) {
        if let Some(editor) = self.preferred() {
            self.open_in_editor(editor, cx);
        }
    }

    fn call(&mut self, call: Call, payload: Value, pending: Pending) {
        self.next_token += 1;
        let token = self.next_token;
        self.pending.insert(token, pending);
        let action = Action::Call {
            token,
            call,
            payload,
        };
        if self.actions.try_send(action).is_err() {
            self.pending.remove(&token);
            eprintln!("[details] could not send the request");
        }
    }

    fn refresh_status(&mut self) {
        let Some(cwd) = self.props.as_ref().map(|props| props.cwd.clone()) else {
            return;
        };
        self.call(
            Call::RefreshStatus,
            json!({"cwd": cwd, "localOnly": false}),
            Pending::Refresh,
        );
    }

    fn list_refs(&mut self, append: bool) {
        let Some(cwd) = self.props.as_ref().map(|props| props.cwd.clone()) else {
            return;
        };
        let mut payload = json!({"cwd": cwd, "limit": REF_PAGE});
        if !self.refs_query.is_empty() {
            payload["query"] = json!(self.refs_query);
        }
        if append {
            let Some(cursor) = self.refs.next_cursor else {
                return;
            };
            payload["cursor"] = json!(cursor);
        }
        self.refs_loading = true;
        self.refs_error = None;
        let query = self.refs_query.clone();
        self.call(Call::ListRefs, payload, Pending::Refs { query, append });
    }

    fn query_changed(&mut self, cx: &mut Context<Self>) {
        let query = self.search.read(cx).value().to_string();
        if query == self.refs_query {
            return;
        }
        self.refs_query = query;
        self.refs = Page::default();
        self.list_refs(false);
        cx.notify();
    }

    fn set_ref_open(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.ref_open = open;
        if open {
            self.refs_query.clear();
            self.refs = Page::default();
            self.search
                .update(cx, |state, cx| state.set_value("", window, cx));
            self.list_refs(false);
        }
        cx.notify();
    }

    fn switch_ref(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(cwd) = self.props.as_ref().map(|props| props.cwd.clone()) else {
            return;
        };
        self.ref_open = false;
        self.notice = None;
        self.call(
            Call::SwitchRef,
            json!({"cwd": cwd, "refName": name}),
            Pending::Switch,
        );
        cx.notify();
    }

    fn create_ref(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(cwd) = self.props.as_ref().map(|props| props.cwd.clone()) else {
            return;
        };
        self.ref_open = false;
        self.notice = None;
        self.call(
            Call::CreateRef,
            json!({"cwd": cwd, "refName": name, "switchRef": true}),
            Pending::Create,
        );
        cx.notify();
    }

    fn start_run(&mut self, cx: &mut Context<Self>) {
        self.notice = None;
        self.run = Some(Run {
            started: Instant::now(),
            progress: Progress::default(),
        });
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let alive = this.update(cx, |this, cx| {
                    cx.notify();
                    this.run.is_some()
                });
                if !matches!(alive, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    fn finish(&mut self, outcome: Outcome, cx: &mut Context<Self>) {
        self.run = None;
        self.notice_epoch += 1;
        let epoch = self.notice_epoch;
        self.notice = Some(match outcome {
            Outcome::Success { title, description } => {
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(SUCCESS_VISIBLE).await;
                    this.update(cx, |this, cx| {
                        if this.notice_epoch == epoch {
                            this.notice = None;
                            cx.notify();
                        }
                    })
                    .ok();
                })
                .detach();
                Notice::Success { title, description }
            }
            Outcome::Failed(message) => Notice::Error(message),
        });
        self.refresh_status();
        cx.notify();
    }

    /// `runGitAction`: one `git.runStackedAction` stream, its progress shown on the primary button.
    pub fn start_stacked(
        &mut self,
        action: GitAction,
        message: Option<String>,
        feature_branch: bool,
        file_paths: Option<Vec<String>>,
        cx: &mut Context<Self>,
    ) {
        let Some(props) = &self.props else { return };
        if self.run.is_some() {
            return;
        }
        let mut payload = json!({
            "actionId": action_id(),
            "cwd": props.cwd,
            "action": action.wire(),
        });
        if let Some(message) = message.filter(|message| !message.trim().is_empty()) {
            payload["commitMessage"] = json!(message.trim());
        }
        if feature_branch {
            payload["featureBranch"] = json!(true);
        }
        if let Some(paths) = file_paths {
            payload["filePaths"] = json!(paths);
        }
        if let Some(id) = &props.thread_id {
            payload["threadId"] = json!(id);
        }
        if let Some(id) = &props.project_id {
            payload["projectId"] = json!(id);
        }
        self.start_run(cx);
        self.call(Call::RunStackedAction, payload, Pending::Stacked);
    }

    fn start_pull(&mut self, cx: &mut Context<Self>) {
        let Some(cwd) = self.props.as_ref().map(|props| props.cwd.clone()) else {
            return;
        };
        if self.run.is_some() {
            return;
        }
        self.start_run(cx);
        if let Some(run) = &mut self.run {
            run.progress.phase = Some("Pulling latest changes...".to_owned());
        }
        self.call(Call::Pull, json!({"cwd": cwd}), Pending::Pull);
    }

    /// The outcome of a call the card sent.
    pub fn on_call(&mut self, token: u64, event: CallEvent, cx: &mut Context<Self>) {
        match event {
            CallEvent::Progress(value) => {
                if matches!(self.pending.get(&token), Some(Pending::Stacked))
                    && let Some(run) = &mut self.run
                    && let Some(outcome) = run.progress.apply(&value)
                {
                    self.pending.remove(&token);
                    self.finish(outcome, cx);
                    return;
                }
            }
            CallEvent::Done(value) => match self.pending.remove(&token) {
                Some(Pending::Refs { query, append }) => {
                    if query == self.refs_query {
                        let page = Page::decode(&value);
                        if append {
                            self.refs.refs.extend(page.refs);
                            self.refs.total = page.total;
                            self.refs.has_more = page.has_more;
                            self.refs.next_cursor = page.next_cursor;
                        } else {
                            self.refs = page;
                        }
                        self.refs_loading = false;
                    }
                }
                Some(Pending::Switch | Pending::Create) => self.refresh_status(),
                Some(Pending::Pull) => {
                    self.finish(git_actions::pull_outcome(&value), cx);
                    return;
                }
                Some(Pending::Stacked) => {
                    if self.run.is_some() {
                        self.finish(Progress::finished(&value), cx);
                        return;
                    }
                }
                Some(Pending::Refresh) | None => {}
            },
            CallEvent::Failed(message) => match self.pending.remove(&token) {
                Some(Pending::Refs { query, .. }) => {
                    if query == self.refs_query {
                        self.refs_loading = false;
                        self.refs_error = Some(message);
                    }
                }
                Some(Pending::Switch | Pending::Create) => {
                    self.notice = Some(Notice::Error(message));
                }
                Some(Pending::Pull | Pending::Stacked) => {
                    self.finish(Outcome::Failed(message), cx);
                    return;
                }
                Some(Pending::Refresh) | None => {}
            },
        }
        cx.notify();
    }

    /// A quick or menu action: the default ref confirms first, a commit with a dialog.
    fn request(&mut self, action: GitAction, window: &mut Window, cx: &mut Context<Self>) {
        let Some(props) = self.props.clone() else {
            return;
        };
        let Some(status) = &props.status else { return };
        let local = &status.local;
        if git_actions::requires_default_branch_confirmation(action, local.is_default_ref) {
            let includes_commit = local.has_working_tree_changes
                && matches!(action, GitAction::CommitPush | GitAction::CommitPushPr);
            let branch = local.ref_name.clone().unwrap_or_default();
            let copy = git_actions::default_branch_dialog_copy(
                action,
                &branch,
                includes_commit,
                Terminology::of(Some(status)),
            );
            let unavailable =
                git_actions::feature_branch_unavailable_reason(&branch, includes_commit);
            self.open_default_branch_dialog(action, copy, unavailable, window, cx);
            return;
        }
        self.start_stacked(action, None, false, None, cx);
    }

    fn open_default_branch_dialog(
        &mut self,
        action: GitAction,
        copy: git_actions::DialogCopy,
        unavailable: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let card = cx.weak_entity();
        window.open_dialog(cx, move |dialog, _, cx| {
            let muted = cx.theme().muted_foreground;
            let run = |feature_branch: bool| {
                let card = card.clone();
                move |_: &_, window: &mut Window, cx: &mut App| {
                    window.close_dialog(cx);
                    card.update(cx, |card, cx| {
                        card.start_stacked(action, None, feature_branch, None, cx)
                    })
                    .ok();
                }
            };
            dialog.title(copy.title.clone()).w(px(440.)).child(
                v_flex()
                    .gap_3()
                    .child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child(copy.description.clone()),
                    )
                    .children(
                        unavailable
                            .clone()
                            .map(|reason| div().text_xs().text_color(muted).child(reason)),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .justify_end()
                            .flex_wrap()
                            .child(
                                Button::new("default-ref-abort")
                                    .label("Abort")
                                    .outline()
                                    .on_click(|_, window, cx| window.close_dialog(cx)),
                            )
                            .child(
                                Button::new("default-ref-continue")
                                    .label(copy.continue_label.clone())
                                    .outline()
                                    .on_click(run(false)),
                            )
                            .child(
                                Button::new("default-ref-feature")
                                    .label("Check out feature branch & continue")
                                    .primary()
                                    .disabled(unavailable.is_some())
                                    .on_click(run(true)),
                            ),
                    ),
            )
        });
    }

    fn open_commit_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(status) = self.props.as_ref().and_then(|props| props.status.clone()) else {
            return;
        };
        let card = cx.weak_entity();
        let message = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Leave empty to auto-generate")
                .auto_grow(3, 8)
        });
        let state = cx.new(|_| CommitDialog {
            card,
            branch: status.local.ref_name.clone(),
            is_default: status.local.is_default_ref,
            files: status.local.files.clone(),
            excluded: HashSet::new(),
            editing: false,
            message,
        });
        window.open_dialog(cx, move |dialog, _, _| {
            dialog
                .title("Commit changes")
                .w(px(480.))
                .child(state.clone())
        });
    }

    fn pick_menu(&mut self, id: MenuId, window: &mut Window, cx: &mut Context<Self>) {
        self.git_open = false;
        cx.notify();
        match id {
            MenuId::Commit => self.open_commit_dialog(window, cx),
            MenuId::Push => self.request(GitAction::Push, window, cx),
            MenuId::Pr => self.request(GitAction::CreatePr, window, cx),
        }
    }
}

/// A random version 4 UUID for `actionId`.
fn action_id() -> String {
    let mut bytes = [0u8; 16];
    if getrandom::fill(&mut bytes).is_err() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        bytes = nanos.to_le_bytes();
    }
    bytes[6] = bytes[6] & 0x0f | 0x40;
    bytes[8] = bytes[8] & 0x3f | 0x80;
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as i64)
}

fn tip(text: impl Into<gpui::SharedString>) -> impl Fn(&mut Window, &mut App) -> gpui::AnyView {
    let text = text.into();
    move |window, cx| {
        Tooltip::new(text.clone())
            .text_xs()
            .py_1()
            .rounded(px(6.))
            .build(window, cx)
    }
}

/// The panel row hover fill: white 8% in dark, black 3.5% in light.
fn hover_fill(dark: bool) -> Hsla {
    if dark {
        gpui::white().opacity(0.08)
    } else {
        gpui::black().opacity(0.035)
    }
}

type TriggerBuild = Box<dyn FnOnce(bool, &App) -> AnyElement>;

/// A popover trigger styled by the card, which knows whether the popover is open.
#[derive(IntoElement)]
struct Trigger {
    open: bool,
    build: Option<TriggerBuild>,
}

impl Trigger {
    fn new(build: impl FnOnce(bool, &App) -> AnyElement + 'static) -> Self {
        Self {
            open: false,
            build: Some(Box::new(build)),
        }
    }
}

impl Selectable for Trigger {
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

impl RenderOnce for Trigger {
    fn render(mut self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let build = self.build.take().expect("a trigger renders once");
        build(self.open, cx)
    }
}

/// The chevron segment of a split button.
fn chevron(id: &'static str, label: &'static str, open: bool, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let (muted, foreground) = (theme.muted_foreground, theme.foreground);
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .w_8()
        .h_full()
        .flex_shrink_0()
        .cursor_pointer()
        .text_color(if open { foreground } else { muted })
        .tooltip(tip(label))
        .child(Icon::new(Icons::ChevronDown).size_4())
        .into_any_element()
}

/// A `h-8` panel row: the primary segment, a separator on hover, and the chevron.
fn split_row(
    group: &'static str,
    primary: impl IntoElement,
    chevron: impl IntoElement,
    cx: &App,
) -> gpui::Div {
    let theme = cx.theme();
    let (fill, separator) = (hover_fill(theme.is_dark()), theme.border);
    h_flex()
        .group(group)
        .h_8()
        .rounded(px(8.))
        .hover(move |style| style.bg(fill))
        .child(primary)
        .child(
            div()
                .w(px(1.))
                .h_4()
                .flex_shrink_0()
                .bg(separator)
                .opacity(0.)
                .group_hover(group, |style| style.opacity(1.)),
        )
        .child(chevron)
}

/// The primary segment of a split button: icon, label, `flex-1`.
fn primary_segment(
    id: &'static str,
    icon: impl IntoElement,
    label: String,
    cx: &App,
) -> gpui::Stateful<gpui::Div> {
    let foreground = cx.theme().foreground;
    h_flex()
        .id(id)
        .flex_1()
        .min_w_0()
        .h_full()
        .px(px(10.))
        .gap(px(10.))
        .rounded_l(px(8.))
        .cursor_pointer()
        .text_size(px(13.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(foreground.opacity(0.8))
        .child(icon)
        .child(div().min_w_0().truncate().child(label))
}

/// A plain `h-8` panel row.
fn panel_row(id: &'static str, cx: &App) -> gpui::Stateful<gpui::Div> {
    let theme = cx.theme();
    let (fill, foreground) = (hover_fill(theme.is_dark()), theme.foreground);
    h_flex()
        .id(id)
        .h_8()
        .px(px(10.))
        .gap(px(10.))
        .rounded(px(8.))
        .text_size(px(13.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(foreground.opacity(0.8))
        .hover(move |style| style.bg(fill))
}

fn row_icon(icon: Icons, cx: &App) -> Icon {
    Icon::new(icon)
        .size_4()
        .flex_shrink_0()
        .text_color(cx.theme().muted_foreground)
}

fn editor_mark(editor: &str, cx: &App) -> AnyElement {
    match editor_icon(editor, cx.theme().is_dark()) {
        Some(path) => img(path).size_4().flex_shrink_0().into_any_element(),
        None => row_icon(Icons::SquareArrowOutUpRight, cx).into_any_element(),
    }
}

fn quick_icon(kind: QuickKind, label: &str) -> Icons {
    match kind {
        QuickKind::Run(GitAction::Commit) => Icons::GitCommitHorizontal,
        QuickKind::Run(GitAction::Push | GitAction::CommitPush) | QuickKind::Publish => {
            Icons::CloudUpload
        }
        QuickKind::Run(GitAction::CreatePr | GitAction::CommitPushPr) => Icons::GitPullRequest,
        QuickKind::Pull => Icons::CloudDownload,
        QuickKind::Hint => match label {
            "Commit" => Icons::GitCommitHorizontal,
            "Push" => Icons::CloudUpload,
            _ => Icons::Info,
        },
    }
}

fn menu_icon(id: MenuId) -> Icons {
    match id {
        MenuId::Commit => Icons::GitCommitHorizontal,
        MenuId::Push => Icons::CloudUpload,
        MenuId::Pr => Icons::GitPullRequest,
    }
}

impl DetailsCard {
    fn open_in_section(
        &self,
        editors: &[&'static str],
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let preferred = self.preferred();
        let view = cx.entity();
        let label = preferred.map_or("Open in".to_owned(), |id| {
            format!("Open in {}", details::editor_label(id))
        });
        let icon = match preferred {
            Some(id) => editor_mark(id, cx),
            None => row_icon(Icons::SquareArrowOutUpRight, cx).into_any_element(),
        };
        let primary = primary_segment("open-in-editor", icon, label, cx)
            .on_click(cx.listener(|this, _, _, cx| this.open_favorite(cx)));
        let editors = editors.to_vec();
        let open = self.editor_open;
        let popover = Popover::new("editor-menu")
            .anchor(Anchor::TopRight)
            .open(open)
            .p_0()
            .on_open_change({
                let view = view.clone();
                move |open, _, cx| {
                    view.update(cx, |this, cx| {
                        this.editor_open = *open;
                        cx.notify();
                    });
                }
            })
            .trigger(Trigger::new(move |open, cx| {
                chevron("editor-menu-trigger", "Choose editor", open, cx)
            }))
            .content(move |_, _, cx| {
                let theme = cx.theme();
                let (foreground, muted, hover) =
                    (theme.foreground, theme.muted_foreground, theme.accent);
                v_flex()
                    .w(px(240.))
                    .p_1()
                    .children(editors.iter().map(|&id| {
                        let view = view.clone();
                        h_flex()
                            .id(id)
                            .h_8()
                            .px_2()
                            .gap_2()
                            .rounded(px(6.))
                            .cursor_pointer()
                            .text_sm()
                            .text_color(foreground)
                            .hover(move |style| style.bg(hover))
                            .child(editor_mark(id, cx))
                            .child(div().flex_1().truncate().child(details::editor_label(id)))
                            .when(preferred == Some(id), |row| {
                                row.child(
                                    div()
                                        .text_xs()
                                        .text_color(muted)
                                        .child(details::OPEN_FAVORITE_SHORTCUT),
                                )
                            })
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.editor_open = false;
                                    this.open_in_editor(id, cx);
                                });
                            })
                    }))
            });
        split_row("open-in-split", primary, popover, cx)
    }

    fn branch_row(&self, status: &VcsStatus, cx: &mut Context<Self>) -> AnyElement {
        let Some(props) = self.props.clone() else {
            return div().into_any_element();
        };
        let view = cx.entity();
        let label = status
            .local
            .ref_name
            .clone()
            .unwrap_or_else(|| "Select ref".to_owned());
        let snapshot = RefPicker {
            query: self.refs_query.clone(),
            page: self.refs.clone(),
            loading: self.refs_loading,
            error: self.refs_error.clone(),
            cwd: props.cwd,
            project_cwd: props.project_cwd,
            search: self.search.clone(),
            scroll: self.ref_scroll.clone(),
        };
        Popover::new("ref-picker")
            .anchor(Anchor::TopRight)
            .open(self.ref_open)
            .p_0()
            .on_open_change({
                let view = view.clone();
                move |open, window, cx| {
                    view.update(cx, |this, cx| this.set_ref_open(*open, window, cx));
                }
            })
            .trigger(Trigger::new(move |_, cx| {
                panel_row("branch-trigger", cx)
                    .w_full()
                    .cursor_pointer()
                    .child(row_icon(Icons::GitBranch, cx))
                    .child(div().flex_1().min_w_0().truncate().child(label))
                    .into_any_element()
            }))
            .content(move |_, window, cx| snapshot.render(view.clone(), window, cx))
            .into_any_element()
    }

    fn git_row(&self, status: &VcsStatus, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let (muted, danger, foreground) = (theme.muted_foreground, theme.danger, theme.foreground);
        if let Some(run) = &self.run {
            let seconds = run.started.elapsed().as_secs();
            return v_flex()
                .min_h_8()
                .px(px(10.))
                .py_1()
                .justify_center()
                .text_size(px(13.))
                .child(
                    h_flex()
                        .gap(px(10.))
                        .text_color(foreground.opacity(0.8))
                        .child(row_icon(Icons::CircleDotDashed, cx))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .child(run.progress.label()),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child(git_actions::format_elapsed(seconds)),
                        ),
                )
                .children(run.progress.output.clone().map(|line| {
                    div()
                        .pl(px(26.))
                        .text_xs()
                        .text_color(muted)
                        .truncate()
                        .child(line)
                }))
                .into_any_element();
        }
        match &self.notice {
            Some(Notice::Success { title, description }) => {
                let row = panel_row("git-success", cx)
                    .child(Icon::new(Icons::Check).size_4().text_color(foreground))
                    .child(div().flex_1().min_w_0().truncate().child(title.clone()));
                return match description {
                    Some(text) => row.tooltip(tip(text.clone())).into_any_element(),
                    None => row.into_any_element(),
                };
            }
            Some(Notice::Error(message)) => {
                let view = cx.entity();
                return panel_row("git-error", cx)
                    .cursor_pointer()
                    .text_color(danger)
                    .child(Icon::new(Icons::CircleAlert).size_4().text_color(danger))
                    .child(div().flex_1().min_w_0().truncate().child(message.clone()))
                    .tooltip(tip(message.clone()))
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.notice = None;
                            cx.notify();
                        });
                    })
                    .into_any_element();
            }
            None => {}
        }
        let local = &status.local;
        let busy = self.run.is_some();
        let quick = git_actions::resolve_quick_action(
            Some(status),
            busy,
            local.is_default_ref,
            local.has_primary_remote,
        );
        // Publishing a repository needs the hosting flow, which this client does not have.
        let unsupported = quick.kind == QuickKind::Publish;
        let disabled = quick.disabled || unsupported;
        let hint = if unsupported {
            Some("Publishing a repository is not supported in this client yet.".to_owned())
        } else {
            quick.hint.clone()
        };
        let icon = row_icon(quick_icon(quick.kind, &quick.label), cx);
        let mut primary = primary_segment("git-quick-action", icon, quick.label.clone(), cx);
        if disabled {
            primary = primary.opacity(0.5).cursor_default();
            if let Some(hint) = hint {
                primary = primary.tooltip(tip(hint));
            }
        } else {
            let kind = quick.kind;
            primary = primary.on_click(cx.listener(move |this, _, window, cx| match kind {
                QuickKind::Run(action) => this.request(action, window, cx),
                QuickKind::Pull => this.start_pull(cx),
                QuickKind::Publish | QuickKind::Hint => {}
            }));
        }
        let items = git_actions::build_menu_items(Some(status), busy, local.has_primary_remote);
        let warning = menu_warning(status);
        let view = cx.entity();
        let popover = Popover::new("git-menu")
            .anchor(Anchor::TopRight)
            .open(self.git_open)
            .p_0()
            .on_open_change({
                let view = view.clone();
                move |open, _, cx| {
                    view.update(cx, |this, cx| {
                        this.git_open = *open;
                        if *open {
                            this.refresh_status();
                        }
                        cx.notify();
                    });
                }
            })
            .trigger(Trigger::new(move |open, cx| {
                chevron("git-menu-trigger", "Git actions", open, cx)
            }))
            .content(move |_, _, cx| {
                let theme = cx.theme();
                let (foreground, muted, hover) =
                    (theme.foreground, theme.muted_foreground, theme.accent);
                v_flex()
                    .w(px(240.))
                    .p_1()
                    .children(items.iter().map(|item| {
                        let (view, id, disabled) = (view.clone(), item.id, item.disabled);
                        h_flex()
                            .id(match item.id {
                                MenuId::Commit => "git-menu-commit",
                                MenuId::Push => "git-menu-push",
                                MenuId::Pr => "git-menu-pr",
                            })
                            .h_8()
                            .px_2()
                            .gap_2()
                            .rounded(px(6.))
                            .text_sm()
                            .text_color(foreground)
                            .when(disabled, |row| row.opacity(0.5))
                            .when(!disabled, |row| {
                                row.cursor_pointer()
                                    .hover(move |style| style.bg(hover))
                                    .on_click(move |_, window, cx| {
                                        view.update(cx, |this, cx| this.pick_menu(id, window, cx));
                                    })
                            })
                            .child(Icon::new(menu_icon(item.id)).size_4().text_color(muted))
                            .child(div().flex_1().truncate().child(item.label.clone()))
                    }))
                    .children(warning.clone().map(|text| {
                        div()
                            .px_2()
                            .py_1()
                            .text_xs()
                            .text_color(theme.warning)
                            .child(text)
                    }))
            });
        split_row("git-split", primary, popover, cx).into_any_element()
    }

    fn changes_row(&self, status: &VcsStatus, cx: &mut Context<Self>) -> impl IntoElement {
        let totals = status.changes_totals();
        let opens_diff = self
            .props
            .as_ref()
            .is_some_and(|props| props.thread_id.is_some());
        let open_diff = cx.listener(|_, _, _, cx| cx.emit(OpenDiff));
        let theme = cx.theme();
        let (muted, mono) = (theme.muted_foreground, theme.mono_font_family.clone());
        let count = |sign: char, count: u64, tint: Hsla| {
            div()
                .text_color(if count == 0 { muted } else { tint })
                .child(format!("{sign}{}", details::format_diff_count(count)))
        };
        panel_row("changes-row", cx)
            .when(opens_diff, |row| row.cursor_pointer().on_click(open_diff))
            .child(row_icon(Icons::FileDiff, cx))
            .child(div().flex_shrink_0().child("Changes"))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_xs()
                    .text_color(muted)
                    .children(
                        status
                            .local
                            .base_ref
                            .as_ref()
                            .map(|base| format!("vs {base}")),
                    ),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .gap_1()
                    .text_xs()
                    .font_family(mono)
                    .child(count('+', totals.insertions, gpui::rgb(0x22c55e).into()))
                    .child(count('-', totals.deletions, gpui::rgb(0xef4444).into())),
            )
    }

    fn usage_section(&self, limits: &Limits, cx: &App) -> AnyElement {
        let theme = cx.theme();
        let (muted, foreground) = (theme.muted_foreground, theme.foreground);
        let (danger, warning, fill) = (theme.danger, theme.warning, hover_fill(theme.is_dark()));
        let row = |id: usize| {
            h_flex()
                .id(("usage", id))
                .h_6()
                .px(px(10.))
                .gap_2()
                .rounded(px(6.))
                .text_xs()
                .hover(move |style| style.bg(fill))
        };
        if let Some(notice) = limits.notice() {
            return v_flex()
                .child(
                    row(0)
                        .text_color(muted)
                        .tooltip(tip(notice))
                        .child(div().flex_1().child("Limits"))
                        .child("Unavailable"),
                )
                .into_any_element();
        }
        let now = now_ms();
        v_flex()
            .children(limits.windows.iter().enumerate().map(|(index, window)| {
                let left = window.remaining_percent();
                let bar = match usage::tone(left) {
                    Some(Tone::Error) => danger,
                    Some(Tone::Warning) => warning,
                    None => foreground.opacity(0.8),
                };
                row(index)
                    .tooltip(tip(window.tooltip(now).join("\n")))
                    .child(
                        div()
                            .w(px(112.))
                            .flex_shrink_0()
                            .truncate()
                            .text_color(muted)
                            .child(window.label.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h(px(3.))
                            .rounded_full()
                            .bg(muted.opacity(0.3))
                            .child(
                                div()
                                    .h_full()
                                    .rounded_full()
                                    .bg(bar)
                                    .min_w(px(6.))
                                    .w(relative(left.clamp(0, 100) as f32 / 100.)),
                            ),
                    )
                    .child(
                        div()
                            .w(px(32.))
                            .flex_shrink_0()
                            .text_right()
                            .text_color(foreground.opacity(0.8))
                            .child(format!("{left}%")),
                    )
            }))
            .into_any_element()
    }
}

/// The warning under the menu: a detached HEAD or a branch behind its upstream.
fn menu_warning(status: &VcsStatus) -> Option<String> {
    let terminology = Terminology::of(Some(status));
    if status.local.ref_name.is_none() {
        return Some(format!(
            "Detached HEAD: create and check out a branch to enable push and {} actions.",
            terminology.singular
        ));
    }
    (status.remote.behind > 0).then(|| "Behind upstream. Pull/rebase first.".to_owned())
}

/// What the ref picker's popover draws, snapshotted at each render of the card.
struct RefPicker {
    query: String,
    page: Page,
    loading: bool,
    error: Option<String>,
    cwd: String,
    project_cwd: String,
    search: Entity<InputState>,
    scroll: ScrollHandle,
}

impl RefPicker {
    fn render(&self, view: Entity<DetailsCard>, window: &mut Window, cx: &mut App) -> AnyElement {
        let theme = cx.theme();
        let (muted, border, ring) = (theme.muted_foreground, theme.border, theme.ring);
        let (hover, selected) = (theme.list_hover, theme.list_active);
        let searching = self.search.focus_handle(cx).is_focused(window);
        let sanitized = refs::sanitize_new_ref_name(&self.query);
        let creatable =
            !sanitized.is_empty() && !self.page.refs.iter().any(|r| r.name == sanitized);
        let rows = self.page.refs.iter().enumerate().map(|(index, entry)| {
            let tag = refs::tag(entry, &self.project_cwd);
            let elsewhere = !entry.current
                && entry
                    .worktree_path
                    .as_deref()
                    .is_some_and(|path| path != self.cwd);
            let (view, name) = (view.clone(), entry.name.clone());
            let current = entry.current;
            let label = match refs::split_for_middle_truncate(&entry.name, None) {
                Some((head, tail)) => h_flex()
                    .flex_1()
                    .min_w_0()
                    .child(div().min_w_0().truncate().child(head))
                    .child(div().flex_shrink_0().child(tail))
                    .into_any_element(),
                None => div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(entry.name.clone())
                    .into_any_element(),
            };
            h_flex()
                .id(("ref", index))
                .gap_2()
                .px_2()
                .py_1p5()
                .items_center()
                .rounded_md()
                .text_sm()
                .when(current, |row| row.bg(selected))
                .when(elsewhere, |row| {
                    row.opacity(0.5)
                        .tooltip(tip("This ref is checked out in another worktree."))
                })
                .when(!elsewhere, |row| {
                    row.cursor_pointer()
                        .hover(move |style| style.bg(hover))
                        .on_click(move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                if current {
                                    this.ref_open = false;
                                    cx.notify();
                                } else {
                                    this.switch_ref(name.clone(), cx);
                                }
                            });
                        })
                })
                .child(label)
                .children(tag.map(|tag: Tag| {
                    div()
                        .flex_shrink_0()
                        .text_xs()
                        .text_color(muted)
                        .child(tag.label())
                }))
        });
        let create = creatable.then(|| {
            let (view, name) = (view.clone(), sanitized.clone());
            h_flex()
                .id("ref-create")
                .gap_2()
                .px_2()
                .py_1p5()
                .items_center()
                .rounded_md()
                .text_sm()
                .cursor_pointer()
                .hover(move |style| style.bg(hover))
                .child(Icon::new(Icons::GitBranchPlus).size_4().text_color(muted))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .child(format!("Create new ref \"{sanitized}\"")),
                )
                .on_click(move |_, _, cx| {
                    view.update(cx, |this, cx| this.create_ref(name.clone(), cx));
                })
        });
        let more = (self.page.has_more && !self.loading).then(|| view.clone());
        let scroll = self.scroll.clone();
        let footer = self.page.status(self.loading);
        let empty = self.page.refs.is_empty() && !self.loading && create.is_none();
        v_flex()
            .w(px(288.))
            .rounded_lg()
            .overflow_hidden()
            .child(
                div().px_3().pt(px(10.)).child(
                    h_flex()
                        .pb(px(6.))
                        .border_b_1()
                        .border_color(if searching { ring } else { border.opacity(0.7) })
                        .child(
                            Input::new(&self.search)
                                .appearance(false)
                                .h(px(26.))
                                .px_0()
                                .prefix(
                                    Icon::new(IconName::Search)
                                        .size_4()
                                        .text_color(muted.opacity(0.55)),
                                ),
                        ),
                ),
            )
            .child(
                v_flex()
                    .id("ref-list")
                    .max_h(px(280.))
                    .overflow_y_scroll()
                    .track_scroll(&scroll)
                    .on_scroll({
                        let scroll = scroll.clone();
                        move |_, _, cx| {
                            let Some(view) = &more else { return };
                            let offset = scroll.offset().y;
                            let remaining = scroll.max_offset().y + offset;
                            if remaining < px(160.) {
                                view.update(cx, |this, cx| {
                                    if !this.refs_loading {
                                        this.list_refs(true);
                                        cx.notify();
                                    }
                                });
                            }
                        }
                    })
                    .p_1()
                    .children(create)
                    .children(rows)
                    .when(empty, |list| {
                        list.child(
                            div().p_2().text_sm().text_color(muted).child(
                                self.error
                                    .clone()
                                    .unwrap_or_else(|| "No matching refs.".to_owned()),
                            ),
                        )
                    }),
            )
            .children(footer.map(|text| {
                div()
                    .px_3()
                    .py_1p5()
                    .border_t_1()
                    .border_color(border)
                    .text_xs()
                    .text_color(muted)
                    .child(text)
            }))
            .into_any_element()
    }
}

impl Render for DetailsCard {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(props) = self.props.clone() else {
            return div().into_any_element();
        };
        let status = props.status.as_ref();
        let usage_rows = props.limits.as_ref().map_or(0, |limits| {
            if limits.notice().is_some() {
                1
            } else {
                limits.windows.len()
            }
        });
        let inputs = Inputs {
            has_cwd: true,
            is_repo: status.is_some_and(|status| status.local.is_repo),
            has_editors: !props.editors.is_empty(),
            quiet_empty_draft: details::quiet_empty_draft(props.has_run, status),
            usage_rows,
        };
        let density = details::density_for(props.max_height, inputs);
        let show = Show::resolve(density, inputs);
        if show.is_empty() {
            return div().into_any_element();
        }
        let mut sections: Vec<AnyElement> = Vec::new();
        if show.open_in {
            sections.push(self.open_in_section(&props.editors, cx).into_any_element());
        }
        if let Some(status) = status
            && (show.branch || show.git_actions || show.changes)
        {
            let mut rows = v_flex();
            if show.branch {
                rows = rows.child(self.branch_row(status, cx));
            }
            if show.git_actions {
                rows = rows.child(self.git_row(status, cx));
            }
            if show.changes {
                rows = rows.child(self.changes_row(status, cx));
            }
            sections.push(rows.into_any_element());
        }
        if let (true, Some(limits)) = (show.usage, props.limits.as_ref()) {
            sections.push(self.usage_section(limits, cx));
        }
        let theme = cx.theme();
        let divider = theme.border;
        let last = sections.len().saturating_sub(1);
        let body = sections.into_iter().enumerate().map(|(index, section)| {
            v_flex()
                .pt_2()
                .pb(px(10.))
                .px_2()
                .when(density == Density::Full && index < last, |wrap| {
                    wrap.border_b_1().border_color(divider)
                })
                .child(section)
        });
        div()
            .id("thread-details-card")
            .w(px(details::CARD_WIDTH))
            .max_h(px(props.max_height))
            .overflow_y_scroll()
            .rounded(px(16.))
            .border_1()
            .border_color(gpui::white().opacity(0.05))
            .bg(theme.secondary.opacity(0.8))
            .shadow_lg()
            .children(body)
            .into_any_element()
    }
}

/// The web's `CommitDialog` body: the branch, the files to include, and the message.
struct CommitDialog {
    card: WeakEntity<DetailsCard>,
    branch: Option<String>,
    is_default: bool,
    files: Vec<ChangedFile>,
    excluded: HashSet<String>,
    editing: bool,
    message: Entity<TextareaState>,
}

impl CommitDialog {
    fn selected(&self) -> Vec<&ChangedFile> {
        self.files
            .iter()
            .filter(|file| !self.excluded.contains(&file.path))
            .collect()
    }

    fn submit(&self, feature_branch: bool, window: &mut Window, cx: &mut App) {
        let paths = (!self.excluded.is_empty()).then(|| {
            self.selected()
                .iter()
                .map(|file| file.path.clone())
                .collect()
        });
        let message = self.message.read(cx).value().to_string();
        window.close_dialog(cx);
        self.card
            .update(cx, |card, cx| {
                card.start_stacked(GitAction::Commit, Some(message), feature_branch, paths, cx)
            })
            .ok();
    }
}

impl Render for CommitDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (muted, warning, border) = (theme.muted_foreground, theme.warning, theme.border);
        let mono = theme.mono_font_family.clone();
        let selected = self.selected();
        let none_selected = selected.is_empty();
        let lines = selected.iter().fold((0, 0), |(ins, del), file| {
            (ins + file.lines.insertions, del + file.lines.deletions)
        });
        let total = if none_selected {
            "none".to_owned()
        } else {
            format!(
                "{} of {} files  +{} -{}",
                selected.len(),
                self.files.len(),
                details::format_diff_count(lines.0),
                details::format_diff_count(lines.1)
            )
        };
        let editing = self.editing;
        let entity = cx.entity();
        let files = self.files.iter().enumerate().map(|(index, file)| {
            let (entity, path) = (entity.clone(), file.path.clone());
            let included = !self.excluded.contains(&file.path);
            h_flex()
                .gap_2()
                .h_6()
                .text_xs()
                .when(editing, |row| {
                    row.child(
                        Checkbox::new(("commit-file", index))
                            .checked(included)
                            .on_click(move |_, _, cx| {
                                entity.update(cx, |this, cx| {
                                    if !this.excluded.remove(&path) {
                                        this.excluded.insert(path.clone());
                                    }
                                    cx.notify();
                                });
                            }),
                    )
                })
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .font_family(mono.clone())
                        .when(!included, |label| label.text_color(muted))
                        .child(file.path.clone()),
                )
                .child(div().text_color(gpui::rgb(0x22c55e)).child(format!(
                    "+{}",
                    details::format_diff_count(file.lines.insertions)
                )))
                .child(div().text_color(gpui::rgb(0xef4444)).child(format!(
                    "-{}",
                    details::format_diff_count(file.lines.deletions)
                )))
        });
        let toggle = entity.clone();
        v_flex()
            .gap_3()
            .child(div().text_sm().text_color(muted).child(
                "Review and confirm your commit. Leave the message blank to auto-generate one.",
            ))
            .child(
                v_flex()
                    .gap_2()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(border)
                    .text_sm()
                    .child(
                        h_flex()
                            .gap_2()
                            .child(div().text_color(muted).child("Branch"))
                            .child(
                                div().font_weight(FontWeight::MEDIUM).child(
                                    self.branch
                                        .clone()
                                        .unwrap_or_else(|| "(detached HEAD)".to_owned()),
                                ),
                            )
                            .when(self.is_default, |row| {
                                row.child(
                                    div().text_xs().text_color(warning).child("Default branch"),
                                )
                            }),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(div().text_color(muted).child("Files"))
                            .child(div().flex_1().text_xs().text_color(muted).child(total))
                            .when(self.files.len() > 1, |row| {
                                row.child(
                                    Button::new("commit-edit")
                                        .label(if editing { "Done" } else { "Edit" })
                                        .ghost()
                                        .xsmall()
                                        .on_click(move |_, _, cx| {
                                            toggle.update(cx, |this, cx| {
                                                this.editing = !this.editing;
                                                cx.notify();
                                            });
                                        }),
                                )
                            }),
                    )
                    .child(
                        v_flex()
                            .id("commit-files")
                            .max_h(px(200.))
                            .overflow_y_scroll()
                            .children(files),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .child("Commit message (optional)"),
                    )
                    .child(Textarea::new(&self.message)),
            )
            .child(
                h_flex()
                    .gap_2()
                    .justify_end()
                    .child(
                        Button::new("commit-cancel")
                            .label("Cancel")
                            .outline()
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        Button::new("commit-new-branch")
                            .label("Commit on new branch")
                            .outline()
                            .disabled(none_selected)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.submit(true, window, cx)),
                            ),
                    )
                    .child(
                        Button::new("commit-confirm")
                            .label("Commit")
                            .primary()
                            .disabled(none_selected)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.submit(false, window, cx)),
                            ),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_ids_are_version_four_uuids() {
        let id = action_id();
        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(
            parts.iter().map(|part| part.len()).collect::<Vec<_>>(),
            [8, 4, 4, 4, 12]
        );
        assert!(parts[2].starts_with('4'));
        assert!("89ab".contains(parts[3].chars().next().unwrap()));
        assert_ne!(id, action_id());
    }
}
