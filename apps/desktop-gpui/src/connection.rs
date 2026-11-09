//! The authenticated RPC WebSocket: connect, keep alive, reconnect.

use crate::auth::{self, AuthError, Credential};
use crate::details::VcsStatus;
use crate::diff;
use crate::projects::MachineKind;
use crate::providers::Catalog;
use crate::rpc::{
    AttachmentsCreateUploadUrl, AttachmentsDelete, DeviceConfigure, DeviceEasAccount, DeviceList,
    DeviceTestHost, Event, GitRunStackedAction, OrchestrationDispatchCommand,
    OrchestrationLaunchThread, PING_FRAME, ProjectsMutate, ProviderConsumeResetCredit,
    ReviewGetDiffPreview, ServerCheckSecretSource, ServerDiscoverSourceControl, ServerGetConfig,
    ServerGetUsageSummary, ServerRefreshProviders, ServerRefreshUsageRates, ServerUpdateSettings,
    Session, ShellOpenInEditor, SubscribeShell, SubscribeThread, SubscribeVcsStatus,
    TerminalAttach, TerminalClose, TerminalResize, TerminalWrite, VcsCreateRef, VcsListRefs,
    VcsPull, VcsRefreshStatus, VcsSwitchRef,
};
use crate::rpc::{
    ProjectCloneStart, ProjectsListIgnoredFiles, ProjectsReadFile, ProjectsWriteFile,
    SubscribeProjectClones,
};
use crate::shell::ShellState;
use crate::status::{CallEvent, Status, StatusSender, publish};
use crate::strip::{self, Branch};
use crate::terminal::{self, Grid, Target};
use crate::thread::ThreadState;
use crate::upload::Uploader;
use crate::wire::{
    ProjectMutation, ProjectMutationCreateType, ProjectMutationType, ShellItem,
    SubscribeShellPayload, SubscribeThreadPayload,
};
use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use tokio::time::{Instant, MissedTickBehavior, interval, sleep, sleep_until};
use tokio_tungstenite::tungstenite::{Message, protocol::WebSocketConfig};

const PING_INTERVAL: Duration = Duration::from_secs(5);
// Pings go out every 5 s and a busy server stops answering them, so allow 15 s
// without any frame (packages/client-runtime/src/rpc/protocol.ts PING_TIMEOUT).
const PING_TIMEOUT: Duration = Duration::from_secs(15);
const ORCHESTRATION_PROTOCOL_VERSION: &str = "2";

/// What the connection needs to know about the server it dialed.
pub struct Environment {
    pub id: String,
    pub label: String,
    /// The machine icon the server's descriptor reports (`platform.machine`).
    pub machine: MachineKind,
    /// The server keeps `pinned` from `project.update` (`capabilities.projectOrganization`).
    pub project_organization: bool,
}

/// A request from the window that the connection sends as an RPC.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    SetProjectPinned {
        project_id: String,
        pinned: bool,
    },
    /// Registers a folder as a project; the outcome comes back as `Status::ProjectAdded` or
    /// `Status::ProjectFailed`.
    AddProject {
        project_id: String,
        title: String,
        workspace_root: String,
    },
    /// The branch checked out in a project's workspace root, for the strip under the composer.
    LoadBranch {
        root: String,
    },
    /// The thread whose transcript the window shows; `None` closes the subscription.
    SelectThread(Option<String>),
    /// The directory whose version control status the details card shows; `None` stops watching.
    WatchVcs(Option<String>),
    /// The working-tree diff of `cwd`, for the right panel's Diff surface.
    LoadDiff {
        cwd: String,
    },
    /// Launches `editor` (an `EditorId`) on the server's machine at `cwd`.
    OpenInEditor {
        cwd: String,
        editor: String,
    },
    /// The terminals the drawer shows; any not listed detach, and their shells keep running.
    AttachTerminals(Vec<Target>),
    TerminalWrite {
        thread_id: String,
        terminal_id: String,
        data: String,
    },
    TerminalResize {
        thread_id: String,
        terminal_id: String,
        grid: Grid,
    },
    /// Ends a shell and its history.
    TerminalClose {
        thread_id: String,
        terminal_id: String,
    },
    /// Posts a file to the signed URL `attachments.createUploadUrl` minted; progress and the
    /// outcome come back as `Status::Call` with `token`.
    Upload {
        token: u64,
        relative_url: String,
        path: std::path::PathBuf,
        mime: String,
    },
    /// A request the details card makes; the outcome comes back as `Status::Call` with `token`.
    Call {
        token: u64,
        call: Call,
        payload: serde_json::Value,
    },
    /// Stops a streaming `Call`; nothing more arrives under `token`.
    EndCall {
        token: u64,
    },
}

/// The RPCs an `Action::Call` can name, all with untyped payloads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Call {
    ListRefs,
    SwitchRef,
    CreateRef,
    Pull,
    RefreshStatus,
    LaunchThread,
    DispatchCommand,
    UpdateSettings,
    /// A stream: progress events, then the result.
    RunStackedAction,
    CreateUploadUrl,
    DeleteAttachment,
    GetUsageSummary,
    RefreshUsageRates,
    RefreshProviders,
    ConsumeResetCredit,
    DeviceList,
    DeviceConfigure,
    DeviceTestHost,
    DeviceEasAccount,
    DiscoverSourceControl,
    CheckSecretSource,
    GetConfig,
    ListIgnoredFiles,
    ReadFile,
    WriteFile,
    StartProjectClone,
    /// A stream of the clone list that never ends by itself.
    WatchProjectClones,
}

pub type ActionSender = async_channel::Sender<Action>;
pub type ActionReceiver = async_channel::Receiver<Action>;

impl Action {
    fn describe(&self) -> &'static str {
        match self {
            Self::SetProjectPinned { pinned: true, .. } => "pin project",
            Self::SetProjectPinned { pinned: false, .. } => "unpin project",
            Self::AddProject { .. } => "add the project",
            Self::LoadBranch { .. } => "load the branch",
            Self::SelectThread(_) => "open the thread",
            Self::WatchVcs(_) => "watch the version control status",
            Self::LoadDiff { .. } => "load the diff",
            Self::OpenInEditor { .. } => "open in the editor",
            Self::AttachTerminals(_) => "attach the terminals",
            Self::TerminalWrite { .. } => "write to the terminal",
            Self::TerminalResize { .. } => "resize the terminal",
            Self::TerminalClose { .. } => "close the terminal",
            Self::Upload { .. } => "upload the attachment",
            Self::Call { call, .. } => call.describe(),
            Self::EndCall { .. } => "end the request",
        }
    }
}

impl Call {
    fn describe(self) -> &'static str {
        match self {
            Self::ListRefs => "list the refs",
            Self::SwitchRef => "switch the ref",
            Self::CreateRef => "create the ref",
            Self::Pull => "pull",
            Self::RefreshStatus => "refresh the status",
            Self::LaunchThread => "start the thread",
            Self::DispatchCommand => "send the command",
            Self::UpdateSettings => "update the settings",
            Self::RunStackedAction => "run the action",
            Self::CreateUploadUrl => "prepare the upload",
            Self::DeleteAttachment => "delete the attachment",
            Self::GetUsageSummary => "load usage",
            Self::RefreshUsageRates => "refresh usage rates",
            Self::RefreshProviders => "refresh the provider limits",
            Self::ConsumeResetCredit => "use the reset credit",
            Self::DeviceList => "load the devices",
            Self::DeviceConfigure => "update the device settings",
            Self::DeviceTestHost => "test the connection",
            Self::DeviceEasAccount => "check the Expo account",
            Self::DiscoverSourceControl => "scan the server environment",
            Self::CheckSecretSource => "check the secret",
            Self::GetConfig => "load the server settings",
            Self::ListIgnoredFiles => "list the ignored files",
            Self::ReadFile => "read the file",
            Self::WriteFile => "write the file",
            Self::StartProjectClone => "start the clone",
            Self::WatchProjectClones => "watch the clones",
        }
    }

    fn request(
        self,
        session: &mut Session,
        payload: &serde_json::Value,
    ) -> Result<(String, String), String> {
        match self {
            Self::ListRefs => session.request::<VcsListRefs>(payload),
            Self::SwitchRef => session.request::<VcsSwitchRef>(payload),
            Self::CreateRef => session.request::<VcsCreateRef>(payload),
            Self::Pull => session.request::<VcsPull>(payload),
            Self::RefreshStatus => session.request::<VcsRefreshStatus>(payload),
            Self::LaunchThread => session.request::<OrchestrationLaunchThread>(payload),
            Self::DispatchCommand => session.request::<OrchestrationDispatchCommand>(payload),
            Self::UpdateSettings => session.request::<ServerUpdateSettings>(payload),
            Self::RunStackedAction => session.request::<GitRunStackedAction>(payload),
            Self::CreateUploadUrl => session.request::<AttachmentsCreateUploadUrl>(payload),
            Self::DeleteAttachment => session.request::<AttachmentsDelete>(payload),
            Self::GetUsageSummary => session.request::<ServerGetUsageSummary>(payload),
            Self::RefreshUsageRates => session.request::<ServerRefreshUsageRates>(payload),
            Self::RefreshProviders => session.request::<ServerRefreshProviders>(payload),
            Self::ConsumeResetCredit => session.request::<ProviderConsumeResetCredit>(payload),
            Self::DeviceList => session.request::<DeviceList>(payload),
            Self::DeviceConfigure => session.request::<DeviceConfigure>(payload),
            Self::DeviceTestHost => session.request::<DeviceTestHost>(payload),
            Self::DeviceEasAccount => session.request::<DeviceEasAccount>(payload),
            Self::DiscoverSourceControl => session.request::<ServerDiscoverSourceControl>(payload),
            Self::CheckSecretSource => session.request::<ServerCheckSecretSource>(payload),
            Self::GetConfig => session.request::<ServerGetConfig>(payload),
            Self::ListIgnoredFiles => session.request::<ProjectsListIgnoredFiles>(payload),
            Self::ReadFile => session.request::<ProjectsReadFile>(payload),
            Self::WriteFile => session.request::<ProjectsWriteFile>(payload),
            Self::StartProjectClone => session.request::<ProjectCloneStart>(payload),
            Self::WatchProjectClones => session.request::<SubscribeProjectClones>(payload),
        }
    }
}

fn pin_mutation(project_id: &str, pinned: bool, command_id: String) -> ProjectMutation {
    ProjectMutation::Variant1 {
        archived: None,
        auto_pull: None,
        command_id,
        default_model_selection: None,
        default_thread_env_mode: None,
        favicon_path: None,
        pinned: Some(pinned),
        project_icon: None,
        project_id: project_id.to_owned(),
        scripts: None,
        title: None,
        type_: ProjectMutationType::ProjectUpdate,
        workspace_root: None,
    }
}

fn create_mutation(
    project_id: &str,
    title: &str,
    workspace_root: &str,
    command_id: String,
) -> ProjectMutation {
    ProjectMutation::Variant0 {
        command_id,
        // The web's add project flow creates a folder that is missing.
        create_workspace_root_if_missing: Some(true),
        default_model_selection: None,
        project_id: project_id.to_owned(),
        scripts: None,
        title: title.to_owned(),
        type_: ProjectMutationCreateType::ProjectCreate,
        workspace_root: workspace_root.to_owned(),
    }
}

fn refs_query(root: &str) -> serde_json::Value {
    serde_json::json!({"cwd": root, "refKind": "local", "limit": strip::REFS_LIMIT})
}

/// Doubling delay between attempts, capped (Electron restarts the backend the same way).
pub struct Backoff {
    next: Duration,
}

impl Backoff {
    const INITIAL: Duration = Duration::from_millis(500);
    const MAX: Duration = Duration::from_secs(10);

    pub fn new() -> Self {
        Self {
            next: Self::INITIAL,
        }
    }

    pub fn reset(&mut self) {
        self.next = Self::INITIAL;
    }

    pub fn next_delay(&mut self) -> Duration {
        let delay = self.next;
        self.next = (self.next * 2).min(Self::MAX);
        delay
    }
}

fn socket_url(http_base: &str, ticket: &str) -> Result<reqwest::Url, String> {
    let ws_base = http_base
        .strip_prefix("https://")
        .map(|rest| format!("wss://{rest}"))
        .or_else(|| {
            http_base
                .strip_prefix("http://")
                .map(|rest| format!("ws://{rest}"))
        })
        .ok_or_else(|| format!("unsupported server URL: {http_base}"))?;
    reqwest::Url::parse_with_params(
        &format!("{}/ws", ws_base.trim_end_matches('/')),
        [
            ("orchestrationProtocol", ORCHESTRATION_PROTOCOL_VERSION),
            ("wsTicket", ticket),
            ("clientSurface", "desktop"),
            ("clientDeviceType", "desktop"),
            ("clientAppVersion", env!("CARGO_PKG_VERSION")),
            ("connectionMethod", "direct"),
        ],
    )
    .map_err(|error| error.to_string())
}

/// Why `run` stopped: only a saved session the server no longer honors ends it.
#[derive(Debug, PartialEq, Eq)]
pub enum Ended {
    /// The server answered 401 to a saved session (revoked or expired); pairing again is the fix.
    SessionRejected,
}

/// Connects and stays connected until cancelled, reporting through `status`. A saved
/// (`Credential::Bearer`) session that the server rejects ends it instead of retrying forever.
pub async fn run(
    http_base: &str,
    credential: &Credential,
    environment: &Environment,
    actions: &ActionReceiver,
    status: &StatusSender,
) -> Ended {
    let client = auth::http_client();
    let uploader = Uploader::new(client.clone(), http_base);
    let mut bearer: Option<String> = None;
    // Outlives a socket: a reconnect subscribes to the thread again.
    let mut selected: Option<String> = None;
    let mut watched: Option<String> = None;
    let mut terminal: Vec<Target> = Vec::new();
    let mut backoff = Backoff::new();
    loop {
        publish(status, Status::Connecting);
        let failure = match connect(&client, http_base, credential, &mut bearer).await {
            Ok(socket) => {
                publish(
                    status,
                    Status::Connected {
                        label: environment.label.clone(),
                        machine: environment.machine,
                        can_pin: environment.project_organization,
                        shell: None,
                    },
                );
                backoff.reset();
                let failure = serve(
                    socket,
                    environment,
                    &uploader,
                    actions,
                    (&mut selected, &mut watched, &mut terminal),
                    status,
                )
                .await;
                // The requests died with the socket.
                publish(
                    status,
                    Status::SlowRequests {
                        label: environment.label.clone(),
                        requests: Vec::new(),
                    },
                );
                failure
            }
            Err(ConnectError::SessionRejected) => return Ended::SessionRejected,
            Err(ConnectError::Failed(message)) => message,
        };
        publish(status, Status::Failed(format!("Reconnecting: {failure}")));
        sleep(backoff.next_delay()).await;
    }
}

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

enum ConnectError {
    SessionRejected,
    Failed(String),
}

impl From<String> for ConnectError {
    fn from(message: String) -> Self {
        Self::Failed(message)
    }
}

async fn connect(
    client: &reqwest::Client,
    http_base: &str,
    credential: &Credential,
    bearer: &mut Option<String>,
) -> Result<Socket, ConnectError> {
    let token = match bearer {
        Some(token) => token.clone(),
        None => {
            let token = auth::exchange_bearer(client, http_base, credential)
                .await
                .map_err(|error| error.to_string())?;
            *bearer = Some(token.clone());
            token
        }
    };
    let ticket = match auth::issue_ws_ticket(client, http_base, &token).await {
        Ok(ticket) => ticket,
        Err(error) => {
            if matches!(error, AuthError::Rejected(_)) {
                *bearer = None;
            }
            let saved = matches!(credential, Credential::Bearer(_));
            if saved && matches!(error, AuthError::Rejected(401)) {
                return Err(ConnectError::SessionRejected);
            }
            return Err(ConnectError::Failed(error.to_string()));
        }
    };
    let url = socket_url(http_base, &ticket)?;
    // Browsers cap neither frames nor messages, and shell snapshots outgrow tungstenite's 16 MiB default.
    let config = WebSocketConfig::default()
        .max_frame_size(None)
        .max_message_size(None);
    let (socket, _) =
        tokio_tungstenite::connect_async_with_config(url.as_str(), Some(config), false)
            .await
            .map_err(|error| ConnectError::Failed(error.to_string()))?;
    Ok(socket)
}

/// Subscribes to the shell stream and keeps the socket alive; returns why it ended.
async fn serve(
    socket: Socket,
    environment: &Environment,
    uploader: &Uploader,
    actions: &ActionReceiver,
    (selected, watched, wanted_terminal): (
        &mut Option<String>,
        &mut Option<String>,
        &mut Vec<Target>,
    ),
    status: &StatusSender,
) -> String {
    let (mut sink, mut stream) = socket.split();
    let mut session = Session::new();
    let mut shell = ShellState::default();
    let request = match session.request::<SubscribeShell>(&SubscribeShellPayload::default()) {
        Ok((_, frame)) => frame,
        Err(error) => return error,
    };
    if let Err(error) = sink.send(Message::text(request)).await {
        return error.to_string();
    }
    let mut config_request = match session.request::<ServerGetConfig>(&serde_json::json!({})) {
        Ok((id, frame)) => match sink.send(Message::text(frame)).await {
            Ok(()) => Some(id),
            Err(error) => return error.to_string(),
        },
        Err(error) => {
            eprintln!("[rpc] could not request the provider config: {error}");
            None
        }
    };
    // Whatever was clicked while disconnected is stale by now, except which thread is open.
    while let Ok(action) = actions.try_recv() {
        match action {
            Action::SelectThread(id) => *selected = id,
            Action::WatchVcs(cwd) => *watched = cwd,
            Action::AttachTerminals(targets) => *wanted_terminal = targets,
            _ => {}
        }
    }
    let mut threads = Threads::default();
    if let Err(error) = threads
        .select(selected.as_deref(), &mut session, &mut sink)
        .await
    {
        return error;
    }
    let mut vcs = VcsWatch::default();
    if let Err(error) = vcs
        .select(watched.as_deref(), &mut session, &mut sink)
        .await
    {
        return error;
    }
    let mut terminals = Terminals::default();
    if let Err(error) = terminals
        .select(wanted_terminal, &mut session, &mut sink)
        .await
    {
        return error;
    }
    let mut pending: HashMap<String, &'static str> = HashMap::new();
    // Branch lookups by request id, with the workspace root each was for.
    let mut branches: HashMap<String, String> = HashMap::new();
    // Diff loads by request id, with the directory each was for.
    let mut diffs: HashMap<String, String> = HashMap::new();
    // The card's calls by request id, with the token its answers carry.
    let mut calls: HashMap<String, u64> = HashMap::new();
    // Add project requests by request id, with the project id each created.
    let mut adds: HashMap<String, String> = HashMap::new();
    let mut ping = interval(PING_INTERVAL);
    ping.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut deadline = Instant::now() + PING_TIMEOUT;
    let mut reported_slow = Vec::new();
    loop {
        let now = std::time::Instant::now();
        let slow = session.slow(now);
        if slow != reported_slow {
            publish(
                status,
                Status::SlowRequests {
                    label: environment.label.clone(),
                    requests: slow.clone(),
                },
            );
            reported_slow = slow;
        }
        // Parked far out when nothing waits; any frame or action wakes the loop anyway.
        let slow_wait = session.next_slow(now).unwrap_or(Duration::from_secs(3600));
        tokio::select! {
            _ = sleep(slow_wait) => {}
            _ = ping.tick() => {
                if let Err(error) = sink.send(Message::text(PING_FRAME)).await {
                    return error.to_string();
                }
            }
            action = actions.recv() => {
                let Ok(action) = action else {
                    return "window closed".to_owned();
                };
                if let Action::Call { token, call, payload } = &action {
                    match call.request(&mut session, payload) {
                        Ok((id, frame)) => {
                            calls.insert(id, *token);
                            if let Err(error) = sink.send(Message::text(frame)).await {
                                return error.to_string();
                            }
                        }
                        Err(error) => publish(
                            status,
                            Status::Call { token: *token, event: CallEvent::Failed(error) },
                        ),
                    }
                    continue;
                }
                if let Action::EndCall { token } = &action {
                    let id = calls.iter().find_map(|(id, owner)| (owner == token).then(|| id.clone()));
                    if let Some(id) = id {
                        calls.remove(&id);
                        let frame = session.interrupt(&id);
                        if let Err(error) = sink.send(Message::text(frame)).await {
                            return error.to_string();
                        }
                    }
                    continue;
                }
                let request = match &action {
                    Action::Upload { token, relative_url, path, mime } => {
                        uploader.start(*token, relative_url, path.clone(), mime.clone(), status);
                        continue;
                    }
                    Action::SelectThread(id) => {
                        *selected = id.clone();
                        if let Err(error) =
                            threads.select(id.as_deref(), &mut session, &mut sink).await
                        {
                            return error;
                        }
                        continue;
                    }
                    Action::WatchVcs(cwd) => {
                        *watched = cwd.clone();
                        if let Err(error) =
                            vcs.select(cwd.as_deref(), &mut session, &mut sink).await
                        {
                            return error;
                        }
                        continue;
                    }
                    Action::AttachTerminals(targets) => {
                        *wanted_terminal = targets.clone();
                        if let Err(error) = terminals
                            .select(targets, &mut session, &mut sink)
                            .await
                        {
                            return error;
                        }
                        continue;
                    }
                    Action::TerminalWrite {
                        thread_id,
                        terminal_id,
                        data,
                    } => session
                        .request::<TerminalWrite>(&terminal::write_payload(
                            thread_id,
                            terminal_id,
                            data,
                        ))
                        .map(|(id, frame)| (id, frame, None)),
                    Action::TerminalResize {
                        thread_id,
                        terminal_id,
                        grid,
                    } => session
                        .request::<TerminalResize>(&terminal::resize_payload(
                            thread_id,
                            terminal_id,
                            *grid,
                        ))
                        .map(|(id, frame)| (id, frame, None)),
                    Action::TerminalClose {
                        thread_id,
                        terminal_id,
                    } => session
                        .request::<TerminalClose>(&terminal::close_payload(thread_id, terminal_id))
                        .map(|(id, frame)| (id, frame, None)),
                    Action::LoadDiff { cwd } => {
                        match session.request::<ReviewGetDiffPreview>(&diff_query(cwd)) {
                            Ok((id, frame)) => {
                                diffs.insert(id, cwd.clone());
                                if let Err(error) = sink.send(Message::text(frame)).await {
                                    return error.to_string();
                                }
                            }
                            Err(error) => eprintln!("[rpc] could not {}: {error}", action.describe()),
                        }
                        continue;
                    }
                    Action::OpenInEditor { cwd, editor } => session
                        .request::<ShellOpenInEditor>(&serde_json::json!({
                            "cwd": cwd,
                            "editor": editor,
                        }))
                        .map(|(id, frame)| (id, frame, None)),
                    Action::SetProjectPinned { project_id, pinned } => session
                        .request::<ProjectsMutate>(&pin_mutation(project_id, *pinned, command_id()))
                        .map(|(id, frame)| (id, frame, None)),
                    Action::AddProject {
                        project_id,
                        title,
                        workspace_root,
                    } => {
                        let mutation =
                            create_mutation(project_id, title, workspace_root, command_id());
                        match session.request::<ProjectsMutate>(&mutation) {
                            Ok((id, frame)) => {
                                adds.insert(id, project_id.clone());
                                if let Err(error) = sink.send(Message::text(frame)).await {
                                    return error.to_string();
                                }
                            }
                            Err(error) => publish(status, Status::ProjectFailed(error)),
                        }
                        continue;
                    }
                    Action::LoadBranch { root } => session
                        .request::<VcsListRefs>(&refs_query(root))
                        .map(|(id, frame)| (id, frame, Some(root.clone()))),
                    Action::Call { .. } | Action::EndCall { .. } => continue,
                };
                match request {
                    Ok((id, frame, root)) => {
                        if let Some(root) = root {
                            branches.insert(id, root);
                        } else {
                            pending.insert(id, action.describe());
                        }
                        if let Err(error) = sink.send(Message::text(frame)).await {
                            return error.to_string();
                        }
                    }
                    Err(error) => eprintln!("[rpc] could not {}: {error}", action.describe()),
                }
            }
            _ = sleep_until(deadline) => {
                return format!("server stopped responding for {}s", PING_TIMEOUT.as_secs());
            }
            frame = stream.next() => match frame {
                None => return "connection closed".to_owned(),
                Some(Err(error)) => return error.to_string(),
                Some(Ok(Message::Close(frame))) => {
                    return frame
                        .map(|frame| format!("connection closed: {}", frame.reason))
                        .unwrap_or_else(|| "connection closed".to_owned());
                }
                Some(Ok(Message::Text(text))) => {
                    // Any frame, Pong included, proves the server is alive.
                    deadline = Instant::now() + PING_TIMEOUT;
                    let reaction = session.react(text.as_str());
                    for reply in reaction.send {
                        if let Err(error) = sink.send(Message::text(reply)).await {
                            return error.to_string();
                        }
                    }
                    for event in reaction.events {
                        let Some(event) = threads.route(event, status) else {
                            continue;
                        };
                        let Some(event) = vcs.route(event, status) else {
                            continue;
                        };
                        let Some(event) = terminals.route(event, status) else {
                            continue;
                        };
                        if config_request.as_ref().is_some_and(|id| owns(&event, id)) {
                            config_request = None;
                            publish_catalog(event, status);
                            continue;
                        }
                        if let Some(cwd) = request_id(&event).and_then(|id| diffs.remove(id)) {
                            publish_diff(cwd, event, status);
                            continue;
                        }
                        let Some(event) = route_call(&mut calls, event, status) else {
                            continue;
                        };
                        if let Some(root) = request_id(&event).and_then(|id| branches.remove(id)) {
                            publish_branch(root, event, status);
                            continue;
                        }
                        if route_add(&mut adds, &event, status) {
                            continue;
                        }
                        if settle(&mut pending, &event) {
                            continue;
                        }
                        if let Some(reason) = handle(event, &mut shell, environment, status) {
                            return reason;
                        }
                    }
                }
                Some(Ok(_)) => deadline = Instant::now() + PING_TIMEOUT,
            }
        }
    }
}

type Sink = SplitSink<Socket, Message>;

/// The open thread subscription, and the interrupted ones whose last frames still arrive.
#[derive(Default)]
struct Threads {
    open: Option<OpenThread>,
    stale: HashSet<String>,
}

struct OpenThread {
    thread_id: String,
    request_id: String,
    state: ThreadState,
}

impl Threads {
    /// Switches the subscription to `wanted`, interrupting the previous one.
    async fn select(
        &mut self,
        wanted: Option<&str>,
        session: &mut Session,
        sink: &mut Sink,
    ) -> Result<(), String> {
        if self.open.as_ref().map(|open| open.thread_id.as_str()) == wanted {
            return Ok(());
        }
        if let Some(open) = self.open.take() {
            let frame = session.interrupt(&open.request_id);
            self.stale.insert(open.request_id);
            sink.send(Message::text(frame))
                .await
                .map_err(|error| error.to_string())?;
        }
        let Some(thread_id) = wanted else {
            return Ok(());
        };
        let payload = SubscribeThreadPayload {
            accept_bounded_snapshot: None,
            accept_compact_ids: None,
            accept_compact_turn_items: None,
            accept_deduped_bodies: None,
            accept_deferred_work: None,
            after_sequence: None,
            request_completion_marker: None,
            thread_id: thread_id.to_owned(),
        };
        let (request_id, frame) = session.request::<SubscribeThread>(&payload)?;
        self.open = Some(OpenThread {
            thread_id: thread_id.to_owned(),
            request_id,
            state: ThreadState::default(),
        });
        sink.send(Message::text(frame))
            .await
            .map_err(|error| error.to_string())
    }

    /// Consumes the events of thread requests, publishing the transcript; hands back the rest.
    fn route(&mut self, event: Event, status: &StatusSender) -> Option<Event> {
        let id = match &event {
            Event::Item { request_id, .. }
            | Event::Done { request_id, .. }
            | Event::Failed { request_id, .. } => request_id.clone(),
            _ => return Some(event),
        };
        if self.stale.contains(&id) {
            if !matches!(event, Event::Item { .. }) {
                self.stale.remove(&id);
            }
            return None;
        }
        let Some(open) = self.open.as_mut().filter(|open| open.request_id == id) else {
            return Some(event);
        };
        let thread_id = open.thread_id.clone();
        match event {
            Event::Item { value, .. } => {
                if open.state.apply(&value) && open.state.is_seeded() {
                    publish(
                        status,
                        Status::Thread {
                            id: thread_id,
                            transcript: Ok(open.state.entries()),
                        },
                    );
                }
            }
            Event::Failed { message, .. } => {
                self.open = None;
                publish(
                    status,
                    Status::Thread {
                        id: thread_id,
                        transcript: Err(message),
                    },
                );
            }
            _ => self.open = None,
        }
        None
    }
}

/// The open version control subscription, and the interrupted ones whose last frames still arrive.
#[derive(Default)]
struct VcsWatch {
    open: Option<OpenVcs>,
    stale: HashSet<String>,
}

struct OpenVcs {
    cwd: String,
    request_id: String,
    status: Option<VcsStatus>,
}

impl VcsWatch {
    /// Switches the subscription to `wanted`, interrupting the previous one.
    async fn select(
        &mut self,
        wanted: Option<&str>,
        session: &mut Session,
        sink: &mut Sink,
    ) -> Result<(), String> {
        if self.open.as_ref().map(|open| open.cwd.as_str()) == wanted {
            return Ok(());
        }
        if let Some(open) = self.open.take() {
            let frame = session.interrupt(&open.request_id);
            self.stale.insert(open.request_id);
            sink.send(Message::text(frame))
                .await
                .map_err(|error| error.to_string())?;
        }
        let Some(cwd) = wanted else {
            return Ok(());
        };
        let (request_id, frame) =
            session.request::<SubscribeVcsStatus>(&serde_json::json!({ "cwd": cwd }))?;
        self.open = Some(OpenVcs {
            cwd: cwd.to_owned(),
            request_id,
            status: None,
        });
        sink.send(Message::text(frame))
            .await
            .map_err(|error| error.to_string())
    }

    /// Consumes the events of status requests, publishing the merged status; hands back the rest.
    fn route(&mut self, event: Event, status: &StatusSender) -> Option<Event> {
        let id = match &event {
            Event::Item { request_id, .. }
            | Event::Done { request_id, .. }
            | Event::Failed { request_id, .. } => request_id.clone(),
            _ => return Some(event),
        };
        if self.stale.contains(&id) {
            if !matches!(event, Event::Item { .. }) {
                self.stale.remove(&id);
            }
            return None;
        }
        let Some(open) = self.open.as_mut().filter(|open| open.request_id == id) else {
            return Some(event);
        };
        let cwd = open.cwd.clone();
        match event {
            Event::Item { value, .. } => {
                if let Some(next) = VcsStatus::apply(open.status.as_ref(), &value) {
                    open.status = Some(next.clone());
                    publish(
                        status,
                        Status::Vcs {
                            cwd,
                            status: Some(next),
                        },
                    );
                }
            }
            Event::Failed { message, .. } => {
                eprintln!("[rpc] could not watch the status of {cwd}: {message}");
                self.open = None;
                publish(status, Status::Vcs { cwd, status: None });
            }
            _ => {
                self.open = None;
                publish(status, Status::Vcs { cwd, status: None });
            }
        }
        None
    }
}

/// The open terminal attachments, and the interrupted ones whose last frames still arrive.
#[derive(Default)]
struct Terminals {
    open: Vec<OpenTerminal>,
    stale: HashSet<String>,
}

struct OpenTerminal {
    target: Target,
    request_id: String,
}

impl Terminals {
    /// Attaches exactly `wanted`, interrupting the rest. Detaching leaves a shell running on the
    /// server; attaching to a target again replays its history.
    async fn select(
        &mut self,
        wanted: &[Target],
        session: &mut Session,
        sink: &mut Sink,
    ) -> Result<(), String> {
        let (keep, dropped): (Vec<_>, Vec<_>) = std::mem::take(&mut self.open)
            .into_iter()
            .partition(|open| wanted.contains(&open.target));
        self.open = keep;
        for open in dropped {
            let frame = session.interrupt(&open.request_id);
            self.stale.insert(open.request_id);
            sink.send(Message::text(frame))
                .await
                .map_err(|error| error.to_string())?;
        }
        for target in wanted {
            if self.open.iter().any(|open| &open.target == target) {
                continue;
            }
            let (request_id, frame) =
                session.request::<TerminalAttach>(&target.attach_payload())?;
            self.open.push(OpenTerminal {
                target: target.clone(),
                request_id,
            });
            sink.send(Message::text(frame))
                .await
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    /// Consumes the events of the attach requests, publishing each; hands back the rest.
    fn route(&mut self, event: Event, status: &StatusSender) -> Option<Event> {
        let id = match &event {
            Event::Item { request_id, .. }
            | Event::Done { request_id, .. }
            | Event::Failed { request_id, .. } => request_id.clone(),
            _ => return Some(event),
        };
        if self.stale.contains(&id) {
            if !matches!(event, Event::Item { .. }) {
                self.stale.remove(&id);
            }
            return None;
        }
        let Some(index) = self.open.iter().position(|open| open.request_id == id) else {
            return Some(event);
        };
        let target = self.open[index].target.clone();
        let event = match event {
            Event::Item { value, .. } => terminal::decode_event(&value)?,
            Event::Failed { message, .. } => {
                self.open.remove(index);
                terminal::Event::Error(message)
            }
            _ => {
                self.open.remove(index);
                terminal::Event::Closed
            }
        };
        publish(
            status,
            Status::Terminal {
                thread_id: target.thread_id,
                terminal_id: target.terminal_id,
                event,
            },
        );
        None
    }
}

/// A project id in the UUID v4 text form the web mints.
pub fn new_project_id() -> String {
    let mut bytes = [0u8; 16];
    // A failed read leaves zeros, and the server rejects an id that is already a project.
    let _ = getrandom::fill(&mut bytes);
    uuid_v4(bytes)
}

fn uuid_v4(mut bytes: [u8; 16]) -> String {
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex = hex::encode(bytes);
    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    )
}

pub fn command_id() -> String {
    let mut bytes = [0u8; 8];
    // A failed read leaves zeros, and the server rejects a repeated command id rather than applying it twice.
    let _ = getrandom::fill(&mut bytes);
    format!("gpui-{}", hex::encode(bytes))
}

fn request_id(event: &Event) -> Option<&str> {
    match event {
        Event::Done { request_id, .. } | Event::Failed { request_id, .. } => Some(request_id),
        _ => None,
    }
}

fn owns(event: &Event, id: &str) -> bool {
    request_id(event) == Some(id)
}

/// Publishes the outcome of a card call; hands back what belongs to no call.
fn route_call(
    calls: &mut HashMap<String, u64>,
    event: Event,
    status: &StatusSender,
) -> Option<Event> {
    let (id, answer) = match event {
        Event::Item { request_id, value } => (request_id, CallEvent::Progress(value)),
        Event::Done { request_id, value } => (request_id, CallEvent::Done(value)),
        Event::Failed {
            request_id,
            message,
        } => (request_id, CallEvent::Failed(message)),
        other => return Some(other),
    };
    let Some(token) = calls.get(&id).copied() else {
        return Some(match answer {
            CallEvent::Progress(value) => Event::Item {
                request_id: id,
                value,
            },
            CallEvent::Done(value) => Event::Done {
                request_id: id,
                value,
            },
            CallEvent::Failed(message) => Event::Failed {
                request_id: id,
                message,
            },
        });
    };
    if !matches!(answer, CallEvent::Progress(_)) {
        calls.remove(&id);
    }
    publish(
        status,
        Status::Call {
            token,
            event: answer,
        },
    );
    None
}

/// Publishes the branch of a workspace root; a failure leaves the strip without git controls.
fn publish_branch(root: String, event: Event, status: &StatusSender) {
    let branch = match event {
        Event::Done { value, .. } => Branch::decode(&value),
        Event::Failed { message, .. } => {
            eprintln!("[rpc] could not load the branch of {root}: {message}");
            None
        }
        _ => return,
    };
    publish(status, Status::Branch { root, branch });
}

/// The web's `branchDiffPreviewInput` for the unstaged scope.
fn diff_query(cwd: &str) -> serde_json::Value {
    serde_json::json!({
        "cwd": cwd,
        "sourceKind": "working-tree",
        "ignoreWhitespace": false,
    })
}

/// Publishes a directory's diff, or the reason it could not be loaded.
fn publish_diff(cwd: String, event: Event, status: &StatusSender) {
    let diff = match event {
        Event::Done { value, .. } => {
            diff::Preview::decode(&value).ok_or_else(|| "The server sent no diff.".to_owned())
        }
        Event::Failed { message, .. } => Err(message),
        _ => return,
    };
    publish(status, Status::Diff { cwd, diff });
}

/// Publishes the provider catalog; a failure leaves the composer without a model trigger.
fn publish_catalog(event: Event, status: &StatusSender) {
    match event {
        Event::Done { value, .. } => match Catalog::decode(value) {
            Ok(catalog) => publish(status, Status::Providers(Box::new(catalog))),
            Err(error) => eprintln!("[rpc] unreadable provider config: {error}"),
        },
        Event::Failed { message, .. } => eprintln!("[rpc] could not load providers: {message}"),
        _ => {}
    }
}

/// Publishes the outcome of an add project request; true when `event` was one.
fn route_add(adds: &mut HashMap<String, String>, event: &Event, status: &StatusSender) -> bool {
    match event {
        Event::Done { request_id, .. } => adds.remove(request_id).is_some_and(|project_id| {
            publish(status, Status::ProjectAdded(project_id));
            true
        }),
        Event::Failed {
            request_id,
            message,
        } => adds.remove(request_id).is_some_and(|_| {
            publish(status, Status::ProjectFailed(message.clone()));
            true
        }),
        _ => false,
    }
}

/// Consumes the outcome of an action's request, logging a failure; true when `event` was one.
fn settle(pending: &mut HashMap<String, &'static str>, event: &Event) -> bool {
    match event {
        Event::Done { request_id, .. } => pending.remove(request_id).is_some(),
        Event::Failed {
            request_id,
            message,
        } => pending.remove(request_id).is_some_and(|what| {
            eprintln!("[rpc] could not {what}: {message}");
            true
        }),
        _ => false,
    }
}

/// Applies one event; `Some(reason)` means the subscription is over and the socket should reopen.
fn handle(
    event: Event,
    shell: &mut ShellState,
    environment: &Environment,
    status: &StatusSender,
) -> Option<String> {
    match event {
        Event::Item { value, .. } => match serde_json::from_value::<ShellItem>(value) {
            Ok(item) => {
                if shell.apply(item) && shell.is_seeded() {
                    publish(
                        status,
                        Status::Connected {
                            label: environment.label.clone(),
                            machine: environment.machine,
                            can_pin: environment.project_organization,
                            shell: Some(shell.view()),
                        },
                    );
                }
                None
            }
            Err(error) => Some(format!("unreadable shell item: {error}")),
        },
        Event::Done { .. } => Some("shell stream ended".to_owned()),
        Event::Failed { message, .. } => Some(format!("shell stream failed: {message}")),
        Event::Defect(message) => Some(format!("server defect: {message}")),
        Event::Pong | Event::Ignored => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_to_a_cap_and_resets() {
        let mut backoff = Backoff::new();
        let delays: Vec<_> = (0..7).map(|_| backoff.next_delay().as_millis()).collect();
        assert_eq!(delays, vec![500, 1000, 2000, 4000, 8000, 10_000, 10_000]);
        backoff.reset();
        assert_eq!(backoff.next_delay().as_millis(), 500);
    }

    #[test]
    fn project_ids_have_the_uuid_v4_shape() {
        assert_eq!(uuid_v4([0xff; 16]), "ffffffff-ffff-4fff-bfff-ffffffffffff");
    }

    #[test]
    fn add_action_builds_a_project_create_mutation() {
        let value = serde_json::to_value(create_mutation(
            "p1",
            "t3code",
            "/work/t3code",
            "c1".to_owned(),
        ))
        .expect("json");
        assert_eq!(
            value,
            serde_json::json!({
                "commandId": "c1",
                "createWorkspaceRootIfMissing": true,
                "projectId": "p1",
                "title": "t3code",
                "type": "project.create",
                "workspaceRoot": "/work/t3code",
            })
        );
    }

    #[test]
    fn route_add_consumes_only_the_outcome_of_an_add() {
        let (status, receiver) = async_channel::unbounded();
        let mut adds = HashMap::from([("7".to_owned(), "p1".to_owned())]);
        let done = Event::Done {
            request_id: "7".to_owned(),
            value: serde_json::Value::Null,
        };
        let failed = |id: &str| Event::Failed {
            request_id: id.to_owned(),
            message: "boom".to_owned(),
        };
        assert!(!route_add(&mut adds, &failed("0"), &status));
        assert!(route_add(&mut adds, &done, &status));
        assert_eq!(
            receiver.try_recv().ok(),
            Some(Status::ProjectAdded("p1".to_owned()))
        );
        adds.insert("8".to_owned(), "p2".to_owned());
        assert!(route_add(&mut adds, &failed("8"), &status));
        assert_eq!(
            receiver.try_recv().ok(),
            Some(Status::ProjectFailed("boom".to_owned()))
        );
        assert!(adds.is_empty());
    }

    #[test]
    fn pin_action_builds_a_project_update_mutation() {
        let value = serde_json::to_value(pin_mutation("p1", true, "c1".to_owned())).expect("json");
        assert_eq!(
            value,
            serde_json::json!({
                "type": "project.update",
                "commandId": "c1",
                "projectId": "p1",
                "pinned": true,
            })
        );
        let value = serde_json::to_value(pin_mutation("p1", false, "c2".to_owned())).expect("json");
        assert_eq!(value["pinned"], false);
    }

    #[test]
    fn settle_consumes_only_the_outcome_of_a_pending_action() {
        let mut pending = HashMap::from([("7".to_owned(), "pin project")]);
        let failed = |id: &str| Event::Failed {
            request_id: id.to_owned(),
            message: "no".to_owned(),
        };
        assert!(!settle(&mut pending, &failed("0")));
        assert!(settle(&mut pending, &failed("7")));
        assert!(pending.is_empty());
        assert!(!settle(&mut pending, &failed("7")));
    }

    #[test]
    fn call_events_carry_their_token_and_others_pass_through() {
        let (status, published) = async_channel::unbounded();
        let mut calls = HashMap::from([("4".to_owned(), 9u64)]);
        let item = Event::Item {
            request_id: "4".to_owned(),
            value: serde_json::json!({"type": "phase_started"}),
        };
        assert!(route_call(&mut calls, item, &status).is_none());
        assert!(calls.contains_key("4"));
        let done = Event::Done {
            request_id: "4".to_owned(),
            value: serde_json::Value::Null,
        };
        assert!(route_call(&mut calls, done, &status).is_none());
        assert!(calls.is_empty());
        let other = Event::Failed {
            request_id: "5".to_owned(),
            message: "no".to_owned(),
        };
        assert!(route_call(&mut calls, other, &status).is_some());
        assert!(matches!(
            published.try_recv(),
            Ok(Status::Call {
                token: 9,
                event: CallEvent::Progress(_)
            })
        ));
        assert!(matches!(
            published.try_recv(),
            Ok(Status::Call {
                token: 9,
                event: CallEvent::Done(_)
            })
        ));
        assert!(published.try_recv().is_err());
    }

    #[test]
    fn thread_events_are_routed_and_interrupted_ones_swallowed() {
        let (status, published) = async_channel::unbounded();
        let item = |id: &str, value: serde_json::Value| Event::Item {
            request_id: id.to_owned(),
            value,
        };
        let snapshot = serde_json::json!({
            "kind": "snapshot",
            "snapshotSequence": 1,
            "projection": {"visibleTurnItems": [{
                "visibility": "local",
                "item": {"id": "u", "ordinal": 1, "type": "user_message", "text": "hi"},
            }]},
        });
        let mut threads = Threads {
            open: Some(OpenThread {
                thread_id: "t1".to_owned(),
                request_id: "5".to_owned(),
                state: ThreadState::default(),
            }),
            stale: HashSet::from(["4".to_owned()]),
        };
        // Other requests pass through, and a stale stream's frames never reach the shell.
        assert!(
            threads
                .route(item("9", snapshot.clone()), &status)
                .is_some()
        );
        assert!(
            threads
                .route(item("4", snapshot.clone()), &status)
                .is_none()
        );
        let done = Event::Done {
            request_id: "4".to_owned(),
            value: serde_json::Value::Null,
        };
        assert!(threads.route(done, &status).is_none());
        assert!(threads.stale.is_empty());
        assert!(published.try_recv().is_err());
        assert!(threads.route(item("5", snapshot), &status).is_none());
        let Ok(Status::Thread { id, transcript }) = published.try_recv() else {
            panic!("the open thread publishes its transcript");
        };
        assert_eq!(
            (id.as_str(), transcript.map(|rows| rows.len())),
            ("t1", Ok(1))
        );
        let failed = Event::Failed {
            request_id: "5".to_owned(),
            message: "gone".to_owned(),
        };
        assert!(threads.route(failed, &status).is_none());
        assert!(threads.open.is_none());
        assert!(matches!(
            published.try_recv(),
            Ok(Status::Thread {
                transcript: Err(_),
                ..
            })
        ));
    }

    #[test]
    fn status_events_merge_and_the_stream_ending_clears_it() {
        let (status, published) = async_channel::unbounded();
        let item = |id: &str, value: serde_json::Value| Event::Item {
            request_id: id.to_owned(),
            value,
        };
        let snapshot = serde_json::json!({
            "_tag": "snapshot",
            "local": {"isRepo": true, "refName": "main", "workingTree": {"insertions": 1, "deletions": 0}},
            "remote": null,
        });
        let mut watch = VcsWatch {
            open: Some(OpenVcs {
                cwd: "/repo".to_owned(),
                request_id: "3".to_owned(),
                status: None,
            }),
            stale: HashSet::from(["2".to_owned()]),
        };
        assert!(watch.route(item("9", snapshot.clone()), &status).is_some());
        assert!(watch.route(item("2", snapshot.clone()), &status).is_none());
        assert!(published.try_recv().is_err());
        assert!(watch.route(item("3", snapshot), &status).is_none());
        let Ok(Status::Vcs {
            cwd,
            status: Some(merged),
        }) = published.try_recv()
        else {
            panic!("the open watch publishes its status");
        };
        assert_eq!(
            (cwd.as_str(), merged.local.working_tree.insertions),
            ("/repo", 1)
        );
        let done = Event::Done {
            request_id: "3".to_owned(),
            value: serde_json::Value::Null,
        };
        assert!(watch.route(done, &status).is_none());
        assert!(watch.open.is_none());
        assert!(matches!(
            published.try_recv(),
            Ok(Status::Vcs { status: None, .. })
        ));
    }

    #[test]
    fn socket_url_carries_protocol_and_escaped_ticket() {
        let url = socket_url("http://127.0.0.1:13773", "a b&c").expect("url");
        assert_eq!(url.scheme(), "ws");
        assert_eq!(url.path(), "/ws");
        let remote = socket_url("https://host.example/", "t").expect("url");
        assert_eq!(remote.scheme(), "wss");
        assert_eq!(remote.path(), "/ws");
        let pairs: Vec<_> = url.query_pairs().into_owned().collect();
        assert!(pairs.contains(&("orchestrationProtocol".into(), "2".into())));
        assert!(pairs.contains(&("wsTicket".into(), "a b&c".into())));
    }
}
