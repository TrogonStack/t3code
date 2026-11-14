//! The managed T3 Connect relay client, mirroring
//! `packages/client-runtime/src/relay/managedRelay.ts` and the endpoints in
//! `packages/contracts/src/relay.ts`.
//!
//! A Clerk session JWT is exchanged (RFC 8693) for a DPoP-bound relay access token, which then
//! authorizes `connect` and `status` calls. Tokens are cached per scope set and dropped five
//! seconds before they expire; a token the relay rejects is dropped and fetched once more.

use std::sync::{Arc, Mutex};

use serde::Deserialize;

use crate::dpop::{DpopKey, random_jti};

pub const DEFAULT_RELAY_URL: &str = "https://relay.t3.codes";
/// The public client id the web and desktop clients use.
pub const CLIENT_ID: &str = "t3-web";
pub const CONNECT_SCOPE: &str = "environment:connect";
#[cfg(test)]
pub const STATUS_SCOPE: &str = "environment:status";

const EXPIRY_GUARD_MS: u64 = 5_000;
const GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:token-exchange";
const SUBJECT_TOKEN_TYPE: &str = "urn:ietf:params:oauth:token-type:jwt";
const ACCESS_TOKEN_TYPE: &str = "urn:ietf:params:oauth:token-type:access_token";

#[derive(Debug)]
pub enum RelayError {
    /// The relay refused the credential (401); a retry needs a fresh one.
    Rejected(u16),
    Failed(String),
}

impl std::fmt::Display for RelayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rejected(status) => write!(f, "the relay refused the credential ({status})"),
            Self::Failed(message) => f.write_str(message),
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManagedEndpoint {
    pub http_base_url: String,
    pub ws_base_url: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RelayEnvironment {
    pub environment_id: String,
    pub label: String,
    pub endpoint: ManagedEndpoint,
}

#[derive(Deserialize)]
struct ListResponse {
    environments: Vec<RelayEnvironment>,
}

/// The one-time bootstrap credential for an environment, bound to our key thumbprint.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectGrant {
    pub environment_id: String,
    pub endpoint: ManagedEndpoint,
    pub credential: String,
    pub expires_at: String,
}

impl std::fmt::Debug for ConnectGrant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectGrant")
            .field("environment_id", &self.environment_id)
            .field("endpoint", &self.endpoint)
            .field("credential", &"<redacted>")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
    scope: String,
}

#[derive(Clone)]
struct CachedToken {
    access_token: String,
    scopes: Vec<String>,
    thumbprint: String,
    expires_at_ms: u64,
}

/// `matchesCachedToken`: same key, covers every scope, and not about to expire.
fn usable(token: &CachedToken, thumbprint: &str, scopes: &[&str], now_ms: u64) -> bool {
    token.thumbprint == thumbprint
        && token.expires_at_ms > now_ms + EXPIRY_GUARD_MS
        && scopes
            .iter()
            .all(|scope| token.scopes.iter().any(|have| have == scope))
}

fn scope_set_equals(granted: &str, requested: &[&str]) -> bool {
    let mut have: Vec<&str> = granted.split_whitespace().collect();
    let mut want: Vec<&str> = requested.to_vec();
    have.sort_unstable();
    have.dedup();
    want.sort_unstable();
    want.dedup();
    have == want
}

/// The form body of the RFC 8693 exchange.
fn exchange_form(
    clerk_token: &str,
    relay_url: &str,
    scopes: &[&str],
) -> Vec<(&'static str, String)> {
    vec![
        ("grant_type", GRANT_TYPE.to_owned()),
        ("subject_token", clerk_token.to_owned()),
        ("subject_token_type", SUBJECT_TOKEN_TYPE.to_owned()),
        ("requested_token_type", ACCESS_TOKEN_TYPE.to_owned()),
        ("resource", relay_url.to_owned()),
        ("scope", scopes.join(" ")),
        ("client_id", CLIENT_ID.to_owned()),
    ]
}

/// The body `connectEnvironment` sends: the deprecated `clientKeyThumbprint` alias, as web does.
fn connect_body(device_id: Option<&str>, thumbprint: &str) -> serde_json::Value {
    let mut body = serde_json::json!({ "clientKeyThumbprint": thumbprint });
    if let Some(device_id) = device_id.filter(|id| !id.is_empty()) {
        body["deviceId"] = serde_json::json!(device_id);
    }
    body
}

pub struct RelayClient {
    http: reqwest::Client,
    relay_url: String,
    key: Arc<DpopKey>,
    tokens: Mutex<Vec<CachedToken>>,
}

impl RelayClient {
    pub fn new(relay_url: &str, key: Arc<DpopKey>) -> Self {
        Self {
            http: reqwest::Client::new(),
            relay_url: relay_url.trim_end_matches('/').to_owned(),
            key,
            tokens: Mutex::new(Vec::new()),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.relay_url)
    }

    fn proof(
        &self,
        method: &str,
        url: &str,
        access_token: Option<&str>,
    ) -> Result<String, RelayError> {
        let now = crate::auth::now_ms() / 1_000;
        let jti = random_jti().map_err(RelayError::Failed)?;
        self.key
            .proof(method, url, access_token, now, &jti)
            .map_err(RelayError::Failed)
    }

    fn cached(&self, scopes: &[&str]) -> Option<CachedToken> {
        let thumbprint = self.key.thumbprint();
        let now = crate::auth::now_ms();
        self.tokens
            .lock()
            .ok()?
            .iter()
            .find(|token| usable(token, &thumbprint, scopes, now))
            .cloned()
    }

    fn forget(&self, access_token: &str) {
        if let Ok(mut tokens) = self.tokens.lock() {
            tokens.retain(|token| token.access_token != access_token);
        }
    }

    async fn access_token(
        &self,
        clerk_token: &str,
        scopes: &[&str],
    ) -> Result<CachedToken, RelayError> {
        if let Some(token) = self.cached(scopes) {
            return Ok(token);
        }
        let url = self.url("/v1/client/dpop-token");
        let proof = self.proof("POST", &url, None)?;
        let response = self
            .http
            .post(&url)
            .header("dpop", proof)
            .form(&exchange_form(clerk_token, &self.relay_url, scopes))
            .send()
            .await
            .map_err(|error| RelayError::Failed(format!("relay token exchange failed: {error}")))?;
        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(RelayError::Rejected(status.as_u16()));
        }
        if !status.is_success() {
            return Err(RelayError::Failed(format!(
                "relay token exchange returned {status}"
            )));
        }
        let body: TokenResponse = response.json().await.map_err(|error| {
            RelayError::Failed(format!("relay token response unreadable: {error}"))
        })?;
        if !scope_set_equals(&body.scope, scopes) {
            return Err(RelayError::Failed(
                "Relay granted unexpected DPoP access token scopes.".to_owned(),
            ));
        }
        let token = CachedToken {
            access_token: body.access_token,
            scopes: scopes.iter().map(|scope| (*scope).to_owned()).collect(),
            thumbprint: self.key.thumbprint(),
            expires_at_ms: crate::auth::now_ms() + body.expires_in * 1_000,
        };
        if let Ok(mut tokens) = self.tokens.lock() {
            tokens.push(token.clone());
        }
        Ok(token)
    }

    /// Run a DPoP-authorized request, refreshing a rejected cached token exactly once.
    async fn dpop_request(
        &self,
        clerk_token: &str,
        scopes: &[&str],
        method: &str,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<reqwest::Response, RelayError> {
        let url = self.url(path);
        for attempt in 0..2 {
            let token = self.access_token(clerk_token, scopes).await?;
            let proof = self.proof(method, &url, Some(&token.access_token))?;
            let builder = match method {
                "GET" => self.http.get(&url),
                _ => self.http.post(&url),
            };
            let mut builder = builder
                .header("authorization", format!("DPoP {}", token.access_token))
                .header("dpop", proof);
            if let Some(body) = &body {
                builder = builder.json(body);
            }
            let response = builder
                .send()
                .await
                .map_err(|error| RelayError::Failed(format!("relay request failed: {error}")))?;
            if response.status() == reqwest::StatusCode::UNAUTHORIZED {
                self.forget(&token.access_token);
                if attempt == 0 {
                    continue;
                }
                return Err(RelayError::Rejected(401));
            }
            return Ok(response);
        }
        Err(RelayError::Rejected(401))
    }

    /// `GET /v1/environments` with the Clerk token as a bearer.
    pub async fn list_environments(
        &self,
        clerk_token: &str,
    ) -> Result<Vec<RelayEnvironment>, RelayError> {
        let response = self
            .http
            .get(self.url("/v1/environments"))
            .bearer_auth(clerk_token)
            .send()
            .await
            .map_err(|error| RelayError::Failed(format!("relay list failed: {error}")))?;
        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(RelayError::Rejected(status.as_u16()));
        }
        if !status.is_success() {
            return Err(RelayError::Failed(format!("relay list returned {status}")));
        }
        let body: ListResponse = response
            .json()
            .await
            .map_err(|error| RelayError::Failed(format!("relay list unreadable: {error}")))?;
        Ok(body.environments)
    }

    /// Mint a one-time bootstrap credential for an environment.
    pub async fn connect_environment(
        &self,
        clerk_token: &str,
        environment_id: &str,
        device_id: Option<&str>,
    ) -> Result<ConnectGrant, RelayError> {
        let body = connect_body(device_id, &self.key.thumbprint());
        let response = self
            .dpop_request(
                clerk_token,
                &[CONNECT_SCOPE],
                "POST",
                &format!("/v1/environments/{environment_id}/connect"),
                Some(body),
            )
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(RelayError::Failed(format!(
                "relay connect returned {status}"
            )));
        }
        response
            .json()
            .await
            .map_err(|error| RelayError::Failed(format!("relay connect unreadable: {error}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(scopes: &[&str], expires_at_ms: u64) -> CachedToken {
        CachedToken {
            access_token: "a".into(),
            scopes: scopes.iter().map(|s| (*s).to_owned()).collect(),
            thumbprint: "tp".into(),
            expires_at_ms,
        }
    }

    #[test]
    fn a_cached_token_needs_key_scope_and_five_seconds_of_life() {
        let t = token(&[CONNECT_SCOPE, STATUS_SCOPE], 100_000);
        assert!(usable(&t, "tp", &[CONNECT_SCOPE], 10_000));
        assert!(usable(&t, "tp", &[CONNECT_SCOPE, STATUS_SCOPE], 10_000));
        assert!(!usable(&t, "other", &[CONNECT_SCOPE], 10_000));
        assert!(!usable(&t, "tp", &["mobile:registration"], 10_000));
        assert!(!usable(&t, "tp", &[CONNECT_SCOPE], 95_000));
        assert!(usable(&t, "tp", &[CONNECT_SCOPE], 94_999));
    }

    #[test]
    fn granted_scopes_must_equal_requested_ignoring_order() {
        assert!(scope_set_equals("a b", &["b", "a"]));
        assert!(!scope_set_equals("a b", &["a"]));
        assert!(!scope_set_equals("a", &["a", "b"]));
    }

    #[test]
    fn the_exchange_form_names_the_relay_as_the_resource() {
        let form = exchange_form("jwt", "https://relay.t3.codes", &[CONNECT_SCOPE]);
        let get = |key: &str| {
            form.iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(get("grant_type"), Some(GRANT_TYPE));
        assert_eq!(get("resource"), Some("https://relay.t3.codes"));
        assert_eq!(get("scope"), Some("environment:connect"));
        assert_eq!(get("client_id"), Some("t3-web"));
        assert_eq!(get("subject_token"), Some("jwt"));
    }

    #[test]
    fn the_connect_body_binds_the_thumbprint_and_optional_device() {
        assert_eq!(
            connect_body(None, "tp"),
            serde_json::json!({ "clientKeyThumbprint": "tp" })
        );
        assert_eq!(
            connect_body(Some("dev"), "tp"),
            serde_json::json!({ "clientKeyThumbprint": "tp", "deviceId": "dev" })
        );
        assert_eq!(
            connect_body(Some(""), "tp"),
            serde_json::json!({ "clientKeyThumbprint": "tp" })
        );
    }

    #[test]
    fn responses_parse_and_never_debug_the_credential() {
        let grant: ConnectGrant = serde_json::from_str(
            r#"{"environmentId":"e","endpoint":{"httpBaseUrl":"https://h","wsBaseUrl":"wss://h","providerKind":"x"},"credential":"SECRET","expiresAt":"t"}"#,
        )
        .unwrap();
        assert!(!format!("{grant:?}").contains("SECRET"));
        let list: ListResponse = serde_json::from_str(
            r#"{"environments":[{"environmentId":"e","label":"L","endpoint":{"httpBaseUrl":"https://h","wsBaseUrl":"wss://h","providerKind":"x"},"linkedAt":"t"}]}"#,
        )
        .unwrap();
        assert_eq!(list.environments[0].label, "L");
    }
}
