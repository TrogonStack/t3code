//! What the composer sends, mirroring the web's `useComposerSend` and `startThreadTurn`
//! (`packages/client-runtime/src/operations/commands.ts`): a draft becomes a thread through
//! `orchestration.launchThread` with its first message, and a message in an open thread is a
//! `message.dispatch` command. The payloads are untyped JSON like the other unary calls; the
//! tests check them against `rpc.schema.json`.

use crate::connection::Call;
use crate::strip::{Branch, EnvMode, RuntimeMode};
use serde_json::{Value, json};

/// `truncate`'s default in `packages/shared/src/String.ts`.
const TITLE_MAX_CHARS: usize = 50;
const NEW_THREAD_TITLE: &str = "New thread";
/// Both the web and the desktop shell it wraps report the web as the creation source.
const CREATION_SOURCE: &str = "web";

/// Whether the send button is live: text to send and no send already waiting on the server.
pub fn can_send(text: &str, in_flight: bool) -> bool {
    !in_flight && !text.trim().is_empty()
}

/// What the primary button does, as `ComposerPrimaryActions` picks it: a running turn with
/// nothing typed offers Stop; anything else is Send.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Primary {
    Send { enabled: bool },
    Stop,
}

pub fn primary(running: bool, text: &str, in_flight: bool) -> Primary {
    if running && text.trim().is_empty() {
        Primary::Stop
    } else {
        Primary::Send {
            enabled: can_send(text, in_flight),
        }
    }
}

/// `deriveThreadTitleSeed` for a text-only message: whitespace collapsed, cut at 50 characters.
pub fn title_seed(text: &str) -> String {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return NEW_THREAD_TITLE.to_owned();
    }
    let mut chars = normalized.chars();
    let head: String = chars.by_ref().take(TITLE_MAX_CHARS).collect();
    if chars.next().is_some() {
        format!("{head}...")
    } else {
        head
    }
}

/// A model on a provider instance, the `modelSelection` the contract takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Model<'a> {
    pub instance_id: &'a str,
    pub slug: &'a str,
}

impl Model<'_> {
    fn selection(self) -> Value {
        json!({"instanceId": self.instance_id, "model": self.slug})
    }
}

/// Where a draft's thread runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Workspace<'a> {
    /// The project's checkout, on `branch` when it is known.
    Root { branch: Option<&'a str> },
    /// A new worktree started from `base`.
    Worktree {
        base: &'a str,
        start_from_origin: bool,
    },
}

impl<'a> Workspace<'a> {
    /// `planComposerTurn`: worktree mode needs an explicit base ref, so a missing one is an
    /// error instead of a silent local run.
    pub fn resolve(
        mode: EnvMode,
        shown: Option<&'a Branch>,
        start_from_origin: bool,
    ) -> Result<Self, String> {
        let name = shown.and_then(|branch| branch.name.as_deref());
        match (mode, name) {
            (EnvMode::Local, branch) => Ok(Self::Root { branch }),
            (EnvMode::Worktree, Some(base)) => Ok(Self::Worktree {
                base,
                start_from_origin,
            }),
            (EnvMode::Worktree, None) => {
                Err("Select a base branch before sending in New worktree mode.".to_owned())
            }
        }
    }

    fn strategy(self) -> Value {
        match self {
            Self::Root { branch: None } => json!({"type": "root"}),
            Self::Root {
                branch: Some(branch),
            } => json!({"type": "root", "branch": branch}),
            Self::Worktree {
                base,
                start_from_origin: true,
            } => json!({"type": "worktree", "baseRef": base, "startFromOrigin": true}),
            Self::Worktree { base, .. } => json!({"type": "worktree", "baseRef": base}),
        }
    }
}

/// How a message reaches a thread, `resolveComposerDispatchMode` with the default
/// `followUpBehavior` of steering a running turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delivery {
    Auto,
    Steer,
}

impl Delivery {
    pub fn of(running: bool) -> Self {
        if running { Self::Steer } else { Self::Auto }
    }

    fn intent(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Steer => "steer",
        }
    }
}

/// The ids a send mints; the caller supplies them so the plan stays deterministic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ids {
    pub command: String,
    pub thread: String,
    pub message: String,
}

/// The composer as it stands when the user sends.
pub struct Composer<'a> {
    /// The open thread; `None` for a draft.
    pub thread: Option<&'a str>,
    /// The draft's project.
    pub project: Option<&'a str>,
    pub text: &'a str,
    /// The model the trigger shows.
    pub model: Option<Model<'a>>,
    /// Whether the user picked it, so an open thread keeps its own model otherwise.
    pub picked: bool,
    pub runtime_mode: RuntimeMode,
    pub workspace: Result<Workspace<'a>, String>,
    pub running: bool,
}

/// One request to make, and the thread the window opens once it succeeds.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub call: Call,
    pub payload: Value,
    pub opens: Option<String>,
}

/// The first send of a draft launches its thread with the message; every later one dispatches.
pub fn plan(composer: &Composer, ids: Ids) -> Result<Plan, String> {
    let text = composer.text.trim();
    if text.is_empty() {
        return Err("There is nothing to send.".to_owned());
    }
    match composer.thread {
        Some(thread) => Ok(Plan {
            call: Call::DispatchCommand,
            payload: dispatch_payload(
                &ids.command,
                thread,
                &ids.message,
                text,
                composer.model.filter(|_| composer.picked),
                Delivery::of(composer.running),
            ),
            opens: None,
        }),
        None => {
            let project = composer.project.ok_or("There is no project to start in.")?;
            let model = composer.model.ok_or("No model is available.")?;
            let workspace = composer.workspace.clone()?;
            Ok(Plan {
                call: Call::LaunchThread,
                payload: launch_payload(
                    &ids,
                    project,
                    text,
                    model,
                    composer.runtime_mode,
                    workspace,
                ),
                opens: Some(ids.thread),
            })
        }
    }
}

/// `orchestration.launchThread` as `startThreadTurn` sends it for a draft.
pub fn launch_payload(
    ids: &Ids,
    project_id: &str,
    text: &str,
    model: Model,
    runtime_mode: RuntimeMode,
    workspace: Workspace,
) -> Value {
    json!({
        "commandId": ids.command,
        "creationSource": CREATION_SOURCE,
        "threadId": ids.thread,
        "projectId": project_id,
        "title": title_seed(text),
        "generateTitle": true,
        "modelSelection": model.selection(),
        "runtimeMode": runtime_mode.wire(),
        "interactionMode": "default",
        "workspaceStrategy": workspace.strategy(),
        "initialMessage": {
            "messageId": ids.message,
            "text": text,
            "attachments": [],
        },
    })
}

/// `message.dispatch` as `startThreadTurn` sends it to an existing thread.
pub fn dispatch_payload(
    command_id: &str,
    thread_id: &str,
    message_id: &str,
    text: &str,
    model: Option<Model>,
    delivery: Delivery,
) -> Value {
    let mut command = json!({
        "type": "message.dispatch",
        "commandId": command_id,
        "createdBy": "user",
        "creationSource": CREATION_SOURCE,
        "threadId": thread_id,
        "messageId": message_id,
        "text": text,
        "attachments": [],
        "titleSeed": title_seed(text),
        "deliveryIntent": delivery.intent(),
        "dispatchMode": {"type": "start_immediately"},
    });
    if let Some(model) = model {
        command["modelSelection"] = model.selection();
    }
    command
}

/// `thread.stop`, which the web sends when the server reports `capabilities.threadStop`.
pub fn stop_payload(command_id: &str, thread_id: &str) -> Value {
    json!({"type": "thread.stop", "commandId": command_id, "threadId": thread_id})
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn ids() -> Ids {
        Ids {
            command: "gpui-1".to_owned(),
            thread: "thread-1".to_owned(),
            message: "message-1".to_owned(),
        }
    }

    const MODEL: Model = Model {
        instance_id: "codex",
        slug: "gpt-5",
    };

    fn branch(name: &str) -> Branch {
        Branch {
            name: Some(name.to_owned()),
            is_remote: Some(false),
        }
    }

    fn composer<'a>(text: &'a str, workspace: Result<Workspace<'a>, String>) -> Composer<'a> {
        Composer {
            thread: None,
            project: Some("project-1"),
            text,
            model: Some(MODEL),
            picked: false,
            runtime_mode: RuntimeMode::FullAccess,
            workspace,
            running: false,
        }
    }

    /// The schema's definition for `payload`: the method's own, or the union member whose `type` it names.
    fn definition(schema: &Value, method: &str, payload: &Value) -> Value {
        let target = schema["methods"][method]["payload"]["$ref"]
            .as_str()
            .and_then(|r| r.strip_prefix("#/$defs/"))
            .unwrap_or_else(|| panic!("{method} has no payload ref"));
        let def = &schema["$defs"][target];
        let Some(members) = def["anyOf"].as_array() else {
            return def.clone();
        };
        members
            .iter()
            .find(|member| member["properties"]["type"]["enum"][0] == payload["type"])
            .unwrap_or_else(|| panic!("{method} has no member for {}", payload["type"]))
            .clone()
    }

    /// Every required property is present and nothing is named that the contract lacks.
    fn assert_matches_contract(method: &str, payload: &Value) {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../packages/contracts/schema/rpc.schema.json"
        );
        let schema: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let def = definition(&schema, method, payload);
        let keys = |value: &Value| -> BTreeSet<String> {
            value.as_object().unwrap().keys().cloned().collect()
        };
        let sent = keys(payload);
        let known = keys(&def["properties"]);
        let required: BTreeSet<String> = def["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|name| name.as_str().unwrap().to_owned())
            .collect();
        assert!(
            required.is_subset(&sent),
            "missing {:?}",
            required.difference(&sent)
        );
        assert!(
            sent.is_subset(&known),
            "unknown {:?}",
            sent.difference(&known)
        );
    }

    #[test]
    fn a_draft_launches_its_thread_with_the_first_message() {
        let root = Workspace::Root {
            branch: Some("main"),
        };
        let plan = plan(&composer("  fix it\n now ", Ok(root)), ids()).unwrap();
        assert_eq!(plan.call, Call::LaunchThread);
        assert_eq!(plan.opens.as_deref(), Some("thread-1"));
        assert_eq!(
            plan.payload,
            json!({
                "commandId": "gpui-1",
                "creationSource": "web",
                "threadId": "thread-1",
                "projectId": "project-1",
                "title": "fix it now",
                "generateTitle": true,
                "modelSelection": {"instanceId": "codex", "model": "gpt-5"},
                "runtimeMode": "full-access",
                "interactionMode": "default",
                "workspaceStrategy": {"type": "root", "branch": "main"},
                "initialMessage": {
                    "messageId": "message-1",
                    "text": "fix it\n now",
                    "attachments": [],
                },
            })
        );
        assert_matches_contract("orchestration.launchThread", &plan.payload);
    }

    #[test]
    fn a_worktree_draft_names_its_base_ref() {
        let main = branch("main");
        let origin = Workspace::resolve(EnvMode::Worktree, Some(&main), true).unwrap();
        let local = Workspace::resolve(EnvMode::Worktree, Some(&main), false).unwrap();
        let strategy = |workspace| {
            plan(&composer("go", Ok(workspace)), ids()).unwrap().payload["workspaceStrategy"]
                .clone()
        };
        assert_eq!(
            strategy(origin),
            json!({"type": "worktree", "baseRef": "main", "startFromOrigin": true})
        );
        assert_eq!(
            strategy(local),
            json!({"type": "worktree", "baseRef": "main"})
        );
        assert_matches_contract(
            "orchestration.launchThread",
            &plan(&composer("go", Ok(origin)), ids()).unwrap().payload,
        );
    }

    #[test]
    fn worktree_mode_without_a_base_ref_does_not_send() {
        let nameless = Branch {
            name: None,
            is_remote: None,
        };
        for shown in [None, Some(&nameless)] {
            let workspace = Workspace::resolve(EnvMode::Worktree, shown, true);
            assert!(workspace.is_err());
            assert!(plan(&composer("go", workspace), ids()).is_err());
        }
        assert_eq!(
            Workspace::resolve(EnvMode::Local, None, true),
            Ok(Workspace::Root { branch: None })
        );
    }

    #[test]
    fn a_message_in_an_open_thread_dispatches() {
        let mut open = composer("again", Ok(Workspace::Root { branch: None }));
        open.thread = Some("thread-9");
        let plan = plan(&open, ids()).unwrap();
        assert_eq!(plan.call, Call::DispatchCommand);
        assert_eq!(plan.opens, None);
        assert_eq!(
            plan.payload,
            json!({
                "type": "message.dispatch",
                "commandId": "gpui-1",
                "createdBy": "user",
                "creationSource": "web",
                "threadId": "thread-9",
                "messageId": "message-1",
                "text": "again",
                "attachments": [],
                "titleSeed": "again",
                "deliveryIntent": "auto",
                "dispatchMode": {"type": "start_immediately"},
            })
        );
        assert_matches_contract("orchestration.dispatchCommand", &plan.payload);
    }

    #[test]
    fn an_open_thread_changes_its_model_only_when_one_was_picked() {
        let mut open = composer("again", Ok(Workspace::Root { branch: None }));
        open.thread = Some("thread-9");
        let selection =
            |open: &Composer| plan(open, ids()).unwrap().payload["modelSelection"].clone();
        assert_eq!(selection(&open), Value::Null);
        open.picked = true;
        assert_eq!(
            selection(&open),
            json!({"instanceId": "codex", "model": "gpt-5"})
        );
        assert_matches_contract(
            "orchestration.dispatchCommand",
            &plan(&open, ids()).unwrap().payload,
        );
    }

    #[test]
    fn a_running_turn_is_steered() {
        let mut open = composer("wait", Ok(Workspace::Root { branch: None }));
        open.thread = Some("thread-9");
        open.running = true;
        let payload = plan(&open, ids()).unwrap().payload;
        assert_eq!(payload["deliveryIntent"], "steer");
        assert_eq!(
            payload["dispatchMode"],
            json!({"type": "start_immediately"})
        );
    }

    #[test]
    fn the_runtime_mode_goes_out_as_the_contract_spells_it() {
        for mode in RuntimeMode::ALL {
            let mut draft = composer("go", Ok(Workspace::Root { branch: None }));
            draft.runtime_mode = mode;
            let payload = plan(&draft, ids()).unwrap().payload;
            assert_eq!(
                RuntimeMode::parse(payload["runtimeMode"].as_str().unwrap()),
                Some(mode)
            );
        }
    }

    #[test]
    fn an_empty_message_or_a_draft_without_a_model_is_not_planned() {
        let root = || Ok(Workspace::Root { branch: None });
        assert!(plan(&composer("  \n ", root()), ids()).is_err());
        let mut no_model = composer("go", root());
        no_model.model = None;
        assert!(plan(&no_model, ids()).is_err());
        let mut no_project = composer("go", root());
        no_project.project = None;
        assert!(plan(&no_project, ids()).is_err());
    }

    #[test]
    fn stop_is_one_command() {
        let payload = stop_payload("gpui-2", "thread-9");
        assert_eq!(
            payload,
            json!({"type": "thread.stop", "commandId": "gpui-2", "threadId": "thread-9"})
        );
        assert_matches_contract("orchestration.dispatchCommand", &payload);
    }

    #[test]
    fn send_is_enabled_by_text_and_blocked_while_in_flight() {
        assert!(can_send("hi", false));
        assert!(!can_send(" \n", false));
        assert!(!can_send("hi", true));
    }

    #[test]
    fn a_running_turn_with_an_empty_composer_offers_stop() {
        assert_eq!(primary(true, "", false), Primary::Stop);
        assert_eq!(primary(true, "  ", true), Primary::Stop);
        assert_eq!(
            primary(true, "more", false),
            Primary::Send { enabled: true }
        );
        assert_eq!(primary(false, "", false), Primary::Send { enabled: false });
        assert_eq!(primary(false, "hi", true), Primary::Send { enabled: false });
    }

    #[test]
    fn titles_collapse_whitespace_and_cut_at_fifty_characters() {
        assert_eq!(title_seed("  a \n\t b  "), "a b");
        assert_eq!(title_seed(""), "New thread");
        let fifty = "x".repeat(50);
        assert_eq!(title_seed(&fifty), fifty);
        assert_eq!(title_seed(&format!("{fifty}y")), format!("{fifty}..."));
        // Characters, not bytes.
        let wide = "\u{e9}".repeat(51);
        assert_eq!(title_seed(&wide), format!("{}...", "\u{e9}".repeat(50)));
    }
}
