//! The authenticated RPC WebSocket: connect, keep alive, reconnect.

use crate::auth::{self, AuthError, Credential};
use crate::electron_dpop;
use crate::projects::MachineKind;
use crate::providers::Catalog;
use crate::rpc::{
    Event, PING_FRAME, ProjectsMutate, ServerGetConfig, Session, SubscribeShell, VcsListRefs,
};
use crate::shell::ShellState;
use crate::status::{Status, StatusSender, publish};
use crate::strip::{self, Branch};
use crate::wire::{ProjectMutation, ProjectMutationType, ShellItem, SubscribeShellPayload};
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::{Instant, MissedTickBehavior, interval, sleep, sleep_until};
use tokio_tungstenite::tungstenite::Message;

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
    /// The branch checked out in a project's workspace root, for the strip under the composer.
    LoadBranch {
        root: String,
    },
}

pub type ActionSender = async_channel::Sender<Action>;
pub type ActionReceiver = async_channel::Receiver<Action>;

impl Action {
    fn describe(&self) -> &'static str {
        match self {
            Self::SetProjectPinned { pinned: true, .. } => "pin project",
            Self::SetProjectPinned { pinned: false, .. } => "unpin project",
            Self::LoadBranch { .. } => "load the branch",
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

/// The HTTP request a WebSocket upgrade is, for the proof's `htu`: scheme swapped, no query.
fn upgrade_htu(url: &reqwest::Url) -> String {
    let scheme = if url.scheme() == "wss" {
        "https"
    } else {
        "http"
    };
    let port = url
        .port()
        .map(|port| format!(":{port}"))
        .unwrap_or_default();
    format!(
        "{scheme}://{}{port}{}",
        url.host_str().unwrap_or_default(),
        url.path()
    )
}

/// Connects and stays connected until cancelled, reporting through `status`.
pub async fn run(
    http_base: &str,
    credential: &Credential,
    environment: &Environment,
    actions: &ActionReceiver,
    status: &StatusSender,
) {
    let client = auth::http_client();
    let mut bearer: Option<String> = None;
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
                serve(socket, environment, actions, status).await
            }
            Err(message) => message,
        };
        publish(status, Status::Failed(format!("Reconnecting: {failure}")));
        sleep(backoff.next_delay()).await;
    }
}

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn connect(
    client: &reqwest::Client,
    http_base: &str,
    credential: &Credential,
    bearer: &mut Option<String>,
) -> Result<Socket, String> {
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
    let ticket = match auth::issue_ws_ticket(client, http_base, credential, &token).await {
        Ok(ticket) => ticket,
        Err(error) => {
            if matches!(error, AuthError::Rejected(_)) {
                *bearer = None;
            }
            return Err(error.to_string());
        }
    };
    let mut url = socket_url(http_base, &ticket)?;
    // TODO(gpui-credentials): HACK. Reads Electron's private IndexedDB and non-extractable DPoP key. Replace with GPUI's own pairing + Keychain before shipping.
    if let Credential::Dpop { key, .. } = credential {
        // A DPoP ticket is key-bound: the upgrade carries a proof for the socket URL in `dpop`,
        // with the ticket as the token it hashes (remote.ts resolveRemoteDpopWebSocketConnectionUrl).
        let proof = electron_dpop::create_proof(
            key,
            "GET",
            &upgrade_htu(&url),
            Some(&ticket),
            auth::now_ms(),
        )?;
        url.query_pairs_mut().append_pair("dpop", &proof);
    }
    let (socket, _) = tokio_tungstenite::connect_async(url.as_str())
        .await
        .map_err(|error| error.to_string())?;
    Ok(socket)
}

/// Subscribes to the shell stream and keeps the socket alive; returns why it ended.
async fn serve(
    socket: Socket,
    environment: &Environment,
    actions: &ActionReceiver,
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
    // Whatever was clicked while disconnected is stale by now.
    while actions.try_recv().is_ok() {}
    let mut pending: HashMap<String, &'static str> = HashMap::new();
    // Branch lookups by request id, with the workspace root each was for.
    let mut branches: HashMap<String, String> = HashMap::new();
    let mut ping = interval(PING_INTERVAL);
    ping.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut deadline = Instant::now() + PING_TIMEOUT;
    loop {
        tokio::select! {
            _ = ping.tick() => {
                if let Err(error) = sink.send(Message::text(PING_FRAME)).await {
                    return error.to_string();
                }
            }
            action = actions.recv() => {
                let Ok(action) = action else {
                    return "window closed".to_owned();
                };
                let request = match &action {
                    Action::SetProjectPinned { project_id, pinned } => session
                        .request::<ProjectsMutate>(&pin_mutation(project_id, *pinned, command_id()))
                        .map(|(id, frame)| (id, frame, None)),
                    Action::LoadBranch { root } => session
                        .request::<VcsListRefs>(&refs_query(root))
                        .map(|(id, frame)| (id, frame, Some(root.clone()))),
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
                        if config_request.as_ref().is_some_and(|id| owns(&event, id)) {
                            config_request = None;
                            publish_catalog(event, status);
                            continue;
                        }
                        if let Some(root) = request_id(&event).and_then(|id| branches.remove(id)) {
                            publish_branch(root, event, status);
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

fn command_id() -> String {
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

/// Publishes the provider catalog; a failure leaves the composer without a model trigger.
fn publish_catalog(event: Event, status: &StatusSender) {
    match event {
        Event::Done { value, .. } => match Catalog::decode(value) {
            Ok(catalog) => publish(status, Status::Providers(catalog)),
            Err(error) => eprintln!("[rpc] unreadable provider config: {error}"),
        },
        Event::Failed { message, .. } => eprintln!("[rpc] could not load providers: {message}"),
        _ => {}
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
