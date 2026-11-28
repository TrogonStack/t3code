//! The environment side of T3 Connect: trading the relay's one-time bootstrap credential for a
//! DPoP-bound access token, and the DPoP-bound WebSocket ticket. Mirrors
//! `exchangeRemoteDpopAccessToken` and `resolveRemoteDpopWebSocketConnectionUrl` in
//! `packages/client-runtime/src/authorization/remote.ts`.

use std::sync::Arc;

use serde::Deserialize;

use crate::auth::{AuthError, now_ms};
use crate::dpop::{DpopKey, normalize_htu, random_jti};

const GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:token-exchange";
const ACCESS_TOKEN_TYPE: &str = "urn:ietf:params:oauth:token-type:access_token";
const BOOTSTRAP_TOKEN_TYPE: &str = "urn:t3:params:oauth:token-type:environment-bootstrap";

/// A DPoP-bound environment session: the key that signs every request and the token it binds.
#[derive(Clone)]
pub struct DpopSession {
    pub key: Arc<DpopKey>,
    pub access_token: String,
    /// The socket base the relay named; `None` derives it from the HTTP base.
    pub ws_base: Option<String>,
    /// `relay` or `direct`, as the server records it.
    pub connection_method: &'static str,
}

impl std::fmt::Debug for DpopSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DpopSession")
            .field("access_token", &"<redacted>")
            .field("ws_base", &self.ws_base)
            .field("connection_method", &self.connection_method)
            .finish()
    }
}

/// A freshly exchanged token. It lives an hour; a rejected one is replaced by a new exchange.
pub struct DpopToken {
    pub access_token: String,
}

#[derive(Deserialize)]
struct TokenResult {
    access_token: String,
}

#[derive(Deserialize)]
struct TicketResult {
    ticket: String,
}

fn proof(
    key: &DpopKey,
    method: &str,
    url: &str,
    access_token: Option<&str>,
) -> Result<String, AuthError> {
    let jti = random_jti().map_err(AuthError::Failed)?;
    key.proof(method, url, access_token, now_ms() / 1_000, &jti)
        .map_err(AuthError::Failed)
}

/// `POST /oauth/token` with a proof for that URL and no access token yet.
pub async fn exchange_dpop_token(
    client: &reqwest::Client,
    http_base: &str,
    key: &DpopKey,
    bootstrap_credential: &str,
) -> Result<DpopToken, AuthError> {
    let url = format!("{}/oauth/token", http_base.trim_end_matches('/'));
    let response = client
        .post(&url)
        .header("dpop", proof(key, "POST", &url, None)?)
        .form(&[
            ("grant_type", GRANT_TYPE),
            ("subject_token", bootstrap_credential),
            ("subject_token_type", BOOTSTRAP_TOKEN_TYPE),
            ("requested_token_type", ACCESS_TOKEN_TYPE),
            ("client_label", "T3 Code Desktop (GPUI)"),
            ("client_device_type", "desktop"),
        ])
        .send()
        .await?;
    let status = response.status();
    if status.is_client_error() {
        return Err(AuthError::Rejected(status.as_u16()));
    }
    let result: TokenResult = response.error_for_status()?.json().await?;
    Ok(DpopToken {
        access_token: result.access_token,
    })
}

/// An RPC ticket bound to the key. A server without the `/:use` route answers 404 and gets the
/// bare path, which needs its own proof because the proof names the path it signs.
pub async fn issue_dpop_ws_ticket(
    client: &reqwest::Client,
    http_base: &str,
    key: &DpopKey,
    access_token: &str,
) -> Result<String, AuthError> {
    let base = http_base.trim_end_matches('/');
    let mint = |path: &'static str| async move {
        let url = format!("{base}{path}");
        client
            .post(&url)
            .header("authorization", format!("DPoP {access_token}"))
            .header("dpop", proof(key, "POST", &url, Some(access_token))?)
            .send()
            .await
            .map_err(AuthError::from)
    };
    let mut response = mint("/api/auth/websocket-ticket/rpc").await?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        response = mint("/api/auth/websocket-ticket").await?;
    }
    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(AuthError::Rejected(status.as_u16()));
    }
    let result: TicketResult = response.error_for_status()?.json().await?;
    Ok(result.ticket)
}

/// The proof that rides in the `dpop` query parameter of the upgrade (a socket cannot set
/// headers). It names the socket URL as the HTTP request the server sees, and its `ath` is the
/// ticket.
pub fn upgrade_proof(
    key: &DpopKey,
    ws_url: &reqwest::Url,
    ticket: &str,
) -> Result<String, AuthError> {
    let scheme = if ws_url.scheme() == "wss" {
        "https"
    } else {
        "http"
    };
    let host = ws_url.host_str().unwrap_or_default();
    let authority = match ws_url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_owned(),
    };
    let target = format!("{scheme}://{authority}{}", ws_url.path());
    let htu = normalize_htu(&target)
        .ok_or_else(|| AuthError::Failed("DPoP URL is invalid.".to_owned()))?;
    proof(key, "GET", &htu, Some(ticket))
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;

    fn payload(proof: &str) -> serde_json::Value {
        serde_json::from_slice(&B64.decode(proof.split('.').nth(1).unwrap()).unwrap()).unwrap()
    }

    #[test]
    fn the_upgrade_proof_names_the_https_socket_url_and_binds_the_ticket() {
        let key = DpopKey::generate().unwrap();
        let url = reqwest::Url::parse("wss://env.example:8443/ws?wsTicket=t&x=1").unwrap();
        let claims = payload(&upgrade_proof(&key, &url, "ticket-1").unwrap());
        assert_eq!(claims["htm"], "GET");
        assert_eq!(claims["htu"], "https://env.example:8443/ws");
        assert_eq!(
            claims["ath"],
            B64.encode(<sha2::Sha256 as sha2::Digest>::digest(b"ticket-1"))
        );
    }

    #[test]
    fn a_plain_socket_maps_to_http() {
        let key = DpopKey::generate().unwrap();
        let url = reqwest::Url::parse("ws://127.0.0.1:3773/ws").unwrap();
        let claims = payload(&upgrade_proof(&key, &url, "t").unwrap());
        assert_eq!(claims["htu"], "http://127.0.0.1:3773/ws");
    }

    #[test]
    fn a_session_never_debugs_its_token() {
        let session = DpopSession {
            key: Arc::new(DpopKey::generate().unwrap()),
            access_token: "SECRET".into(),
            ws_base: None,
            connection_method: "relay",
        };
        assert!(!format!("{session:?}").contains("SECRET"));
    }
}
