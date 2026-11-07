//! Effect RPC over the WebSocket in its JSON serialization: request, ack, and response frames.
//! Shapes follow effect's `RpcMessage` (`FromClientEncoded`, `FromServerEncoded`).

use crate::wire;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json};
use std::collections::HashSet;

/// RpcMessage.constPing in the JSON serialization.
pub const PING_FRAME: &str = r#"{"_tag":"Ping"}"#;

/// One RPC method: its tag and the generated wire types for its payload and success.
pub trait Method {
    const TAG: &'static str;
    type Payload: Serialize;
    type Success: DeserializeOwned;
}

/// `orchestration.subscribeShell`, a stream of shell snapshot and delta items.
pub struct SubscribeShell;

impl Method for SubscribeShell {
    const TAG: &'static str = "orchestration.subscribeShell";
    type Payload = wire::SubscribeShellPayload;
    type Success = wire::ShellItem;
}

/// `orchestration.subscribeThread`, a stream of one thread's snapshot and events. The success
/// type is opaque in the schema; `thread::ThreadState` reads the items.
pub struct SubscribeThread;

impl Method for SubscribeThread {
    const TAG: &'static str = "orchestration.subscribeThread";
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
        Ok((id, frame.to_string()))
    }

    /// The frame that stops a stream, as `Fiber.interrupt` does on the web client.
    pub fn interrupt(&mut self, id: &str) -> String {
        self.open.remove(id);
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

fn describe_cause(cause: &[CauseFrame]) -> String {
    let parts: Vec<String> = cause
        .iter()
        .map(|entry| match entry {
            CauseFrame::Fail { error } => format!("failed: {error}"),
            CauseFrame::Die { defect } => format!("defect: {defect}"),
            CauseFrame::Interrupt => "interrupted".to_owned(),
        })
        .collect();
    if parts.is_empty() {
        "request failed".to_owned()
    } else {
        parts.join("; ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
                message: r#"failed: {"_tag":"Nope"}; interrupted"#.into()
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
