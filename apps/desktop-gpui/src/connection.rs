//! The authenticated RPC WebSocket: connect, keep alive, reconnect.

use crate::auth::{self, AuthError};
use crate::config::Config;
use crate::status::{Status, StatusSender, publish};
use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::time::{Instant, MissedTickBehavior, interval, sleep, sleep_until};
use tokio_tungstenite::tungstenite::Message;

/// Effect RPC ping envelope (RpcMessage.constPing) in the JSON serialization.
const PING_FRAME: &str = r#"{"_tag":"Ping"}"#;
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

fn socket_url(port: u16, ticket: &str) -> Result<reqwest::Url, String> {
    reqwest::Url::parse_with_params(
        &format!("ws://127.0.0.1:{port}/ws"),
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
pub async fn run(config: &Config, secret: &str, label: &str, status: &StatusSender) {
    let client = auth::http_client();
    let http_base = config.http_base();
    let mut bearer: Option<String> = None;
    let mut backoff = Backoff::new();
    loop {
        publish(status, Status::Connecting);
        let failure = match connect(&client, config, &http_base, secret, &mut bearer).await {
            Ok(socket) => {
                publish(status, Status::Connected(label.to_owned()));
                backoff.reset();
                serve(socket).await
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
    config: &Config,
    http_base: &str,
    secret: &str,
    bearer: &mut Option<String>,
) -> Result<Socket, String> {
    let token = match bearer {
        Some(token) => token.clone(),
        None => {
            let token = auth::exchange_bearer(client, http_base, secret)
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
    let url = socket_url(config.port, &ticket)?;
    let (socket, _) = tokio_tungstenite::connect_async(url.as_str())
        .await
        .map_err(|error| error.to_string())?;
    Ok(socket)
}

/// Pings until the socket closes or goes quiet; returns why it ended.
async fn serve(socket: Socket) -> String {
    let (mut sink, mut stream) = socket.split();
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
                // No RPC methods yet: any frame, Pong included, proves the server is alive.
                Some(Ok(_)) => deadline = Instant::now() + PING_TIMEOUT,
            }
        }
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
        let url = socket_url(13773, "a b&c").expect("url");
        assert_eq!(url.scheme(), "ws");
        assert_eq!(url.path(), "/ws");
        let pairs: Vec<_> = url.query_pairs().into_owned().collect();
        assert!(pairs.contains(&("orchestrationProtocol".into(), "2".into())));
        assert!(pairs.contains(&("wsTicket".into(), "a b&c".into())));
    }
}
