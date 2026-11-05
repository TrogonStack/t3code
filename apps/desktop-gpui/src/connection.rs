//! The authenticated RPC WebSocket: connect, keep alive, reconnect.

use crate::auth::{self, AuthError, Credential};
use crate::rpc::{Event, PING_FRAME, Session, SubscribeShell};
use crate::shell::ShellState;
use crate::status::{Counts, Status, StatusSender, publish};
use crate::wire::{ShellItem, SubscribeShellPayload};
use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::time::{Instant, MissedTickBehavior, interval, sleep, sleep_until};
use tokio_tungstenite::tungstenite::Message;

const PING_INTERVAL: Duration = Duration::from_secs(5);
// Pings go out every 5 s and a busy server stops answering them, so allow 15 s
// without any frame (packages/client-runtime/src/rpc/protocol.ts PING_TIMEOUT).
const PING_TIMEOUT: Duration = Duration::from_secs(15);
const ORCHESTRATION_PROTOCOL_VERSION: &str = "2";

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

/// Connects and stays connected until cancelled, reporting through `status`.
pub async fn run(http_base: &str, credential: &Credential, label: &str, status: &StatusSender) {
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
                        label: label.to_owned(),
                        counts: None,
                    },
                );
                backoff.reset();
                serve(socket, label, status).await
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
    let ticket = match auth::issue_ws_ticket(client, http_base, &token).await {
        Ok(ticket) => ticket,
        Err(error) => {
            if matches!(error, AuthError::Rejected(_)) {
                *bearer = None;
            }
            return Err(error.to_string());
        }
    };
    let url = socket_url(http_base, &ticket)?;
    let (socket, _) = tokio_tungstenite::connect_async(url.as_str())
        .await
        .map_err(|error| error.to_string())?;
    Ok(socket)
}

/// Subscribes to the shell stream and keeps the socket alive; returns why it ended.
async fn serve(socket: Socket, label: &str, status: &StatusSender) -> String {
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
                        if let Some(reason) = handle(event, &mut shell, label, status) {
                            return reason;
                        }
                    }
                }
                Some(Ok(_)) => deadline = Instant::now() + PING_TIMEOUT,
            }
        }
    }
}

/// Applies one event; `Some(reason)` means the subscription is over and the socket should reopen.
fn handle(
    event: Event,
    shell: &mut ShellState,
    label: &str,
    status: &StatusSender,
) -> Option<String> {
    match event {
        Event::Item { value, .. } => match serde_json::from_value::<ShellItem>(value) {
            Ok(item) => {
                if shell.apply(item) && shell.is_seeded() {
                    publish(
                        status,
                        Status::Connected {
                            label: label.to_owned(),
                            counts: Some(Counts {
                                projects: shell.project_count(),
                                threads: shell.thread_count(),
                            }),
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
