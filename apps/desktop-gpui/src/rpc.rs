//! Effect RPC over the WebSocket in its JSON serialization: request, ack, and response frames.
//! Shapes follow effect's `RpcMessage` (`FromClientEncoded`, `FromServerEncoded`).

use crate::slow_requests::{SlowRequest, Tracker};
use crate::wire;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::time::{Duration, Instant, SystemTime};

/// RpcMessage.constPing in the JSON serialization.
pub const PING_FRAME: &str = r#"{"_tag":"Ping"}"#;

/// One RPC method: its tag and the generated wire types for its payload and success.
pub trait Method {
    const TAG: &'static str;
    /// The call lives as long as its subscription, so it is never a slow request.
    const STREAMING: bool = false;
    type Payload: Serialize;
    type Success: DeserializeOwned;
}

/// `orchestration.subscribeShell`, a stream of shell snapshot and delta items.
pub struct SubscribeShell;

impl Method for SubscribeShell {
    const TAG: &'static str = "orchestration.subscribeShell";
    const STREAMING: bool = true;
    type Payload = wire::SubscribeShellPayload;
    type Success = wire::ShellItem;
}

/// `orchestration.subscribeThread`, a stream of one thread's snapshot and events. The success
/// type is opaque in the schema; `thread::ThreadState` reads the items.
pub struct SubscribeThread;

impl Method for SubscribeThread {
    const TAG: &'static str = "orchestration.subscribeThread";
    const STREAMING: bool = true;
    type Payload = wire::SubscribeThreadPayload;
    type Success = Value;
}

/// `projects.mutate`, which carries `project.update` among its mutations.
pub struct ProjectsMutate;

impl Method for ProjectsMutate {
    const TAG: &'static str = "projects.mutate";
    type Payload = wire::ProjectMutation;
    type Success = wire::ProjectsMutateSuccess;
}

/// `projects.mutate` with a payload the caller builds, for the settings tab.
pub struct ProjectsMutateJson;

impl Method for ProjectsMutateJson {
    const TAG: &'static str = "projects.mutate";
    type Payload = Value;
    type Success = Value;
}

/// `orchestration.launchThread`, a unary call that creates a thread and starts its first turn.
pub struct OrchestrationLaunchThread;

impl Method for OrchestrationLaunchThread {
    const TAG: &'static str = "orchestration.launchThread";
    type Payload = Value;
    type Success = Value;
}

/// `orchestration.dispatchCommand`, a unary call that carries one thread command.
pub struct OrchestrationDispatchCommand;

impl Method for OrchestrationDispatchCommand {
    const TAG: &'static str = "orchestration.dispatchCommand";
    type Payload = Value;
    type Success = Value;
}

/// `server.updateSettings`, a unary call that carries a settings patch.
pub struct ServerUpdateSettings;

impl Method for ServerUpdateSettings {
    const TAG: &'static str = "server.updateSettings";
    type Payload = Value;
    type Success = Value;
}

/// `device.list`, the Device panel's host and platform state.
pub struct DeviceList;

impl Method for DeviceList {
    const TAG: &'static str = "device.list";
    type Payload = Value;
    type Success = Value;
}

/// `server.discoverSourceControl`, the version control tools and hosting integrations on the server.
pub struct ServerDiscoverSourceControl;

impl Method for ServerDiscoverSourceControl {
    const TAG: &'static str = "server.discoverSourceControl";
    type Payload = Value;
    type Success = Value;
}

/// `server.checkSecretSource`, resolving a secret reference on the server.
pub struct ServerCheckSecretSource;

impl Method for ServerCheckSecretSource {
    const TAG: &'static str = "server.checkSecretSource";
    type Payload = Value;
    type Success = Value;
}

/// `server.listOnePasswordAccounts`, the accounts the server's `op` CLI is signed into.
pub struct ServerListOnePasswordAccounts;

impl Method for ServerListOnePasswordAccounts {
    const TAG: &'static str = "server.listOnePasswordAccounts";
    type Payload = Value;
    type Success = Value;
}

/// `device.configure`, the hub and agent-access switches.
pub struct DeviceConfigure;

impl Method for DeviceConfigure {
    const TAG: &'static str = "device.configure";
    type Payload = Value;
    type Success = Value;
}

/// `device.testHost`, a connection check of an SSH host.
pub struct DeviceTestHost;

impl Method for DeviceTestHost {
    const TAG: &'static str = "device.testHost";
    type Payload = Value;
    type Success = Value;
}

/// `device.easAccount`, the Expo account behind an EAS host.
pub struct DeviceEasAccount;

impl Method for DeviceEasAccount {
    const TAG: &'static str = "device.easAccount";
    type Payload = Value;
    type Success = Value;
}

/// `attachments.createUploadUrl`, which mints the signed URL an attachment is posted to.
pub struct AttachmentsCreateUploadUrl;

impl Method for AttachmentsCreateUploadUrl {
    const TAG: &'static str = "attachments.createUploadUrl";
    type Payload = Value;
    type Success = Value;
}

/// `attachments.delete`, which discards an uploaded attachment that no message took.
pub struct AttachmentsDelete;

impl Method for AttachmentsDelete {
    const TAG: &'static str = "attachments.delete";
    type Payload = Value;
    type Success = Value;
}

/// `server.getConfig`, a unary call. The wire types are not generated; `providers::Catalog` decodes it.
pub struct ServerGetConfig;

impl Method for ServerGetConfig {
    const TAG: &'static str = "server.getConfig";
    type Payload = Value;
    type Success = Value;
}

/// `vcs.listRefs`, a unary call. The wire types are not generated; `strip::Branch` decodes it.
pub struct VcsListRefs;

impl Method for VcsListRefs {
    const TAG: &'static str = "vcs.listRefs";
    type Payload = Value;
    type Success = Value;
}

/// `shell.openInEditor`, a unary call that launches an editor on the server's machine.
pub struct ShellOpenInEditor;

impl Method for ShellOpenInEditor {
    const TAG: &'static str = "shell.openInEditor";
    type Payload = Value;
    type Success = Value;
}

/// `review.getDiffPreview`, a unary call. The wire types are not generated; `diff::Preview` decodes it.
pub struct ReviewGetDiffPreview;

impl Method for ReviewGetDiffPreview {
    const TAG: &'static str = "review.getDiffPreview";
    type Payload = Value;
    type Success = Value;
}

/// `subscribeVcsStatus`, a stream of snapshot and update events; `details::VcsStatus` reads them.
pub struct SubscribeVcsStatus;

impl Method for SubscribeVcsStatus {
    const TAG: &'static str = "subscribeVcsStatus";
    const STREAMING: bool = true;
    type Payload = Value;
    type Success = Value;
}

/// `terminal.attach`, a stream that opens the session when the server has none and then carries
/// its snapshot and output. The wire types are not generated; `terminal::decode_event` reads it.
pub struct TerminalAttach;

impl Method for TerminalAttach {
    const TAG: &'static str = "terminal.attach";
    const STREAMING: bool = true;
    type Payload = Value;
    type Success = Value;
}

/// `terminal.write`, a unary call that types into the shell.
pub struct TerminalWrite;

impl Method for TerminalWrite {
    const TAG: &'static str = "terminal.write";
    type Payload = Value;
    type Success = Value;
}

/// `terminal.resize`, a unary call that sets the session's shared grid.
pub struct TerminalResize;

impl Method for TerminalResize {
    const TAG: &'static str = "terminal.resize";
    type Payload = Value;
    type Success = Value;
}

/// `terminal.close`, a unary call that ends the session.
pub struct TerminalClose;

impl Method for TerminalClose {
    const TAG: &'static str = "terminal.close";
    type Payload = Value;
    type Success = Value;
}
macro_rules! untyped_method {
    ($(#[$doc:meta])* $name:ident, $tag:literal) => {
        $(#[$doc])*
        pub struct $name;

        impl Method for $name {
            const TAG: &'static str = $tag;
            type Payload = Value;
            type Success = Value;
        }
    };
}

untyped_method!(
    /// `vcs.switchRef`: checks out a ref in `cwd`.
    VcsSwitchRef,
    "vcs.switchRef"
);
untyped_method!(
    /// `vcs.createRef`: creates a branch and, with `switchRef`, checks it out.
    VcsCreateRef,
    "vcs.createRef"
);
untyped_method!(
    /// `vcs.pull`: pulls the current branch from its upstream.
    VcsPull,
    "vcs.pull"
);
untyped_method!(
    /// `vcs.refreshStatus`: re-reads the status the card watches.
    VcsRefreshStatus,
    "vcs.refreshStatus"
);
untyped_method!(
    /// `server.upsertKeybinding`: writes one keybinding rule; replies `{keybindings, issues}`.
    ServerUpsertKeybinding,
    "server.upsertKeybinding"
);
untyped_method!(
    /// `server.removeKeybinding`: removes one keybinding rule; replies `{keybindings, issues}`.
    ServerRemoveKeybinding,
    "server.removeKeybinding"
);
untyped_method!(
    /// `server.getUsageSummary`: token and cost buckets for a day window.
    ServerGetUsageSummary,
    "server.getUsageSummary"
);
untyped_method!(
    /// `server.refreshUsageRates`: refetches the price table.
    ServerRefreshUsageRates,
    "server.refreshUsageRates"
);
untyped_method!(
    /// `server.refreshProviders`: re-probes the providers and replies `{providers}`.
    ServerRefreshProviders,
    "server.refreshProviders"
);
untyped_method!(
    /// `provider.consumeResetCredit`: spends one reset credit of an instance.
    ProviderConsumeResetCredit,
    "provider.consumeResetCredit"
);
untyped_method!(
    /// `projects.listIgnoredFiles`: the files git ignores in a project, for the clone dialog.
    ProjectsListIgnoredFiles,
    "projects.listIgnoredFiles"
);
untyped_method!(
    /// `projects.readFile`: a workspace file's text.
    ProjectsReadFile,
    "projects.readFile"
);
untyped_method!(
    /// `projects.writeFile`: writes a workspace file.
    ProjectsWriteFile,
    "projects.writeFile"
);
untyped_method!(
    /// `projectClone.start`: begins a clone and adds the project once it lands.
    ProjectCloneStart,
    "projectClone.start"
);
untyped_method!(
    /// `pullRequests.list`: the pull requests of the projects on the environment.
    PullRequestsList,
    "pullRequests.list"
);
untyped_method!(
    /// `pullRequests.listStats`: the list's counts per state.
    PullRequestsListStats,
    "pullRequests.listStats"
);
untyped_method!(
    /// `pullRequests.filterOptions`: the authors and labels the list can filter by.
    PullRequestsFilterOptions,
    "pullRequests.filterOptions"
);
untyped_method!(
    /// `pullRequests.summary`: one pull request's row.
    PullRequestsSummary,
    "pullRequests.summary"
);
untyped_method!(
    /// `pullRequests.routing`: the account and host a pull request is read through.
    PullRequestsRouting,
    "pullRequests.routing"
);
untyped_method!(
    /// `pullRequests.stack`: the stack a pull request belongs to, or null.
    PullRequestsStack,
    "pullRequests.stack"
);
untyped_method!(
    /// `pullRequests.linkedThreads`: the threads linked to a pull request.
    PullRequestsLinkedThreads,
    "pullRequests.linkedThreads"
);
untyped_method!(
    /// `pullRequests.detail`: a pull request's detail.
    PullRequestsDetail,
    "pullRequests.detail"
);
untyped_method!(
    /// `pullRequests.activity`: a pull request's timeline.
    PullRequestsActivity,
    "pullRequests.activity"
);
untyped_method!(
    /// `pullRequests.checks`: a pull request's checks, or null.
    PullRequestsChecks,
    "pullRequests.checks"
);
untyped_method!(
    /// `pullRequests.changeBreakdown`: a pull request's files and commits.
    PullRequestsChangeBreakdown,
    "pullRequests.changeBreakdown"
);
untyped_method!(
    /// `pullRequests.invalidate`: drops the server's cached read so the next one hits the host.
    PullRequestsInvalidate,
    "pullRequests.invalidate"
);
untyped_method!(
    /// `pullRequests.runAction`: merge, close, reopen, draft and the other actions.
    PullRequestsRunAction,
    "pullRequests.runAction"
);
untyped_method!(
    /// `pullRequests.comment`: posts a comment.
    PullRequestsComment,
    "pullRequests.comment"
);
/// `subscribeProjectClones`, a stream of the clone list that stays open until interrupted.
pub struct SubscribeProjectClones;

impl Method for SubscribeProjectClones {
    const TAG: &'static str = "subscribeProjectClones";
    const STREAMING: bool = true;
    type Payload = Value;
    type Success = Value;
}
/// `scheduledTasks.subscribe`, a stream of the task list that stays open until interrupted.
pub struct SubscribeScheduledTasks;

impl Method for SubscribeScheduledTasks {
    const TAG: &'static str = "scheduledTasks.subscribe";
    const STREAMING: bool = true;
    type Payload = Value;
    type Success = Value;
}
untyped_method!(
    /// `scheduledTasks.setEnabled`: pauses or resumes a task.
    ScheduledTasksSetEnabled,
    "scheduledTasks.setEnabled"
);
untyped_method!(
    /// `scheduledTasks.runNow`: starts a run of a task immediately.
    ScheduledTasksRunNow,
    "scheduledTasks.runNow"
);
untyped_method!(
    /// `scheduledTasks.upsert`: creates a task, or replaces the one `id` names.
    ScheduledTasksUpsert,
    "scheduledTasks.upsert"
);
untyped_method!(
    /// `scheduledTasks.rotateWebhookToken`: issues a new webhook URL, which ends the old one.
    ScheduledTasksRotateWebhookToken,
    "scheduledTasks.rotateWebhookToken"
);
untyped_method!(
    /// `scheduledTasks.delete`: removes a task.
    ScheduledTasksDelete,
    "scheduledTasks.delete"
);
/// `subscribeServerConfig`: a snapshot of the server config, then its changes.
pub struct SubscribeServerConfig;

impl Method for SubscribeServerConfig {
    const TAG: &'static str = "subscribeServerConfig";
    const STREAMING: bool = true;
    type Payload = Value;
    type Success = Value;
}
/// `git.runStackedAction`, a stream of progress events ending with the action's result.
pub struct GitRunStackedAction;

impl Method for GitRunStackedAction {
    const TAG: &'static str = "git.runStackedAction";
    const STREAMING: bool = true;
    type Payload = Value;
    type Success = Value;
}

/// Request ids are bigints on the wire, so they travel as strings.
fn id_text<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    match Value::deserialize(deserializer)? {
        Value::String(text) => Ok(text),
        Value::Number(number) => Ok(number.to_string()),
        other => Err(serde::de::Error::custom(format!("bad request id: {other}"))),
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "_tag")]
enum ServerFrame {
    Pong,
    Chunk {
        #[serde(rename = "requestId", deserialize_with = "id_text")]
        request_id: String,
        values: Vec<Value>,
    },
    Exit {
        #[serde(rename = "requestId", deserialize_with = "id_text")]
        request_id: String,
        exit: ExitFrame,
    },
    Defect {
        defect: Value,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "_tag")]
enum ExitFrame {
    Success { value: Value },
    Failure { cause: Vec<CauseFrame> },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "_tag")]
enum CauseFrame {
    Fail { error: Value },
    Die { defect: Value },
    Interrupt,
}

/// What a server frame means to the caller.
#[derive(Debug, PartialEq)]
pub enum Event {
    Pong,
    /// One element of a stream. The element is already acknowledged in `Reaction::send`.
    Item {
        request_id: String,
        value: Value,
    },
    /// The request finished successfully (a stream ended, or a unary call returned `value`).
    Done {
        request_id: String,
        value: Value,
    },
    /// The request failed with a typed error, a defect, or an interrupt.
    Failed {
        request_id: String,
        message: String,
    },
    /// A defect not tied to one request.
    Defect(String),
    /// Frame this client does not understand; ignored but proves the server is alive.
    Ignored,
}

#[derive(Debug, Default, PartialEq)]
pub struct Reaction {
    /// Frames to write back (an Ack per Chunk).
    pub send: Vec<String>,
    pub events: Vec<Event>,
}

/// Request id bookkeeping for one socket. Create a new one per connection.
#[derive(Default)]
pub struct Session {
    next_id: u64,
    open: HashSet<String>,
    slow: Tracker,
}

impl Session {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the request id and the frame to send.
    pub fn request<M: Method>(&mut self, payload: &M::Payload) -> Result<(String, String), String> {
        let id = self.next_id.to_string();
        self.next_id += 1;
        let frame = json!({
            "_tag": "Request",
            "id": id,
            "tag": M::TAG,
            "payload": serde_json::to_value(payload).map_err(|error| error.to_string())?,
            "headers": [],
        });
        self.open.insert(id.clone());
        if !M::STREAMING {
            self.slow
                .start(&id, M::TAG, Instant::now(), SystemTime::now());
        }
        Ok((id, frame.to_string()))
    }

    /// The unary requests still waiting past their threshold, in the order they turned slow.
    pub fn slow(&self, now: Instant) -> Vec<SlowRequest> {
        self.slow.slow(now)
    }

    /// When the next waiting request turns slow.
    pub fn next_slow(&self, now: Instant) -> Option<Duration> {
        self.slow.next_slow(now)
    }

    /// The frame that stops a stream, as `Fiber.interrupt` does on the web client.
    pub fn interrupt(&mut self, id: &str) -> String {
        self.open.remove(id);
        self.slow.finish(id);
        json!({ "_tag": "Interrupt", "requestId": id }).to_string()
    }

    pub fn react(&mut self, text: &str) -> Reaction {
        let mut reaction = Reaction::default();
        let Ok(frame) = serde_json::from_str::<ServerFrame>(text) else {
            reaction.events.push(Event::Ignored);
            return reaction;
        };
        match frame {
            ServerFrame::Pong => reaction.events.push(Event::Pong),
            ServerFrame::Chunk { request_id, values } => {
                // The server holds the next chunk until the previous one is acked.
                reaction
                    .send
                    .push(json!({ "_tag": "Ack", "requestId": request_id }).to_string());
                for value in values {
                    reaction.events.push(Event::Item {
                        request_id: request_id.clone(),
                        value,
                    });
                }
            }
            ServerFrame::Exit { request_id, exit } => {
                self.open.remove(&request_id);
                self.slow.finish(&request_id);
                reaction.events.push(match exit {
                    ExitFrame::Success { value } => Event::Done { request_id, value },
                    ExitFrame::Failure { cause } => Event::Failed {
                        request_id,
                        message: describe_cause(&cause),
                    },
                });
            }
            ServerFrame::Defect { defect } => {
                reaction.events.push(Event::Defect(defect.to_string()))
            }
            ServerFrame::Unknown => reaction.events.push(Event::Ignored),
        }
        reaction
    }
}

// Mirrors the web's `Cause.squash(cause).message`: the first failure wins, then the first defect.
// Contract errors compute `message` from a getter that is not on the wire, so fall back to the
// fields those getters usually return.
fn describe_cause(cause: &[CauseFrame]) -> String {
    let squashed = cause
        .iter()
        .find_map(|entry| match entry {
            CauseFrame::Fail { error } => Some(error),
            _ => None,
        })
        .or_else(|| {
            cause.iter().find_map(|entry| match entry {
                CauseFrame::Die { defect } => Some(defect),
                _ => None,
            })
        });
    match squashed {
        Some(error) => error_message(error),
        None if cause.is_empty() => "request failed".to_owned(),
        None => "interrupted".to_owned(),
    }
}

/// Leads the message of a pull request failure the host reported as not found, so the panel can
/// title it as the web does; `split_not_found` removes it.
pub const NOT_FOUND_MARK: &str = "\u{1f}not-found\u{1f}";

/// The message and whether it carries `NOT_FOUND_MARK`.
pub fn split_not_found(message: &str) -> (&str, bool) {
    match message.strip_prefix(NOT_FOUND_MARK) {
        Some(rest) => (rest, true),
        None => (message, false),
    }
}

/// `PullRequestUnavailableError.message` and `PullRequestOperationError.message`, which are
/// getters and so are not on the wire.
fn pull_request_message(error: &Value) -> Option<String> {
    let field = |key: &str| {
        error
            .get(key)
            .and_then(Value::as_str)
            .filter(|t| !t.is_empty())
    };
    match error.get("_tag").and_then(Value::as_str)? {
        "PullRequestOperationError" => {
            let text = format!(
                "Pull request operation {} failed: {}",
                field("operation")?,
                field("detail")?
            );
            Some(if field("reason") == Some("not-found") {
                format!("{NOT_FOUND_MARK}{text}")
            } else {
                text
            })
        }
        "PullRequestUnavailableError" => Some(
            match field("reason")? {
                "cli-missing" => field("requirement")
                    .unwrap_or("The tool this host is read through is not installed or set up."),
                "cli-unauthenticated" => {
                    field("requirement").unwrap_or("This host has no working credentials.")
                }
                "provider-unsupported" => {
                    "Change requests cannot be browsed for this project's host yet."
                }
                _ => return None,
            }
            .to_owned(),
        ),
        _ => None,
    }
}

fn error_message(error: &Value) -> String {
    if let Some(text) = error.as_str() {
        return text.to_owned();
    }
    if let Some(text) = pull_request_message(error) {
        return text;
    }
    ["message", "reason", "detail", "_tag"]
        .iter()
        .find_map(|key| error.get(key).and_then(Value::as_str))
        .filter(|text| !text.is_empty())
        .map_or_else(|| error.to_string(), str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pull_request_failures_read_as_the_web_message() {
        let operation = json!({
            "_tag": "PullRequestOperationError",
            "operation": "detail",
            "detail": "requests are paused",
        });
        assert_eq!(
            error_message(&operation),
            "Pull request operation detail failed: requests are paused"
        );
        let missing = json!({
            "_tag": "PullRequestOperationError",
            "operation": "detail",
            "detail": "no such pull request",
            "reason": "not-found",
        });
        let message = error_message(&missing);
        let (text, not_found) = split_not_found(&message);
        assert!(not_found);
        assert_eq!(
            text,
            "Pull request operation detail failed: no such pull request"
        );
        assert_eq!(split_not_found("plain"), ("plain", false));
        let unavailable = json!({
            "_tag": "PullRequestUnavailableError",
            "reason": "cli-unauthenticated",
            "requirement": "Run gh auth login.",
        });
        assert_eq!(error_message(&unavailable), "Run gh auth login.");
        assert_eq!(
            error_message(&json!({"_tag": "PullRequestUnavailableError", "reason": "cli-missing"})),
            "The tool this host is read through is not installed or set up."
        );
    }

    #[test]
    fn request_frame_matches_the_effect_envelope() {
        let mut session = Session::new();
        let payload = wire::SubscribeShellPayload {
            request_completion_marker: Some(true),
            ..Default::default()
        };
        let (id, frame) = session.request::<SubscribeShell>(&payload).expect("frame");
        let value: Value = serde_json::from_str(&frame).expect("json");
        assert_eq!(id, "0");
        assert_eq!(
            value,
            json!({
                "_tag": "Request",
                "id": "0",
                "tag": "orchestration.subscribeShell",
                "payload": { "requestCompletionMarker": true },
                "headers": [],
            })
        );
        let (second, _) = session.request::<SubscribeShell>(&payload).expect("frame");
        assert_eq!(second, "1");
    }

    #[test]
    fn interrupt_frame_names_the_request() {
        assert_eq!(
            Session::new().interrupt("4"),
            r#"{"_tag":"Interrupt","requestId":"4"}"#
        );
    }

    #[test]
    fn chunk_is_acked_once_and_yields_each_value() {
        let mut session = Session::new();
        let reaction =
            session.react(r#"{"_tag":"Chunk","requestId":"3","values":[{"a":1},{"a":2}]}"#);
        assert_eq!(
            reaction.send,
            vec![r#"{"_tag":"Ack","requestId":"3"}"#.to_owned()]
        );
        assert_eq!(
            reaction.events,
            vec![
                Event::Item {
                    request_id: "3".into(),
                    value: json!({"a":1})
                },
                Event::Item {
                    request_id: "3".into(),
                    value: json!({"a":2})
                },
            ]
        );
    }

    #[test]
    fn numeric_request_ids_are_accepted() {
        let reaction = Session::new().react(r#"{"_tag":"Chunk","requestId":7,"values":[1]}"#);
        assert_eq!(
            reaction.send,
            vec![r#"{"_tag":"Ack","requestId":"7"}"#.to_owned()]
        );
    }

    #[test]
    fn exit_success_failure_defect_and_pong() {
        let mut session = Session::new();
        assert_eq!(
            session
                .react(r#"{"_tag":"Exit","requestId":"1","exit":{"_tag":"Success","value":null}}"#)
                .events,
            vec![Event::Done {
                request_id: "1".into(),
                value: Value::Null
            }]
        );
        let failed = session.react(
            r#"{"_tag":"Exit","requestId":"2","exit":{"_tag":"Failure","cause":[{"_tag":"Fail","error":{"_tag":"Nope"}},{"_tag":"Interrupt","fiberId":1}]}}"#,
        );
        assert_eq!(
            failed.events,
            vec![Event::Failed {
                request_id: "2".into(),
                message: "Nope".into()
            }]
        );
        let reasoned = session.react(
            r#"{"_tag":"Exit","requestId":"3","exit":{"_tag":"Failure","cause":[{"_tag":"Die","defect":"boom"},{"_tag":"Fail","error":{"_tag":"DeviceEasAccountError","reason":"No Expo credential"}}]}}"#,
        );
        assert_eq!(
            reasoned.events,
            vec![Event::Failed {
                request_id: "3".into(),
                message: "No Expo credential".into()
            }]
        );
        assert!(matches!(
            session.react(r#"{"_tag":"Defect","defect":"boom"}"#).events[0],
            Event::Defect(_)
        ));
        assert_eq!(
            session.react(r#"{"_tag":"Pong"}"#).events,
            vec![Event::Pong]
        );
        assert_eq!(session.react("not json").events, vec![Event::Ignored]);
        assert_eq!(
            session.react(r#"{"_tag":"Mystery"}"#).events,
            vec![Event::Ignored]
        );
    }
}
