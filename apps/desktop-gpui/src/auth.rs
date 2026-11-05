//! Desktop bootstrap credential and the bearer/WebSocket-ticket exchange.
//! Mirrors packages/shared/src/desktopBootstrapToken.ts and
//! packages/client-runtime/src/authorization/remote.ts.

use crate::electron_dpop;
use hmac::{Hmac, KeyInit, Mac};
use p256::ecdsa::SigningKey;
use serde::Deserialize;
use sha2::Sha256;
use std::fmt::Write;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

const TOKEN_WINDOW_MS: u64 = 12 * 60 * 60 * 1000;
const GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:token-exchange";
const ACCESS_TOKEN_TYPE: &str = "urn:ietf:params:oauth:token-type:access_token";
const BOOTSTRAP_TOKEN_TYPE: &str = "urn:t3:params:oauth:token-type:environment-bootstrap";
const CLIENT_LABEL: &str = "T3 Code Desktop (GPUI)";

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

/// 256 random bits, hex encoded. Only this process and its server ever see it.
pub fn generate_secret() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|error| format!("could not read random bytes: {error}"))?;
    Ok(hex::encode(bytes))
}

/// HMAC-SHA256 keyed by the secret over `t3-desktop-bootstrap:<window>`, hex encoded.
pub fn bootstrap_token(secret: &str, now_ms: u64) -> String {
    let window = now_ms / TOKEN_WINDOW_MS;
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC takes any key length");
    let mut message = String::new();
    write!(message, "t3-desktop-bootstrap:{window}").expect("writing to a String cannot fail");
    mac.update(message.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// Loopback only: an ambient HTTP(S)_PROXY must never capture traffic to the local server.
pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("the HTTP client configuration is static")
}

#[derive(Debug)]
pub enum AuthError {
    /// The server refused the credential, so a retry needs a fresh one.
    Rejected(u16),
    Failed(String),
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rejected(status) => write!(f, "server rejected the credential (HTTP {status})"),
            Self::Failed(message) => f.write_str(message),
        }
    }
}

impl From<reqwest::Error> for AuthError {
    fn from(error: reqwest::Error) -> Self {
        Self::Failed(error.without_url().to_string())
    }
}

#[derive(Deserialize)]
struct AccessTokenResult {
    access_token: String,
}

#[derive(Deserialize)]
struct TicketResult {
    ticket: String,
}

/// What the app proves itself with: the secret it handed a server it spawned,
/// a pairing token minted by `t3 pair` for a server it attaches to, or a bearer
/// session another client already saved for it (the Electron app's catalog).
pub enum Credential {
    DesktopSecret(String),
    Pairing(String),
    Bearer(String),
    // TODO(gpui-credentials): HACK. Reads Electron's private IndexedDB and non-extractable DPoP key. Replace with GPUI's own pairing + Keychain before shipping.
    /// A DPoP-bound access token and the proof key it is bound to (`Authorization: DPoP`).
    Dpop {
        token: String,
        key: Arc<SigningKey>,
    },
}

impl Credential {
    fn subject_token(&self) -> String {
        match self {
            Self::DesktopSecret(secret) => bootstrap_token(secret, now_ms()),
            Self::Pairing(token) | Self::Bearer(token) | Self::Dpop { token, .. } => token.clone(),
        }
    }
}

/// RFC 8693 exchange of a bootstrap credential for a bearer session.
pub async fn exchange_bearer(
    client: &reqwest::Client,
    http_base: &str,
    credential: &Credential,
) -> Result<String, AuthError> {
    // A saved bearer or DPoP access token is already the session; there is nothing to exchange.
    if let Credential::Bearer(token) | Credential::Dpop { token, .. } = credential {
        return Ok(token.clone());
    }
    let credential = credential.subject_token();
    let response = client
        .post(format!("{http_base}/oauth/token"))
        .form(&[
            ("grant_type", GRANT_TYPE),
            ("subject_token", credential.as_str()),
            ("subject_token_type", BOOTSTRAP_TOKEN_TYPE),
            ("requested_token_type", ACCESS_TOKEN_TYPE),
            ("client_label", CLIENT_LABEL),
            ("client_device_type", "desktop"),
        ])
        .send()
        .await?;
    let status = response.status();
    if status.is_client_error() {
        return Err(AuthError::Rejected(status.as_u16()));
    }
    let result: AccessTokenResult = response.error_for_status()?.json().await?;
    Ok(result.access_token)
}

/// Mints an RPC WebSocket ticket. A server without audience routes answers 404
/// for `/:use`, and its bare-path tickets are valid for RPC. A DPoP credential signs a proof for
/// each path it tries (packages/client-runtime/src/authorization/remote.ts).
pub async fn issue_ws_ticket(
    client: &reqwest::Client,
    http_base: &str,
    credential: &Credential,
    bearer: &str,
) -> Result<String, AuthError> {
    let mint = |path: &'static str| async move {
        let url = format!("{http_base}{path}");
        let request = client.post(&url);
        let request = match credential {
            Credential::Dpop { key, .. } => {
                let proof = electron_dpop::create_proof(key, "POST", &url, Some(bearer), now_ms())
                    .map_err(AuthError::Failed)?;
                request
                    .header("authorization", format!("DPoP {bearer}"))
                    .header("dpop", proof)
            }
            _ => request.bearer_auth(bearer),
        };
        request.send().await.map_err(AuthError::from)
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

#[cfg(test)]
mod tests {
    use super::*;

    // Computed with currentDesktopBootstrapToken from packages/shared.
    #[test]
    fn matches_the_typescript_token() {
        let vectors = [
            (
                "test-secret",
                0,
                "29cdd8a2256df92036e83f50c98bafcefa22fbf04e12d040cd53bd1680f62e4c",
            ),
            (
                "test-secret",
                1_700_000_000_000,
                "fdcc803bed0e5fcbde01221220f9abcd496feaa9b06965a0761d6927d9b5c38e",
            ),
            (
                "another-secret-value",
                86_400_000,
                "7e1f8f7f6a6ecc013ea23398711d2b7c762684f88e9f688f8bf4e5155da281ae",
            ),
        ];
        for (secret, now, expected) in vectors {
            assert_eq!(bootstrap_token(secret, now), expected);
        }
    }

    #[test]
    fn token_is_stable_inside_a_window_and_changes_across_it() {
        let start = 5 * TOKEN_WINDOW_MS;
        assert_eq!(
            bootstrap_token("s", start),
            bootstrap_token("s", start + TOKEN_WINDOW_MS - 1)
        );
        assert_ne!(
            bootstrap_token("s", start),
            bootstrap_token("s", start + TOKEN_WINDOW_MS)
        );
    }

    #[test]
    fn secrets_are_random_hex() {
        let first = generate_secret().expect("secret");
        assert_eq!(first.len(), 64);
        assert_ne!(first, generate_secret().expect("secret"));
    }
}
