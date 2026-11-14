//! The T3 Connect account: a Clerk sign-in done the way the Electron desktop does it
//! (`apps/desktop/src/app/DesktopClerk.ts` over `@clerk/electron`'s native flow).
//!
//! Every Clerk Frontend API call carries `_is_native=1`; the client JWT Clerk answers with in the
//! `Authorization` header is kept and sent back as `Bearer`. Google OAuth opens in the system
//! browser and returns through the registered `t3code[-dev]://app/` scheme, whose only allowlisted
//! redirects are those two. The relay wants a session JWT from the `t3-relay` template.

use serde::Deserialize;
use std::sync::{Arc, Mutex, OnceLock};
use url::Url;

pub const FAPI_BASE: &str = "https://clerk.t3.codes";
pub const RELAY_TEMPLATE: &str = "t3-relay";
/// How long the browser round trip may take, as in the Electron desktop.
pub const CALLBACK_TIMEOUT_SECS: u64 = 180;

/// The scheme registered for this build: `t3code-dev` in development, `t3code` shipped.
pub fn url_scheme() -> &'static str {
    if cfg!(debug_assertions) {
        "t3code-dev"
    } else {
        "t3code"
    }
}

pub fn redirect_url(scheme: &str, attempt: &str) -> String {
    format!("{scheme}://app/?__clerk_oauth_attempt={attempt}")
}

/// What the browser handed back for one attempt.
#[derive(Debug, PartialEq, Eq)]
pub enum Callback {
    /// Some other deep link, or another attempt's callback.
    Unrelated,
    Failed(String),
    Nonce(String),
}

pub fn parse_callback(raw: &str, scheme: &str, attempt: &str) -> Callback {
    let Ok(url) = Url::parse(raw) else {
        return Callback::Unrelated;
    };
    if url.scheme() != scheme || url.host_str() != Some("app") {
        return Callback::Unrelated;
    }
    let query = |name: &str| {
        url.query_pairs()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.into_owned())
    };
    if query("__clerk_oauth_attempt").as_deref() != Some(attempt) {
        return Callback::Unrelated;
    }
    if query("__clerk_status").as_deref() == Some("failed") {
        return Callback::Failed(
            query("__clerk_error_code").unwrap_or_else(|| "sign-in failed".to_owned()),
        );
    }
    match query("rotating_token_nonce") {
        Some(nonce) => Callback::Nonce(nonce),
        None => Callback::Failed("the sign-in callback carried no nonce".to_owned()),
    }
}

#[derive(Debug)]
pub enum ClerkError {
    /// Clerk no longer accepts this session (401/403/404).
    Rejected,
    Failed(String),
}

impl std::fmt::Display for ClerkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rejected => f.write_str("the T3 Connect session ended; sign in again"),
            Self::Failed(message) => f.write_str(message),
        }
    }
}

/// The durable part of a signed-in account. The client JWT is a secret: `Debug` hides it.
#[derive(Clone, PartialEq, Eq, serde::Serialize, Deserialize)]
pub struct Account {
    pub client_jwt: String,
    pub session_id: String,
}

impl std::fmt::Debug for Account {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Account")
            .field("client_jwt", &"<redacted>")
            .field("session_id", &self.session_id)
            .finish()
    }
}

/// A sign-in the browser has been sent to finish.
#[derive(Debug)]
pub struct Pending {
    pub sign_in_id: String,
    pub browser_url: String,
    pub client_jwt: Option<String>,
}

#[derive(Deserialize)]
struct SignInEnvelope {
    response: SignInResponse,
}

#[derive(Deserialize)]
struct SignInResponse {
    id: String,
    #[serde(default)]
    created_session_id: Option<String>,
    #[serde(default)]
    first_factor_verification: Option<Verification>,
}

#[derive(Deserialize)]
struct Verification {
    #[serde(default)]
    external_verification_redirect_url: Option<String>,
}

#[derive(Deserialize)]
struct TokenResponse {
    jwt: String,
}

pub struct ClerkClient {
    http: reqwest::Client,
    base: String,
}

impl ClerkClient {
    pub fn new(base: &str) -> Self {
        Self {
            http: crate::auth::http_client(),
            base: base.trim_end_matches('/').to_owned(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!(
            "{}{path}{}_is_native=1",
            self.base,
            if path.contains('?') { '&' } else { '?' }
        )
    }

    fn with_jwt(request: reqwest::RequestBuilder, jwt: Option<&str>) -> reqwest::RequestBuilder {
        match jwt {
            Some(jwt) => request.bearer_auth(jwt),
            None => request,
        }
    }

    fn rotated(response: &reqwest::Response) -> Option<String> {
        let value = response.headers().get("authorization")?.to_str().ok()?;
        Some(value.strip_prefix("Bearer ").unwrap_or(value).to_owned())
    }

    async fn check(response: reqwest::Response) -> Result<reqwest::Response, ClerkError> {
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        if matches!(status.as_u16(), 401 | 403 | 404) {
            return Err(ClerkError::Rejected);
        }
        Err(ClerkError::Failed(format!("Clerk answered {status}")))
    }

    /// Starts Google OAuth; the caller opens `browser_url` and waits for the callback.
    pub async fn begin(&self, redirect: &str) -> Result<Pending, ClerkError> {
        let response = self
            .http
            .post(self.url("/v1/client/sign_ins"))
            .form(&[
                ("strategy", "oauth_google"),
                ("redirect_url", redirect),
                ("action_complete_redirect_url", redirect),
            ])
            .send()
            .await
            .map_err(|error| ClerkError::Failed(format!("could not reach Clerk: {error}")))?;
        let response = Self::check(response).await?;
        let client_jwt = Self::rotated(&response);
        let body: SignInEnvelope = response
            .json()
            .await
            .map_err(|_| ClerkError::Failed("Clerk sent an unreadable sign-in".to_owned()))?;
        let browser_url = body
            .response
            .first_factor_verification
            .and_then(|verification| verification.external_verification_redirect_url)
            .ok_or_else(|| ClerkError::Failed("Clerk sent no browser sign-in link".to_owned()))?;
        Ok(Pending {
            sign_in_id: body.response.id,
            browser_url,
            client_jwt,
        })
    }

    /// Trades the callback's nonce for the session the sign-in created.
    pub async fn complete(&self, pending: &Pending, nonce: &str) -> Result<Account, ClerkError> {
        let mut url = Url::parse(&self.url(&format!("/v1/client/sign_ins/{}", pending.sign_in_id)))
            .map_err(|error| ClerkError::Failed(error.to_string()))?;
        url.query_pairs_mut()
            .append_pair("rotating_token_nonce", nonce);
        let response = Self::with_jwt(self.http.get(url), pending.client_jwt.as_deref())
            .send()
            .await
            .map_err(|error| ClerkError::Failed(format!("could not reach Clerk: {error}")))?;
        let response = Self::check(response).await?;
        let rotated = Self::rotated(&response);
        let body: SignInEnvelope = response
            .json()
            .await
            .map_err(|_| ClerkError::Failed("Clerk sent an unreadable sign-in".to_owned()))?;
        let session_id = body
            .response
            .created_session_id
            .ok_or_else(|| ClerkError::Failed("the sign-in did not create a session".to_owned()))?;
        let client_jwt = rotated
            .or_else(|| pending.client_jwt.clone())
            .ok_or_else(|| ClerkError::Failed("Clerk sent no client token".to_owned()))?;
        Ok(Account {
            client_jwt,
            session_id,
        })
    }

    /// A relay-audience session JWT. Returns the account too, since Clerk may rotate its token.
    pub async fn mint(&self, account: &Account) -> Result<(String, Account), ClerkError> {
        let path = format!(
            "/v1/client/sessions/{}/tokens/{RELAY_TEMPLATE}",
            account.session_id
        );
        let response = self
            .http
            .post(self.url(&path))
            .bearer_auth(&account.client_jwt)
            .send()
            .await
            .map_err(|error| ClerkError::Failed(format!("could not reach Clerk: {error}")))?;
        let response = Self::check(response).await?;
        let rotated = Self::rotated(&response);
        let token: TokenResponse = response
            .json()
            .await
            .map_err(|_| ClerkError::Failed("Clerk sent an unreadable token".to_owned()))?;
        let account = Account {
            client_jwt: rotated.unwrap_or_else(|| account.client_jwt.clone()),
            session_id: account.session_id.clone(),
        };
        Ok((token.jwt, account))
    }

    /// Ends the session at Clerk; the caller forgets the account whatever this returns.
    pub async fn end(&self, account: &Account) -> Result<(), ClerkError> {
        let path = format!("/v1/client/sessions/{}/end", account.session_id);
        let response = self
            .http
            .post(self.url(&path))
            .bearer_auth(&account.client_jwt)
            .send()
            .await
            .map_err(|error| ClerkError::Failed(format!("could not reach Clerk: {error}")))?;
        Self::check(response).await.map(|_| ())
    }
}

/// Where the signed-in account lives. Blocking: the Keychain may prompt.
pub trait AccountStore: Send + Sync {
    fn load(&self) -> Result<Option<Account>, String>;
    fn save(&self, account: &Account) -> Result<(), String>;
    fn clear(&self) -> Result<(), String>;
}

#[cfg(target_os = "macos")]
pub struct KeychainAccount;

#[cfg(target_os = "macos")]
impl KeychainAccount {
    const SERVICE: &'static str = "T3 Code (GPUI) Cloud Account";
    const ACCOUNT: &'static str = "clerk";
    const NOT_FOUND: i32 = -25300;
}

#[cfg(target_os = "macos")]
impl AccountStore for KeychainAccount {
    fn load(&self) -> Result<Option<Account>, String> {
        use security_framework::passwords::get_generic_password;
        match get_generic_password(Self::SERVICE, Self::ACCOUNT) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|_| "the stored account is unreadable".to_owned()),
            Err(error) if error.code() == Self::NOT_FOUND => Ok(None),
            Err(error) => Err(format!("Keychain: {error}")),
        }
    }

    fn save(&self, account: &Account) -> Result<(), String> {
        let bytes = serde_json::to_vec(account).map_err(|error| error.to_string())?;
        security_framework::passwords::set_generic_password(Self::SERVICE, Self::ACCOUNT, &bytes)
            .map_err(|error| format!("Keychain: {error}"))
    }

    fn clear(&self) -> Result<(), String> {
        use security_framework::passwords::delete_generic_password;
        match delete_generic_password(Self::SERVICE, Self::ACCOUNT) {
            Ok(()) => Ok(()),
            Err(error) if error.code() == Self::NOT_FOUND => Ok(()),
            Err(error) => Err(format!("Keychain: {error}")),
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub struct KeychainAccount;

#[cfg(not(target_os = "macos"))]
impl AccountStore for KeychainAccount {
    fn load(&self) -> Result<Option<Account>, String> {
        Ok(None)
    }
    fn save(&self, _: &Account) -> Result<(), String> {
        Err("T3 Connect sign-in needs the macOS Keychain".to_owned())
    }
    fn clear(&self) -> Result<(), String> {
        Ok(())
    }
}

/// What the settings panel shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CloudState {
    pub signed_in: bool,
    pub signing_in: bool,
    pub error: Option<String>,
}

/// The account for this process: one Keychain item, one Clerk client, one sign-in at a time.
pub struct Cloud {
    store: Arc<dyn AccountStore>,
    clerk: ClerkClient,
    state: tokio::sync::watch::Sender<CloudState>,
    attempt: Mutex<Option<(String, tokio::sync::oneshot::Sender<String>)>>,
    /// Serializes token minting so a rotated client token is never overwritten by a stale one.
    mint: tokio::sync::Mutex<()>,
}

/// The shared instance, backed by the Keychain.
pub fn service() -> &'static Cloud {
    static CLOUD: OnceLock<Cloud> = OnceLock::new();
    CLOUD.get_or_init(|| Cloud::new(Arc::new(KeychainAccount), FAPI_BASE))
}

impl Cloud {
    pub fn new(store: Arc<dyn AccountStore>, fapi_base: &str) -> Self {
        let (state, _) = tokio::sync::watch::channel(CloudState::default());
        Self {
            store,
            clerk: ClerkClient::new(fapi_base),
            state,
            attempt: Mutex::new(None),
            mint: tokio::sync::Mutex::new(()),
        }
    }

    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<CloudState> {
        self.state.subscribe()
    }

    fn update(&self, change: impl FnOnce(&mut CloudState)) {
        self.state.send_modify(change);
    }

    async fn blocking<T: Send + 'static>(
        &self,
        work: impl FnOnce(Arc<dyn AccountStore>) -> T + Send + 'static,
    ) -> Result<T, String> {
        let store = self.store.clone();
        tokio::task::spawn_blocking(move || work(store))
            .await
            .map_err(|error| error.to_string())
    }

    /// Reads the Keychain once at startup so the panel and the relay routes know the state.
    pub async fn restore(&self) {
        match self.blocking(|store| store.load()).await {
            Ok(Ok(account)) => self.update(|state| state.signed_in = account.is_some()),
            Ok(Err(message)) | Err(message) => {
                eprintln!("[cloud] could not read the saved account: {message}");
            }
        }
    }

    /// Called with every deep link the OS hands the app; only the pending attempt's is taken.
    pub fn deliver_url(&self, raw: &str) {
        let Ok(mut pending) = self.attempt.lock() else {
            return;
        };
        let Some((attempt, _)) = pending.as_ref() else {
            return;
        };
        let outcome = parse_callback(raw, url_scheme(), attempt);
        let value = match outcome {
            Callback::Unrelated => return,
            Callback::Nonce(nonce) => nonce,
            // An empty nonce is how a failure reaches the waiting sign-in.
            Callback::Failed(_) => String::new(),
        };
        if let Some((_, sender)) = pending.take() {
            let _ = sender.send(value);
        }
    }

    /// Signs in through the system browser. Returns once signed in, failed, or timed out.
    pub async fn sign_in(&self, open: impl FnOnce(&str)) -> Result<(), String> {
        if self.state.borrow().signing_in {
            return Err("a sign-in is already in progress".to_owned());
        }
        self.update(|state| {
            state.signing_in = true;
            state.error = None;
        });
        let result = self.run_sign_in(open).await;
        self.update(|state| {
            state.signing_in = false;
            match &result {
                Ok(()) => state.signed_in = true,
                Err(message) => state.error = Some(message.clone()),
            }
        });
        result
    }

    async fn run_sign_in(&self, open: impl FnOnce(&str)) -> Result<(), String> {
        let attempt = crate::auth::generate_secret()?;
        let redirect = redirect_url(url_scheme(), &attempt);
        let pending = self
            .clerk
            .begin(&redirect)
            .await
            .map_err(|e| e.to_string())?;
        let (tx, rx) = tokio::sync::oneshot::channel();
        if let Ok(mut slot) = self.attempt.lock() {
            *slot = Some((attempt, tx));
        }
        open(&pending.browser_url);
        let waited =
            tokio::time::timeout(std::time::Duration::from_secs(CALLBACK_TIMEOUT_SECS), rx).await;
        if let Ok(mut slot) = self.attempt.lock() {
            slot.take();
        }
        let nonce = match waited {
            Err(_) => return Err("sign-in timed out; try again".to_owned()),
            Ok(Err(_)) => return Err("sign-in was cancelled".to_owned()),
            Ok(Ok(nonce)) if nonce.is_empty() => return Err("sign-in failed".to_owned()),
            Ok(Ok(nonce)) => nonce,
        };
        let account = self
            .clerk
            .complete(&pending, &nonce)
            .await
            .map_err(|e| e.to_string())?;
        let to_save = account.clone();
        self.blocking(move |store| store.save(&to_save))
            .await
            .and_then(|saved| saved)
            .map_err(|message| format!("signed in, but could not save the account: {message}"))
    }

    /// A fresh relay-audience JWT, or `None` when signed out. A session Clerk refuses signs out.
    pub async fn relay_jwt(&self) -> Result<Option<String>, String> {
        let _turn = self.mint.lock().await;
        let account = match self.blocking(|store| store.load()).await {
            Ok(Ok(Some(account))) => account,
            Ok(Ok(None)) => return Ok(None),
            Ok(Err(message)) | Err(message) => return Err(message),
        };
        match self.clerk.mint(&account).await {
            Ok((jwt, rotated)) => {
                if rotated != account {
                    let _ = self.blocking(move |store| store.save(&rotated)).await;
                }
                Ok(Some(jwt))
            }
            Err(ClerkError::Rejected) => {
                let _ = self.blocking(|store| store.clear()).await;
                self.update(|state| state.signed_in = false);
                Err(ClerkError::Rejected.to_string())
            }
            Err(error) => Err(error.to_string()),
        }
    }

    pub async fn sign_out(&self) {
        if let Ok(Ok(Some(account))) = self.blocking(|store| store.load()).await
            && let Err(error) = self.clerk.end(&account).await
        {
            eprintln!("[cloud] Clerk did not end the session: {error}");
        }
        if let Ok(Err(message)) | Err(message) = self.blocking(|store| store.clear()).await {
            eprintln!("[cloud] could not clear the saved account: {message}");
        }
        self.update(|state| *state = CloudState::default());
    }
}

/// This install's DPoP key, kept in the Keychain so the relay sees one stable device. A Keychain
/// that cannot be used leaves a key for this process only, and says so.
pub fn device_key() -> Arc<crate::dpop::DpopKey> {
    static KEY: OnceLock<Arc<crate::dpop::DpopKey>> = OnceLock::new();
    KEY.get_or_init(|| match load_or_create_device_key() {
        Ok(key) => Arc::new(key),
        Err(message) => {
            eprintln!("[cloud] using a temporary device key: {message}");
            Arc::new(crate::dpop::DpopKey::generate().expect("the system can generate keys"))
        }
    })
    .clone()
}

#[cfg(target_os = "macos")]
fn load_or_create_device_key() -> Result<crate::dpop::DpopKey, String> {
    use security_framework::passwords::{get_generic_password, set_generic_password};
    const SERVICE: &str = "T3 Code (GPUI) Device Key";
    const ACCOUNT: &str = "dpop";
    match get_generic_password(SERVICE, ACCOUNT) {
        Ok(bytes) => return crate::dpop::DpopKey::from_bytes(&bytes),
        Err(error) if error.code() == -25300 => {}
        Err(error) => return Err(format!("Keychain: {error}")),
    }
    let key = crate::dpop::DpopKey::generate()?;
    set_generic_password(SERVICE, ACCOUNT, &key.to_bytes())
        .map_err(|error| format!("Keychain: {error}"))?;
    Ok(key)
}

#[cfg(not(target_os = "macos"))]
fn load_or_create_device_key() -> Result<crate::dpop::DpopKey, String> {
    Err("the device key needs the macOS Keychain".to_owned())
}

static RUNTIME: OnceLock<tokio::runtime::Handle> = OnceLock::new();

/// The async runtime the account's network calls run on; set once at startup.
pub fn bind_runtime(handle: tokio::runtime::Handle) {
    let _ = RUNTIME.set(handle);
}

/// Starts the browser sign-in from the UI thread.
pub fn spawn_sign_in() {
    if let Some(runtime) = RUNTIME.get() {
        runtime.spawn(async {
            if let Err(message) = service().sign_in(open_in_browser).await {
                eprintln!("[cloud] sign-in failed: {message}");
            }
        });
    }
}

pub fn spawn_sign_out() {
    if let Some(runtime) = RUNTIME.get() {
        runtime.spawn(service().sign_out());
    }
}

/// Opens `url` in the default browser, as Electron's `shell.openExternal` does.
pub fn open_in_browser(url: &str) {
    let program = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    if let Err(error) = std::process::Command::new(program).arg(url).spawn() {
        eprintln!("[cloud] could not open the browser: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redirect_carries_the_attempt_on_the_allowlisted_host() {
        assert_eq!(
            redirect_url("t3code-dev", "abc"),
            "t3code-dev://app/?__clerk_oauth_attempt=abc"
        );
    }

    #[test]
    fn callback_yields_the_nonce_for_its_own_attempt_only() {
        let ok = "t3code-dev://app/?__clerk_oauth_attempt=abc&rotating_token_nonce=n1";
        assert_eq!(
            parse_callback(ok, "t3code-dev", "abc"),
            Callback::Nonce("n1".into())
        );
        assert_eq!(
            parse_callback(ok, "t3code-dev", "other"),
            Callback::Unrelated
        );
        assert_eq!(parse_callback(ok, "t3code", "abc"), Callback::Unrelated);
        assert_eq!(
            parse_callback("not a url", "t3code", "abc"),
            Callback::Unrelated
        );
    }

    #[test]
    fn callback_reports_failure_and_missing_nonce() {
        let failed = "t3code://app/?__clerk_oauth_attempt=a&__clerk_status=failed&__clerk_error_code=access_denied";
        assert_eq!(
            parse_callback(failed, "t3code", "a"),
            Callback::Failed("access_denied".into())
        );
        let bare = "t3code://app/?__clerk_oauth_attempt=a";
        assert!(matches!(
            parse_callback(bare, "t3code", "a"),
            Callback::Failed(_)
        ));
    }

    #[test]
    fn requests_are_marked_native() {
        let client = ClerkClient::new("https://clerk.t3.codes/");
        assert_eq!(
            client.url("/v1/client/sign_ins"),
            "https://clerk.t3.codes/v1/client/sign_ins?_is_native=1"
        );
    }

    #[test]
    fn account_debug_hides_the_client_token() {
        let account = Account {
            client_jwt: "secret".into(),
            session_id: "sess_1".into(),
        };
        assert!(!format!("{account:?}").contains("secret"));
    }

    #[derive(Default)]
    struct MemoryAccount(Mutex<Option<Account>>);

    impl AccountStore for MemoryAccount {
        fn load(&self) -> Result<Option<Account>, String> {
            Ok(self.0.lock().unwrap().clone())
        }
        fn save(&self, account: &Account) -> Result<(), String> {
            *self.0.lock().unwrap() = Some(account.clone());
            Ok(())
        }
        fn clear(&self) -> Result<(), String> {
            *self.0.lock().unwrap() = None;
            Ok(())
        }
    }

    fn cloud() -> Cloud {
        Cloud::new(Arc::new(MemoryAccount::default()), "http://127.0.0.1:9")
    }

    #[test]
    fn only_the_pending_attempts_callback_reaches_the_waiting_sign_in() {
        let cloud = cloud();
        let link = |attempt: &str| {
            format!(
                "{}://app/?__clerk_oauth_attempt={attempt}&rotating_token_nonce=n9",
                url_scheme()
            )
        };
        cloud.deliver_url(&link("a1"));
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        *cloud.attempt.lock().unwrap() = Some(("a1".into(), tx));
        cloud.deliver_url(&link("zzz"));
        assert!(rx.try_recv().is_err());
        cloud.deliver_url(&link("a1"));
        assert_eq!(rx.try_recv().as_deref(), Ok("n9"));
        assert!(cloud.attempt.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn signed_out_has_no_relay_token_and_restore_reads_the_saved_account() {
        let cloud = cloud();
        assert_eq!(cloud.relay_jwt().await, Ok(None));
        let account = Account {
            client_jwt: "j".into(),
            session_id: "s".into(),
        };
        cloud.store.save(&account).unwrap();
        cloud.restore().await;
        assert!(cloud.subscribe().borrow().signed_in);
        cloud.store.clear().unwrap();
        cloud.sign_out().await;
        assert_eq!(*cloud.subscribe().borrow(), CloudState::default());
    }
}
