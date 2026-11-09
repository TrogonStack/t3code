use crate::assets::{MESSAGE_SQUARE_DASHED, STAGE_DEV, STAGE_DEV_BUTTON, chip_icon, provider_icon};
use crate::attachments::{self, Upload};
use crate::branch_picker::{self, BranchPicker};
use crate::connection::{Action, ActionSender, Call, command_id, new_project_id};
use crate::details::{self, VcsStatus};
use crate::details_card::{DetailsCard, OpenDiff, Props};
use crate::history::History;
use crate::minimap;
use crate::model_picker;
use crate::projects::{self, EnvironmentProjects, MachineKind, NewProject, ProjectEntry};
use crate::providers::{self, Catalog, Mark, Pick, Trigger};
use crate::send::{self, Primary};
use crate::slow_requests;
use crate::status::{
    CallEvent, LastError, PrBadge, PrState, Section, ShellView, Status, ThreadRow, TopStatus,
};
use crate::strip::{
    self, Branch, Defaults, EnvMode, HOST_SHORTCUT, PERMISSIONS_SHORTCUT, RuntimeMode,
    WORKSPACE_SHORTCUT,
};
use crate::terminal::{self, Event as TerminalEvent, Target};
use crate::terminal_state::{self, Placement, Terminals};
use crate::terminal_view::TerminalView;
use crate::thread::{Entry, Kind};
use crate::thread_card::{self, ChainKind, PrLine};
use crate::toast::{Kind as ToastKind, Toast, ToastHost, ToastId};
use crate::workspace::{self, Choice, Draft};
use gpui::{
    Anchor, Animation, AnimationExt as _, AnyElement, App, Bounds, Context, DragMoveEvent, Entity,
    Focusable as _, FollowMode, FontWeight, Hsla, IntoElement, Keystroke, ListAlignment,
    ListOffset, ListState, MouseButton, MouseDownEvent, MouseMoveEvent, ObjectFit, ParentElement,
    PathPromptOptions, Pixels, Render, RenderOnce, SharedString, Styled, Window, actions, canvas,
    div, ease_in_out, img, list, prelude::*, px, rgb,
};
use gpui_component::{
    ActiveTheme, Icon, IconName, Selectable, Sizable, TitleBar,
    button::{Button, ButtonVariants},
    h_flex,
    input::{
        InlineToken, InlineTokenContext, Input, InputEvent, InputState, Textarea, TextareaState,
    },
    kbd::Kbd,
    popover::Popover,
    tooltip::Tooltip,
    v_flex,
};
use gpui_kit_assets::IconName as Icons;
use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    rc::Rc,
    time::Duration,
};

/// The terminal sidebar's `w-36`.
const TERMINAL_SIDEBAR_WIDTH: f32 = 144.;
/// The drag handle on the terminal drawer's top edge.
const TERMINAL_HANDLE_HEIGHT: f32 = 8.;
mod add_to_machine;
mod composer_menu;
mod model_popover;
mod pull_request_panel;
mod pull_requests_page;
mod right_panel;
mod run_on_menu;
mod settings;
mod usage_dialog;
mod workspace_menu;

pub use add_to_machine::bindings as add_to_machine_bindings;
pub use settings::bindings as settings_bindings;
pub use usage_dialog::bindings as usage_bindings;

/// The web's `THREAD_SIDEBAR_DEFAULT_WIDTH`.
const SIDEBAR_DEFAULT_WIDTH: f32 = 256.;
/// The web's `THREAD_SIDEBAR_MIN_WIDTH`, `13 * 16`.
const SIDEBAR_MIN_WIDTH: f32 = 208.;
/// The web's `THREAD_MAIN_CONTENT_MIN_WIDTH`, `40 * 16`.
const MAIN_CONTENT_MIN_WIDTH: f32 = 640.;
/// The web's `--chat-content-max-width` (`index.css:131`), `46rem`: the heading, composer and
/// transcript column.
const CHAT_CONTENT_WIDTH: f32 = 736.;
/// The rail's hit area, the web's `w-4`, centered on the sidebar's right edge.
const SIDEBAR_RAIL_WIDTH: f32 = 16.;
/// The web's `--workspace-topbar-height` on macOS desktop.
const TOPBAR_HEIGHT: f32 = 52.;
/// Clears the traffic lights, like `--workspace-controls-left` from `titlebar-area-x`.
const TITLEBAR_CONTENT_LEFT: f32 = 84.;
/// The web shows the dev stage art in dev builds (`APP_STAGE_LABEL` is "Dev").
const STAGE_BACKDROP: bool = cfg!(debug_assertions);
/// `SidebarStageBackdrop`'s `h-20`.
const STAGE_BACKDROP_HEIGHT: f32 = 80.;

actions!(
    t3,
    [
        ToggleSidebar,
        DismissSidebar,
        OpenPermissions,
        OpenWorkspace,
        UsePreviousWorktree,
        GoBack,
        GoForward,
        ToggleDetails,
        ToggleTerminal,
        SplitTerminal,
        SplitTerminalVertical,
        NewTerminal,
        CloseTerminal,
        ToggleRightPanel,
        OpenFavoriteEditor,
        NewThread,
        NewScratchThread,
        OpenUsage,
        OpenSettings
    ]
);

/// The web's `SIDEBAR_MOBILE_QUERY`, `(max-width: 767px)`.
const SIDEBAR_BREAKPOINT: f32 = 767.;
/// The left sheet's `w-[calc(100%-(--spacing(12)))] max-w-md`.
const SIDEBAR_OVERLAY_GAP: f32 = 48.;
const SIDEBAR_OVERLAY_MAX: f32 = 448.;
/// The sheet's `duration-200` and its `-translate-x-8` entrance.
const SHEET_MS: u64 = 200;
const SHEET_SLIDE: f32 = 32.;
/// Key context while the overlay is open, so Escape only dismisses it then.
pub const OVERLAY_CONTEXT: &str = "SidebarOverlay";

#[derive(Clone)]
struct SidebarResize;

/// The drag payload of the terminal drawer's top edge.
#[derive(Clone)]
struct TerminalResizeDrag;

impl Render for TerminalResizeDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui::Empty
    }
}

impl Render for SidebarResize {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui::Empty
    }
}

/// The web's sidebar bounds: at least the brand row (`resolveThreadSidebarMinimumWidth`), and
/// the window leaves the content its minimum.
fn clamp_sidebar_width(width: f32, brand_width: f32, window_width: f32) -> f32 {
    let min = SIDEBAR_MIN_WIDTH.max(brand_width.ceil());
    let max = min.max(window_width - MAIN_CONTENT_MIN_WIDTH);
    width.clamp(min, max)
}

fn is_narrow(width: f32) -> bool {
    width <= SIDEBAR_BREAKPOINT
}

fn narrow_window(window: &Window) -> bool {
    is_narrow(f32::from(window.viewport_size().width))
}

fn overlay_width(window_width: f32) -> f32 {
    (window_width - SIDEBAR_OVERLAY_GAP).min(SIDEBAR_OVERLAY_MAX)
}

/// The web's `toggleSidebar`: narrow viewports flip the overlay, wide ones the persistent state.
fn toggled(narrow: bool, open: bool, overlay_open: bool) -> (bool, bool) {
    if narrow {
        (open, !overlay_open)
    } else {
        (!open, overlay_open)
    }
}
/// A thread card (`h-[4.875rem]`) between the `py-0.5` of its list item.
const CARD_HEIGHT: f32 = 78.;
const ROW_HEIGHT: f32 = CARD_HEIGHT + 4.;
/// Where the window is: the project in the picker and the thread open in the main area, if any.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Route {
    project: Option<String>,
    thread: Option<String>,
}

/// The open thread's transcript as last published by the connection.
struct Transcript {
    id: String,
    result: Result<Vec<Entry>, String>,
}

const NO_PROJECT: &str = "No project";
const HEADLINE_SCRATCH: &str = "What should we work on?";
/// The web's `ComboboxPopup` cap, `max-h-[min(var(--available-height),23rem)]`.
const PICKER_MAX_HEIGHT: f32 = 368.;
/// `ComboboxSearchInput`: `pt-2.5`, a 26px input, `pb-1.5` and its 1px border.
const PICKER_SEARCH_HEIGHT: f32 = 43.;
/// The popover's 4px offset from the trigger plus the positioner's 4px viewport margin.
const PICKER_GAP: f32 = 8.;

pub struct MainView {
    status: Status,
    search: Entity<InputState>,
    project_search: Entity<InputState>,
    composer: Entity<TextareaState>,
    settled_shown: usize,
    /// Working, Snoozed and Settled shelves; the web starts all of them collapsed.
    shelves: [bool; 3],
    sidebar_open: bool,
    sidebar_overlay_open: bool,
    /// The docked sidebar's width as dragged, before fitting the window.
    sidebar_width: f32,
    /// The docked width at the last render, for the cards' branch truncation.
    rendered_sidebar_width: Cell<f32>,
    /// The topbar brand row's natural width at the last paint, the web's `SidebarBrandWidthProbe`.
    brand_width: Rc<Cell<f32>>,
    /// The floating composer's height at the last paint; the transcript's last row reserves it.
    composer_height: Rc<Cell<f32>>,
    reserved_composer_height: Cell<f32>,
    /// Hover styles cannot change layout, so the send button's hover growth is view state.
    send_hovered: bool,
    permissions_open: bool,
    workspace_open: bool,
    host_open: bool,
    host_highlight: usize,
    /// The row the Workspace menu's keys and pointer have lit.
    workspace_highlight: usize,
    /// Routes the menu's keys while it is open.
    workspace_keys: Option<gpui::Subscription>,
    /// The worktree a draft was pointed at by Previous worktree, by project id.
    draft_worktrees: HashMap<String, workspace::Seed>,
    /// The workspace mode the user picked for a draft, by project id.
    env_modes: HashMap<String, EnvMode>,
    /// The ref a new worktree will start from, by project id.
    draft_bases: HashMap<String, Branch>,
    branch_picker: Entity<BranchPicker>,
    model_open: bool,
    model_search: Entity<InputState>,
    /// The model picked in each composer (an open thread or a project's draft), by details key.
    model_picks: HashMap<String, Pick>,
    model_ui: model_picker::State,
    /// Routes the picker's keys while it is open; they never reach the search input otherwise.
    model_keys: Option<gpui::Subscription>,
    selected_thread: Option<String>,
    transcript: Option<Transcript>,
    transcript_list: ListState,
    minimap_items: Vec<minimap::Item>,
    /// The hovered tick and its card text, parsed once when the hover lands on it.
    minimap_hover: Option<(usize, minimap::Preview)>,
    /// Local view state until the draft's mode is sent to the server.
    runtime_mode: Option<RuntimeMode>,
    permissions: Entity<PermissionsPanel>,
    selected_project: Option<String>,
    history: History<Route>,
    picker_open: bool,
    /// The sidebar's project scope as group keys, empty for all projects; in memory only.
    project_scope: Vec<String>,
    scope_open: bool,
    scope_search: Entity<InputState>,
    /// The project the server was asked to create, opened once the shell lists it.
    adding_project: Option<String>,
    /// Last painted trigger bounds, so the popup can shrink to fit below it as on the web.
    picker_trigger: Rc<Cell<Bounds<Pixels>>>,
    picker_order: Option<Vec<String>>,
    /// The other saved environments, by id.
    environments: Vec<EnvironmentProjects>,
    providers: Option<Catalog>,
    /// The branch per workspace root; `None` for a root that is not a repository.
    branches: HashMap<String, Option<Branch>>,
    /// Roots whose branch was asked for on this connection.
    branches_requested: HashSet<String>,
    /// Threads (and drafts, by project) whose details card the user closed; it starts open.
    details_closed: HashSet<String>,
    /// The version control status per directory the card watched, `None` where it failed.
    vcs: HashMap<String, Option<VcsStatus>>,
    /// The directory the connection was last asked to watch.
    vcs_watching: Option<String>,
    /// Threads whose right panel is open, in memory like the web's `rightPanelStore` state.
    right_panel_open: HashSet<String>,
    /// The docked panel's width as dragged, before fitting the window.
    right_panel_width: f32,
    /// The working-tree diff per directory the panel loaded.
    diffs: HashMap<String, right_panel::Load>,
    /// Directories whose diff was asked for on this connection.
    diffs_requested: HashSet<String>,
    diff_list: Entity<right_panel::DiffList>,
    /// The pull request tabs opened per thread, and the surface each panel shows.
    pull_request_tabs: HashMap<String, Vec<right_panel::PullRequestTab>>,
    right_panel_active: HashMap<String, right_panel::Active>,
    pull_request_panel: Entity<pull_request_panel::PullRequestPanel>,
    /// The pull requests page, in place of the thread area while open.
    pull_requests_open: bool,
    pull_requests_page: Entity<pull_requests_page::PullRequestsPage>,
    /// Threads (and drafts, by project) whose terminal drawer is open; it starts closed.
    terminal_open: HashSet<String>,
    /// The drawer's height per thread, as dragged.
    terminal_heights: HashMap<String, f32>,
    /// The terminals of each thread with a drawer, in groups.
    terminals: HashMap<String, Terminals>,
    /// One surface per attached terminal, by thread and terminal id.
    terminal_views: HashMap<(String, String), Entity<TerminalView>>,
    /// The terminals the connection was last asked to attach.
    terminal_attached: Vec<Target>,
    card: Entity<DetailsCard>,
    toasts: Entity<ToastHost>,
    usage: Entity<usage_dialog::UsageDialog>,
    add_to_machine: Entity<add_to_machine::AddToMachine>,
    settings: Entity<settings::SettingsDialog>,
    /// The slow requests toast; kept after the user dismisses it, like the web coordinator.
    slow_toast: Option<ToastId>,
    /// The main area's width at the last render, for fitting the details card.
    main_width: Cell<f32>,
    /// The send on its way to the server, which keeps the composer's text until it answers.
    sending: Option<Sending>,
    /// Set when a send succeeded; the next render empties the composer, which needs a window.
    clear_composer: bool,
    /// The files attached to the composer, in the order they were picked.
    attachments: Vec<attachments::Item>,
    /// The mint and upload calls in flight, by token.
    attachment_calls: HashMap<u64, AttachmentCall>,
    attachment_token: u64,
    /// Set when Send found failed uploads: they retry, and the send resumes once all settle.
    resume_send: bool,
    actions: ActionSender,
}

/// An attachment's call in flight: first the server mints the upload URL, then the file posts to it.
struct AttachmentCall {
    item: String,
    minted: Option<String>,
}

/// What a successful send changes in the window.
struct Sending {
    /// The thread a draft became.
    opens: Option<String>,
    /// The draft's details key, whose model pick the new thread inherits.
    from_key: Option<String>,
    /// The project whose draft this send consumes.
    draft_project: Option<String>,
    /// The attachments the message carries, which leave the composer with it.
    attachments: Vec<String>,
}

/// Send and stop calls, above the branch picker's range.
const SEND_TOKEN: u64 = 1 << 41;
const STOP_TOKEN: u64 = SEND_TOKEN + 1;
const FAVORITE_TOKEN: u64 = SEND_TOKEN + 2;
const DELETE_ATTACHMENT_TOKEN: u64 = SEND_TOKEN + 3;
/// Attachment mint and upload calls, one token each, above the send calls.
const ATTACH_TOKEN: u64 = 1 << 42;

impl MainView {
    pub fn new(
        status: Status,
        actions: ActionSender,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let project_search =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search projects..."));
        cx.subscribe(&project_search, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        })
        .detach();
        let scope_search =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search projects..."));
        cx.subscribe(&scope_search, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        })
        .detach();
        let composer = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Ask anything. @ files, $ skills, / commands")
                // The web's `min-h-[2lh]` and `max-h-52`.
                .auto_grow(2, 9)
                // Enter sends; Shift+Enter inserts a newline.
                .submit_on_enter(true)
        });
        // The send button enables with content.
        cx.subscribe(&composer, |this, _, event: &InputEvent, cx| match event {
            InputEvent::Change => cx.notify(),
            InputEvent::PressEnter { shift: false, .. } => this.submit(cx),
            _ => {}
        })
        .detach();
        let view = cx.weak_entity();
        let permissions = cx.new(|_| PermissionsPanel {
            view,
            selected: RuntimeMode::FullAccess,
            preview: None,
        });
        let model_search = cx.new(|cx| InputState::new(window, cx).placeholder("Search models..."));
        cx.subscribe(&model_search, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.model_ui.highlight = 0;
                this.model_ui.focus = model_picker::Focus::List;
                cx.notify();
            }
        })
        .detach();
        let branch_picker = cx.new(|cx| BranchPicker::new(actions.clone(), window, cx));
        cx.subscribe(
            &branch_picker,
            |this, _, event: &branch_picker::Event, cx| this.on_branch_event(event, cx),
        )
        .detach();
        let add_to_machine =
            cx.new(|cx| add_to_machine::AddToMachine::new(actions.clone(), window, cx));
        cx.subscribe(
            &add_to_machine,
            |this, _, notice: &add_to_machine::Notice, cx| {
                let kind = if notice.error {
                    ToastKind::Error
                } else {
                    ToastKind::Info
                };
                let toast = Toast::new(notice.title.clone()).kind(kind);
                let toast = match &notice.description {
                    Some(description) => toast.description(description.clone()),
                    None => toast,
                };
                this.toasts.update(cx, |toasts, cx| {
                    toasts.add(toast, cx);
                });
            },
        )
        .detach();
        let card = cx.new(|cx| DetailsCard::new(actions.clone(), window, cx));
        cx.subscribe(&card, |this, _, _: &OpenDiff, cx| this.open_diff(cx))
            .detach();
        let pull_request_panel = cx.new({
            let actions = actions.clone();
            move |_| pull_request_panel::PullRequestPanel::new(actions)
        });
        Self {
            status,
            search: cx.new(|cx| InputState::new(window, cx).placeholder("Search")),
            project_search,
            composer,
            settled_shown: SETTLED_TAIL_INITIAL_COUNT,
            shelves: [false; 3],
            sidebar_open: true,
            sidebar_overlay_open: false,
            sidebar_width: SIDEBAR_DEFAULT_WIDTH,
            rendered_sidebar_width: Cell::new(SIDEBAR_DEFAULT_WIDTH),
            brand_width: Rc::new(Cell::new(0.)),
            composer_height: Rc::new(Cell::new(0.)),
            reserved_composer_height: Cell::new(0.),
            send_hovered: false,
            permissions_open: false,
            workspace_open: false,
            host_open: false,
            host_highlight: 0,
            workspace_highlight: 0,
            workspace_keys: None,
            draft_worktrees: HashMap::new(),
            env_modes: HashMap::new(),
            draft_bases: HashMap::new(),
            branch_picker,
            model_open: false,
            model_search,
            model_picks: HashMap::new(),
            model_ui: model_picker::State::default(),
            model_keys: None,
            selected_thread: None,
            transcript: None,
            // Overdraw keeps a screenful of rows measured beyond each edge.
            transcript_list: ListState::new(0, ListAlignment::Top, px(600.)),
            minimap_items: Vec::new(),
            minimap_hover: None,
            runtime_mode: None,
            permissions,
            selected_project: None,
            history: History::default(),
            picker_open: false,
            project_scope: Vec::new(),
            scope_open: false,
            scope_search,
            adding_project: None,
            picker_trigger: Rc::default(),
            picker_order: None,
            environments: Vec::new(),
            providers: None,
            branches: HashMap::new(),
            branches_requested: HashSet::new(),
            details_closed: HashSet::new(),
            vcs: HashMap::new(),
            vcs_watching: None,
            right_panel_open: HashSet::new(),
            right_panel_width: right_panel::DEFAULT_WIDTH,
            diffs: HashMap::new(),
            diffs_requested: HashSet::new(),
            diff_list: cx.new(|_| right_panel::DiffList::new()),
            pull_request_tabs: HashMap::new(),
            right_panel_active: HashMap::new(),
            pull_request_panel: pull_request_panel.clone(),
            pull_requests_open: false,
            pull_requests_page: cx.new({
                let actions = actions.clone();
                move |_| pull_requests_page::PullRequestsPage::new(actions, pull_request_panel)
            }),
            terminal_open: HashSet::new(),
            terminal_heights: HashMap::new(),
            terminals: HashMap::new(),
            terminal_views: HashMap::new(),
            terminal_attached: Vec::new(),
            card,
            toasts: cx.new(|cx| ToastHost::new(TOPBAR_HEIGHT, window, cx)),
            usage: cx.new(|cx| usage_dialog::UsageDialog::new(actions.clone(), cx)),
            add_to_machine,
            settings: cx.new(|cx| settings::SettingsDialog::new(actions.clone(), window, cx)),
            slow_toast: None,
            main_width: Cell::new(0.),
            sending: None,
            clear_composer: false,
            attachments: Vec::new(),
            attachment_calls: HashMap::new(),
            attachment_token: ATTACH_TOKEN,
            resume_send: false,
            actions,
        }
    }

    /// `SlowRpcRequestToastCoordinator`: one toast for the list, shown, updated and closed with it.
    fn sync_slow_toast(
        &mut self,
        label: &str,
        requests: &[slow_requests::SlowRequest],
        cx: &mut Context<Self>,
    ) {
        let toast = slow_requests::toast(label, requests);
        self.toasts.update(cx, |toasts, cx| {
            match (requests.is_empty(), self.slow_toast) {
                (true, id) => {
                    if let Some(id) = id {
                        toasts.dismiss(id, cx);
                    }
                    self.slow_toast = None;
                }
                (false, None) => self.slow_toast = Some(toasts.add(toast, cx)),
                (false, Some(id)) => toasts.update(id, toast, cx),
            }
        });
    }

    /// Hands the Labs tab the scope bar's choices and the attached environment's flags.
    fn sync_labs(&mut self, cx: &mut Context<Self>) {
        let (label, machine, entries) = match &self.status {
            Status::Connected {
                label,
                machine,
                shell,
                ..
            } => (
                label.clone(),
                *machine,
                shell
                    .as_ref()
                    .map(|s| s.project_entries.clone())
                    .unwrap_or_default(),
            ),
            _ => return,
        };
        let data = crate::settings_labs::scope_data(
            &label,
            machine,
            &crate::config::primary_url(),
            &entries,
            &self.environments,
        );
        let flags = self.providers.as_ref().map(|c| c.labs_flags());
        let devices = self.providers.as_ref().map(|c| {
            (
                c.devices().clone(),
                crate::settings_devices::project_ids(&entries),
                c.source_control().secret_sources().to_vec(),
            )
        });
        let source_control = self.providers.as_ref().map(|c| c.source_control().clone());
        let secrets = self.providers.as_ref().map(|c| c.secrets().clone());
        let project_ids = crate::settings_devices::project_ids(&entries);
        self.settings.update(cx, |settings, cx| {
            settings.set_labs_data(data, flags, cx);
            if let Some((config, projects, sources)) = devices {
                settings.set_devices_data(config, projects, sources, cx);
            }
            if let Some(config) = source_control {
                settings.set_source_control_data(config, project_ids, cx);
            }
            if let Some(config) = secrets {
                settings.set_secrets_data(config, cx);
            }
        });
    }

    pub fn set_status(&mut self, status: Status, cx: &mut Context<Self>) {
        if let Status::ProjectFailed(message) = status {
            self.adding_project = None;
            self.error_toast("Failed to add project", message, cx);
            return;
        }
        if let Status::ProjectAdded(id) = status {
            self.adding_project = Some(id);
            return self.open_added_project(cx);
        }
        if let Status::SlowRequests { label, requests } = status {
            self.sync_slow_toast(&label, &requests, cx);
            return;
        }
        if let Status::Environment(other) = status {
            match self.environments.iter_mut().find(|e| e.id == other.id) {
                Some(known) => *known = other,
                None => self.environments.push(other),
            }
            self.sync_labs(cx);
            return cx.notify();
        }
        if let Status::Providers(catalog) = status {
            self.usage
                .update(cx, |usage, cx| usage.set_providers(&catalog, cx));
            self.providers = Some(*catalog);
            self.sync_labs(cx);
            return cx.notify();
        }
        if let Status::Thread { id, transcript } = status {
            self.set_transcript(id, transcript);
            return cx.notify();
        }
        if let Status::Branch { root, branch } = status {
            self.branches.insert(root, branch);
            return cx.notify();
        }
        if let Status::Vcs { cwd, status } = status {
            self.vcs.insert(cwd, status);
            return cx.notify();
        }
        if let Status::Terminal {
            thread_id,
            terminal_id,
            event,
        } = status
        {
            // The web closes a terminal whose session exited, without asking.
            let exited = matches!(
                event,
                TerminalEvent::Exited | TerminalEvent::Snapshot { exited: true, .. }
            );
            if let Some(view) = self
                .terminal_views
                .get(&(thread_id.clone(), terminal_id.clone()))
            {
                view.update(cx, |view, cx| view.apply(event, cx));
            }
            if exited {
                self.close_terminal(&thread_id, &terminal_id, cx);
            }
            return;
        }
        if let Status::Diff { cwd, diff } = status {
            self.set_diff(cwd, diff);
            return cx.notify();
        }
        if let Status::Call { token, event } = status {
            if token >= crate::settings_secrets::TOKEN_BASE {
                self.settings.update(cx, |settings, cx| {
                    settings.on_secrets_call(token, event, cx)
                });
                return;
            }
            if token >= pull_requests_page::TOKEN_BASE {
                self.pull_requests_page
                    .update(cx, |page, cx| page.on_call(token, event, cx));
                return;
            }
            if token >= pull_request_panel::TOKEN_BASE {
                self.pull_request_panel
                    .update(cx, |panel, cx| panel.on_call(token, event, cx));
                return;
            }
            if token >= add_to_machine::TOKEN_BASE {
                self.add_to_machine
                    .update(cx, |dialog, cx| dialog.on_call(token, event, cx));
                return;
            }
            if token >= crate::settings_source_control::TOKEN_BASE {
                self.settings.update(cx, |settings, cx| {
                    settings.on_source_control_call(token, event, cx)
                });
                return;
            }
            if token >= crate::settings_devices::TOKEN_BASE {
                self.settings.update(cx, |settings, cx| {
                    settings.on_devices_call(token, event, cx)
                });
                return;
            }
            if token >= crate::settings_labs::TOKEN_BASE {
                self.settings
                    .update(cx, |settings, cx| settings.on_labs_call(token, event, cx));
                return;
            }
            if token >= usage_dialog::TOKEN_BASE {
                self.usage
                    .update(cx, |usage, cx| usage.on_call(token, event, cx));
                return;
            }
            if token >= ATTACH_TOKEN {
                self.on_attachment_call(token, event, cx);
                return;
            }
            if token >= SEND_TOKEN {
                self.on_send_call(token, event, cx);
                return;
            }
            if token >= branch_picker::TOKEN_BASE {
                self.branch_picker
                    .update(cx, |picker, cx| picker.on_call(token, event, cx));
                return;
            }
            self.card
                .update(cx, |card, cx| card.on_call(token, event, cx));
            return;
        }
        if matches!(status, Status::Connecting) {
            self.branches_requested.clear();
            self.vcs.clear();
            self.diffs_requested.clear();
            self.pull_request_panel
                .update(cx, |panel, cx| panel.reset(cx));
            if self.pull_requests_open {
                self.pull_requests_page
                    .update(cx, |page, cx| page.reset(cx));
            } else {
                self.pull_requests_page.update(cx, |page, _| page.forget());
            }
        }
        let access = match &status {
            Status::NotConnected => usage_dialog::Access::Unavailable,
            Status::Starting | Status::Connecting => usage_dialog::Access::Pending,
            Status::Failed(_) => usage_dialog::Access::Offline,
            _ => usage_dialog::Access::Ready,
        };
        let environment = match &status {
            Status::Connected { label, .. } => strip::environment_label(label),
            _ => String::new(),
        };
        self.usage.update(cx, |usage, cx| {
            usage.set_environment(environment);
            usage.set_access(access, cx);
        });
        self.status = status;
        // Thread activity reorders recency on every delta, so the default must not follow it.
        if self.selected_project.is_none() {
            self.selected_project = self
                .shell()
                .and_then(|shell| active_project(None, shell))
                .map(|entry| entry.id.clone());
            if let Some(id) = &self.selected_project {
                self.history.visit(Route {
                    project: Some(id.clone()),
                    thread: None,
                });
            }
        }
        self.open_added_project(cx);
        self.request_branch();
        self.sync_vcs();
        self.sync_terminal(cx);
        self.sync_diff();
        cx.notify();
    }

    /// Sends the composer: a draft launches its thread, an open thread takes a message.
    fn submit(&mut self, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).value().to_string();
        if !send::can_send(&text, &self.attachments, self.sending.is_some()) {
            return;
        }
        let failed: Vec<String> = self
            .attachments
            .iter()
            .filter(|item| matches!(item.upload, Upload::Failed { .. }))
            .map(|item| item.id.clone())
            .collect();
        if !failed.is_empty() {
            for id in &failed {
                self.start_upload(id, cx);
            }
            self.resume_send = true;
            return cx.notify();
        }
        let planned = {
            let project = self
                .shell()
                .and_then(|shell| active_project(self.selected_project.as_deref(), shell));
            let project_id = project.map(|entry| entry.id.as_str());
            let defaults = self.providers.as_ref().map_or_else(
                || Defaults::resolve(None, None, None),
                |catalog| catalog.defaults(project_id),
            );
            let key = self.details_key();
            let pick = self.model_pick(key.as_deref());
            let trigger = self
                .providers
                .as_ref()
                .and_then(|catalog| catalog.trigger_for(project_id, pick));
            let mode = project
                .and_then(|entry| self.env_modes.get(&entry.id).copied())
                .unwrap_or(defaults.env_mode);
            let current = project
                .and_then(|entry| self.branches.get(&entry.root))
                .and_then(Option::as_ref);
            let base = project.and_then(|entry| self.draft_bases.get(&entry.id));
            let composer = send::Composer {
                thread: self.selected_thread.as_deref(),
                project: project_id,
                text: &text,
                model: trigger.as_ref().map(|trigger| send::Model {
                    instance_id: &trigger.instance_id,
                    slug: &trigger.slug,
                    options: &trigger.options,
                }),
                picked: pick.is_some(),
                runtime_mode: self.runtime_mode.unwrap_or(defaults.runtime_mode),
                workspace: match project.and_then(|entry| self.draft_worktrees.get(&entry.id)) {
                    Some(seed) => Ok(send::Workspace::Existing {
                        worktree_path: &seed.worktree_path,
                        branch: seed.branch.as_deref(),
                    }),
                    None => send::Workspace::resolve(
                        mode,
                        strip::shown_branch(mode, current, base),
                        defaults.start_from_origin,
                    ),
                },
                running: self.thread_running(),
                attachments: &self.attachments,
            };
            send::plan(
                &composer,
                send::Ids {
                    command: command_id(),
                    thread: new_project_id(),
                    message: new_project_id(),
                },
            )
            .map(|plan| (plan, key, project_id.map(str::to_owned)))
        };
        let (plan, from_key, draft_project) = match planned {
            Ok(planned) => planned,
            Err(message) => return self.error_toast("Could not send the message", message, cx),
        };
        let action = Action::Call {
            token: SEND_TOKEN,
            call: plan.call,
            payload: plan.payload,
        };
        if self.actions.try_send(action).is_err() {
            return self.error_toast(
                "Could not send the message",
                "The connection is not ready.".to_owned(),
                cx,
            );
        }
        self.sending = Some(Sending {
            opens: plan.opens,
            from_key,
            draft_project,
            attachments: self
                .attachments
                .iter()
                .map(|item| item.id.clone())
                .collect(),
        });
        cx.notify();
    }

    /// The attach button: the native picker stands in for the web's file input.
    fn attach_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Attach files".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = picked.await;
            this.update_in(cx, |this, window, cx| match result {
                Ok(Ok(Some(paths))) => this.add_attachments(paths, window, cx),
                Ok(Err(error)) => this.error_toast("Failed to attach files", error.to_string(), cx),
                _ => {}
            })
            .ok();
        })
        .detach();
    }

    /// The web's `addComposerAttachments`: validate, stage and start uploading each file.
    fn add_attachments(
        &mut self,
        paths: Vec<std::path::PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let picked: Vec<attachments::Picked> = paths
            .into_iter()
            .map(|path| attachments::Picked {
                name: path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                size: std::fs::metadata(&path).map_or(0, |meta| meta.len()),
                path,
            })
            .collect();
        let limit = attachments::staging_limit(
            self.providers
                .as_ref()
                .map(|catalog| catalog.file_upload_limit()),
        );
        let (accepted, error) = attachments::intake(&picked, self.attachments.len(), limit);
        if let Some(message) = error {
            self.message_toast(message, cx);
        }
        let no_project = self.selected_thread.is_none()
            && self
                .shell()
                .and_then(|shell| active_project(self.selected_project.as_deref(), shell))
                .is_none();
        let images_chip =
            attachments::images_get_chips(&self.composer.read(cx).value(), no_project);
        let mut added = Vec::new();
        for file in accepted {
            let id = new_project_id();
            let item = attachments::Item {
                id: id.clone(),
                path: file.path,
                kind: file.kind,
                name: file.name,
                mime: file.mime,
                size: file.size,
                upload: Upload::Uploading { progress: 0. },
            };
            added.push(item.clone());
            self.attachments.push(item);
            self.start_upload(&id, cx);
        }
        added.sort_by_key(|item| item.kind == attachments::Kind::Image);
        added.retain(|item| item.kind == attachments::Kind::File || images_chip);
        self.insert_references(&added, window, cx);
        cx.notify();
    }

    /// The web's `insertAttachmentReferences`: one chip per item at the cursor, each followed by a
    /// space, with a leading space unless the cursor follows whitespace.
    fn insert_references(
        &mut self,
        items: &[attachments::Item],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if items.is_empty() {
            return;
        }
        self.composer.update(cx, |composer, cx| {
            let text = composer.value();
            let cursor = composer.cursor().min(text.len());
            let plan = attachments::insertion(
                text[..cursor].chars().next_back(),
                text[cursor..].chars().next(),
            );
            composer.set_selected_range(cursor..cursor, cx);
            if plan.leading_space {
                composer.insert(" ", window, cx);
            }
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    composer.insert(" ", window, cx);
                }
                let reference = item.reference();
                let token = InlineToken::new(item.context_id(), reference.clone());
                if composer.replace_with_token(token, window, cx).is_err() {
                    composer.insert(reference, window, cx);
                }
            }
            if plan.reuse_next_space {
                let end = composer.cursor() + 1;
                composer.set_selected_range(end..end, cx);
            } else {
                composer.insert(" ", window, cx);
            }
            composer.focus(window, cx);
        });
    }

    /// The web's `removeInlineContextReference`.
    fn remove_inline_reference(
        &mut self,
        context_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.composer.update(cx, |composer, cx| {
            let text = composer.value();
            for range in attachments::removal_ranges(&text, context_id) {
                composer.set_selected_range(range, cx);
                composer.replace("", window, cx);
            }
        });
    }

    /// Asks the server for an upload URL; `on_attachment_call` posts the file once it answers.
    fn start_upload(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(item) = self.attachments.iter_mut().find(|item| item.id == id) else {
            return;
        };
        item.upload = Upload::Uploading { progress: 0. };
        let mut payload = serde_json::json!({
            "name": item.name,
            "mimeType": item.mime,
            "sizeBytes": item.size,
        });
        if item.kind == attachments::Kind::File {
            payload["type"] = "file".into();
        }
        self.attachment_token += 1;
        let token = self.attachment_token;
        let sent = self.actions.try_send(Action::Call {
            token,
            call: Call::CreateUploadUrl,
            payload,
        });
        if sent.is_ok() {
            self.attachment_calls.insert(
                token,
                AttachmentCall {
                    item: id.to_owned(),
                    minted: None,
                },
            );
        } else {
            self.fail_upload(id, "Upload could not start".to_owned());
        }
        cx.notify();
    }

    fn fail_upload(&mut self, id: &str, reason: String) {
        if let Some(item) = self.attachments.iter_mut().find(|item| item.id == id) {
            item.upload = Upload::Failed { reason };
        }
    }

    fn delete_attachment(&mut self, attachment_id: &str) {
        self.actions
            .try_send(Action::Call {
                token: DELETE_ATTACHMENT_TOKEN,
                call: Call::DeleteAttachment,
                payload: serde_json::json!({ "attachmentId": attachment_id }),
            })
            .ok();
    }

    fn on_attachment_call(&mut self, token: u64, event: CallEvent, cx: &mut Context<Self>) {
        let Some(call) = self.attachment_calls.get(&token) else {
            return;
        };
        let (item, minted) = (call.item.clone(), call.minted.clone());
        let result = match event {
            CallEvent::Progress(progress) => {
                let fraction = progress.as_f64().unwrap_or_default() as f32;
                if let Some(entry) = self.attachments.iter_mut().find(|entry| entry.id == item) {
                    entry.upload = Upload::Uploading { progress: fraction };
                }
                return cx.notify();
            }
            CallEvent::Done(value) => Ok(value),
            CallEvent::Failed(reason) => Err(reason),
        };
        self.attachment_calls.remove(&token);
        let present = self.attachments.iter().any(|entry| entry.id == item);
        match (minted, result) {
            (None, Ok(value)) => {
                let url = value.get("relativeUrl").and_then(|url| url.as_str());
                let id = value.get("attachmentId").and_then(|id| id.as_str());
                let entry = self.attachments.iter().find(|entry| entry.id == item);
                match (url, id, entry) {
                    (Some(url), Some(id), Some(entry)) => {
                        let action = Action::Upload {
                            token,
                            relative_url: url.to_owned(),
                            path: entry.path.clone(),
                            mime: entry.mime.clone(),
                        };
                        if self.actions.try_send(action).is_ok() {
                            self.attachment_calls.insert(
                                token,
                                AttachmentCall {
                                    item,
                                    minted: Some(id.to_owned()),
                                },
                            );
                        } else {
                            self.fail_upload(&item, "Upload could not start".to_owned());
                        }
                    }
                    (_, _, None) => {}
                    _ => self.fail_upload(&item, "Upload could not start".to_owned()),
                }
            }
            (None, Err(_)) => self.fail_upload(&item, "Upload could not start".to_owned()),
            (Some(attachment_id), Ok(_)) => {
                if present {
                    if let Some(entry) = self.attachments.iter_mut().find(|entry| entry.id == item)
                    {
                        entry.upload = Upload::Ready { attachment_id };
                    }
                } else {
                    self.delete_attachment(&attachment_id);
                }
            }
            (Some(attachment_id), Err(reason)) => {
                self.delete_attachment(&attachment_id);
                self.fail_upload(&item, reason);
            }
        }
        self.resume_pending_send(cx);
        cx.notify();
    }

    /// Resumes a send that retried failed uploads once none is still uploading.
    fn resume_pending_send(&mut self, cx: &mut Context<Self>) {
        if !self.resume_send
            || self
                .attachments
                .iter()
                .any(|item| matches!(item.upload, Upload::Uploading { .. }))
        {
            return;
        }
        self.resume_send = false;
        if self
            .attachments
            .iter()
            .any(|item| matches!(item.upload, Upload::Failed { .. }))
        {
            return self.message_toast(
                "Retry or remove failed uploads before sending.".to_owned(),
                cx,
            );
        }
        self.submit(cx);
    }

    fn remove_attachment(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.attachments.iter().position(|item| item.id == id) else {
            return;
        };
        let item = self.attachments.remove(index);
        self.remove_inline_reference(&item.context_id(), window, cx);
        if let Some(attachment_id) = item.attachment_id() {
            self.delete_attachment(attachment_id);
        }
        self.resume_pending_send(cx);
        cx.notify();
    }

    fn message_toast(&mut self, message: String, cx: &mut Context<Self>) {
        let toast = Toast::new(message).kind(ToastKind::Error);
        self.toasts.update(cx, |toasts, cx| {
            toasts.add(toast, cx);
        });
    }

    /// The web's `ComposerAttachmentMediaRow` and `ComposerAttachmentFileList`.
    fn attachment_strip(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let inline: HashSet<String> =
            attachments::collect_references(&self.composer.read(cx).value())
                .into_iter()
                .map(|reference| reference.context_id)
                .collect();
        let shown: Vec<&attachments::Item> = self
            .attachments
            .iter()
            .filter(|item| {
                item.kind == attachments::Kind::Image || !inline.contains(&item.context_id())
            })
            .collect();
        if shown.is_empty() {
            return None;
        }
        let theme = cx.theme();
        let (border, background, foreground, muted, accent) = (
            theme.border.opacity(0.8),
            theme.background,
            theme.foreground,
            theme.muted_foreground,
            theme.accent,
        );
        let ghost = StripButton::Ghost {
            icon: muted,
            hover: accent,
        };
        let mut tiles = Vec::new();
        let mut rows = Vec::new();
        for item in shown {
            let remove_id = item.id.clone();
            let retry_id = item.id.clone();
            let progress = match &item.upload {
                Upload::Uploading { progress } => Some(attachments::format_progress(*progress)),
                _ => None,
            };
            let failed = match &item.upload {
                Upload::Failed { reason } => Some(reason.clone()),
                _ => None,
            };
            let image = item.kind == attachments::Kind::Image;
            let remove = attachment_button(
                format!("attachment-remove-{}", item.id),
                Icons::X,
                format!("Remove {}", item.name),
                if image {
                    StripButton::MediaClose
                } else {
                    ghost
                },
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.remove_attachment(&remove_id, window, cx)
            }));
            let retry = failed.map(|reason| {
                attachment_button(
                    format!("attachment-retry-{}", item.id),
                    Icons::RefreshCw,
                    reason,
                    if image { StripButton::Overlay } else { ghost },
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.start_upload(&retry_id, cx);
                    this.resume_send = false;
                }))
            });
            if image {
                tiles.push(
                    div()
                        .id(SharedString::from(format!("attachment-{}", item.id)))
                        .relative()
                        .flex_shrink_0()
                        .size(px(64.))
                        .overflow_hidden()
                        .rounded(px(8.))
                        .border_1()
                        .border_color(border)
                        .bg(background)
                        .child(
                            img(item.path.clone())
                                .size_full()
                                .object_fit(ObjectFit::Cover),
                        )
                        .children(progress.map(|label| {
                            div()
                                .absolute()
                                .bottom_0()
                                .left_0()
                                .right_0()
                                .px_1()
                                .text_center()
                                .text_size(px(10.))
                                .text_color(foreground)
                                .bg(background.opacity(0.85))
                                .child(label)
                        }))
                        .children(
                            retry.map(|retry| div().absolute().bottom_1().left_1().child(retry)),
                        )
                        .child(div().absolute().top_1().right_1().child(remove))
                        .into_any_element(),
                );
            } else {
                let detail = progress.unwrap_or_else(|| attachments::format_size(item.size));
                rows.push(
                    h_flex()
                        .min_w_0()
                        .items_center()
                        .gap_2()
                        .py_1()
                        .text_sm()
                        .text_color(foreground)
                        .child(Icon::new(Icons::File).size_4().text_color(muted))
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .child(item.name.clone()),
                        )
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_xs()
                                .text_color(muted)
                                .child(detail),
                        )
                        .children(retry)
                        .child(remove)
                        .into_any_element(),
                );
            }
        }
        Some(
            v_flex()
                .mb_3()
                .gap_1()
                .when(!tiles.is_empty(), |strip| {
                    strip.child(h_flex().flex_wrap().gap_2().children(tiles))
                })
                .children(rows)
                .into_any_element(),
        )
    }

    /// The web's stop button: `thread.stop` for the open thread.
    fn stop(&mut self, cx: &mut Context<Self>) {
        let Some(thread) = self.selected_thread.clone() else {
            return;
        };
        let action = Action::Call {
            token: STOP_TOKEN,
            call: crate::connection::Call::DispatchCommand,
            payload: send::stop_payload(&command_id(), &thread),
        };
        if self.actions.try_send(action).is_err() {
            self.error_toast(
                "Failed to stop the turn",
                "The connection is not ready.".to_owned(),
                cx,
            );
        }
    }

    fn on_send_call(&mut self, token: u64, event: CallEvent, cx: &mut Context<Self>) {
        let failed = match event {
            CallEvent::Progress(_) => return,
            CallEvent::Done(_) => None,
            CallEvent::Failed(error) => Some(error),
        };
        if token == STOP_TOKEN {
            if let Some(error) = failed {
                self.error_toast("Failed to stop the turn", error, cx);
            }
            return;
        }
        if token == FAVORITE_TOKEN {
            if let Some(error) = failed {
                self.error_toast("Failed to update favorites", error, cx);
            }
            return;
        }
        if token == DELETE_ATTACHMENT_TOKEN {
            return;
        }
        let Some(sent) = self.sending.take() else {
            return;
        };
        if let Some(error) = failed {
            self.error_toast("Could not send the message", error, cx);
            return cx.notify();
        }
        self.clear_composer = true;
        self.attachments
            .retain(|item| !sent.attachments.contains(&item.id));
        if let Some(thread) = sent.opens {
            if let Some(project) = &sent.draft_project {
                self.draft_worktrees.remove(project);
            }
            if let Some(pick) = sent.from_key.and_then(|key| self.model_picks.remove(&key)) {
                self.model_picks.insert(thread.clone(), pick);
            }
            self.open_thread(Some(thread), cx);
        }
        cx.notify();
    }

    /// Whether the open thread is mid-turn, which turns Stop on and makes a message steer.
    fn thread_running(&self) -> bool {
        let id = self.selected_thread.as_deref();
        self.shell().is_some_and(|shell| {
            shell
                .rows
                .iter()
                .any(|row| Some(row.id.as_str()) == id && row.top == Some(TopStatus::Working))
        })
    }

    fn error_toast(&mut self, title: &str, description: String, cx: &mut Context<Self>) {
        let toast = Toast::new(title.to_owned())
            .kind(ToastKind::Error)
            .description(description);
        self.toasts.update(cx, |toasts, cx| {
            toasts.add(toast, cx);
        });
    }

    /// Opens a new thread in the project the server created, as the web does once it exists.
    fn open_added_project(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.adding_project.clone() else {
            return;
        };
        let listed = self.shell().is_some_and(|shell| {
            shell
                .project_entries
                .iter()
                .any(|entry| entry.contains(&id))
        });
        if listed {
            self.adding_project = None;
            self.select_project(id, cx);
        }
    }

    /// Keeps the rows of the open thread; an update for any other thread is stale.
    fn set_transcript(&mut self, id: String, result: Result<Vec<Entry>, String>) {
        if self.selected_thread.as_deref() != Some(id.as_str()) {
            return;
        }
        let count = |result: &Result<Vec<Entry>, String>| result.as_ref().map_or(0, Vec::len);
        let previous = self.transcript.take().filter(|t| t.id == id);
        match &previous {
            // Rows that streamed keep their measured height; only the changed tail is remeasured.
            Some(previous) => {
                let same = match (&previous.result, &result) {
                    (Ok(old), Ok(new)) => old.iter().zip(new).take_while(|(a, b)| a == b).count(),
                    _ => 0,
                };
                let (old, new) = (count(&previous.result), count(&result));
                // The old last row loses the composer reserve when rows arrive after it.
                let same = if same == old && new > old {
                    same.saturating_sub(1)
                } else {
                    same
                };
                self.transcript_list.splice(same..old, new - same);
            }
            None => {
                self.transcript_list.reset(count(&result));
                self.transcript_list.set_follow_mode(FollowMode::Tail);
            }
        }
        self.minimap_items = result
            .as_ref()
            .map_or_else(|_| Vec::new(), |entries| minimap::items(entries));
        self.minimap_hover = None;
        self.transcript = Some(Transcript { id, result });
    }

    /// Points the connection's thread subscription at the selection.
    fn sync_thread(&mut self) {
        if self.transcript.as_ref().map(|t| &t.id) != self.selected_thread.as_ref() {
            self.transcript = None;
        }
        let action = Action::SelectThread(self.selected_thread.clone());
        if self.actions.try_send(action).is_err() {
            eprintln!("[ui] could not ask for the thread");
        }
    }

    fn apply_route(&mut self, route: Route, cx: &mut Context<Self>) {
        self.sidebar_overlay_open = false;
        self.pull_requests_open = false;
        // A thread the scope hides widens it to all projects, like the web's route reveal.
        let hidden = self
            .shell()
            .zip(route.thread.as_deref())
            .is_some_and(|(shell, id)| {
                shell
                    .rows
                    .iter()
                    .find(|row| row.id == id)
                    .is_some_and(|row| {
                        !projects::in_scope(&shell.project_entries, &self.scope(), &row.project_id)
                    })
            });
        if hidden {
            self.project_scope.clear();
        }
        self.selected_project = route.project;
        self.selected_thread = route.thread;
        self.sync_thread();
        self.request_branch();
        self.sync_vcs();
        self.sync_terminal(cx);
        self.sync_diff();
        cx.notify();
    }

    /// Opens a thread in its project, or the empty new-thread state for `None`.
    fn open_thread(&mut self, id: Option<String>, cx: &mut Context<Self>) {
        let project = self.shell().and_then(|shell| {
            let row = shell.rows.iter().find(|row| Some(&row.id) == id.as_ref())?;
            let entry = shell
                .project_entries
                .iter()
                .find(|entry| entry.contains(&row.project_id))?;
            Some(entry.id.clone())
        });
        let route = Route {
            project: project.or_else(|| self.selected_project.clone()),
            thread: id,
        };
        self.history.visit(route.clone());
        self.apply_route(route, cx);
    }

    /// The web's brand link to `/`, which opens a new draft in the most recently updated project.
    fn go_home(&mut self, cx: &mut Context<Self>) {
        let project = self
            .shell()
            .and_then(|shell| active_project(None, shell))
            .map(|entry| entry.id.clone())
            .or_else(|| self.selected_project.clone());
        let route = Route {
            project,
            thread: None,
        };
        self.history.visit(route.clone());
        self.apply_route(route, cx);
    }

    /// The `chat.newLocal` command: a draft in the open thread's project, else the selected one.
    fn new_thread(&mut self, cx: &mut Context<Self>) {
        if self
            .shell()
            .is_some_and(|shell| !shell.project_entries.is_empty())
        {
            self.open_thread(None, cx);
        }
    }

    /// The `chat.newWithoutProject` command: a draft in the managed scratch project.
    fn new_scratch_thread(&mut self, cx: &mut Context<Self>) {
        let scratch = self
            .shell()
            .and_then(|shell| scratch_entry(&shell.project_entries))
            .map(|entry| entry.id.clone());
        if let Some(id) = scratch {
            self.select_project(id, cx);
        }
    }

    /// The web's add project flow with the native folder picker standing in for its browse step.
    fn add_project(&mut self, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Add project".into()),
        });
        cx.spawn(async move |this, cx| {
            let result = picked.await;
            this.update(cx, |this, cx| match result {
                Ok(Ok(Some(paths))) => {
                    if let Some(path) = paths.first() {
                        this.create_project(&path.to_string_lossy(), cx);
                    }
                }
                Ok(Err(error)) => this.error_toast("Failed to add project", error.to_string(), cx),
                _ => {}
            })
            .ok();
        })
        .detach();
    }

    /// A folder that is already a project opens a new thread in it; any other becomes one.
    fn create_project(&mut self, path: &str, cx: &mut Context<Self>) {
        let Some(new) = NewProject::from_path(path) else {
            return;
        };
        let Some(shell) = self.shell() else {
            return;
        };
        if let Some(existing) = projects::project_at(&shell.project_entries, &new.workspace_root) {
            let id = existing.id.clone();
            return self.select_project(id, cx);
        }
        let project_id = new_project_id();
        self.adding_project = Some(project_id.clone());
        let add = Action::AddProject {
            project_id,
            title: new.title,
            workspace_root: new.workspace_root,
        };
        if self.actions.try_send(add).is_err() {
            eprintln!("[ui] could not add the project");
        }
    }

    /// The sidebar's project scope without the projects that are gone.
    fn scope(&self) -> Vec<String> {
        self.shell().map_or_else(Vec::new, |shell| {
            projects::resolve_scope(&shell.project_entries, &self.project_scope)
        })
    }

    /// A pick in the scope menu; a plain pick closes it, a shift pick keeps it open to add more.
    fn pick_scope(&mut self, key: Option<&str>, additive: bool, cx: &mut Context<Self>) {
        self.project_scope = projects::pick_scope(&self.scope(), key, additive);
        self.scope_open = additive && key.is_some();
        cx.notify();
    }

    /// Asks for the branch of the active project's root once per connection.
    fn request_branch(&mut self) {
        let Some(entry) = self
            .shell()
            .and_then(|shell| active_project(self.selected_project.as_deref(), shell))
        else {
            return;
        };
        let root = entry.root.clone();
        if entry.scratch || root.is_empty() || !self.branches_requested.insert(root.clone()) {
            return;
        }
        if self.actions.try_send(Action::LoadBranch { root }).is_err() {
            eprintln!("[ui] could not ask for the branch");
        }
    }

    /// Where the details card's state is kept: the open thread, or the project of a draft.
    fn details_key(&self) -> Option<String> {
        if let Some(id) = &self.selected_thread {
            return Some(id.clone());
        }
        let shell = self.shell()?;
        let entry = active_project(self.selected_project.as_deref(), shell)?;
        Some(format!("draft:{}", entry.id))
    }

    fn details_open(&self) -> bool {
        self.details_key()
            .is_some_and(|key| !self.details_closed.contains(&key))
    }

    /// The directory the card works on: the thread's worktree, else its project's root.
    fn details_cwd(&self) -> Option<String> {
        let shell = self.shell()?;
        let row = self
            .selected_thread
            .as_deref()
            .and_then(|id| shell.rows.iter().find(|row| row.id == id));
        if let Some(path) = row.and_then(|row| row.worktree_path.clone()) {
            return Some(path);
        }
        let entry = match row {
            Some(row) => shell
                .project_entries
                .iter()
                .find(|entry| entry.contains(&row.project_id)),
            None if self.selected_thread.is_some() => None,
            None => active_project(self.selected_project.as_deref(), shell),
        }?;
        (!entry.scratch && !entry.root.is_empty()).then(|| entry.root.clone())
    }

    fn toggle_details(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.details_key() else {
            return;
        };
        if !self.details_closed.remove(&key) {
            self.details_closed.insert(key);
        }
        self.card.update(cx, |card, _| card.close_popovers());
        self.sync_vcs();
        cx.notify();
    }

    /// Watches the status of the card's directory while the card is open, and only then.
    fn sync_vcs(&mut self) {
        let wanted = self.details_open().then(|| self.details_cwd()).flatten();
        if wanted == self.vcs_watching {
            return;
        }
        self.vcs_watching = wanted.clone();
        if self.actions.try_send(Action::WatchVcs(wanted)).is_err() {
            eprintln!("[ui] could not ask for the version control status");
        }
    }

    /// Where the open thread's terminal runs, which needs the same directory the card works on.
    fn terminal_target(&self) -> Option<Target> {
        let key = self.details_key()?;
        let cwd = self.details_cwd()?;
        let shell = self.shell()?;
        let row = self
            .selected_thread
            .as_deref()
            .and_then(|id| shell.rows.iter().find(|row| row.id == id));
        let worktree_path = row.and_then(|row| row.worktree_path.clone());
        let entry = match row {
            Some(row) => shell
                .project_entries
                .iter()
                .find(|entry| entry.contains(&row.project_id)),
            None => active_project(self.selected_project.as_deref(), shell),
        };
        let project_root = entry
            .map(|entry| entry.root.clone())
            .filter(|root| !root.is_empty())
            .unwrap_or(cwd);
        Some(Target {
            thread_id: key,
            project_root,
            worktree_path,
            terminal_id: terminal::DEFAULT_TERMINAL_ID.into(),
        })
    }

    fn terminal_visible(&self) -> bool {
        self.details_key()
            .is_some_and(|key| self.terminal_open.contains(&key))
            && self.terminal_target().is_some()
    }

    /// The web's `terminal.toggle`. Closing the drawer leaves the shells running.
    fn toggle_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(key) = self.details_key() else {
            return;
        };
        if self.terminal_target().is_none() {
            return;
        }
        let opened = self.terminal_open.insert(key.clone());
        if opened {
            let terminals = self.terminals.entry(key).or_default();
            if terminals.is_empty() {
                terminals.add(terminal::DEFAULT_TERMINAL_ID, Placement::New);
            }
        } else {
            self.terminal_open.remove(&key);
        }
        self.sync_terminal(cx);
        if opened {
            self.focus_terminal(window, cx);
        }
        cx.notify();
    }

    fn active_terminal(&self) -> Option<(String, &Terminals)> {
        let key = self.details_key()?;
        let terminals = self.terminals.get(&key)?;
        Some((key, terminals))
    }

    fn focus_terminal(&self, window: &mut Window, cx: &mut Context<Self>) {
        let view = self
            .active_terminal()
            .and_then(|(key, terminals)| self.terminal_views.get(&(key, terminals.active.clone())));
        if let Some(view) = view {
            let handle = view.focus_handle(cx);
            window.focus(&handle, cx);
        }
    }

    /// The terminals the open drawer shows: the visible group.
    fn wanted_terminals(&self) -> Vec<Target> {
        if !self.terminal_visible() {
            return Vec::new();
        }
        let Some(base) = self.terminal_target() else {
            return Vec::new();
        };
        self.terminals
            .get(&base.thread_id)
            .and_then(Terminals::visible)
            .map(|group| group.ids.iter().map(|id| base.for_terminal(id)).collect())
            .unwrap_or_default()
    }

    /// Attaches the visible group's terminals to the connection, and detaches the rest.
    fn sync_terminal(&mut self, cx: &mut Context<Self>) {
        let wanted = self.wanted_terminals();
        for target in &wanted {
            let key = (target.thread_id.clone(), target.terminal_id.clone());
            if !self.terminal_views.contains_key(&key) {
                let actions = self.actions.clone();
                let (thread_id, terminal_id) = key.clone();
                let view = cx.new(|cx| TerminalView::new(thread_id, terminal_id, actions, cx));
                self.terminal_views.insert(key, view);
            }
        }
        self.terminal_views.retain(|(thread_id, terminal_id), _| {
            wanted
                .iter()
                .any(|target| &target.thread_id == thread_id && &target.terminal_id == terminal_id)
        });
        if wanted == self.terminal_attached {
            return;
        }
        self.terminal_attached = wanted.clone();
        if self
            .actions
            .try_send(Action::AttachTerminals(wanted))
            .is_err()
        {
            eprintln!("[ui] could not ask for the terminals");
        }
    }

    /// `onNewTerminal`, `onSplitTerminal` and `onSplitTerminalVertical`; a full group refuses.
    fn add_terminal(&mut self, placement: Placement, window: &mut Window, cx: &mut Context<Self>) {
        if !self.terminal_visible() {
            return;
        }
        let Some(key) = self.details_key() else {
            return;
        };
        let terminals = self.terminals.entry(key).or_default();
        let id = terminals.next_id(&terminal_state::uuid_suffix());
        if !terminals.add(&id, placement) {
            return;
        }
        self.sync_terminal(cx);
        self.focus_terminal(window, cx);
        cx.notify();
    }

    fn activate_terminal(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(key) = self.details_key() else {
            return;
        };
        let Some(terminals) = self.terminals.get_mut(&key) else {
            return;
        };
        if terminals.active == id {
            return;
        }
        terminals.activate(id);
        self.sync_terminal(cx);
        self.focus_terminal(window, cx);
        cx.notify();
    }

    /// The web asks before closing a terminal by hand (`confirmTerminalClose`).
    fn request_close(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(key) = self.details_key() else {
            return;
        };
        let label = terminal_state::label(&id);
        let answer = window.prompt(
            gpui::PromptLevel::Warning,
            &format!("Close terminal \"{label}\"?"),
            Some("This stops the running process and clears its history."),
            &["Cancel", "Confirm"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            if answer.await == Ok(1) {
                this.update_in(cx, |this, window, cx| {
                    this.close_terminal(&key, &id, cx);
                    this.focus_terminal(window, cx);
                })
                .ok();
            }
        })
        .detach();
    }

    /// `closeThreadTerminal`: ends the shell, and closes the drawer with the last terminal.
    fn close_terminal(&mut self, thread_id: &str, terminal_id: &str, cx: &mut Context<Self>) {
        let Some(terminals) = self.terminals.get_mut(thread_id) else {
            return;
        };
        if !terminals.ids().any(|id| id == terminal_id) {
            return;
        }
        terminals.close(terminal_id);
        if terminals.is_empty() {
            self.terminals.remove(thread_id);
            self.terminal_open.remove(thread_id);
        }
        let close = Action::TerminalClose {
            thread_id: thread_id.to_owned(),
            terminal_id: terminal_id.to_owned(),
        };
        if self.actions.try_send(close).is_err() {
            eprintln!("[ui] could not close the terminal");
        }
        self.sync_terminal(cx);
        cx.notify();
    }

    /// The drawer's height: as dragged, within what the window and chat column leave.
    fn terminal_height(&self, viewport_height: f32) -> f32 {
        let max = terminal::max_height(
            viewport_height,
            viewport_height - TOPBAR_HEIGHT - self.composer_height.get(),
        );
        let dragged = self
            .details_key()
            .and_then(|key| self.terminal_heights.get(&key).copied())
            .unwrap_or(terminal::DEFAULT_HEIGHT);
        terminal::clamp_height(dragged, max)
    }

    fn installed_editors(&self) -> Vec<&'static str> {
        self.providers.as_ref().map_or_else(Vec::new, |catalog| {
            details::installed_editors(catalog.available_editors())
        })
    }

    /// The web's `editor.openFavorite`, live while the card's picker is.
    fn open_favorite_editor(&mut self, cx: &mut Context<Self>) {
        if self.details_open() {
            self.card.update(cx, |card, cx| card.open_favorite(cx));
        }
    }

    fn shell(&self) -> Option<&ShellView> {
        match &self.status {
            Status::Connected { shell, .. } => shell.as_ref(),
            _ => None,
        }
    }

    fn can_pin(&self) -> bool {
        matches!(&self.status, Status::Connected { can_pin: true, .. })
    }

    fn picker_ids(&self) -> Option<Vec<String>> {
        let shell = self.shell()?;
        let active = active_project(self.selected_project.as_deref(), shell);
        let entries = &shell.project_entries;
        let listed = projects::picker_entries(entries, selected_id(self, active), "");
        Some(listed.into_iter().map(|entry| entry.key.clone()).collect())
    }

    /// One draggable row: the sidebar's `SidebarChrome` on the left, the thread breadcrumb right.
    fn top_bar(
        &self,
        narrow: bool,
        sidebar_width: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let border = cx.theme().border;
        let active = self
            .shell()
            .and_then(|shell| active_project(self.selected_project.as_deref(), shell));
        let open = self.sidebar_open && !narrow;
        let title = self
            .shell()
            .zip(self.selected_thread.as_deref())
            .and_then(|(shell, id)| shell.rows.iter().find(|row| row.id == id))
            .map_or("New thread", |row| row.title.as_str());
        TitleBar::new()
            .h(px(TOPBAR_HEIGHT))
            .pl_0()
            .border_b_0()
            .bg(gpui::transparent_black())
            .child(self.sidebar_chrome(open, sidebar_width, cx))
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .px_4()
                    .border_b_1()
                    .border_color(border)
                    .when(self.shell().is_some(), |row| {
                        row.child(self.breadcrumb(active, title, cx))
                            .child(div().flex_1())
                            .when(self.details_cwd().is_some(), |row| {
                                row.child(self.details_toggle(cx))
                                    .child(self.terminal_toggle(cx))
                            })
                            .when(self.right_panel_available(), |row| {
                                row.child(self.right_panel_toggle(cx))
                            })
                    }),
            )
    }

    /// The web's `PanelLayoutControls` terminal drawer toggle, pressed while the drawer is open.
    fn terminal_toggle(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let pressed = self.terminal_visible();
        let foreground = cx.theme().foreground;
        header_icon_button(
            "toggle-terminal",
            Icons::PanelBottom,
            format!("Toggle terminal drawer ({})", terminal::TOGGLE_SHORTCUT).into(),
            true,
            cx,
        )
        .when(pressed, move |button| {
            button.bg(foreground.opacity(0.06)).text_color(foreground)
        })
        .on_click(cx.listener(|this, _, window, cx| this.toggle_terminal(window, cx)))
    }

    /// The web's `PanelLayoutControls` thread details toggle, pressed while the card is open.
    fn details_toggle(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let pressed = self.details_open();
        let foreground = cx.theme().foreground;
        header_icon_button(
            "toggle-details",
            Icons::SquareMenu,
            format!("Toggle thread details ({})", details::TOGGLE_SHORTCUT).into(),
            true,
            cx,
        )
        .when(pressed, move |button| {
            button.bg(foreground.opacity(0.06)).text_color(foreground)
        })
        .on_click(cx.listener(|this, _, _, cx| this.toggle_details(cx)))
    }

    /// The web's `ThreadDetailsCard`, fed the open thread's directory, status and limits.
    fn details_card(&self, available: f32, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.details_open() {
            return None;
        }
        let cwd = self.details_cwd()?;
        let max_height = details::card_height(self.main_width.get(), available)?;
        let shell = self.shell();
        let row = self
            .selected_thread
            .as_deref()
            .and_then(|id| shell?.rows.iter().find(|row| row.id == id));
        let project = shell.and_then(|shell| match row {
            Some(row) => shell
                .project_entries
                .iter()
                .find(|entry| entry.contains(&row.project_id)),
            None => active_project(self.selected_project.as_deref(), shell),
        });
        let props = Props {
            project_cwd: project.map_or_else(|| cwd.clone(), |entry| entry.root.clone()),
            status: self.vcs.get(&cwd).and_then(Option::as_ref).cloned(),
            editors: self.installed_editors(),
            limits: row
                .zip(self.providers.as_ref())
                .and_then(|(row, catalog)| catalog.usage_limits(&row.provider_instance)),
            thread_id: row.map(|row| row.id.clone()),
            project_id: row
                .map(|row| row.project_id.clone())
                .or_else(|| project.map(|entry| entry.id.clone())),
            has_run: row.is_some_and(|row| row.has_run),
            max_height,
            cwd,
        };
        self.card.update(cx, |card, _| card.sync(props));
        Some(
            div()
                .absolute()
                .top(px(details::CARD_GAP))
                .right(px(details::CARD_GAP))
                .child(self.card.clone())
                .into_any_element(),
        )
    }

    /// The web's `SidebarChrome`: toggle, then the brand and history while the sidebar is shown.
    fn sidebar_chrome(&self, open: bool, width: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let backdrop = open && STAGE_BACKDROP;
        let (brand, brand_muted) = if backdrop {
            (gpui::white(), gpui::white().opacity(0.7))
        } else {
            (cx.theme().foreground, cx.theme().muted_foreground)
        };
        let toggle = Kbd::format(&Keystroke::parse("cmd-b").expect("valid keystroke"));
        let shortcut = |keys| Kbd::format(&Keystroke::parse(keys).expect("valid keystroke"));
        let border = cx.theme().border;
        let brand_width = self.brand_width.clone();
        let content =
            h_flex()
                .relative()
                .h_full()
                .flex_shrink_0()
                .gap_1()
                .when(open, |row| {
                    row.child(
                        canvas(
                            move |rect, _, _| brand_width.set(f32::from(rect.right()) + 12.),
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .inset_0(),
                    )
                })
                .child(
                    nav_button(
                        "toggle-sidebar",
                        if open {
                            IconName::PanelLeftClose
                        } else {
                            IconName::PanelLeft
                        },
                        format!("Toggle main sidebar ({toggle})").into(),
                        true,
                        backdrop,
                        cx,
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.toggle_sidebar(narrow_window(window), cx)
                    })),
                )
                // Like the web, the brand and history live in the sidebar and hide with it.
                .when(open, |bar| {
                    bar.child(
                        h_flex()
                            .id("sidebar-brand")
                            .ml_1()
                            .h(px(28.))
                            .px_1()
                            .gap_1()
                            .rounded_md()
                            .text_sm()
                            .text_color(brand)
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, _, cx| this.go_home(cx)))
                            .child(div().font_weight(FontWeight::BOLD).child("T3"))
                            .child(
                                div()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(brand_muted)
                                    .child("Code"),
                            ),
                    )
                    .child(
                        nav_button(
                            "go-back",
                            IconName::ArrowLeft,
                            format!("Go back ({})", shortcut("cmd-[")).into(),
                            self.history.can_back(),
                            backdrop,
                            cx,
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.go_back(cx))),
                    )
                    .child(
                        nav_button(
                            "go-forward",
                            IconName::ArrowRight,
                            format!("Go forward ({})", shortcut("cmd-]")).into(),
                            self.history.can_forward(),
                            backdrop,
                            cx,
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.go_forward(cx))),
                    )
                });
        h_flex()
            .h_full()
            .flex_shrink_0()
            .when(open, |row| row.w(px(width)))
            .when(!open, |row| row.border_b_1().border_color(border))
            .pl(px(TITLEBAR_CONTENT_LEFT))
            .pr_3()
            .child(content)
    }

    /// Picking a project starts a new thread in it.
    fn select_project(&mut self, id: String, cx: &mut Context<Self>) {
        let route = Route {
            project: Some(id),
            thread: None,
        };
        self.history.visit(route.clone());
        self.picker_open = false;
        self.apply_route(route, cx);
    }

    fn go_back(&mut self, cx: &mut Context<Self>) {
        if let Some(route) = self.history.back().cloned() {
            self.apply_route(route, cx);
        }
    }

    fn go_forward(&mut self, cx: &mut Context<Self>) {
        if let Some(route) = self.history.forward().cloned() {
            self.apply_route(route, cx);
        }
    }

    fn toggle_sidebar(&mut self, narrow: bool, cx: &mut Context<Self>) {
        (self.sidebar_open, self.sidebar_overlay_open) =
            toggled(narrow, self.sidebar_open, self.sidebar_overlay_open);
        cx.notify();
    }

    fn close_sidebar_overlay(&mut self, cx: &mut Context<Self>) {
        self.sidebar_overlay_open = false;
        cx.notify();
    }

    /// The web's mobile `Sheet`: a dimmed backdrop and a left drawer sliding in over the content.
    fn sidebar_drawer(&self, width: f32, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let drawer = div()
            .absolute()
            .top_0()
            .bottom_0()
            .left_0()
            .w(px(width))
            .occlude()
            .shadow_lg()
            .child(self.sidebar_surface(width, cx))
            .child(
                v_flex()
                    .size_full()
                    .child(
                        TitleBar::new()
                            .h(px(TOPBAR_HEIGHT))
                            .pl_0()
                            .border_b_0()
                            .bg(gpui::transparent_black())
                            .child(self.sidebar_chrome(true, width, cx)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .child(self.sidebar(width, window, cx)),
                    ),
            )
            .with_animation(
                "sidebar-drawer",
                Animation::new(Duration::from_millis(SHEET_MS)).with_easing(ease_in_out),
                |drawer, delta| drawer.left(px(-SHEET_SLIDE * (1. - delta))).opacity(delta),
            );
        div()
            .absolute()
            .inset_0()
            .child(
                div()
                    .id("sidebar-backdrop")
                    .absolute()
                    .inset_0()
                    .cursor_default()
                    .bg(cx.theme().background.opacity(0.6))
                    .on_click(cx.listener(|this, _, _, cx| this.close_sidebar_overlay(cx)))
                    .with_animation(
                        "sidebar-backdrop",
                        Animation::new(Duration::from_millis(SHEET_MS)).with_easing(ease_in_out),
                        |backdrop, delta| backdrop.opacity(delta),
                    ),
            )
            .child(drawer)
            .into_any_element()
    }

    /// The sidebar's background, border and stage art, painted under the top bar and sidebar
    /// so the art runs behind both, as the web's `SidebarStageBackdrop` does.
    fn sidebar_surface(&self, width: f32, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .absolute()
            .top_0()
            .left_0()
            .bottom_0()
            .w(px(width))
            .bg(cx.theme().sidebar)
            .border_r_1()
            .border_color(cx.theme().sidebar_border)
            .when(STAGE_BACKDROP, |surface| {
                surface.child(
                    img(STAGE_DEV)
                        .absolute()
                        .top_0()
                        .left_0()
                        .w(px(width))
                        .h(px(STAGE_BACKDROP_HEIGHT))
                        .object_fit(ObjectFit::Cover),
                )
            })
    }

    /// The web's `SidebarRail`: drag to resize, double-click to reset.
    fn sidebar_rail(&self, width: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let line = cx.theme().sidebar_border;
        h_flex()
            .id("sidebar-rail")
            .group("sidebar-rail")
            .absolute()
            .top(px(TOPBAR_HEIGHT))
            .bottom_0()
            .left(px(width - SIDEBAR_RAIL_WIDTH / 2.))
            .w(px(SIDEBAR_RAIL_WIDTH))
            .justify_center()
            .occlude()
            .cursor_col_resize()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    if event.click_count == 2 {
                        this.sidebar_width = SIDEBAR_DEFAULT_WIDTH;
                        cx.notify();
                    }
                }),
            )
            .on_drag(SidebarResize, |drag, _, _, cx| {
                cx.stop_propagation();
                cx.new(|_| drag.clone())
            })
            .on_drag_move(
                cx.listener(|this, event: &DragMoveEvent<SidebarResize>, window, cx| {
                    let viewport = f32::from(window.viewport_size().width);
                    this.sidebar_width = clamp_sidebar_width(
                        f32::from(event.event.position.x),
                        this.brand_width.get(),
                        viewport,
                    );
                    cx.notify();
                }),
            )
            .child(
                div()
                    .h_full()
                    .w(px(2.))
                    .group_hover("sidebar-rail", move |style| style.bg(line)),
            )
    }

    fn sidebar(&self, width: f32, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.rendered_sidebar_width.set(width);
        let scope = self.scope();
        let scoped = self
            .shell()
            .map(|shell| projects::scoped_rows(&shell.rows, &shell.project_entries, &scope));
        let rows: &[ThreadRow] = scoped.as_deref().unwrap_or(&[]);
        let shelves = shelves(rows, self.shelves);
        let top = top_rows(rows);
        let mut list = v_flex()
            .id("threads")
            .flex_1()
            .min_h_0()
            .gap_px()
            .overflow_y_scroll();
        for index in top {
            list = list.child(self.thread_item(&rows[index], cx).flex_shrink_0());
        }
        for (position, shelf) in shelves.into_iter().enumerate() {
            list = list.child(self.shelf(shelf, position == 0, rows, cx));
        }
        v_flex()
            .w(px(width))
            .flex_shrink_0()
            .h_full()
            .gap_2()
            .p_2()
            .child(self.sidebar_header(window, cx))
            .child(list)
            .child(self.sidebar_footer(cx))
    }

    /// A sidebar row; picking it opens the thread, and closes the overlay like the web.
    fn thread_item(&self, row: &ThreadRow, cx: &mut Context<Self>) -> gpui::Stateful<gpui::Div> {
        let selected = self.selected_thread.as_deref() == Some(row.id.as_str());
        let id = row.id.clone();
        let project = self.shell().and_then(|shell| {
            shell
                .project_entries
                .iter()
                .find(|e| e.contains(&row.project_id))
        });
        let provider = self
            .providers
            .as_ref()
            .and_then(|catalog| catalog.instance(&row.provider_instance));
        let anchor = Rc::new(Cell::new(Bounds::default()));
        let card = ThreadCard::new(
            row,
            project,
            provider.as_ref(),
            self.providers.as_ref(),
            match &self.status {
                Status::Connected { label, machine, .. } => {
                    Some((strip::environment_label(label), *machine))
                }
                _ => None,
            },
        );
        let theme = cx.theme();
        let (muted, dark) = (theme.muted_foreground, theme.is_dark());
        let pr_control = row.pr.as_ref().map(|badge| {
            let weak = cx.weak_entity();
            let list_id = row.id.clone();
            let (open_weak, thread, project_id) =
                (weak.clone(), row.id.clone(), row.project_id.clone());
            let open: OpenPullRequest = Rc::new(move |line, cx| {
                let reference = crate::pull_request::Reference {
                    project_id: project_id.clone(),
                    host: Some(line.host.clone()).filter(|host| !host.is_empty()),
                    repository: line.repository.clone(),
                    number: line.number,
                };
                let (thread, url) = (thread.clone(), Some(line.url.clone()));
                open_weak
                    .update(cx, |this, cx| {
                        this.open_pull_request(thread, reference, url, cx)
                    })
                    .ok();
            });
            pr_badge_control(
                row,
                badge,
                muted,
                dark,
                move |cx| {
                    let list_id = list_id.clone();
                    weak.update(cx, |this, cx| this.open_thread(Some(list_id), cx))
                        .ok();
                },
                open,
            )
        });
        thread_row(
            row,
            project,
            provider,
            selected,
            self.rendered_sidebar_width.get(),
            RowParts {
                anchor: anchor.clone(),
                pr_control,
            },
            cx,
        )
        .tooltip(move |window, cx| {
            let card = card.clone();
            let place = card_offset(anchor.get(), window.mouse_position());
            cx.new(|_| ThreadHoverCard { card, place }).into()
        })
        .on_click(cx.listener(move |this, _, _, cx| this.open_thread(Some(id.clone()), cx)))
    }

    /// One collapsible shelf: its header, then its rows while expanded. The first shelf takes
    /// the free space above it (`mt-auto`), so shelves sit at the bottom until content overflows.
    fn shelf(
        &self,
        shelf: Shelf,
        first: bool,
        all: &[ThreadRow],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Shelf {
            section,
            count,
            expanded,
        } = shelf;
        let index = shelf_index(section);
        let (muted, hover) = (cx.theme().muted_foreground, cx.theme().list_hover);
        let mut items: Vec<AnyElement> = vec![
            shelf_header(section, count, expanded, muted, hover)
                .flex_shrink_0()
                .when(first, |header| header.mt_auto())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.shelves[index] = !this.shelves[index];
                    cx.notify();
                }))
                .into_any_element(),
        ];
        let mut more = 0;
        {
            let selected = self.selected_thread.as_deref();
            let mut rows = shelf_rows(all, section, expanded, selected);
            if section == Section::Settled && expanded {
                let window = settled_window(&rows, all, self.settled_shown, selected);
                rows = window.rows;
                more = window.more;
            }
            items.extend(rows.into_iter().map(|row| {
                self.thread_item(&all[row], cx)
                    .flex_shrink_0()
                    .into_any_element()
            }));
        }
        if more > 0 {
            items.push(
                h_flex()
                    .id("settled-show-more")
                    .h(px(36.))
                    .flex_shrink_0()
                    .mx(px(2.))
                    .px_2()
                    .gap_2()
                    .rounded_md()
                    .text_xs()
                    .text_color(muted)
                    .cursor_pointer()
                    .hover(|style| style.bg(hover))
                    .child(Icon::new(IconName::ChevronDown).size(px(12.)))
                    .child(format!("Show {more} more"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.settled_shown += SETTLED_TAIL_PAGE_COUNT;
                        cx.notify();
                    }))
                    .into_any_element(),
            );
        }
        v_flex()
            .gap_px()
            .flex_shrink_0()
            .when(first, |shelf| shelf.mt_auto())
            .children(items)
            .into_any_element()
    }

    /// The web's `SidebarChromeFooter`: utility buttons left, the update control right.
    fn sidebar_footer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .pt_1()
            .gap_px()
            .child(
                footer_button("settings", Icon::new(IconName::Settings), "Settings", cx).on_click(
                    cx.listener(|this, _, window, cx| {
                        this.settings
                            .update(cx, |settings, cx| settings.open(window, cx));
                    }),
                ),
            )
            .child(
                footer_button(
                    "pull-requests",
                    Icon::new(Icons::GitPullRequest),
                    "Pull requests",
                    cx,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.pull_requests_open = true;
                    this.sidebar_overlay_open = false;
                    this.pull_requests_page.update(cx, |page, cx| page.open(cx));
                    cx.notify();
                })),
            )
            .child(
                footer_button("usage", Icon::new(Icons::ChartNoAxesColumn), "Usage", cx).on_click(
                    cx.listener(|this, _, window, cx| {
                        this.usage.update(cx, |usage, cx| usage.open(window, cx));
                    }),
                ),
            )
            .child(div().flex_1())
            .child(update_button(cx))
    }

    /// The web's `SidebarThreadHeader`: search, then the project filter, add project and new thread.
    fn sidebar_header(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        use gpui_kit_assets::IconName as Icons;
        let has_projects = self
            .shell()
            .is_some_and(|shell| !shell.project_entries.is_empty());
        let new_thread = Kbd::format(&Keystroke::parse("cmd-n").expect("valid keystroke"));
        let focused = self.search.focus_handle(cx).is_focused(window);
        let (hover, ring) = (cx.theme().list_hover, cx.theme().ring.opacity(0.7));
        h_flex()
            .gap_1()
            .child(
                // The web's unstyled search: only hover and focus paint the row.
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .h_8()
                    .px_2()
                    .gap_2()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(gpui::transparent_black())
                    .text_color(cx.theme().muted_foreground)
                    .hover(move |style| style.bg(hover))
                    .when(focused, |row| row.bg(hover).border_color(ring))
                    .child(Icon::new(IconName::Search).size_4())
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font_weight(FontWeight::MEDIUM)
                            .child(Input::new(&self.search).appearance(false).small()),
                    ),
            )
            .when(has_projects, |row| {
                row.child(self.scope_menu(cx)).child(
                    header_icon_button(
                        "add-project",
                        Icons::FolderPlus,
                        "Add project".into(),
                        true,
                        cx,
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.add_project(cx))),
                )
            })
            .child(
                header_icon_button(
                    "new-thread",
                    Icons::SquarePen,
                    format!("New thread ({new_thread})").into(),
                    has_projects,
                    cx,
                )
                .when(has_projects, |button| {
                    button.on_click(cx.listener(|this, _, _, cx| this.new_thread(cx)))
                }),
            )
    }

    /// The web's project scope combobox: "All projects" and one row per project; a shift click
    /// adds a project to the scope instead of replacing it.
    fn scope_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let search = self.scope_search.clone();
        let entries = self
            .shell()
            .map_or_else(Vec::new, |shell| shell.project_entries.clone());
        let scope = self.scope();
        let dark = cx.theme().is_dark();
        let scoped: Vec<(String, u32)> = entries
            .iter()
            .filter(|entry| scope.contains(&entry.key))
            .map(|entry| (entry.initials.clone(), entry.color_hex(dark)))
            .collect();
        let label = {
            let names: Vec<&str> = entries
                .iter()
                .filter(|entry| scope.contains(&entry.key))
                .map(|entry| entry.title.as_str())
                .collect();
            if names.is_empty() {
                "Filter threads by project".to_owned()
            } else {
                format!("Filter threads by project: {}", names.join(", "))
            }
        };
        Popover::new("project-scope")
            .anchor(Anchor::TopLeft)
            .open(self.scope_open)
            .track_focus(&search.focus_handle(cx))
            .p_0()
            .on_open_change({
                let (view, search) = (view.clone(), search.clone());
                move |open, window, cx| {
                    search.update(cx, |state, cx| state.set_value("", window, cx));
                    view.update(cx, |this, cx| {
                        this.scope_open = *open;
                        cx.notify();
                    });
                }
            })
            .trigger(ScopeTrigger {
                label: label.into(),
                scoped,
                open: false,
            })
            .content(move |_, _, cx| {
                let query = search.read(cx).value();
                let listed = projects::picker_entries(&entries, None, &query);
                let theme = cx.theme();
                let (dark, muted) = (theme.is_dark(), theme.muted_foreground);
                let (hover, selected, border) = (theme.list_hover, theme.list_active, theme.border);
                let mono = theme.mono_font_family.clone();
                let row = |id: SharedString, active: bool, key: Option<String>| {
                    let view = view.clone();
                    h_flex()
                        .id(id)
                        .gap_2()
                        .px_2()
                        .py_1p5()
                        .items_center()
                        .rounded_md()
                        .cursor_pointer()
                        .when(active, |row| row.bg(selected))
                        .hover(move |style| style.bg(hover))
                        .on_click(move |event, _, cx| {
                            let additive = event.modifiers().shift;
                            view.update(cx, |this, cx| {
                                this.pick_scope(key.as_deref(), additive, cx)
                            });
                        })
                };
                let all = "all projects".contains(query.trim().to_lowercase().as_str());
                let rows = listed.iter().enumerate().map(|(index, entry)| {
                    let color: Hsla = rgb(entry.color_hex(dark)).into();
                    let active = scope.contains(&entry.key);
                    row(
                        format!("scope-{index}").into(),
                        active,
                        Some(entry.key.clone()),
                    )
                    .child(monogram(&entry.initials, color, 20., &mono))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .truncate()
                            .child(entry.title.clone()),
                    )
                    .when(active, |row| {
                        row.child(Icon::new(IconName::Check).size_4().text_color(muted))
                    })
                });
                v_flex()
                    .w(px(288.))
                    .rounded_lg()
                    .overflow_hidden()
                    .child(
                        div().px_3().pt(px(10.)).child(
                            h_flex()
                                .pb(px(6.))
                                .border_b_1()
                                .border_color(border.opacity(0.7))
                                .child(
                                    Input::new(&search)
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
                            .id("scope-list")
                            .max_h(px(PICKER_MAX_HEIGHT - PICKER_SEARCH_HEIGHT))
                            .overflow_y_scroll()
                            .p_1()
                            .when(all, |list| {
                                list.child(
                                    row("scope-all".into(), scope.is_empty(), None)
                                        .child(
                                            div()
                                                .flex()
                                                .flex_shrink_0()
                                                .justify_center()
                                                .size(px(20.))
                                                .child(
                                                    Icon::new(Icons::Folder)
                                                        .size_4()
                                                        .text_color(muted),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .text_sm()
                                                .truncate()
                                                .child("All projects"),
                                        )
                                        .when(scope.is_empty(), |row| {
                                            row.child(
                                                Icon::new(IconName::Check)
                                                    .size_4()
                                                    .text_color(muted),
                                            )
                                        }),
                                )
                            })
                            .children(rows)
                            .when(!all && listed.is_empty(), |list| {
                                list.child(
                                    div()
                                        .p_2()
                                        .text_sm()
                                        .text_color(muted)
                                        .child("No matching projects."),
                                )
                            }),
                    )
            })
    }

    /// The main content, with the thread details card floating at its top right when it fits.
    fn main_area(&self, viewport_height: f32, cx: &mut Context<Self>) -> AnyElement {
        if self.pull_requests_open {
            return self.pull_requests_page.clone().into_any_element();
        }
        let drawer = self.terminal_drawer(viewport_height, cx);
        let drawer_height = if self.terminal_visible() {
            self.terminal_height(viewport_height)
        } else {
            0.
        };
        let content = self.main_content(viewport_height, cx);
        let card = self.details_card(viewport_height - TOPBAR_HEIGHT - drawer_height, cx);
        v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .when(card.is_some(), |area| {
                        let (left, right) = details::lane_insets(self.main_width.get(), true);
                        area.pl(px(left)).pr(px(right))
                    })
                    .child(content)
                    .children(card),
            )
            .children(drawer)
            .into_any_element()
    }

    /// The web's `ThreadTerminalDrawer`: the thread's shell under the chat, its top edge draggable.
    fn terminal_drawer(&self, viewport_height: f32, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.terminal_visible() {
            return None;
        }
        let line = cx.theme().border;
        let height = self.terminal_height(viewport_height);
        Some(
            div()
                .relative()
                .flex_none()
                .w_full()
                .h(px(height))
                .border_t_1()
                .border_color(line)
                .child(self.terminal_body(cx))
                .child(
                    div()
                        .id("terminal-resize")
                        .absolute()
                        .top(px(-TERMINAL_HANDLE_HEIGHT / 2.))
                        .left_0()
                        .right_0()
                        .h(px(TERMINAL_HANDLE_HEIGHT))
                        .occlude()
                        .cursor_row_resize()
                        .on_drag(TerminalResizeDrag, |drag, _, _, cx| {
                            cx.stop_propagation();
                            cx.new(|_| drag.clone())
                        })
                        .on_drag_move(cx.listener(
                            |this, event: &DragMoveEvent<TerminalResizeDrag>, window, cx| {
                                let Some(key) = this.details_key() else {
                                    return;
                                };
                                let viewport = f32::from(window.viewport_size().height);
                                let dragged = viewport - f32::from(event.event.position.y);
                                this.terminal_heights.insert(key, dragged);
                                cx.notify();
                            },
                        )),
                )
                .into_any_element(),
        )
    }

    /// The drawer's panes, with the toolbar floating over them, or the sidebar beside them once
    /// there is more than one terminal.
    fn terminal_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some((key, terminals)) = self.active_terminal() else {
            return div().into_any_element();
        };
        let theme = cx.theme();
        let (background, border) = (theme.background, theme.border);
        let sidebar = terminals.count() > 1;
        let panes = self.terminal_panes(&key, terminals, cx);
        let buttons = self.terminal_buttons(terminals, sidebar, cx);
        let (pill, side) = if sidebar {
            (None, Some(buttons))
        } else {
            (Some(buttons), None)
        };
        div()
            .relative()
            .size_full()
            .flex()
            .bg(background)
            .when(sidebar, |body| body.gap(px(6.)))
            .child(div().flex_1().min_w_0().h_full().child(panes))
            .when_some(pill, |body, [split, split_vertical, new, close]| {
                let line = border.opacity(0.8);
                let separator = move || div().h(px(16.)).w(px(1.)).bg(line);
                body.child(
                    h_flex()
                        .absolute()
                        .top(px(8.))
                        .right(px(8.))
                        .items_center()
                        .overflow_hidden()
                        .rounded(px(6.))
                        .border_1()
                        .border_color(line)
                        .bg(background)
                        .shadow_xs()
                        .child(split)
                        .child(separator())
                        .child(split_vertical)
                        .child(separator())
                        .child(new)
                        .child(separator())
                        .child(close),
                )
            })
            .when_some(side, |body, [split, split_vertical, new, close]| {
                let line = border.opacity(0.7);
                let divided =
                    move |button: gpui::Stateful<gpui::Div>| button.border_l_1().border_color(line);
                body.child(
                    v_flex()
                        .w(px(TERMINAL_SIDEBAR_WIDTH))
                        .min_w(px(TERMINAL_SIDEBAR_WIDTH))
                        .h_full()
                        .border_1()
                        .border_color(line)
                        .bg(cx.theme().muted.opacity(0.1))
                        .child(
                            h_flex()
                                .h(px(22.))
                                .items_stretch()
                                .justify_end()
                                .border_b_1()
                                .border_color(line)
                                .child(split)
                                .child(divided(split_vertical))
                                .child(divided(new))
                                .child(divided(close)),
                        )
                        .child(self.terminal_list(terminals, cx)),
                )
            })
            .into_any_element()
    }

    /// The web's split, split vertically, new and close buttons, with its labels and shortcuts.
    fn terminal_buttons(
        &self,
        terminals: &Terminals,
        sidebar: bool,
        cx: &mut Context<Self>,
    ) -> [gpui::Stateful<gpui::Div>; 4] {
        let limit = terminals.split_limit_reached();
        let active = terminals.active.clone();
        let accent = cx
            .theme()
            .foreground
            .opacity(if sidebar { 0.042 } else { 0.06 });
        let split_label = |direction: &str, shortcut: &str| {
            if limit {
                format!(
                    "Split Terminal {direction} (max {} per group)",
                    terminal_state::MAX_PER_GROUP
                )
            } else {
                format!("Split Terminal {direction} ({shortcut})")
            }
        };
        let button = |id: &'static str, icon: Icons, label: String, disabled: bool, cx: &App| {
            let foreground = cx.theme().foreground.opacity(0.9);
            div()
                .id(id)
                .flex()
                .items_center()
                .justify_center()
                .when(sidebar, |button| button.h_full().px(px(4.)))
                .when(!sidebar, |button| button.p(px(4.)))
                .text_color(foreground)
                .child(Icon::new(icon).size(px(13.)))
                .when(disabled, |button| button.opacity(0.64).cursor_not_allowed())
                .when(!disabled, |button| {
                    button.cursor_pointer().hover(move |style| style.bg(accent))
                })
                .tooltip(tooltip(label))
        };
        [
            button(
                "terminal-split",
                Icons::SquareSplitHorizontal,
                split_label("Horizontally", "\u{2318}D"),
                limit,
                cx,
            )
            .on_click(cx.listener(|this, _, window, cx| {
                this.add_terminal(Placement::Split { vertical: false }, window, cx)
            })),
            button(
                "terminal-split-vertical",
                Icons::SquareSplitVertical,
                split_label("Vertically", "\u{21e7}\u{2318}D"),
                limit,
                cx,
            )
            .on_click(cx.listener(|this, _, window, cx| {
                this.add_terminal(Placement::Split { vertical: true }, window, cx)
            })),
            button(
                "terminal-new",
                Icons::Plus,
                "New Terminal (\u{2318}N)".into(),
                false,
                cx,
            )
            .on_click(
                cx.listener(|this, _, window, cx| this.add_terminal(Placement::New, window, cx)),
            ),
            button(
                "terminal-close",
                Icons::Trash,
                "Close Terminal (\u{2318}W)".into(),
                false,
                cx,
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.request_close(active.clone(), window, cx)
            })),
        ]
    }

    /// `TerminalSplitPanes`: the visible group's terminals side by side, or stacked.
    fn terminal_panes(
        &self,
        key: &str,
        terminals: &Terminals,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(group) = terminals.visible() else {
            return div().into_any_element();
        };
        let border = cx.theme().border;
        let vertical = group.vertical;
        let panes = group
            .ids
            .iter()
            .enumerate()
            .filter_map(|(index, id)| {
                let view = self.terminal_views.get(&(key.to_owned(), id.clone()))?;
                let line = if *id == terminals.active {
                    border
                } else {
                    border.opacity(0.7)
                };
                let activate = id.clone();
                Some(
                    div()
                        .flex_1()
                        .min_w_0()
                        .min_h_0()
                        .when(index > 0 && vertical, |pane| pane.border_t_1())
                        .when(index > 0 && !vertical, |pane| pane.border_l_1())
                        .border_color(line)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, window, cx| {
                                this.activate_terminal(&activate, window, cx)
                            }),
                        )
                        .child(view.clone()),
                )
            })
            .collect::<Vec<_>>();
        let container = if vertical { v_flex() } else { h_flex() };
        container
            .size_full()
            .overflow_hidden()
            .children(panes)
            .into_any_element()
    }

    /// `TerminalSidebarGroup` for each group.
    fn terminal_list(&self, terminals: &Terminals, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (foreground, muted, muted_background) =
            (theme.foreground, theme.muted_foreground, theme.muted);
        let accent = |scale: f32| foreground.opacity(0.06 * scale);
        let headers =
            terminals.groups.len() > 1 || terminals.groups.iter().any(|group| group.ids.len() > 1);
        let groups = terminals.groups.iter().map(|group| {
            let active_group = group.ids.contains(&terminals.active);
            let focus = if active_group {
                terminals.active.clone()
            } else {
                group.ids.first().cloned().unwrap_or_default()
            };
            let split = group.ids.len() > 1;
            let (title, icon) = match (split, group.vertical) {
                (false, _) => ("Single", Icons::Square),
                (true, true) => ("Stacked", Icons::SquareSplitVertical),
                (true, false) => ("Side by side", Icons::SquareSplitHorizontal),
            };
            let header = div()
                .id(SharedString::from(format!("terminal-group-{}", group.id)))
                .flex()
                .items_center()
                .gap(px(4.))
                .h(px(22.))
                .w_full()
                .px(px(6.))
                .rounded(px(4.))
                .text_size(px(11.))
                .cursor_pointer()
                .when(active_group, |header| {
                    header.bg(accent(0.5)).text_color(foreground)
                })
                .when(!active_group, |header| {
                    header
                        .text_color(muted)
                        .hover(move |style| style.bg(accent(0.4)).text_color(foreground))
                })
                .child(Icon::new(icon).size(px(12.)).flex_shrink_0())
                .child(div().flex_1().min_w_0().truncate().child(title))
                .child(
                    div()
                        .text_size(px(10.))
                        .text_color(muted.opacity(0.7))
                        .child(group.ids.len().to_string()),
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.activate_terminal(&focus, window, cx)
                }));
            let rows = group.ids.iter().map(|id| {
                let active = *id == terminals.active;
                let label = terminal_state::label(id);
                let close_label = if active {
                    format!("Close {label} (\u{2318}W)")
                } else {
                    format!("Close {label}")
                };
                let hover_group = SharedString::from(format!("terminal-tab-{id}"));
                let (close_id, select_id, close_target, select_target) =
                    (id.clone(), id.clone(), id.clone(), id.clone());
                h_flex()
                    .id(SharedString::from(format!("terminal-row-{id}")))
                    .group(hover_group.clone())
                    .h(px(24.))
                    .w_full()
                    .items_center()
                    .gap(px(2.))
                    .rounded(px(6.))
                    .pr(px(8.))
                    .pl(px(6.))
                    .text_xs()
                    .when(active, |row| row.bg(accent(1.)).text_color(foreground))
                    .when(!active, |row| {
                        row.text_color(muted)
                            .hover(move |style| style.bg(accent(0.6)).text_color(foreground))
                    })
                    .child(
                        div()
                            .id(SharedString::from(format!("terminal-close-{close_id}")))
                            .flex()
                            .flex_none()
                            .items_center()
                            .justify_center()
                            .size(px(16.))
                            .rounded(px(2.))
                            .cursor_pointer()
                            .hover(move |style| style.bg(muted_background))
                            .tooltip(tooltip(close_label))
                            .child(
                                div()
                                    .flex()
                                    .group_hover(hover_group.clone(), |style| style.hidden())
                                    .child(Icon::new(Icons::SquareTerminal).size(px(12.))),
                            )
                            .child(
                                div()
                                    .hidden()
                                    .group_hover(hover_group, |style| style.flex())
                                    .child(Icon::new(Icons::X).size(px(12.))),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.request_close(close_target.clone(), window, cx)
                            })),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("terminal-select-{select_id}")))
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .cursor_pointer()
                            .child(label)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.activate_terminal(&select_target, window, cx)
                            })),
                    )
            });
            v_flex()
                .pb(px(2.))
                .when(headers, |group| group.child(header))
                .child(v_flex().gap(px(2.)).children(rows))
        });
        v_flex()
            .id("terminal-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(px(4.))
            .py(px(4.))
            .children(groups)
    }

    fn main_content(&self, viewport_height: f32, cx: &mut Context<Self>) -> AnyElement {
        let Some(shell) = self.shell() else {
            return centered(cx)
                .text_xl()
                .child(self.status.to_string())
                .into_any_element();
        };
        let active = active_project(self.selected_project.as_deref(), shell);
        let scratch = active.is_some_and(|entry| entry.scratch);
        let project = active.map_or_else(|| "your project".to_owned(), |entry| entry.title.clone());
        if self.selected_thread.is_some() {
            // The web's `persistComposerContextStrip` defaults off, so open threads show no strip,
            // and the composer floats over the transcript, which scrolls beneath it.
            let measured = self.composer_height.clone();
            return v_flex()
                .relative()
                .flex_1()
                .min_w_0()
                .h_full()
                .child(self.transcript_view(viewport_height, cx))
                .child(
                    v_flex()
                        .absolute()
                        .bottom_0()
                        .left_0()
                        .right_0()
                        .items_center()
                        .px_6()
                        .pb_4()
                        .child(
                            canvas(
                                move |rect, window, _| {
                                    let height = f32::from(rect.size.height);
                                    if measured.replace(height) != height {
                                        window.refresh();
                                    }
                                },
                                |_, _, _, _| {},
                            )
                            .absolute()
                            .inset_0(),
                        )
                        .child(self.composer_box(cx)),
                )
                .into_any_element();
        }
        v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(
                centered(cx)
                    .gap_6()
                    .child(self.hero(
                        &project,
                        scratch,
                        selected_id(self, active).map(str::to_owned),
                        projects::with_remote_members(&shell.project_entries, &self.environments),
                        cx,
                    ))
                    .child(
                        v_flex()
                            .w(px(CHAT_CONTENT_WIDTH))
                            .max_w_full()
                            .child(self.composer_box(cx))
                            .children(active.and_then(|entry| self.context_strip(entry, cx))),
                    ),
            )
            .into_any_element()
    }

    /// The open thread's rows, or why there are none yet.
    fn transcript_view(&self, viewport_height: f32, cx: &mut Context<Self>) -> AnyElement {
        let note = |text: String| {
            centered(cx)
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(text)
                .into_any_element()
        };
        match self.transcript.as_ref().map(|t| &t.result) {
            None => note("Loading...".to_owned()),
            Some(Err(message)) => note(format!("Could not load this thread: {message}")),
            Some(Ok(entries)) if entries.is_empty() => note("No messages yet".to_owned()),
            Some(Ok(entries)) => {
                let reserve = self.composer_height.get();
                if self.reserved_composer_height.replace(reserve) != reserve {
                    let last = entries.len() - 1;
                    self.transcript_list.remeasure_items(last..last + 1);
                }
                self.transcript_list_view(viewport_height, cx)
            }
        }
    }

    fn transcript_list_view(&self, viewport_height: f32, cx: &mut Context<Self>) -> AnyElement {
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .w_full()
            .child(
                list(
                    self.transcript_list.clone(),
                    cx.processor(|this, ix, _, cx| this.transcript_item(ix, cx)),
                )
                .size_full(),
            )
            .children(self.transcript_minimap(viewport_height, cx))
            .into_any_element()
    }

    /// The rows on screen, from the list's last layout.
    fn visible_rows(&self) -> Option<(usize, usize)> {
        let viewport = self.transcript_list.viewport_bounds();
        if viewport.size.height <= px(0.) {
            return None;
        }
        let start = self.transcript_list.logical_scroll_top().item_ix;
        let mut end = start;
        for ix in start..self.transcript_list.item_count() {
            match self.transcript_list.bounds_for_item(ix) {
                Some(bounds) if bounds.top() < viewport.bottom() => end = ix,
                _ => break,
            }
        }
        Some((start, end))
    }

    /// The web's `TimelineMinimap`: a tick per user turn at the left edge, brighter while the turn
    /// is on screen, with a card of the turn on hover and a click that scrolls to it.
    fn transcript_minimap(
        &self,
        viewport_height: f32,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let items = &self.minimap_items;
        let count = items.len();
        let viewport = self.transcript_list.viewport_bounds();
        let (width, height) = (
            f32::from(viewport.size.width),
            f32::from(viewport.size.height),
        );
        let hit_width = minimap::hit_strip_width(width);
        if count < minimap::MIN_ITEMS || hit_width <= 0. || height <= 0. {
            return None;
        }
        let theme = cx.theme();
        let (foreground, muted, border, popover) = (
            theme.foreground,
            theme.muted_foreground,
            theme.border,
            theme.popover,
        );
        let rail_height = minimap::rail_height(count, viewport_height);
        let rail_top = (height - rail_height) / 2.;
        let window_top = f32::from(viewport.top()) + rail_top;
        let hover = self
            .minimap_hover
            .as_ref()
            .filter(|(index, _)| *index < count);
        let hover_index = hover.map(|(index, _)| *index);
        let visible = self.visible_rows();
        let persistent = minimap::has_persistent_gutter(width);
        let strips = items.iter().enumerate().map(|(index, item)| {
            let distance = hover_index.map(|active| index.abs_diff(active));
            let top = minimap::top_percent(index, count) / 100. * rail_height;
            div()
                .absolute()
                .left_0()
                .top(px(top - 1.))
                .h(px(2.))
                .w(px(24. * minimap::strip_scale(distance)))
                .rounded_full()
                .bg(muted.opacity(if distance == Some(0) { 0.6 } else { 0.25 }))
                .when(minimap::in_view(item.row, visible), |strip| {
                    strip.child(
                        div()
                            .size_full()
                            .rounded_full()
                            .bg(foreground.opacity(0.55)),
                    )
                })
        });
        let card = hover.map(|(index, preview)| {
            let top = minimap::top_percent(*index, count) / 100. * rail_height;
            // The web shifts the card by a share of its own height, which GPUI cannot do before
            // layout, so the card sits in a zero-height row that is centered, or pinned, on the tick.
            let shift = minimap::card_shift(*index, count);
            div()
                .absolute()
                .left(px(32.))
                .top(px(top))
                .w(px(320.))
                .h_0()
                .flex()
                .flex_col()
                .justify_center()
                .when(shift == 0., |row| row.justify_start())
                .when(shift == -1., |row| row.justify_end())
                .child(
                    div()
                        .id("minimap-card")
                        .flex_shrink_0()
                        .p_3()
                        .rounded(px(12.))
                        .border_1()
                        .border_color(border)
                        .bg(popover)
                        .shadow_xl()
                        .on_mouse_move(|_, _, cx| cx.stop_propagation())
                        .on_click(|_, _, cx| cx.stop_propagation())
                        .child(
                            div()
                                .truncate()
                                .text_sm()
                                .line_height(px(20.))
                                .font_weight(FontWeight::MEDIUM)
                                .child(
                                    preview
                                        .user
                                        .clone()
                                        .unwrap_or_else(|| "User message".to_owned()),
                                ),
                        )
                        .children(preview.assistant.clone().map(|text| {
                            div()
                                .mt_1()
                                .line_clamp(3)
                                .text_sm()
                                .line_height(px(20.))
                                .text_color(muted)
                                .child(text)
                        })),
                )
        });
        Some(
            div()
                .id("transcript-minimap")
                .absolute()
                .left(px(minimap::HIT_STRIP_LEFT_PX))
                .top(px(rail_top))
                .h(px(rail_height))
                .w(px(if hover.is_some() { 352. } else { hit_width }))
                .cursor_pointer()
                .when(!persistent && hover.is_none(), |rail| rail.opacity(0.))
                .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
                    let y = f32::from(event.position.y);
                    let index = minimap::index_from_pointer(
                        this.minimap_items.len(),
                        window_top,
                        rail_height,
                        y,
                    );
                    if index == this.minimap_hover.as_ref().map(|(index, _)| *index) {
                        return;
                    }
                    this.minimap_hover = index.and_then(|index| {
                        let item = this.minimap_items.get(index)?;
                        Some((index, minimap::preview(item)))
                    });
                    cx.notify();
                }))
                .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                    if !hovered && this.minimap_hover.take().is_some() {
                        cx.notify();
                    }
                }))
                .on_click(cx.listener(|this, _, _, cx| {
                    let row = this
                        .minimap_hover
                        .as_ref()
                        .and_then(|(index, _)| this.minimap_items.get(*index).map(|item| item.row));
                    if let Some(item_ix) = row {
                        this.transcript_list.scroll_to(ListOffset {
                            item_ix,
                            offset_in_item: px(0.),
                        });
                        cx.notify();
                    }
                }))
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .left(px(12.))
                        .h_full()
                        .w(px(1.))
                        .bg(border.opacity(0.15)),
                )
                .children(strips)
                .children(card)
                .into_any_element(),
        )
    }

    /// One row in the web's chat content column: the user's words in a bubble on the right, the
    /// assistant's plain, tool work as a muted line.
    fn transcript_item(&self, ix: usize, cx: &App) -> AnyElement {
        let Some(entry) = self
            .transcript
            .as_ref()
            .and_then(|t| t.result.as_ref().ok())
            .and_then(|entries| entries.get(ix))
        else {
            return div().into_any_element();
        };
        let theme = cx.theme();
        let last = self
            .transcript
            .as_ref()
            .and_then(|t| t.result.as_ref().ok())
            .is_some_and(|entries| ix + 1 == entries.len());
        let text: gpui::SharedString = entry.text.to_string().into();
        let body = match entry.kind {
            Kind::User => h_flex().justify_end().child(
                div()
                    .max_w(px(560.))
                    .px_3()
                    .py_2()
                    .rounded(px(16.))
                    .bg(theme.secondary)
                    .text_sm()
                    .child(text),
            ),
            Kind::Assistant => h_flex().child(div().min_w_0().text_sm().child(text)),
            Kind::Work => h_flex().child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_xs()
                    .font_family(theme.mono_font_family.clone())
                    .text_color(theme.muted_foreground)
                    .child(text),
            ),
        };
        h_flex()
            .w_full()
            .justify_center()
            .px_6()
            .py_2()
            .when(last, |row| {
                row.pb(px(8. + self.reserved_composer_height.get()))
            })
            .child(div().w_full().max_w(px(CHAT_CONTENT_WIDTH)).child(body))
            .into_any_element()
    }

    fn breadcrumb(
        &self,
        active: Option<&ProjectEntry>,
        title: &str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let (foreground, muted, dark) = (theme.foreground, theme.muted_foreground, theme.is_dark());
        let mono = theme.mono_font_family.clone();
        let project = active.map(|entry| {
            let color: Hsla = rgb(entry.color_hex(dark)).into();
            let icon = if entry.scratch {
                Icon::empty()
                    .path(MESSAGE_SQUARE_DASHED)
                    .size(px(14.))
                    .text_color(color)
                    .into_any_element()
            } else {
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .justify_center()
                    .size(px(14.))
                    .rounded(px(3.5))
                    .bg(color.opacity(0.14))
                    .text_color(color)
                    .text_size(px(7.21875))
                    .font_weight(FontWeight::BOLD)
                    .font_family(mono)
                    .child(entry.initials.clone())
                    .into_any_element()
            };
            h_flex()
                .gap_3()
                .child(
                    h_flex()
                        .id("breadcrumb-project")
                        .gap(px(6.))
                        .min_w_0()
                        .text_color(muted)
                        .cursor_pointer()
                        .hover(move |style| style.text_color(foreground))
                        .tooltip(tooltip(format!(
                            "New thread in {}",
                            project_label(Some(entry))
                        )))
                        .on_click(cx.listener({
                            let id = entry.id.clone();
                            move |this, _, _, cx| this.select_project(id.clone(), cx)
                        }))
                        .child(icon)
                        .child(
                            div()
                                .max_w(px(160.))
                                .truncate()
                                .child(project_label(Some(entry))),
                        ),
                )
                .child(div().flex_shrink_0().text_color(muted).child("/"))
        });
        h_flex()
            .gap_3()
            .items_center()
            .text_sm()
            .font_weight(FontWeight::MEDIUM)
            .children(project)
            .child(
                div()
                    .text_color(foreground)
                    .truncate()
                    .child(title.to_owned()),
            )
    }

    fn hero(
        &self,
        project: &str,
        scratch: bool,
        active_id: Option<String>,
        entries: Vec<ProjectEntry>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let view = cx.entity();
        let link_view = view.clone();
        let foreground = cx.theme().foreground;
        let (actions, can_pin) = (self.actions.clone(), self.can_pin());
        let scratch_id = self
            .shell()
            .and_then(|shell| scratch_entry(&shell.project_entries))
            .map(|entry| entry.id.clone());
        let order = self.picker_order.clone();
        let search = self.project_search.clone();
        let trigger_bounds = self.picker_trigger.clone();
        let picker = Popover::new("project-picker")
            .anchor(Anchor::TopCenter)
            .open(self.picker_open)
            .track_focus(&search.focus_handle(cx))
            .p_0()
            .on_open_change({
                let (view, search) = (view.clone(), search.clone());
                move |open, window, cx| {
                    search.update(cx, |state, cx| state.set_value("", window, cx));
                    view.update(cx, |this, cx| {
                        this.picker_open = *open;
                        this.picker_order = open.then(|| this.picker_ids()).flatten();
                        cx.notify();
                    });
                }
            })
            .trigger(ProjectTrigger::new(
                project,
                scratch,
                trigger_bounds.clone(),
            ))
            .content(move |_, window, cx| {
                let query = search.read(cx).value();
                let searching = search.focus_handle(cx).is_focused(window);
                // Shrinking keeps the positioner from flipping the popup over the headline.
                let below = window.viewport_size().height - trigger_bounds.get().bottom();
                let list_height =
                    (below - px(PICKER_GAP)).min(px(PICKER_MAX_HEIGHT)) - px(PICKER_SEARCH_HEIGHT);
                let mut listed = projects::picker_entries(&entries, active_id.as_deref(), &query);
                if let Some(order) = &order {
                    listed = projects::order_while_open(listed, order);
                }
                let theme = cx.theme();
                let (dark, muted) = (theme.is_dark(), theme.muted_foreground);
                let (hover, selected) = (theme.list_hover, theme.list_active);
                let mono = theme.mono_font_family.clone();
                let (border, ring) = (theme.border, theme.ring);
                let rows = listed.iter().enumerate().map(|(index, entry)| {
                    let (view, id) = (view.clone(), entry.target(active_id.as_deref()).to_owned());
                    let (actions, pin_ids) = (actions.clone(), entry.member_ids.clone());
                    let pinned = entry.pinned;
                    let color: Hsla = rgb(entry.color_hex(dark)).into();
                    let active = active_id.as_deref().is_some_and(|id| entry.contains(id));
                    h_flex()
                        .id(("project", index))
                        .group(PICKER_ROW_GROUP)
                        .gap_2()
                        .px_2()
                        .py_1p5()
                        .items_center()
                        .rounded_md()
                        .cursor_pointer()
                        .when(active, |row| row.bg(selected))
                        .hover(move |style| style.bg(hover))
                        .child(
                            div()
                                .flex()
                                .flex_shrink_0()
                                .items_center()
                                .justify_center()
                                .size(px(20.))
                                .rounded(px(5.))
                                .bg(color.opacity(0.14))
                                .text_color(color)
                                .text_size(px(10.))
                                .font_weight(FontWeight::BOLD)
                                .font_family(mono.clone())
                                .child(entry.initials.clone()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_sm()
                                .truncate()
                                .child(entry.title.clone()),
                        )
                        .when(can_pin, |row| {
                            row.child(pin_button(index, entry, actions, pin_ids))
                        })
                        .when(!can_pin && pinned, |row| {
                            row.child(
                                Icon::new(gpui_kit_assets::IconName::Pin)
                                    .small()
                                    .text_color(muted),
                            )
                        })
                        .children(environment_badge(index, entry, muted))
                        .on_click(move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                this.select_project(id.clone(), cx);
                            });
                        })
                });
                // The web's `ComboboxPopup` (`w-72`, at most 23rem tall) and `ComboboxSearchInput`.
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
                                    Input::new(&search)
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
                            .id("project-list")
                            .max_h(list_height.max(px(0.)))
                            .overflow_y_scroll()
                            .p_1()
                            .children(rows)
                            .when(listed.is_empty(), |list| {
                                list.child(
                                    div()
                                        .p_2()
                                        .text_sm()
                                        .text_color(muted)
                                        .child("No matching projects."),
                                )
                            }),
                    )
            });
        let (heading, below) = if scratch {
            let heading = h_flex()
                .items_center()
                .text_3xl()
                .text_color(foreground)
                .child(HEADLINE_SCRATCH)
                .into_any_element();
            (heading, picker.into_any_element())
        } else {
            let heading = h_flex()
                .items_center()
                .text_2xl()
                .child("What should we build in")
                .child(picker)
                .child("?")
                .into_any_element();
            let link = div()
                .id("start-without-project")
                .font_weight(FontWeight::MEDIUM)
                .text_color(cx.theme().muted_foreground)
                .cursor_pointer()
                .hover(|style| style.underline().text_color(foreground))
                .tooltip(tooltip(Kbd::format(
                    &Keystroke::parse("alt-cmd-n").expect("valid keystroke"),
                )))
                .on_click(move |_, _, cx| {
                    let Some(id) = scratch_id.clone() else { return };
                    link_view.update(cx, |this, cx| {
                        this.select_project(id, cx);
                    });
                })
                .child("or start without a project")
                .into_any_element();
            (heading, link)
        };
        v_flex().items_center().child(heading).child(
            h_flex()
                .mt_2()
                .h_6()
                .items_center()
                .justify_center()
                .text_sm()
                .child(below),
        )
    }

    /// The web's `ComposerSurface.ContextStrip` under the composer. The no-project draft keeps only the run-on and permissions controls.
    fn context_strip(
        &self,
        project: &ProjectEntry,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement> {
        let Status::Connected { label, machine, .. } = &self.status else {
            return None;
        };
        let defaults = self.providers.as_ref().map_or_else(
            || Defaults::resolve(None, None, None),
            |c| c.defaults(Some(&project.id)),
        );
        let environment = strip::environment_label(label);
        let (draft, previous) = self.workspace_draft(project);
        let workspace = draft.mode;
        let current = self.branches.get(&project.root).and_then(Option::as_ref);
        let is_repo = current.is_some();
        let shown = strip::shown_branch(workspace, current, self.draft_bases.get(&project.id));
        let branch_label = shown.map(|branch| branch.label(workspace, defaults.start_from_origin));
        if !project.scratch {
            self.branch_picker
                .update(cx, |picker, _| picker.sync(&project.root, workspace));
        }
        // A reused worktree fixes the ref, so the control names its branch and does not open.
        let branch = if !is_repo {
            None
        } else if let Some(label) = draft.branch_label() {
            Some(
                strip_control("branch", Icons::GitBranch, label.clone(), label, false, cx)
                    .into_any_element(),
            )
        } else {
            branch_label.map(|label| self.branch_popover(label, cx))
        };
        let workspace_control = is_repo.then(|| {
            workspace_menu::popover(
                workspace_menu::Menu {
                    view: cx.entity(),
                    draft,
                    previous,
                    highlight: self.workspace_highlight,
                },
                self.workspace_open,
                WORKSPACE_SHORTCUT,
            )
        });
        let add_targets = self.add_targets(project);
        // `useLogicalProjectEnvironments` with `scratchDraft`: every connected environment that
        // has its own scratch project, instead of those that share the project.
        let others = if project.scratch {
            self.scratch_environments()
        } else {
            project.remote.clone()
        };
        let host_control = if run_on_menu::is_select(&others, !add_targets.is_empty()) {
            run_on_menu::popover(
                run_on_menu::Menu {
                    view: cx.entity(),
                    current: run_on_menu::Env {
                        label: environment.clone(),
                        machine: *machine,
                    },
                    others,
                    can_add: !add_targets.is_empty(),
                    highlight: self.host_highlight,
                },
                self.host_open,
                HOST_SHORTCUT,
            )
        } else {
            // One environment is the only one a draft can run on: the web's locked label.
            composer_menu::locked("host", machine_icon(*machine), environment.clone(), cx)
                .into_any_element()
        };
        let runtime_mode = self.runtime_mode.unwrap_or(defaults.runtime_mode);
        Some(
            h_flex()
                .w_full()
                .items_center()
                .gap_1()
                .px_2()
                .pt_2()
                .child(host_control)
                .children(is_repo.then(|| strip_separator(cx)))
                .children(workspace_control)
                .child(div().flex_1())
                .children(branch)
                .child(permissions_popover(
                    &self.permissions,
                    // One flag serves both triggers: the model picker's footer owns it while open.
                    self.permissions_open && !self.model_open,
                    runtime_mode,
                    PermissionsTrigger {
                        mode: runtime_mode,
                        open: false,
                    },
                    cx,
                )),
        )
    }

    /// The workspace mode of a project's draft, its worktree, and the worktree Previous worktree
    /// would reuse (`resolvePreviousWorktreeSeed` over the project's threads).
    fn workspace_draft(&self, project: &ProjectEntry) -> (Draft, Option<workspace::Seed>) {
        let defaults = self.providers.as_ref().map_or_else(
            || Defaults::resolve(None, None, None),
            |catalog| catalog.defaults(Some(&project.id)),
        );
        let draft = Draft {
            mode: self
                .env_modes
                .get(&project.id)
                .copied()
                .unwrap_or(defaults.env_mode),
            worktree: self.draft_worktrees.get(&project.id).cloned(),
        };
        let current = draft
            .worktree
            .as_ref()
            .map(|seed| seed.worktree_path.as_str());
        let previous = self.shell().and_then(|shell| {
            workspace::previous_seed(
                shell
                    .rows
                    .iter()
                    .filter(|row| row.project_id == project.id)
                    .map(|row| workspace::Thread {
                        branch: row.branch.as_deref(),
                        worktree_path: row.worktree_path.as_deref(),
                        updated_at_ms: row.updated_at_ms,
                    }),
                current,
            )
        });
        (draft, previous)
    }

    /// Whether the composer shows the workspace control: a project, and a repository.
    fn workspace_project(&self) -> Option<&ProjectEntry> {
        let project = self.draft_project().filter(|entry| !entry.scratch)?;
        self.branches
            .get(&project.root)
            .and_then(Option::as_ref)
            .map(|_| project)
    }

    /// Opens or closes the menu; opening lights the selected row and takes over the arrow keys.
    pub(crate) fn set_workspace_open(
        &mut self,
        open: bool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.workspace_open = open;
        self.workspace_keys = None;
        if open {
            let lit = self.workspace_project().map(|project| {
                let (draft, previous) = self.workspace_draft(project);
                Draft::rows(previous.is_some())
                    .iter()
                    .position(|row| *row == draft.selected())
                    .unwrap_or(0)
            });
            self.workspace_highlight = lit.unwrap_or(0);
            let view = cx.weak_entity();
            self.workspace_keys = Some(cx.intercept_keystrokes(move |event, window, cx| {
                let handled = view
                    .update(cx, |this, cx| {
                        this.workspace_key(&event.keystroke, window, cx)
                    })
                    .unwrap_or(false);
                if handled {
                    cx.stop_propagation();
                }
            }));
        }
        cx.notify();
    }

    pub(crate) fn set_host_open(&mut self, open: bool, _: &mut Window, cx: &mut Context<Self>) {
        self.host_open = open;
        self.host_highlight = 0;
        cx.notify();
    }

    /// The other saved environments a no-project draft lists under "Run on".
    fn scratch_environments(&self) -> Vec<projects::RemoteMember> {
        let mut others: Vec<_> = self
            .environments
            .iter()
            .filter(|environment| {
                environment.connected && environment.projects.iter().any(|entry| entry.scratch)
            })
            .map(|environment| projects::RemoteMember {
                label: environment.label.clone(),
                machine: environment.machine,
            })
            .collect();
        others.sort_by_key(|member| member.label.to_lowercase());
        others
    }

    /// `useAddProjectTargets`: connected machines without the project that can be reached from
    /// here, for a project that has a git remote.
    fn add_targets(&self, project: &ProjectEntry) -> Vec<add_to_machine::Target> {
        if project.repository.is_none() {
            return Vec::new();
        }
        let mut targets: Vec<_> = self
            .environments
            .iter()
            .filter(|environment| {
                environment.connected
                    && crate::environments::sender(&environment.id).is_some()
                    && !environment.projects.iter().any(|other| {
                        project.logical_key.is_some() && other.logical_key == project.logical_key
                    })
            })
            .map(|environment| add_to_machine::Target {
                id: environment.id.clone(),
                label: environment.label.clone(),
                machine: environment.machine,
            })
            .collect();
        targets.sort_by_key(|target| target.label.to_lowercase());
        targets
    }

    /// "Add to another machine": closes the menu and opens the clone dialog.
    pub(crate) fn open_add_to_machine(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_host_open(false, window, cx);
        let Status::Connected { label, machine, .. } = &self.status else {
            return;
        };
        let Some(project) = self.draft_project().filter(|entry| !entry.scratch) else {
            return;
        };
        let Some(repository) = project.repository.clone() else {
            return;
        };
        let targets = self.add_targets(project);
        let request = add_to_machine::Request {
            title: project.title.clone(),
            root: project.root.clone(),
            repository,
            source_label: strip::environment_label(label),
            source_machine: *machine,
            targets,
        };
        self.add_to_machine
            .update(cx, |dialog, cx| dialog.open(request, window, cx));
    }

    pub(crate) fn highlight_host(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.host_highlight != index {
            self.host_highlight = index;
            cx.notify();
        }
    }

    pub(crate) fn highlight_workspace(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.workspace_highlight != index {
            self.workspace_highlight = index;
            cx.notify();
        }
    }

    /// Applies a row to the draft (`onEnvModeChange`, `onUsePreviousWorktree`) and returns focus
    /// to the composer, as the web does (`scheduleComposerFocus`).
    pub(crate) fn pick_workspace(
        &mut self,
        choice: Choice,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.workspace_open = false;
        self.workspace_keys = None;
        let Some(project) = self.workspace_project() else {
            return cx.notify();
        };
        let id = project.id.clone();
        let (draft, previous) = self.workspace_draft(project);
        let next = draft.choose(choice, previous.as_ref());
        self.env_modes.insert(id.clone(), next.mode);
        match next.worktree {
            Some(seed) => {
                self.draft_worktrees.insert(id, seed);
            }
            None => {
                self.draft_worktrees.remove(&id);
            }
        }
        window.focus(&self.composer.focus_handle(cx), cx);
        cx.notify();
    }

    /// The menu's keyboard, which the select gives the web: arrows and Home and End move the lit
    /// row, Enter and Space choose it, Escape closes. Returns whether the key was taken.
    fn workspace_key(
        &mut self,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let modifiers = keystroke.modifiers;
        if !self.workspace_open
            || modifiers.platform
            || modifiers.control
            || modifiers.alt
            || modifiers.shift
        {
            return false;
        }
        let Some(project) = self.workspace_project() else {
            return false;
        };
        let count = Draft::rows(self.workspace_draft(project).1.is_some()).len();
        match keystroke.key.as_str() {
            "down" => {
                self.workspace_highlight = model_picker::step(self.workspace_highlight, count, 1);
            }
            "up" => {
                self.workspace_highlight = model_picker::step(self.workspace_highlight, count, -1);
            }
            "home" => self.workspace_highlight = 0,
            "end" => self.workspace_highlight = count.saturating_sub(1),
            "enter" | "space" => {
                let (draft, previous) = self.workspace_draft(project);
                let rows = Draft::rows(previous.is_some());
                let choice = rows.get(self.workspace_highlight).copied();
                let _ = draft;
                match choice {
                    Some(choice) => self.pick_workspace(choice, window, cx),
                    None => return false,
                }
            }
            "escape" => self.set_workspace_open(false, window, cx),
            _ => return false,
        }
        cx.notify();
        true
    }

    /// `composer.workspace`: opens the menu when the control is on screen.
    fn open_workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.workspace_project().is_some() {
            self.set_workspace_open(true, window, cx);
        }
    }

    /// `composer.previousWorktree` (`usePreviousWorktree`): reuses the previous worktree without
    /// opening the menu, when one is offered.
    fn use_previous_worktree(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let offered = self
            .workspace_project()
            .is_some_and(|project| self.workspace_draft(project).1.is_some());
        if offered {
            self.pick_workspace(Choice::Previous, window, cx);
        }
    }

    /// The web's `BranchToolbarBranchSelector` in the composer strip.
    fn branch_popover(&self, label: String, cx: &mut Context<Self>) -> AnyElement {
        let picker = self.branch_picker.clone();
        let menu = picker.read(cx).menu(&picker);
        Popover::new("branch")
            .anchor(Anchor::BottomRight)
            .open(picker.read(cx).is_open())
            .p_0()
            .on_open_change({
                let picker = picker.clone();
                move |open, window, cx| {
                    picker.update(cx, |picker, cx| picker.set_open(*open, window, cx));
                }
            })
            .trigger(StripTrigger {
                id: "branch",
                icon: Icons::GitBranch,
                tip: label.clone(),
                label,
                shrink: true,
                open: false,
            })
            .content(move |_, window, cx| menu.render(window, cx))
            .into_any_element()
    }

    fn on_branch_event(&mut self, event: &branch_picker::Event, cx: &mut Context<Self>) {
        match event {
            branch_picker::Event::Base(branch) => {
                if let Some(id) = self.draft_project().map(|entry| entry.id.clone()) {
                    self.draft_bases.insert(id, branch.clone());
                }
            }
            branch_picker::Event::Switched => {
                if let Some(root) = self.draft_project().map(|entry| entry.root.clone()) {
                    self.branches_requested.remove(&root);
                    self.request_branch();
                }
            }
            branch_picker::Event::Failed { title, message } => {
                let toast = Toast::new(*title)
                    .kind(ToastKind::Error)
                    .description(message.clone());
                self.toasts.update(cx, |toasts, cx| {
                    toasts.add(toast, cx);
                });
            }
        }
        cx.notify();
    }

    /// The project the composer's draft belongs to.
    fn draft_project(&self) -> Option<&ProjectEntry> {
        active_project(self.selected_project.as_deref(), self.shell()?)
    }

    /// The model picked for the composer in view, when it still resolves.
    fn model_pick(&self, key: Option<&str>) -> Option<&Pick> {
        self.model_picks.get(key?)
    }

    pub(crate) fn pick_model(&mut self, instance_id: &str, slug: &str, cx: &mut Context<Self>) {
        let project = self.selected_project.clone().filter(|id| !id.is_empty());
        self.model_ui.origin = Some(if self.model_ui.tab == Some(model_picker::Tab::Favorites) {
            model_picker::Origin::Favorites
        } else {
            model_picker::Origin::Instance
        });
        self.model_open = false;
        let key = self.details_key();
        let kept = key
            .as_deref()
            .and_then(|key| self.model_pick(Some(key)))
            .is_some_and(|pick| pick.instance_id == instance_id && pick.model == slug);
        if kept {
            return cx.notify();
        }
        let picked = self
            .providers
            .as_ref()
            .and_then(|catalog| catalog.pick(project.as_deref(), instance_id, slug));
        match (picked, key) {
            (Some(pick), Some(key)) => {
                self.model_picks.insert(key, pick);
            }
            _ => {
                let toast = Toast::new("That model is no longer available.").kind(ToastKind::Error);
                self.toasts.update(cx, |toasts, cx| {
                    toasts.add(toast, cx);
                });
            }
        }
        cx.notify();
    }

    /// The picker's instances and the trigger of the model in view.
    fn picker_context(&self) -> Option<(Vec<providers::PickerInstance>, Trigger)> {
        let catalog = self.providers.as_ref()?;
        let project = self.selected_project.clone().filter(|id| !id.is_empty());
        let key = self.details_key();
        let trigger = catalog.trigger_for(project.as_deref(), self.model_pick(key.as_deref()))?;
        Some((catalog.picker(project.as_deref()), trigger))
    }

    /// Opens on the active model's rail entry, or on Favorites (`initialModelPickerProvider`).
    fn picker_opened(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((instances, trigger)) = self.picker_context() else {
            return;
        };
        let visible = instances
            .iter()
            .any(|i| i.instance_id == trigger.instance_id && i.unavailable.is_none());
        let has_favorites = self.providers.as_ref().is_some_and(Catalog::has_favorites);
        let tab = model_picker::initial_tab(
            &trigger.instance_id,
            visible,
            has_favorites,
            self.model_ui.origin,
        );
        let expanded =
            model_picker::starts_expanded(&instances, &trigger.instance_id, &trigger.slug);
        let listing = model_picker::listing(&instances, &tab, "", expanded);
        let highlight = listing
            .rows
            .iter()
            .position(|row| {
                row.instance.instance_id == trigger.instance_id && row.model.slug == trigger.slug
            })
            .unwrap_or(0);
        self.model_ui = model_picker::State {
            tab: Some(tab),
            expanded,
            highlight,
            origin: self.model_ui.origin,
            ..model_picker::State::default()
        };
        let view = cx.weak_entity();
        self.model_keys = Some(cx.intercept_keystrokes(move |event, window, cx| {
            let handled = view
                .update(cx, |this, cx| this.picker_key(&event.keystroke, window, cx))
                .unwrap_or(false);
            if handled {
                cx.stop_propagation();
            }
        }));
        window.focus(&self.model_search.focus_handle(cx), cx);
        cx.notify();
    }

    fn picker_closed(&mut self) {
        self.model_keys = None;
    }

    pub(crate) fn picker_select_tab(
        &mut self,
        tab: model_picker::Tab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.model_search
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.model_ui.tab = Some(tab);
        self.model_ui.expanded = false;
        self.model_ui.highlight = 0;
        self.model_ui.focus = model_picker::Focus::List;
        cx.notify();
    }

    pub(crate) fn picker_toggle_older(&mut self, cx: &mut Context<Self>) {
        self.flip_older(cx);
        cx.notify();
    }

    fn flip_older(&mut self, cx: &App) {
        self.model_ui.expanded = !self.model_ui.expanded;
        let Some((instances, _)) = self.picker_context() else {
            return;
        };
        let tab = self
            .model_ui
            .tab
            .clone()
            .unwrap_or(model_picker::Tab::Favorites);
        let query = self.model_search.read(cx).value();
        self.model_ui.highlight =
            model_picker::listing(&instances, &tab, &query, self.model_ui.expanded)
                .rows
                .len();
    }

    /// Writes one option of the model in view into the composer's pick, so the next turn sends it.
    pub(crate) fn picker_set_option(
        &mut self,
        id: &str,
        value: serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        let (Some((_, trigger)), Some(key)) = (self.picker_context(), self.details_key()) else {
            return;
        };
        let options = providers::with_option(&trigger.options, id, value);
        self.model_picks.insert(
            key,
            Pick {
                instance_id: trigger.instance_id,
                model: trigger.slug,
                options,
            },
        );
        cx.notify();
    }

    /// Stars or unstars a model: the local copy now, the server's `setModelFavorites` behind it.
    pub(crate) fn picker_toggle_favorite(
        &mut self,
        instance_id: &str,
        slug: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(catalog) = self.providers.as_mut() else {
            return;
        };
        let favorite = !catalog.is_favorite(instance_id, slug);
        catalog.set_favorite(instance_id, slug, favorite);
        let action = Action::Call {
            token: FAVORITE_TOKEN,
            call: crate::connection::Call::UpdateSettings,
            payload: send::favorite_payload(instance_id, slug, favorite),
        };
        if self.actions.try_send(action).is_err() {
            self.error_toast(
                "Failed to update favorites",
                "The connection is not ready.".to_owned(),
                cx,
            );
        }
        cx.notify();
    }

    /// The picker's keyboard: Cmd+1..9 jump, arrows move within the focused column, Left and Right
    /// cross columns at the search's edges. Returns whether the key was taken.
    fn picker_key(
        &mut self,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        use model_picker::Focus;
        if !self.model_open || self.permissions_open {
            return false;
        }
        let Some((instances, trigger)) = self.picker_context() else {
            return false;
        };
        let modifiers = keystroke.modifiers;
        let tab = self
            .model_ui
            .tab
            .clone()
            .unwrap_or_else(|| model_picker::Tab::Instance(trigger.instance_id.clone()));
        let search = self.model_search.read(cx);
        let (query, cursor) = (search.value(), search.cursor());
        let listing = model_picker::listing(&instances, &tab, &query, self.model_ui.expanded);
        let key = keystroke.key.as_str();
        if modifiers.platform && !modifiers.alt && !modifiers.control {
            let Some(number) = key.parse::<usize>().ok().filter(|n| (1..=9).contains(n)) else {
                return false;
            };
            if let Some(&at) = listing.jump_rows().get(number - 1) {
                let row = listing.rows[at];
                let (instance, slug) = (row.instance.instance_id.clone(), row.model.slug.clone());
                self.pick_model(&instance, &slug, cx);
            }
            return true;
        }
        if modifiers.platform || modifiers.control || modifiers.alt || modifiers.shift {
            return false;
        }
        let delta = match key {
            "down" => 1,
            "up" => -1,
            _ => 0,
        };
        let actions = trigger.controls.actions();
        match (self.model_ui.focus, key) {
            (Focus::List, "down" | "up") => {
                let at = model_picker::step(self.model_ui.highlight, listing.items(), delta);
                self.model_ui.highlight = at;
            }
            (Focus::List, "enter") => {
                let at = self.model_ui.highlight;
                if let Some(row) = listing.rows.get(at) {
                    if row.usable() {
                        let (instance, slug) =
                            (row.instance.instance_id.clone(), row.model.slug.clone());
                        self.pick_model(&instance, &slug, cx);
                    }
                } else if listing.older.is_some() {
                    self.flip_older(cx);
                }
            }
            (Focus::List, "left") if cursor == 0 => self.model_ui.focus = Focus::Sidebar,
            (Focus::List, "right") if cursor >= query.len() && !actions.is_empty() => {
                self.model_ui.focus = Focus::Effort;
                self.model_ui.effort = 0;
            }
            (Focus::Sidebar, "down" | "up") => {
                let next = model_picker::adjacent_tab(&instances, &tab, delta);
                self.picker_select_tab(next, window, cx);
                self.model_ui.focus = Focus::Sidebar;
            }
            (Focus::Sidebar, "right" | "enter") => self.model_ui.focus = Focus::List,
            (Focus::Effort, "down" | "up") => {
                self.model_ui.effort =
                    model_picker::step(self.model_ui.effort, actions.len(), delta);
            }
            (Focus::Effort, "enter") => {
                if let Some((id, value)) = actions.get(self.model_ui.effort) {
                    self.picker_set_option(id, value.clone(), cx);
                }
            }
            (Focus::Effort, "left") => self.model_ui.focus = Focus::List,
            _ => return false,
        }
        cx.notify();
        true
    }

    fn open_permissions(&mut self, cx: &mut Context<Self>) {
        let strip = self.shell().is_some_and(|shell| {
            active_project(self.selected_project.as_deref(), shell).is_some_and(|e| !e.scratch)
        });
        self.permissions_open = strip;
        cx.notify();
    }

    /// The web's `ProviderModelPicker`: the chip, and the popover opening above it.
    fn model_popover(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (instances, trigger) = self.picker_context()?;
        let project = self.selected_project.clone().filter(|id| !id.is_empty());
        let mode = self.runtime_mode.unwrap_or_else(|| {
            self.providers
                .as_ref()
                .map_or(RuntimeMode::FullAccess, |catalog| {
                    catalog.defaults(project.as_deref()).runtime_mode
                })
        });
        let (view, search) = (cx.entity(), self.model_search.clone());
        let panel = model_popover::Panel {
            view: view.clone(),
            search: search.clone(),
            tab: self
                .model_ui
                .tab
                .clone()
                .unwrap_or_else(|| model_picker::Tab::Instance(trigger.instance_id.clone())),
            expanded: self.model_ui.expanded,
            highlight: self.model_ui.highlight,
            focus: self.model_ui.focus,
            effort: self.model_ui.effort,
            permissions: self.permissions.clone(),
            permissions_open: self.permissions_open && self.model_open,
            mode,
            instances,
            trigger: trigger.clone(),
        };
        Some(
            Popover::new("model-picker")
                .anchor(Anchor::BottomRight)
                .open(self.model_open)
                .track_focus(&search.focus_handle(cx))
                .p_0()
                .bg(cx.theme().popover)
                .border_1()
                .border_color(dropdown_glass_edge(cx))
                .on_open_change(move |open, window, cx| {
                    search.update(cx, |state, cx| state.set_value("", window, cx));
                    view.update(cx, |this, cx| {
                        this.model_open = *open;
                        if *open {
                            this.picker_opened(window, cx);
                        } else {
                            this.picker_closed();
                        }
                        cx.notify();
                    });
                })
                .trigger(ModelTrigger {
                    trigger,
                    open: self.model_open,
                })
                .content(move |_, window, cx| panel.render(window, cx))
                .into_any_element(),
        )
    }

    fn composer_box(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let model_picker = self.model_popover(cx);
        let primary = send::primary(
            self.thread_running(),
            &self.composer.read(cx).value(),
            &self.attachments,
            self.sending.is_some(),
        );
        let chips = chip_infos(&self.attachments);
        v_flex()
            .w(px(CHAT_CONTENT_WIDTH))
            .max_w_full()
            .p_px()
            .rounded(px(16.))
            .border_1()
            .border_color(composer_outline(cx.theme().is_dark()))
            .bg(composer_surface(cx.theme().is_dark()))
            .child(
                // The web's `px-4 pt-4 pb-2` body; the negative margin cancels the textarea's own padding.
                div()
                    .px_4()
                    .pt_4()
                    .pb_2()
                    .children(self.attachment_strip(cx))
                    .child(
                        Textarea::new(&self.composer)
                            .appearance(false)
                            .bordered(false)
                            .mx(px(-10.))
                            .my(px(-8.))
                            // The web's `leading-relaxed` at 14px.
                            .line_height(px(22.75))
                            .token(move |token, _, cx| composer_chip(token, &chips, cx)),
                    ),
            )
            .child(
                h_flex()
                    .px_2()
                    .pb_2()
                    .justify_between()
                    .child(
                        header_icon_button(
                            "attach",
                            gpui_kit_assets::IconName::Paperclip,
                            "Attach files".into(),
                            true,
                            cx,
                        )
                        .on_click(cx.listener(|this, _, window, cx| this.attach_files(window, cx))),
                    )
                    .child(
                        h_flex().gap_2().children(model_picker).child(
                            send_button(primary, self.send_hovered, cx)
                                .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                                    this.send_hovered = *hovered;
                                    cx.notify();
                                }))
                                .on_click(cx.listener(move |this, _, _, cx| match primary {
                                    Primary::Stop => this.stop(cx),
                                    Primary::Send { .. } => this.submit(cx),
                                })),
                        ),
                    ),
            )
    }
}

/// The strip's 20px remove and retry control.
fn attachment_button(
    id: String,
    icon: Icons,
    label: String,
    look: StripButton,
) -> gpui::Stateful<gpui::Div> {
    let black = gpui::black();
    let white = gpui::white();
    let button = div()
        .id(SharedString::from(id))
        .flex()
        .items_center()
        .justify_center()
        .size(px(24.))
        .rounded(px(6.))
        .border_1()
        .cursor_pointer()
        .tooltip(tooltip(label));
    let button = match look {
        StripButton::MediaClose => button
            .border_color(white.opacity(0.2))
            .bg(black.opacity(0.65))
            .text_color(white)
            .hover(move |style| style.bg(black.opacity(0.8))),
        StripButton::Overlay => button
            .border_color(gpui::transparent_black())
            .bg(black.opacity(0.7))
            .text_color(white.opacity(0.65))
            .hover(move |style| style.bg(black.opacity(0.9))),
        StripButton::Ghost { icon, hover } => button
            .border_color(gpui::transparent_black())
            .text_color(icon)
            .hover(move |style| style.bg(hover)),
    };
    button.child(Icon::new(icon).size(px(14.)))
}

/// The web `Button` variants the attachment strip uses at `icon-xs`.
#[derive(Clone, Copy)]
enum StripButton {
    MediaClose,
    Overlay,
    Ghost { icon: Hsla, hover: Hsla },
}

/// The web's `ComposerControl` at `xs`: a ghost button with a 12px icon and a 12px label.
fn strip_control(
    id: &'static str,
    icon: Icons,
    label: String,
    tip: String,
    interactive: bool,
    cx: &App,
) -> gpui::Stateful<gpui::Div> {
    let theme = cx.theme();
    let (foreground, muted) = (theme.foreground, theme.muted_foreground);
    div()
        .id(id)
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(6.))
        .h(px(24.))
        .px(px(8.))
        .rounded(px(8.))
        .text_xs()
        .text_color(muted)
        .when(interactive, |control| {
            control
                .cursor_pointer()
                .hover(move |style| style.bg(foreground.opacity(0.06)).text_color(foreground))
        })
        .tooltip(tooltip(tip))
        .child(Icon::new(icon).size(px(12.)))
        .child(
            div()
                .max_w(px(240.))
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(label),
        )
}

/// A strip control as a popover trigger.
#[derive(IntoElement)]
struct StripTrigger {
    id: &'static str,
    icon: Icons,
    label: String,
    tip: String,
    shrink: bool,
    open: bool,
}

impl Selectable for StripTrigger {
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

impl RenderOnce for StripTrigger {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let tip = if self.open { String::new() } else { self.tip };
        let foreground = cx.theme().foreground;
        strip_control(self.id, self.icon, self.label, tip, true, cx)
            .when(self.shrink, |control| control.flex_shrink(1.).min_w_0())
            .when(self.open, move |control| {
                control.bg(foreground.opacity(0.06)).text_color(foreground)
            })
    }
}

/// The web's `ComposerContextPermissions` menu: opens above its trigger, aligned to the end.
fn permissions_popover(
    panel: &Entity<PermissionsPanel>,
    open: bool,
    mode: RuntimeMode,
    trigger: impl Selectable + IntoElement + 'static,
    cx: &App,
) -> impl IntoElement {
    let view = panel.read(cx).view.clone();
    let (panel, content_panel) = (panel.clone(), panel.clone());
    Popover::new("permissions")
        .anchor(Anchor::BottomRight)
        .open(open)
        .p_0()
        .on_open_change(move |open, _, cx| {
            panel.update(cx, |panel, _| panel.preview = None);
            view.update(cx, |this, cx| {
                this.permissions_open = *open;
                cx.notify();
            })
            .ok();
        })
        .trigger(trigger)
        .content(move |_, _, cx| {
            content_panel.update(cx, |panel, _| panel.selected = mode);
            content_panel.clone()
        })
}

/// The permissions chip as a popover trigger.
#[derive(IntoElement)]
struct PermissionsTrigger {
    mode: RuntimeMode,
    open: bool,
}

impl Selectable for PermissionsTrigger {
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

impl RenderOnce for PermissionsTrigger {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let tip = if self.open {
            String::new()
        } else {
            strip::tooltip_text("Permissions", Some(PERMISSIONS_SHORTCUT))
        };
        let control = strip_control(
            "permissions",
            runtime_icon(self.mode),
            self.mode.label().to_owned(),
            tip,
            true,
            cx,
        );
        let foreground = cx.theme().foreground;
        control.when(self.open, move |control| {
            control.bg(foreground.opacity(0.06)).text_color(foreground)
        })
    }
}

/// The web's `ComposerPermissionsSection`: four icon segments, and the shown mode named below.
struct PermissionsPanel {
    view: gpui::WeakEntity<MainView>,
    selected: RuntimeMode,
    preview: Option<RuntimeMode>,
}

impl Render for PermissionsPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (foreground, muted) = (theme.foreground, theme.muted_foreground);
        let (track, selected_bg, hover_bg) = (
            theme.input.opacity(0.4),
            theme.input.opacity(0.72),
            theme.accent,
        );
        let (shown, is_selected) = strip::previewed(self.selected, self.preview);
        let last = RuntimeMode::ALL.len() - 1;
        let segments = RuntimeMode::ALL
            .into_iter()
            .enumerate()
            .map(|(index, mode)| {
                let chosen = mode == self.selected;
                div()
                    .id(("runtime-mode", index))
                    .flex()
                    .flex_1()
                    .h_6()
                    .items_center()
                    .justify_center()
                    .when(index == 0, |segment| segment.rounded_l_md())
                    .when(index == last, |segment| segment.rounded_r_md())
                    .cursor_pointer()
                    .when(chosen, |segment| {
                        segment.bg(selected_bg).text_color(foreground)
                    })
                    .when(!chosen, |segment| {
                        segment
                            .text_color(muted)
                            .hover(move |style| style.bg(hover_bg).text_color(foreground))
                    })
                    .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                        if *hovered {
                            this.preview = Some(mode);
                        } else if this.preview == Some(mode) {
                            this.preview = None;
                        }
                        cx.notify();
                    }))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.selected = mode;
                        this.preview = None;
                        this.view
                            .update(cx, |view, cx| {
                                view.runtime_mode = Some(mode);
                                view.permissions_open = false;
                                cx.notify();
                            })
                            .ok();
                        cx.notify();
                    }))
                    .child(Icon::new(runtime_icon(mode)).size(px(14.)))
            });
        v_flex()
            .w(px(288.))
            .gap(px(6.))
            .px_2()
            .py(px(6.))
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(muted)
                    .child("Permissions"),
            )
            .child(h_flex().rounded_lg().bg(track).p(px(2.)).children(segments))
            .child(
                v_flex()
                    .gap(px(2.))
                    .child(
                        div()
                            .text_sm()
                            .text_color(if is_selected { foreground } else { muted })
                            .child(shown.label()),
                    )
                    .child(
                        div()
                            .min_h_8()
                            .text_xs()
                            .line_height(px(16.))
                            .text_color(muted)
                            .child(shown.description()),
                    ),
            )
    }
}

/// The web's `Separator` between the environment and the workspace: `mx-0.5 h-3.5`.
fn strip_separator(cx: &App) -> gpui::Div {
    div()
        .flex_shrink_0()
        .mx(px(2.))
        .w(px(1.))
        .h(px(14.))
        .bg(cx.theme().border)
}

/// `runtimeModeConfig`'s icons.
fn runtime_icon(mode: RuntimeMode) -> Icons {
    match mode {
        RuntimeMode::ApprovalRequired => Icons::Lock,
        RuntimeMode::AutoAcceptEdits => Icons::PenLine,
        RuntimeMode::Auto => Icons::Sparkles,
        RuntimeMode::FullAccess => Icons::ShieldOff,
    }
}

/// The web's `ProviderModelPicker` trigger, a `ComposerControl` at `sm`. Clicking does nothing yet.
/// The model chip as a popover trigger.
#[derive(IntoElement)]
struct ModelTrigger {
    trigger: Trigger,
    open: bool,
}

impl Selectable for ModelTrigger {
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

impl RenderOnce for ModelTrigger {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let foreground = cx.theme().foreground;
        model_trigger(&self.trigger, self.open, cx).when(self.open, move |chip| {
            chip.bg(foreground.opacity(0.06)).text_color(foreground)
        })
    }
}

fn model_trigger(trigger: &Trigger, open: bool, cx: &App) -> gpui::Stateful<gpui::Div> {
    let theme = cx.theme();
    let (foreground, muted) = (theme.foreground, theme.muted_foreground);
    div()
        .id("model-trigger")
        .group("model-trigger")
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(6.))
        .h(px(28.))
        .px_2()
        .rounded(px(8.))
        .text_sm()
        .font_weight(FontWeight::MEDIUM)
        .text_color(foreground.opacity(0.8))
        .cursor_pointer()
        .hover(move |style| style.bg(foreground.opacity(0.06)).text_color(foreground))
        .when(!open, |chip| chip.tooltip(tooltip(trigger.title.clone())))
        .child(provider_mark(&trigger.mark, 1., cx))
        .child(
            div()
                .max_w(px(224.))
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(trigger.model.clone()),
        )
        .children(trigger.fast.then(|| {
            div()
                .text_color(muted)
                .group_hover("model-trigger", move |style| style.text_color(foreground))
                .child(Icon::new(Icons::Zap).size(px(12.)))
        }))
        .children(trigger.effort.clone().map(|effort| {
            div()
                .ml(px(-2.))
                .max_w(px(96.))
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_color(muted)
                .group_hover("model-trigger", move |style| style.text_color(foreground))
                .child(effort)
        }))
}

/// The web's `ProviderInstanceIcon` at `size-4`: the driver's art, or its initials, with the
/// instance chip at the bottom-right. `icon_opacity` dims the art only, like `iconClassName`.
fn provider_mark(mark: &Mark, icon_opacity: f32, cx: &App) -> gpui::Div {
    provider_mark_at(mark, 16., 16., icon_opacity, cx)
}

/// The mark in a `frame` square with its art at `art`: the picker's rail uses `size-6` around `size-5`.
fn provider_mark_at(
    mark: &Mark,
    frame: f32,
    art_size: f32,
    icon_opacity: f32,
    cx: &App,
) -> gpui::Div {
    let theme = cx.theme();
    let art = match provider_icon(&mark.driver, theme.is_dark()) {
        Some(path) => img(path).size_full().into_any_element(),
        None => div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(10.))
            .font_weight(FontWeight::SEMIBOLD)
            .child(crate::providers::initials(&mark.display_name))
            .into_any_element(),
    };
    div()
        .relative()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .size(px(frame))
        .child(div().size(px(art_size)).opacity(icon_opacity).child(art))
        .children(mark.badge.as_ref().map(|badge| {
            let (bg, fg) = match badge.accent {
                Some((bg, fg)) => (rgb(bg).into(), rgb(fg).into()),
                None => (theme.background, theme.muted_foreground),
            };
            div()
                .absolute()
                .right(px(-2.))
                .bottom(px(-2.))
                .h(px(12.))
                .min_w(px(12.))
                .px(px(2.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .border_1()
                .border_color(theme.secondary)
                .shadow_sm()
                .bg(bg)
                .text_color(fg)
                .text_size(px(7.))
                .line_height(px(7.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(badge.initials.clone())
        }))
}

/// The project name as plain heading text that underlines on hover and while its picker is open.
/// Under a scratch draft it is the `InlineButton` `picker` tone at `text-sm`, `font-medium`.
#[derive(IntoElement)]
struct ProjectTrigger {
    name: String,
    scratch: bool,
    open: bool,
    bounds: Rc<Cell<Bounds<Pixels>>>,
}

impl ProjectTrigger {
    fn new(name: &str, scratch: bool, bounds: Rc<Cell<Bounds<Pixels>>>) -> Self {
        Self {
            name: if scratch { NO_PROJECT } else { name }.to_owned(),
            scratch,
            open: false,
            bounds,
        }
    }
}

impl Selectable for ProjectTrigger {
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

impl RenderOnce for ProjectTrigger {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let name = self.name.clone();
        let bounds = self.bounds;
        let trigger = div()
            .id("project")
            .relative()
            .child(
                canvas(move |rect, _, _| bounds.set(rect), |_, _, _, _| {})
                    .absolute()
                    .inset_0(),
            )
            .min_w_0()
            .max_w_full()
            .truncate()
            .cursor_pointer()
            .when(self.open, |name| name.underline())
            .hover(|style| style.underline());
        if self.scratch {
            trigger.font_weight(FontWeight::MEDIUM).child(self.name)
        } else {
            trigger
                .ml_2()
                .text_2xl()
                .tooltip(tooltip(name))
                .child(self.name)
        }
    }
}

fn project_label(entry: Option<&ProjectEntry>) -> String {
    match entry {
        Some(entry) if entry.scratch => NO_PROJECT.to_owned(),
        Some(entry) => entry.title.clone(),
        None => String::new(),
    }
}

fn scratch_entry(entries: &[ProjectEntry]) -> Option<&ProjectEntry> {
    entries.iter().find(|entry| entry.scratch)
}

/// The web's `TooltipPopup` sizing: `text-xs`, `px-2 py-1`, `rounded-md`.
/// What a chip shows beyond its reference text.
struct ChipInfo {
    name: String,
    size: String,
    kind: attachments::ChipKind,
    progress: Option<String>,
    failed: Option<String>,
}

fn chip_infos(items: &[attachments::Item]) -> Rc<HashMap<String, ChipInfo>> {
    Rc::new(
        items
            .iter()
            .map(|item| {
                let info = ChipInfo {
                    name: item.name.clone(),
                    size: attachments::format_size(item.size),
                    kind: attachments::ChipKind::of(item),
                    progress: match &item.upload {
                        Upload::Uploading { progress } => {
                            Some(attachments::format_progress(*progress))
                        }
                        _ => None,
                    },
                    failed: match &item.upload {
                        Upload::Failed { reason } => Some(reason.clone()),
                        _ => None,
                    },
                };
                (item.context_id(), info)
            })
            .collect(),
    )
}

/// The web's `FileChipContent` / `ImageChipButton` as an inline chip (`ContextChip`).
fn composer_chip(
    token: &InlineTokenContext,
    chips: &HashMap<String, ChipInfo>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let dark = theme.is_dark();
    let info = chips.get(token.token().id().as_ref());
    let reference = attachments::collect_references(token.token().text())
        .into_iter()
        .next();
    let name = info
        .map(|info| info.name.clone())
        .or_else(|| reference.as_ref().map(|reference| reference.label.clone()))
        .unwrap_or_default();
    let kind = info.map(|info| info.kind).unwrap_or_else(|| {
        let wire = reference
            .as_ref()
            .map_or("file", |reference| &reference.kind);
        attachments::ChipKind::of_reference(wire, &name)
    });
    let failed = info.and_then(|info| info.failed.clone());
    let color: Hsla = rgb(if failed.is_some() {
        attachments::ChipKind::invalid(dark)
    } else {
        kind.accent(dark)
    })
    .into();
    let size = info.map(|info| info.size.clone());
    let mut hint = name.clone();
    if let Some(size) = &size {
        hint = format!("{hint}\n{size}");
    }
    if let Some(reason) = &failed {
        hint = format!("{hint}\n{reason}");
    } else if let Some(progress) = info.and_then(|info| info.progress.as_ref()) {
        hint = format!("{hint}\nUploading {progress}");
    }
    let icon = match chip_icon(kind) {
        Some(path) => Icon::empty().path(path),
        None => Icon::new(Icons::File),
    };
    h_flex()
        .id(("composer-chip", token.range().start))
        .group("composer-chip")
        .max_w_full()
        .items_center()
        .gap(px(2.8))
        .rounded(px(4.))
        .text_size(px(14.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(color)
        .when(token.is_selected(), |chip| chip.bg(theme.selection))
        .tooltip(tooltip(hint))
        .child(icon.size(px(14.)).flex_shrink_0().text_color(color))
        .child(
            h_flex()
                .min_w_0()
                .items_baseline()
                .gap(px(4.62))
                .group_hover("composer-chip", |label| label.underline())
                .child(
                    div()
                        .min_w_0()
                        .max_w(px(288.))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(attachments::middle_truncate_name(&name, 36)),
                )
                .children(size.map(|size| {
                    div()
                        .flex_shrink_0()
                        .text_size(px(10.))
                        .line_height(px(14.))
                        .child(size)
                })),
        )
        .into_any_element()
}

fn tooltip(
    text: impl Into<gpui::SharedString>,
) -> impl Fn(&mut Window, &mut App) -> gpui::AnyView + 'static {
    let text = text.into();
    move |window, cx| {
        Tooltip::new(text.clone())
            .text_xs()
            .py_1()
            .rounded(px(6.))
            .build(window, cx)
    }
}

/// The web's `SidebarHeaderIconButton`: a 28px ghost button whose icon turns to the foreground on hover.
fn header_icon_button(
    id: &'static str,
    icon: gpui_kit_assets::IconName,
    label: gpui::SharedString,
    enabled: bool,
    cx: &App,
) -> gpui::Stateful<gpui::Div> {
    ghost_icon_button(id, Icon::new(icon).size_4(), label, enabled, cx)
}

/// A project's monogram badge: its initials on the project color at 14%.
fn monogram(initials: &str, color: Hsla, size: f32, font: &SharedString) -> gpui::Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .size(px(size))
        .rounded(px(size / 4.))
        .bg(color.opacity(0.14))
        .text_color(color)
        .text_size(px(size / 2.))
        .font_weight(FontWeight::BOLD)
        .font_family(font.clone())
        .child(initials.to_owned())
}

/// The scope menu's header button: the filter icon, or the badges of the scoped projects.
#[derive(IntoElement)]
struct ScopeTrigger {
    label: SharedString,
    scoped: Vec<(String, u32)>,
    open: bool,
}

impl Selectable for ScopeTrigger {
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

impl RenderOnce for ScopeTrigger {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        if self.scoped.is_empty() {
            return ghost_icon_button(
                "filter-projects",
                Icon::new(Icons::ListFilter).size_4(),
                self.label,
                true,
                cx,
            );
        }
        let mono = cx.theme().mono_font_family.clone();
        // Two badges at most, overlapped, as the web's stacked favicons.
        let size = if self.scoped.len() > 1 { 14. } else { 16. };
        let badges = h_flex().children(self.scoped.iter().take(2).enumerate().map(
            |(index, (initials, color))| {
                monogram(initials, rgb(*color).into(), size, &mono)
                    .when(index > 0, |badge| badge.ml(px(-6.)))
            },
        ));
        ghost_icon_button("filter-projects", badges, self.label, true, cx)
    }
}

/// The top bar's `icon-sm` buttons: `ghost`, or `media-navigation` on the stage art.
/// Disabled ones stay dim and ignore the pointer.
fn nav_button(
    id: &'static str,
    icon: IconName,
    label: gpui::SharedString,
    enabled: bool,
    on_backdrop: bool,
    cx: &App,
) -> gpui::Stateful<gpui::Div> {
    let icon = Icon::new(icon).size(px(14.));
    if !on_backdrop {
        return ghost_icon_button(id, icon, label, enabled, cx);
    }
    let white = gpui::white();
    icon_button(
        id,
        icon,
        label,
        enabled,
        white.opacity(0.9),
        white.opacity(0.1),
        white,
    )
}

fn ghost_icon_button(
    id: &'static str,
    icon: impl IntoElement,
    label: gpui::SharedString,
    enabled: bool,
    cx: &App,
) -> gpui::Stateful<gpui::Div> {
    let theme = cx.theme();
    // Translucent like the web's `--accent`, so it shows on the composer as well as the sidebar.
    let (idle, hover) = (theme.muted_foreground, theme.foreground.opacity(0.06));
    icon_button(id, icon, label, enabled, idle, hover, theme.foreground)
}

fn icon_button(
    id: &'static str,
    icon: impl IntoElement,
    label: gpui::SharedString,
    enabled: bool,
    idle: Hsla,
    hover_bg: Hsla,
    hover_fg: Hsla,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex_shrink_0()
        .size(px(28.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.))
        .text_color(idle)
        .when(enabled, |button| {
            button
                .cursor_pointer()
                .hover(move |style| style.bg(hover_bg).text_color(hover_fg))
                .tooltip(tooltip(label))
        })
        // The web's `disabled:opacity-64`.
        .when(!enabled, |button| button.opacity(0.64))
        .child(icon)
}

/// The sidebar footer's `size-8` menu button.
fn footer_button(
    id: &'static str,
    icon: Icon,
    label: &'static str,
    cx: &App,
) -> gpui::Stateful<gpui::Div> {
    let theme = cx.theme();
    let (idle, hover_bg, hover_fg) = (theme.muted_foreground, theme.list_hover, theme.foreground);
    div()
        .id(id)
        .flex_shrink_0()
        .size_8()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.))
        .text_color(idle)
        .cursor_pointer()
        .hover(move |style| style.bg(hover_bg).text_color(hover_fg))
        .tooltip(tooltip(label))
        .child(icon.size_4())
}

/// The web's idle `SidebarUpdatePill`. This build has no update feed, which the desktop reports as
/// `status: "disabled"` (`resolveDisabledReason` in DesktopUpdates.ts): a dimmed, inert refresh.
fn update_button(cx: &App) -> gpui::Stateful<gpui::Div> {
    use gpui_kit_assets::IconName as Icons;
    const TITLE: &str = "Updates unavailable";
    const DETAIL: &str =
        "Automatic updates are not available because no update feed is configured.";
    div()
        .id("update")
        .flex_shrink_0()
        .size_8()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .text_color(cx.theme().muted_foreground)
        .opacity(0.6)
        .cursor_not_allowed()
        .tooltip(|window, cx| {
            Tooltip::element(|_, cx| {
                v_flex()
                    .child(div().font_weight(FontWeight::MEDIUM).child(TITLE))
                    .child(div().text_color(cx.theme().muted_foreground).child(DETAIL))
            })
            .text_xs()
            .py_1()
            .rounded(px(6.))
            .build(window, cx)
        })
        .child(Icon::new(Icons::RefreshCw).size_4())
}

/// The web's `ComposerPrimaryActions` send button: stage art in dev builds, else `message-action`.
fn send_button(primary: Primary, hovered: bool, cx: &App) -> gpui::Stateful<gpui::Div> {
    let enabled = !matches!(primary, Primary::Send { enabled: false });
    let stop = primary == Primary::Stop;
    let theme = cx.theme();
    let (bg, hover_bg, fg) = if STAGE_BACKDROP {
        (
            gpui::transparent_black(),
            gpui::white().opacity(0.1),
            gpui::white(),
        )
    } else if enabled {
        (theme.primary, theme.primary_hover, theme.primary_foreground)
    } else {
        (
            theme.foreground.opacity(0.12),
            theme.foreground.opacity(0.12),
            theme.muted_foreground,
        )
    };
    // The web's `hover:scale-105` over 150ms: the circle grows 5% inside a fixed slot.
    let grown = enabled && hovered;
    div()
        .id("send")
        .group("send")
        .relative()
        .flex_shrink_0()
        .size(px(32.))
        .when(enabled, |slot| {
            slot.cursor_pointer().tooltip(tooltip(if stop {
                "Stop generation"
            } else {
                "Submit message"
            }))
        })
        .child(
            div()
                .absolute()
                .inset_0()
                .rounded_full()
                .bg(bg)
                .when(STAGE_BACKDROP && !enabled, |circle| circle.opacity(0.64))
                .when(STAGE_BACKDROP, |circle| {
                    circle.child(
                        img(STAGE_DEV_BUTTON)
                            .absolute()
                            .size_full()
                            // `overflow_hidden` does not round-clip images.
                            .rounded_full()
                            .object_fit(ObjectFit::Cover),
                    )
                })
                .child(
                    div()
                        .absolute()
                        .size_full()
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(fg)
                        .when(enabled, |face| {
                            face.group_hover("send", move |style| style.bg(hover_bg))
                        })
                        .child(if stop {
                            // The web's `size-3 rounded-[2px] bg-current` stop square.
                            div()
                                .size(px(12.))
                                .rounded(px(2.))
                                .bg(fg)
                                .into_any_element()
                        } else {
                            Icon::new(IconName::ArrowUp).size_4().into_any_element()
                        }),
                )
                .with_animation(
                    ("send-grow", usize::from(grown)),
                    Animation::new(Duration::from_millis(150)).with_easing(ease_in_out),
                    move |circle, delta| {
                        let inset = px(-0.8) * if grown { delta } else { 1. - delta };
                        circle.top(inset).left(inset).right(inset).bottom(inset)
                    },
                ),
        )
}

fn machine_icon(kind: MachineKind) -> gpui_kit_assets::IconName {
    use gpui_kit_assets::IconName as Icons;
    match kind {
        MachineKind::Server => Icons::Server,
        MachineKind::Cloud => Icons::Cloud,
        MachineKind::Linux => Icons::Terminal,
        MachineKind::Desktop => Icons::Monitor,
        MachineKind::Laptop => Icons::Laptop,
        MachineKind::MacMini => Icons::HardDrive,
        MachineKind::MacStudio => Icons::Computer,
    }
}

/// The web's `ProjectEnvironmentBadge`: a machine icon with "Also on <labels>", right of the pin.
fn environment_badge(
    index: usize,
    entry: &ProjectEntry,
    muted: Hsla,
) -> Option<impl IntoElement + use<>> {
    let text = projects::badge_text(&entry.remote)?;
    let machine = projects::badge_machine(&entry.remote)?;
    Some(
        div()
            .id(("environment", index))
            .flex_shrink_0()
            .tooltip(tooltip(text))
            .child(Icon::new(machine_icon(machine)).small().text_color(muted)),
    )
}

const PICKER_ROW_GROUP: &str = "project-row";

/// Shown on row hover, and always for a pinned project. A click toggles the pin and, because
/// `Button` stops propagation, does not select the row.
fn pin_button(
    index: usize,
    entry: &ProjectEntry,
    actions: ActionSender,
    project_ids: Vec<String>,
) -> impl IntoElement + use<> {
    let pinned = entry.pinned;
    let icon = |name| Icon::new(name).small();
    let hint = tooltip(projects::pin_tooltip(&entry.title, pinned));
    let button = Button::new(("pin", index))
        .ghost()
        .xsmall()
        .when(!pinned, |button| {
            button
                .invisible()
                .group_hover(PICKER_ROW_GROUP, |style| style.visible())
        })
        .child(if pinned {
            div()
                .group("pin-icon")
                .child(
                    div()
                        .group_hover("pin-icon", |style| style.invisible())
                        .child(icon(gpui_kit_assets::IconName::Pin)),
                )
                .child(
                    div()
                        .absolute()
                        .invisible()
                        .group_hover("pin-icon", |style| style.visible())
                        .child(icon(gpui_kit_assets::IconName::PinOff)),
                )
        } else {
            div().child(icon(gpui_kit_assets::IconName::Pin))
        })
        .on_click(move |_, _, _| {
            // Clients write every member of the group together.
            for project_id in &project_ids {
                let action = Action::SetProjectPinned {
                    project_id: project_id.clone(),
                    pinned: !pinned,
                };
                if actions.try_send(action).is_err() {
                    eprintln!("[ui] could not queue the pin request");
                }
            }
        });
    div().id(("pin-tooltip", index)).tooltip(hint).child(button)
}

/// The picked project id while its row still exists, else the active row's target.
fn selected_id<'a>(view: &'a MainView, active: Option<&'a ProjectEntry>) -> Option<&'a str> {
    view.selected_project
        .as_deref()
        .filter(|id| active.is_some_and(|entry| entry.contains(id)))
        .or(active.map(|entry| entry.id.as_str()))
}

/// The picked project while it still exists, else the newest thread's project, else the first listed.
fn active_project<'a>(selected: Option<&str>, shell: &'a ShellView) -> Option<&'a ProjectEntry> {
    let by_id = |id: &str| {
        shell
            .project_entries
            .iter()
            .find(|entry| entry.contains(id))
    };
    selected
        .and_then(by_id)
        .or_else(|| {
            shell
                .rows
                .iter()
                .filter_map(|row| by_id(&row.project_id))
                .find(|entry| !entry.scratch)
        })
        .or_else(|| shell.project_entries.iter().find(|entry| !entry.scratch))
}

fn centered(cx: &Context<MainView>) -> gpui::Div {
    v_flex()
        .flex_1()
        .min_h_0()
        .w_full()
        .px_6()
        .items_center()
        .justify_center()
        .text_color(cx.theme().foreground)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Shelf {
    section: Section,
    count: usize,
    expanded: bool,
}

/// The web's `SETTLED_TAIL_INITIAL_COUNT` and `SETTLED_TAIL_PAGE_COUNT`.
const SETTLED_TAIL_INITIAL_COUNT: usize = 10;
const SETTLED_TAIL_PAGE_COUNT: usize = 25;
/// The web's shelf header and footer button size (`h-8`, `size-8`).
const SHELF_HEADER_HEIGHT: f32 = 32.;

fn shelf_index(section: Section) -> usize {
    match section {
        Section::Working => 0,
        Section::Snoozed => 1,
        _ => 2,
    }
}

/// The pinned and active rows, listed above the shelves.
fn top_rows(rows: &[ThreadRow]) -> Vec<usize> {
    let listed = |row: &ThreadRow| matches!(row.section, Section::Pinned | Section::Active);
    (0..rows.len())
        .filter(|&index| listed(&rows[index]))
        .collect()
}

/// A shelf's rows: all while expanded, else only the open thread (`routeThreadOnly`).
fn shelf_rows(
    rows: &[ThreadRow],
    section: Section,
    expanded: bool,
    selected: Option<&str>,
) -> Vec<usize> {
    (0..rows.len())
        .filter(|&index| {
            let row = &rows[index];
            row.section == section && (expanded || selected == Some(row.id.as_str()))
        })
        .collect()
}

struct SettledWindow {
    rows: Vec<usize>,
    more: usize,
}

/// The first `shown` settled rows plus the open thread when it falls outside them, and how many
/// the next "Show more" reveals.
fn settled_window(
    rows: &[usize],
    all: &[ThreadRow],
    shown: usize,
    selected: Option<&str>,
) -> SettledWindow {
    let hidden = rows.len().saturating_sub(shown);
    let open = rows
        .iter()
        .skip(shown)
        .copied()
        .find(|&index| selected == Some(all[index].id.as_str()));
    let mut window = rows[..rows.len().min(shown)].to_vec();
    window.extend(open);
    SettledWindow {
        rows: window,
        more: hidden.min(SETTLED_TAIL_PAGE_COUNT),
    }
}

/// The web's shelves, bottom-anchored: Working and Snoozed only when non-empty, Settled always.
fn shelves(rows: &[ThreadRow], expanded: [bool; 3]) -> Vec<Shelf> {
    if rows.is_empty() {
        return Vec::new();
    }
    [Section::Working, Section::Snoozed, Section::Settled]
        .into_iter()
        .filter_map(|section| {
            let count = rows.iter().filter(|row| row.section == section).count();
            (count > 0 || section == Section::Settled).then(|| Shelf {
                section,
                count,
                expanded: expanded[shelf_index(section)],
            })
        })
        .collect()
}

/// The web's `CollapsibleSectionHeader`: label left, then the count and a chevron right.
fn shelf_header(
    section: Section,
    count: usize,
    expanded: bool,
    muted: Hsla,
    hover: Hsla,
) -> gpui::Stateful<gpui::Div> {
    let label = match section {
        Section::Working => "Working",
        Section::Snoozed => "Snoozed",
        _ => "Settled",
    };
    let chevron = if expanded {
        IconName::ChevronDown
    } else {
        IconName::ChevronRight
    };
    h_flex()
        .id(label)
        .h(px(SHELF_HEADER_HEIGHT))
        .mx(px(2.))
        .px_2()
        .gap_2()
        .rounded_md()
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .text_color(muted)
        .cursor_pointer()
        .hover(|style| style.bg(hover))
        .child(div().flex_1().child(label))
        .child(
            div()
                .font_weight(FontWeight::NORMAL)
                .child(count.to_string()),
        )
        .child(Icon::new(chevron).size(px(12.)))
}

/// The web's `text-info-foreground`, `text-error-foreground` and `text-warning-foreground`.
fn top_status_color(status: TopStatus, dark: bool) -> Option<u32> {
    match (status, dark) {
        (TopStatus::Waiting, _) => None,
        (TopStatus::Working, true) => Some(0x60a5fa),
        (TopStatus::Working, false) => Some(0x1d4ed8),
        (TopStatus::Limited, true) => Some(0xfbbf24),
        (TopStatus::Limited, false) => Some(0xb45309),
        (TopStatus::Failed, true) => Some(0xf87171),
        (TopStatus::Failed, false) => Some(0xb91c1c),
    }
}

fn top_status_icon(status: TopStatus) -> Icons {
    match status {
        TopStatus::Working => Icons::CircleDashed,
        TopStatus::Waiting => Icons::CircleDotDashed,
        TopStatus::Limited | TopStatus::Failed => Icons::CircleAlert,
    }
}

/// The project avatar at the favicon's `size-4`: the monogram on its tinted square.
fn project_badge(entry: Option<&ProjectEntry>, dark: bool, mono: gpui::SharedString) -> gpui::Div {
    let badge = div().flex_shrink_0().size_4();
    let Some(entry) = entry else {
        return badge;
    };
    let color: Hsla = rgb(entry.color_hex(dark)).into();
    if entry.scratch {
        return badge.child(
            Icon::empty()
                .path(MESSAGE_SQUARE_DASHED)
                .size_4()
                .text_color(color),
        );
    }
    badge
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(4.))
        .bg(color.opacity(0.14))
        .text_color(color)
        .text_size(px(8.25))
        .font_weight(FontWeight::BOLD)
        .font_family(mono)
        .child(entry.initials.clone())
}

/// The web's `PULL_REQUEST_STATE_PRESENTATION` tones.
fn pr_color(state: PrState, dark: bool) -> Option<Hsla> {
    let (hex, alpha) = match (state, dark) {
        (PrState::Pending, _) => return None,
        (PrState::Open, false) => (0x059669, 1.),
        (PrState::Open, true) => (0x6ee7b7, 0.9),
        (PrState::Draft, false) => (0x71717a, 1.),
        (PrState::Draft, true) => (0xa1a1aa, 0.8),
        (PrState::Closed, false) => (0xdc2626, 1.),
        (PrState::Closed, true) => (0xfca5a5, 0.9),
        (PrState::Merged, false) => (0x7c3aed, 1.),
        (PrState::Merged, true) => (0xc4b5fd, 0.9),
    };
    Some(Hsla::from(rgb(hex)).opacity(alpha))
}

fn pr_icon(badge: &PrBadge) -> Icons {
    if badge.stack {
        Icons::Layers
    } else {
        pr_state_icon(badge.state)
    }
}

fn pr_state_icon(state: PrState) -> Icons {
    match state {
        PrState::Draft => Icons::GitPullRequestDraft,
        PrState::Closed => Icons::GitPullRequestClosed,
        PrState::Merged => Icons::GitMerge,
        PrState::Pending | PrState::Open => Icons::GitPullRequestArrow,
    }
}

/// The web's `MiddleTruncate` with a character budget in place of measured text.
fn middle_truncate(text: &str, budget: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= budget || budget < 3 {
        return text.to_owned();
    }
    let keep = budget - 1;
    let (head, tail) = (keep.div_ceil(2), keep / 2);
    let start: String = chars[..head].iter().collect();
    let end: String = chars[chars.len() - tail..].iter().collect();
    format!("{start}\u{2026}{end}")
}

/// How many `text-xs` characters fit the branch: the card's width less its padding, icons and
/// the pull request badge.
fn branch_budget(
    sidebar_width: f32,
    worktree: bool,
    pr: Option<&PrBadge>,
    provider: bool,
) -> usize {
    const CHAR_WIDTH: f32 = 6.4;
    const GAP: f32 = 6.;
    // Sidebar padding and the card's horizontal padding.
    let mut room = sidebar_width - 16. - 20.;
    if worktree {
        room -= 14. + GAP;
    }
    if provider {
        room -= 16. + GAP;
    }
    if let Some(pr) = pr {
        room -= 12. + 4. + pr.text.chars().count() as f32 * CHAR_WIDTH + GAP;
    }
    (room / CHAR_WIDTH).max(0.) as usize
}

fn card_action(
    id: &'static str,
    label: &'static str,
    muted: Hsla,
    foreground: Hsla,
) -> gpui::Stateful<gpui::Div> {
    h_flex()
        .id(id)
        .h_full()
        .gap_1()
        .px(px(6.))
        .rounded_md()
        .text_xs()
        .text_color(muted)
        .cursor_pointer()
        .hover(move |style| style.text_color(foreground))
        .tooltip(tooltip(label))
}

/// One `ThreadPullRequestMiniListItem`: state glyph, `#number`, title and the chain tag.
fn pr_line_row(
    line: &PrLine,
    mono: gpui::SharedString,
    muted: Hsla,
    foreground: Hsla,
    dark: bool,
) -> gpui::Div {
    let tone = pr_color(line.state, dark).unwrap_or(muted);
    h_flex()
        .min_w_0()
        .items_center()
        .gap_2()
        .pl(px(line.depth.min(3) as f32 * 12.))
        .child(
            Icon::new(pr_state_icon(line.state))
                .size(px(12.))
                .flex_shrink_0()
                .text_color(tone),
        )
        .child(
            div()
                .flex_shrink_0()
                .font_family(mono)
                .child(format!("#{}", line.number)),
        )
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_color(foreground.opacity(0.75))
                .child(line.title.clone()),
        )
        .children(line.stack.map(|(kind, size)| {
            let word = match kind {
                ChainKind::Native => "stack",
                ChainKind::Derived => "chain",
            };
            div()
                .ml_auto()
                .flex_shrink_0()
                .pl_1()
                .text_size(px(10.))
                .child(format!("{word} \u{b7} {size}"))
        }))
}

/// What the badge popup lists: the thread's links, else the one pull request the badge names.
fn badge_lines(badge: &PrBadge, lines: &[PrLine]) -> Vec<PrLine> {
    if !lines.is_empty() {
        return lines.to_vec();
    }
    match (&badge.url, badge.text.parse::<u64>()) {
        (Some(url), Ok(number)) => vec![PrLine {
            number,
            state: badge.state,
            title: format!("Pull request #{number}"),
            depth: 0,
            url: url.clone(),
            host: String::new(),
            repository: badge.repository.clone().unwrap_or_default(),
            stack: None,
        }],
        _ => Vec::new(),
    }
}

/// The web's `w-80` and the height of one `px-1 py-1` item at `text-xs`.
const POPUP_WIDTH: f32 = 320.;
const POPUP_ITEM_HEIGHT: f32 = 24.;

/// The popup's height: the list's rows, capped at the room above the badge.
fn popup_height(lines: usize, room: f32) -> f32 {
    let rows = lines as f32 * POPUP_ITEM_HEIGHT + lines.saturating_sub(1) as f32 * 4.;
    // The popup's own `p-1`, the border and the list's `-mx-1`.
    (rows + 8. + 2.).min((room - 10.).max(POPUP_ITEM_HEIGHT))
}

/// GPUI places a tooltip at the pointer, so the popup moves itself to sit centred on top of the
/// badge with no gap, which keeps the pointer inside the trigger or the popup while it travels
/// (`side="top" sideOffset={0}`).
fn popup_offset(
    anchor: Bounds<Pixels>,
    mouse: gpui::Point<Pixels>,
    height: f32,
) -> gpui::Point<Pixels> {
    let left = (anchor.center().x - px(POPUP_WIDTH / 2.)).max(px(8.));
    gpui::point(
        left - (mouse.x + px(1.)),
        anchor.top() - px(height) - (mouse.y + px(1.)),
    )
}

/// The web's `PullRequestBadge` tooltip popup: a mini list whose items open their pull request.
struct PrPopup {
    lines: Vec<PrLine>,
    open: OpenPullRequest,
    place: gpui::Point<Pixels>,
    height: f32,
}

impl Render for PrPopup {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (muted, foreground, accent) = (theme.muted_foreground, theme.foreground, theme.accent);
        let (dark, mono) = (theme.is_dark(), theme.mono_font_family.clone());
        let items = self.lines.iter().enumerate().map(|(index, line)| {
            let (line_to_open, open) = (line.clone(), self.open.clone());
            pr_line_row(line, mono.clone(), muted, foreground, dark)
                .id(("pr-popup-item", index))
                .px_1()
                .py_1()
                .rounded_sm()
                .cursor_pointer()
                .hover(move |style| style.bg(accent))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    open(&line_to_open, cx);
                })
        });
        div().relative().left(self.place.x).top(self.place.y).child(
            div()
                .id("pr-popup")
                .w(px(POPUP_WIDTH))
                .max_h(px(self.height))
                .p_1()
                .rounded_lg()
                .border_1()
                .border_color(theme.border)
                .bg(theme.popover)
                .shadow_lg()
                .overflow_y_scroll()
                .text_xs()
                .text_color(muted)
                .child(v_flex().gap_1().children(items)),
        )
    }
}

/// Opens one popup item's pull request in the right panel.
type OpenPullRequest = Rc<dyn Fn(&PrLine, &mut App)>;

/// The badge in a row's meta line: a link for one pull request, else it opens the list. Hovering
/// shows the popup. The web opens the right panel's pull requests view for a list; GPUI has
/// none, so it activates the thread and the popup stays the list.
fn pr_badge_control(
    row: &ThreadRow,
    badge: &PrBadge,
    muted: Hsla,
    dark: bool,
    on_list: impl Fn(&mut App) + 'static,
    open: OpenPullRequest,
) -> gpui::Stateful<gpui::Div> {
    let tone = pr_color(badge.state, dark).unwrap_or(muted);
    let anchor = Rc::new(Cell::new(Bounds::default()));
    let lines = badge_lines(badge, &row.pr_lines);
    let popup_anchor = anchor.clone();
    let url = badge.url.clone().filter(|_| !badge.opens_list);
    let control = h_flex()
        .id(gpui::SharedString::from(format!("pr-badge-{}", row.id)))
        .relative()
        .flex_shrink_0()
        .gap_1()
        .cursor_pointer()
        .child(
            canvas(move |bounds, _, _| anchor.set(bounds), |_, _, _, _| {})
                .absolute()
                .size_full(),
        )
        .child(Icon::new(pr_icon(badge)).size(px(12.)).text_color(tone))
        .child(div().text_color(muted).child(badge.text.clone()))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            match &url {
                Some(url) => cx.open_url(url),
                None => on_list(cx),
            }
        });
    if lines.is_empty() {
        return control;
    }
    control.hoverable_tooltip(move |window, cx| {
        let room = f32::from(popup_anchor.get().top());
        let height = popup_height(lines.len(), room);
        let place = popup_offset(popup_anchor.get(), window.mouse_position(), height);
        let lines = lines.clone();
        let open = open.clone();
        cx.new(|_| PrPopup {
            lines,
            open,
            place,
            height,
        })
        .into()
    })
}

/// What `SidebarThreadTooltip` shows for one thread, resolved when the row is built.
#[derive(Clone)]
struct ThreadCard {
    title: String,
    project: Option<(Option<ProjectEntry>, String)>,
    environment: Option<(String, MachineKind)>,
    branch: Option<String>,
    provider: Option<CardProvider>,
    handed_off_from: Vec<String>,
    last_error: Option<LastError>,
    pr_lines: Vec<PrLine>,
}

#[derive(Clone)]
struct CardProvider {
    /// Without the chip unless the instance has an accent color, like `showBadge` on the web.
    mark: Mark,
    label: String,
}

impl ThreadCard {
    fn new(
        row: &ThreadRow,
        project: Option<&ProjectEntry>,
        provider: Option<&Mark>,
        catalog: Option<&Catalog>,
        environment: Option<(String, MachineKind)>,
    ) -> Self {
        let name = project_name(row, project);
        let model = catalog.map(|catalog| catalog.model_label(&row.provider_instance, &row.model));
        let provider = provider.zip(model).map(|(mark, model)| CardProvider {
            label: match &mark.badge {
                Some(_) => format!("{model} \u{b7} {}", mark.display_name),
                None => model,
            },
            mark: Mark {
                badge: mark.badge.clone().filter(|badge| badge.accent.is_some()),
                ..mark.clone()
            },
        });
        Self {
            title: row.title.clone(),
            project: (!name.is_empty()).then(|| (project.cloned(), name.to_owned())),
            environment,
            branch: row.branch.clone().filter(|branch| !branch.is_empty()),
            provider,
            handed_off_from: thread_card::previous_providers(
                &row.instance_history,
                &row.provider_instance,
                |id| {
                    catalog
                        .and_then(|catalog| catalog.instance(id))
                        .map(|mark| mark.display_name)
                },
            ),
            last_error: row.last_error,
            pr_lines: row.pr_lines.clone(),
        }
    }
}

/// GPUI places a tooltip at the pointer, so the card shifts itself to the trigger's right edge
/// (`side="right" align="start" sideOffset={4}`).
fn card_offset(anchor: Bounds<Pixels>, mouse: gpui::Point<Pixels>) -> gpui::Point<Pixels> {
    gpui::point(
        anchor.right() + px(4.) - (mouse.x + px(1.)),
        anchor.top() - (mouse.y + px(1.)),
    )
}

/// The web's `ThreadHoverCard` in a `ThreadHoverCardPopup`.
struct ThreadHoverCard {
    card: ThreadCard,
    place: gpui::Point<Pixels>,
}

impl Render for ThreadHoverCard {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let dark = theme.is_dark();
        let (muted, foreground) = (theme.muted_foreground, theme.foreground);
        let text = foreground.opacity(0.75);
        let card = &self.card;
        let row = |icon: AnyElement, label: AnyElement| {
            h_flex()
                .min_w_0()
                .items_center()
                .gap_2()
                .child(icon)
                .child(label)
        };
        let label = |text_color: Hsla, content: String| {
            div()
                .min_w_0()
                .truncate()
                .text_color(text_color)
                .child(content)
                .into_any_element()
        };
        let glyph = |icon: Icons, color: Hsla| {
            Icon::new(icon)
                .size(px(12.))
                .flex_shrink_0()
                .text_color(color)
                .into_any_element()
        };
        let mut rows = v_flex().gap(px(6.)).pl(px(2.)).text_xs().text_color(muted);
        if let Some((entry, name)) = &card.project {
            rows = rows.child(row(
                project_badge(entry.as_ref(), dark, theme.mono_font_family.clone())
                    .size(px(12.))
                    .into_any_element(),
                label(text, name.clone()),
            ));
        }
        if let Some((name, machine)) = &card.environment {
            rows = rows.child(row(
                glyph(machine_icon(*machine), muted),
                label(text, name.clone()),
            ));
        }
        if let Some(branch) = &card.branch {
            rows = rows.child(row(
                glyph(Icons::GitBranch, muted),
                label(text, middle_truncate(branch, 40)),
            ));
        }
        if let Some(provider) = &card.provider {
            rows = rows.child(row(
                provider_mark(&provider.mark, 0.6, cx).into_any_element(),
                label(text, provider.label.clone()),
            ));
        }
        if !card.handed_off_from.is_empty() {
            rows = rows.child(row(
                glyph(Icons::ArrowRightLeft, muted),
                label(
                    text,
                    format!("Handed off from {}", card.handed_off_from.join(", ")),
                ),
            ));
        }
        if let Some(error) = card.last_error {
            let (color, message) = match error {
                LastError::UsageLimit => (
                    rgb(if dark { 0xfbbf24 } else { 0xb45309 }),
                    "Usage limit reached",
                ),
                LastError::Other => (
                    rgb(if dark { 0xf87171 } else { 0xdc2626 }),
                    "Error occurred",
                ),
            };
            let color: Hsla = color.into();
            rows = rows.child(row(
                glyph(Icons::CircleAlert, color),
                label(color, message.to_owned()),
            ));
        }
        let footer = (!card.pr_lines.is_empty()).then(|| {
            v_flex()
                .gap_1()
                .pt_2()
                .pl(px(2.))
                .border_t_1()
                .border_color(theme.border.opacity(0.6))
                .text_xs()
                .text_color(muted)
                .children(card.pr_lines.iter().map(|line| {
                    pr_line_row(
                        line,
                        theme.mono_font_family.clone(),
                        muted,
                        foreground,
                        dark,
                    )
                }))
        });
        div().relative().left(self.place.x).top(self.place.y).child(
            v_flex()
                .max_w(px(320.))
                .gap_2()
                .px(px(10.))
                .py(px(8.))
                .rounded_lg()
                .border_1()
                .border_color(theme.border)
                .bg(theme.popover)
                .shadow_lg()
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(foreground)
                        .child(card.title.clone()),
                )
                .child(rows)
                .children(footer),
        )
    }
}

/// The group's label, else the project's own title.
fn project_name<'a>(row: &'a ThreadRow, entry: Option<&'a ProjectEntry>) -> &'a str {
    entry.map_or(row.project.as_str(), |entry| entry.title.as_str())
}

/// What a row takes from its owner: the hover card's anchor and the interactive PR badge.
struct RowParts {
    anchor: Rc<Cell<Bounds<Pixels>>>,
    pr_control: Option<gpui::Stateful<gpui::Div>>,
}

/// The web's card `SidebarThreadRow`. Snooze and Settle appear on hover, as in the web, but do
/// nothing yet.
fn thread_row(
    row: &ThreadRow,
    project: Option<&ProjectEntry>,
    provider: Option<Mark>,
    selected: bool,
    sidebar_width: f32,
    parts: RowParts,
    cx: &App,
) -> gpui::Stateful<gpui::Div> {
    let RowParts { anchor, pr_control } = parts;
    let theme = cx.theme();
    let dark = theme.is_dark();
    let (muted, foreground) = (theme.muted_foreground, theme.foreground);
    let group = gpui::SharedString::from(format!("thread-{}", row.id));
    let name = project_name(row, project);
    let status = row.top.map(|top| {
        let color = top_status_color(top, dark).map_or(muted, |hex| rgb(hex).into());
        h_flex()
            .gap_1()
            .font_weight(FontWeight::MEDIUM)
            .text_color(color)
            .group_hover(group.clone(), |style| style.invisible())
            .child(Icon::new(top_status_icon(top)).size_4())
            .child(top.label())
    });
    let actions = h_flex()
        .absolute()
        .top_0()
        .right_0()
        .h_full()
        .invisible()
        .group_hover(group.clone(), |style| style.visible())
        .child(
            card_action("snooze", "Snooze thread", muted, foreground)
                .child(Icon::new(Icons::Clock).size(px(12.))),
        )
        .child(
            card_action("settle", "Settle thread", muted, foreground)
                .child(Icon::new(Icons::Check).size(px(14.)))
                .child("Settle"),
        );
    let provider = provider.map(|mark| provider_mark(&mark, 0.6, cx));
    div()
        .id(row.id.clone())
        .group(group)
        .h(px(ROW_HEIGHT))
        .w_full()
        .py(px(2.))
        .cursor_pointer()
        .child(
            v_flex()
                .h(px(CARD_HEIGHT))
                .w_full()
                .overflow_hidden()
                .px(px(10.))
                .py_2()
                .rounded_md()
                .when(selected, |card| card.bg(theme.list_active))
                .when(!selected, |card| {
                    card.hover(|style| style.bg(theme.list_hover))
                })
                // Where the hover card anchors: the web's popup sits at this card's right edge.
                .child(
                    canvas(move |bounds, _, _| anchor.set(bounds), |_, _, _, _| {})
                        .absolute()
                        .size_full(),
                )
                .child(
                    h_flex()
                        .h(px(20.))
                        .flex_shrink_0()
                        .items_center()
                        .gap(px(6.))
                        .child(project_badge(project, dark, theme.mono_font_family.clone()))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_xs()
                                .text_color(muted)
                                .child(name.to_owned()),
                        )
                        .child(
                            div()
                                .relative()
                                .flex()
                                .flex_shrink_0()
                                .items_center()
                                .justify_end()
                                .h_full()
                                .min_w_8()
                                .text_xs()
                                .children(status)
                                .child(actions),
                        ),
                )
                .child(
                    div()
                        .mt_1()
                        .h(px(20.))
                        .flex_shrink_0()
                        .truncate()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(foreground)
                        .child(row.title.clone()),
                )
                .child(
                    h_flex()
                        .mt(px(2.))
                        .h(px(16.))
                        .flex_shrink_0()
                        .items_center()
                        .gap(px(6.))
                        .text_xs()
                        .text_color(muted)
                        .when(row.worktree, |line| {
                            line.child(Icon::new(Icons::FolderGit2).size(px(14.)))
                        })
                        .child(div().flex_1().min_w_0().truncate().children(
                            row.branch.as_deref().map(|branch| {
                                middle_truncate(
                                    branch,
                                    branch_budget(
                                        sidebar_width,
                                        row.worktree,
                                        row.pr.as_ref(),
                                        provider.is_some(),
                                    ),
                                )
                            }),
                        ))
                        .children(pr_control)
                        .children(provider),
                ),
        )
}

impl Render for MainView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if std::mem::take(&mut self.clear_composer) {
            self.composer
                .update(cx, |composer, cx| composer.set_value("", window, cx));
        }
        let width = f32::from(window.viewport_size().width);
        let narrow = is_narrow(width);
        let docked = self.sidebar_open && !narrow;
        let sidebar_width = clamp_sidebar_width(self.sidebar_width, self.brand_width.get(), width);
        let overlay = narrow && self.sidebar_overlay_open;
        let container = width - if docked { sidebar_width } else { 0. };
        let panel = self.right_panel(width, container, cx);
        self.main_width.set(container - panel.docked_width);
        v_flex()
            .relative()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .when(overlay, |root| root.key_context(OVERLAY_CONTEXT))
            .on_action(cx.listener(|this, _: &ToggleSidebar, window, cx| {
                this.toggle_sidebar(narrow_window(window), cx)
            }))
            .on_action(
                cx.listener(|this, _: &DismissSidebar, _, cx| this.close_sidebar_overlay(cx)),
            )
            .on_action(cx.listener(|this, _: &OpenPermissions, _, cx| this.open_permissions(cx)))
            .on_action(
                cx.listener(|this, _: &OpenWorkspace, window, cx| this.open_workspace(window, cx)),
            )
            .on_action(cx.listener(|this, _: &UsePreviousWorktree, window, cx| {
                this.use_previous_worktree(window, cx)
            }))
            .on_action(cx.listener(|this, _: &ToggleDetails, _, cx| this.toggle_details(cx)))
            .on_action(cx.listener(|this, _: &OpenUsage, window, cx| {
                this.usage.update(cx, |usage, cx| usage.open(window, cx));
            }))
            .on_action(cx.listener(|this, _: &OpenSettings, window, cx| {
                this.settings
                    .update(cx, |settings, cx| settings.open(window, cx));
            }))
            .on_action(cx.listener(|this, _: &ToggleRightPanel, _, cx| this.toggle_right_panel(cx)))
            .on_action(
                cx.listener(|this, _: &ToggleTerminal, window, cx| {
                    this.toggle_terminal(window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &SplitTerminal, window, cx| {
                this.add_terminal(Placement::Split { vertical: false }, window, cx)
            }))
            .on_action(cx.listener(|this, _: &SplitTerminalVertical, window, cx| {
                this.add_terminal(Placement::Split { vertical: true }, window, cx)
            }))
            .on_action(cx.listener(|this, _: &NewTerminal, window, cx| {
                this.add_terminal(Placement::New, window, cx)
            }))
            .on_action(cx.listener(|this, _: &CloseTerminal, window, cx| {
                let active = this.active_terminal().map(|(_, t)| t.active.clone());
                if let Some(active) = active {
                    this.request_close(active, window, cx)
                }
            }))
            .on_action(
                cx.listener(|this, _: &OpenFavoriteEditor, _, cx| this.open_favorite_editor(cx)),
            )
            .on_action(cx.listener(|this, _: &NewThread, _, cx| this.new_thread(cx)))
            .on_action(cx.listener(|this, _: &NewScratchThread, _, cx| this.new_scratch_thread(cx)))
            .on_action(cx.listener(|this, _: &GoBack, _, cx| this.go_back(cx)))
            .on_action(cx.listener(|this, _: &GoForward, _, cx| this.go_forward(cx)))
            .when(docked, |root| {
                root.child(self.sidebar_surface(sidebar_width, cx))
            })
            .child(self.top_bar(narrow, sidebar_width, cx))
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .when(docked, |row| {
                        row.child(self.sidebar(sidebar_width, window, cx))
                    })
                    .child(self.main_area(f32::from(window.viewport_size().height), cx))
                    .children(panel.column),
            )
            .children(panel.rail)
            .children(panel.sheet)
            .when(docked, |root| {
                root.child(self.sidebar_rail(sidebar_width, cx))
            })
            .when(overlay, |root| {
                root.child(self.sidebar_drawer(overlay_width(width), window, cx))
            })
            .child(self.usage.clone())
            .child(self.add_to_machine.clone())
            .child(self.settings.clone())
            .child(self.toasts.clone())
    }
}

/// `dropdown-glass`'s `1px solid` edge: `--contrast-foreground` at 10%. GPUI has no backdrop blur,
/// so the fill takes the web's no-blur fallback, a solid `--popover` (index.css `@supports not`).
fn dropdown_glass_edge(cx: &App) -> Hsla {
    if cx.theme().is_dark() {
        gpui::white().opacity(0.1)
    } else {
        gpui::black().opacity(0.1)
    }
}

/// The web's `--chat-composer-outline`: white at 5% in dark, black at 8% in light.
fn composer_outline(dark: bool) -> Hsla {
    if dark {
        gpui::white().opacity(0.05)
    } else {
        gpui::black().opacity(0.08)
    }
}

/// The web's `--chat-composer-glass-surface` (`--surface-raised` in dark, neutral-950 mixed 97% with
/// white; `--card` in light), painted opaque as the web does where backdrop blur is unsupported,
/// since GPUI has no per-element backdrop blur and 80% over the timeline leaves text legible.
fn composer_surface(dark: bool) -> Hsla {
    if dark {
        gpui::rgb(0x111111).into()
    } else {
        gpui::white()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sidebar_overlays_at_the_webs_breakpoint() {
        assert!(is_narrow(767.));
        assert!(!is_narrow(768.));
        assert_eq!(overlay_width(583.), 448.);
        assert_eq!(overlay_width(400.), 352.);
    }

    #[test]
    fn the_sidebar_width_stays_between_the_minimum_and_what_the_content_leaves() {
        assert_eq!(clamp_sidebar_width(100., 0., 1200.), 208.);
        assert_eq!(clamp_sidebar_width(300., 0., 1200.), 300.);
        assert_eq!(clamp_sidebar_width(900., 0., 1200.), 560.);
        assert_eq!(clamp_sidebar_width(300., 0., 800.), 208.);
        assert_eq!(clamp_sidebar_width(200., 251.4, 1200.), 252.);
        assert_eq!(clamp_sidebar_width(300., 251.4, 800.), 252.);
    }

    #[test]
    fn toggling_flips_only_the_state_for_the_viewport() {
        assert_eq!(toggled(true, true, false), (true, true));
        assert_eq!(toggled(true, false, true), (false, false));
        assert_eq!(toggled(false, true, false), (false, false));
        assert_eq!(toggled(false, true, true), (false, true));
    }

    fn entry(id: &str, scratch: bool) -> ProjectEntry {
        ProjectEntry {
            key: id.to_owned(),
            id: id.to_owned(),
            member_ids: vec![id.to_owned()],
            title: id.to_owned(),
            initials: "XX".to_owned(),
            color: 0,
            pinned: false,
            scratch,
            root: String::new(),
            logical_key: None,
            remote: Vec::new(),
            repository: None,
        }
    }

    fn row(project_id: &str) -> ThreadRow {
        ThreadRow {
            id: project_id.to_owned(),
            project_id: project_id.to_owned(),
            project: project_id.to_owned(),
            title: String::new(),
            branch: None,
            worktree: false,
            provider_instance: String::new(),
            top: None,
            pr: None,
            section: Section::Active,
            model: String::new(),
            instance_history: Vec::new(),
            pr_lines: Vec::new(),
            last_error: None,
            worktree_path: None,
            has_run: false,
            updated_at_ms: None,
        }
    }

    #[test]
    fn the_card_names_its_project_from_the_group_or_the_row() {
        let mut thread = row("t");
        thread.project = "own".to_owned();
        assert_eq!(project_name(&thread, Some(&entry("group", false))), "group");
        assert_eq!(project_name(&thread, None), "own");
    }

    #[test]
    fn long_branches_lose_their_middle() {
        assert_eq!(middle_truncate("main", 10), "main");
        assert_eq!(
            middle_truncate("yordis/t3/token-security-comprison", 12),
            "yordis\u{2026}rison"
        );
        assert_eq!(middle_truncate("abcdefgh", 2), "abcdefgh");
        let badge = PrBadge {
            state: PrState::Open,
            stack: false,
            text: "19".to_owned(),
            url: None,
            opens_list: false,
            repository: None,
        };
        let plain = branch_budget(300., true, None, true);
        assert!(branch_budget(300., true, Some(&badge), true) < plain);
        assert!(branch_budget(448., true, None, true) > plain);
    }

    #[test]
    fn shelves_list_only_what_the_web_does() {
        let in_section = |id: &str, section| ThreadRow { section, ..row(id) };
        let rows = vec![
            in_section("p", Section::Pinned),
            in_section("a", Section::Active),
            in_section("w", Section::Working),
            in_section("s", Section::Settled),
        ];
        let shelf = |section, expanded| Shelf {
            section,
            count: 1,
            expanded,
        };
        assert_eq!(top_rows(&rows), [0, 1]);
        assert_eq!(
            shelves(&rows, [false; 3]),
            [
                shelf(Section::Working, false),
                shelf(Section::Settled, false)
            ]
        );
        assert_eq!(
            shelves(&rows, [true, false, true]),
            [shelf(Section::Working, true), shelf(Section::Settled, true)]
        );
        assert_eq!(shelf_rows(&rows, Section::Settled, true, None), [3]);
        assert!(shelf_rows(&rows, Section::Settled, false, None).is_empty());
        assert!(shelf_rows(&rows, Section::Settled, false, Some("w")).is_empty());
        assert_eq!(shelf_rows(&rows, Section::Settled, false, Some("s")), [3]);
        assert!(shelves(&[], [true; 3]).is_empty());

        let settled: Vec<ThreadRow> = (0..40)
            .map(|i| in_section(&format!("s{i}"), Section::Settled))
            .collect();
        let all = shelf_rows(&settled, Section::Settled, true, None);
        let first = settled_window(&all, &settled, 10, None);
        assert_eq!((first.rows.len(), first.more), (10, 25));
        let open = settled_window(&all, &settled, 10, Some("s30"));
        assert_eq!((open.rows.len(), open.rows[10], open.more), (11, 30, 25));
        let last = settled_window(&all, &settled, 35, None);
        assert_eq!((last.rows.len(), last.more), (35, 5));
        assert_eq!(settled_window(&all, &settled, 40, None).more, 0);
    }

    #[test]
    fn scratch_reads_as_no_project_and_is_found_in_the_entries() {
        let mut none = entry("none", true);
        none.title = "Whatever the server titled it".to_owned();
        assert_eq!(project_label(Some(&none)), "No project");
        assert_eq!(project_label(Some(&entry("alpha", false))), "alpha");
        assert_eq!(project_label(None), "");
        let entries = [entry("alpha", false), none];
        assert_eq!(scratch_entry(&entries).map(|e| e.id.as_str()), Some("none"));
        assert!(scratch_entry(&entries[..1]).is_none());
        assert_eq!(
            ProjectTrigger::new("alpha", true, Rc::default()).name,
            "No project"
        );
        assert_eq!(
            ProjectTrigger::new("alpha", false, Rc::default()).name,
            "alpha"
        );
    }

    #[test]
    fn active_project_prefers_a_live_pick_then_the_newest_real_project() {
        let shell = ShellView {
            project_entries: vec![
                entry("alpha", false),
                entry("none", true),
                entry("beta", false),
            ],
            rows: vec![row("none"), row("gone"), row("beta")],
            ..ShellView::default()
        };
        let id = |selected| active_project(selected, &shell).map(|e| e.id.as_str());
        assert_eq!(id(Some("alpha")), Some("alpha"));
        assert_eq!(id(Some("gone")), Some("beta"));
        assert_eq!(id(None), Some("beta"));
        let empty = ShellView {
            project_entries: vec![entry("none", true), entry("alpha", false)],
            ..ShellView::default()
        };
        assert_eq!(
            active_project(None, &empty).map(|e| e.id.as_str()),
            Some("alpha")
        );
    }
}
