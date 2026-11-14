use crate::auth::{self, Credential};
use crate::auth_dpop::DpopSession;
use crate::cloud;
use crate::connection::{self, ActionSender, Ended};
use crate::connection_diagnosis::{Failure, Hop, Reason, Via};
use crate::credentials::{CredentialStore, StoredCredential};
use crate::environment_prefs;
use crate::environment_state::{self, EnvState, Phase};
use crate::pairing;
use crate::projects::{EnvironmentProjects, MachineKind};
use crate::relay::{DEFAULT_RELAY_URL, RelayClient, RelayEnvironment};
use crate::routes::{self, Route};
use crate::server;
use crate::status::{Status, StatusSender, publish};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};
use tokio::time::sleep;

fn senders() -> &'static Mutex<HashMap<String, ActionSender>> {
    static SENDERS: OnceLock<Mutex<HashMap<String, ActionSender>>> = OnceLock::new();
    SENDERS.get_or_init(Mutex::default)
}

/// Where actions for a saved environment go: its own connection, whose `Status::Call` answers
/// reach the window through the same status stream as the primary's.
pub fn sender(environment_id: &str) -> Option<ActionSender> {
    senders().lock().ok()?.get(environment_id).cloned()
}

struct Context {
    store: Arc<dyn CredentialStore>,
    status: StatusSender,
    runtime: tokio::runtime::Handle,
}

fn context() -> &'static Mutex<Option<Context>> {
    static CONTEXT: OnceLock<Mutex<Option<Context>>> = OnceLock::new();
    CONTEXT.get_or_init(Mutex::default)
}

/// The ids that have a `follow` task, the primary included.
fn followed() -> &'static Mutex<HashSet<String>> {
    static FOLLOWED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    FOLLOWED.get_or_init(Mutex::default)
}

/// Starts following an environment this app just paired with (Add Environment).
pub fn attach(credential: StoredCredential) {
    let Ok(context) = context().lock() else { return };
    let Some(context) = context.as_ref() else { return };
    if !followed().lock().is_ok_and(|mut ids| ids.insert(credential.environment_id.clone())) {
        return;
    }
    let target = Target {
        id: credential.environment_id.clone(),
        label: credential.label.clone(),
        saved: Some(credential),
        relay: None,
    };
    context
        .runtime
        .spawn(follow(context.store.clone(), target, context.status.clone()));
}

/// Stops following an environment and drops its device-local state; its pairing is forgotten by
/// the caller.
pub fn detach(environment_id: &str) {
    environment_prefs::forget(environment_id);
    environment_state::remove(environment_id);
    if let Ok(mut ids) = followed().lock() {
        ids.remove(environment_id);
    }
}

/// One environment to keep a connection to, and the ways to reach it.
struct Target {
    id: String,
    label: String,
    saved: Option<StoredCredential>,
    relay: Option<RelayEnvironment>,
}

/// Keeps a connection to every environment other than `primary_id` that this app is paired with
/// (the Keychain) or that T3 Connect lists for the signed-in account. Any failure costs only the
/// badges, and is logged once.
pub async fn run(primary_id: String, store: Arc<dyn CredentialStore>, status: StatusSender) {
    let account = cloud::service();
    account.restore().await;
    let mut account_state = account.subscribe();
    let listed = {
        let store = store.clone();
        tokio::task::spawn_blocking(move || store.list()).await
    };
    let mut saved = match listed {
        Ok(Ok(saved)) => saved,
        Ok(Err(message)) => {
            eprintln!("[environments] no saved environments: {message}");
            Vec::new()
        }
        Err(error) => {
            eprintln!("[environments] no saved environments: {error}");
            Vec::new()
        }
    };
    if let Ok(mut context) = context().lock() {
        *context = Some(Context {
            store: store.clone(),
            status: status.clone(),
            runtime: tokio::runtime::Handle::current(),
        });
    }
    if let Ok(mut ids) = followed().lock() {
        ids.insert(primary_id);
    }
    loop {
        let relay = if account_state.borrow().signed_in {
            discover().await
        } else {
            Vec::new()
        };
        let mut targets: Vec<Target> = Vec::new();
        for credential in saved.drain(..) {
            let relay = relay
                .iter()
                .find(|listed| listed.environment_id == credential.environment_id)
                .cloned();
            targets.push(Target {
                id: credential.environment_id.clone(),
                label: credential.label.clone(),
                saved: Some(credential),
                relay,
            });
        }
        for listed in relay {
            if !targets
                .iter()
                .any(|target| target.id == listed.environment_id)
            {
                targets.push(Target {
                    id: listed.environment_id.clone(),
                    label: listed.label.clone(),
                    saved: None,
                    relay: Some(listed),
                });
            }
        }
        for target in targets {
            if followed()
                .lock()
                .is_ok_and(|mut ids| ids.insert(target.id.clone()))
            {
                tokio::spawn(follow(store.clone(), target, status.clone()));
            }
        }
        // Signing in later brings the relay's environments in.
        if account_state.changed().await.is_err() {
            return;
        }
    }
}

/// The environments T3 Connect lists for the signed-in account.
async fn discover() -> Vec<RelayEnvironment> {
    let jwt = match cloud::service().relay_jwt().await {
        Ok(Some(jwt)) => jwt,
        Ok(None) => return Vec::new(),
        Err(message) => {
            eprintln!("[environments] T3 Connect: {message}");
            return Vec::new();
        }
    };
    let relay = RelayClient::new(DEFAULT_RELAY_URL, cloud::device_key());
    match relay.list_environments(&jwt).await {
        Ok(environments) => environments,
        Err(error) => {
            eprintln!("[environments] T3 Connect list failed: {error}");
            Vec::new()
        }
    }
}

/// A route that answered: where to dial, what to prove, and what the server says it is.
struct Ready {
    http_base: String,
    credential: Credential,
    environment: connection::Environment,
}

async fn open(
    route: Route,
    target: &Target,
    client: &reqwest::Client,
    relay: &RelayClient,
) -> Result<(Ready, MachineKind), Failure> {
    let via = match route {
        Route::Direct => Via::Direct,
        Route::Relay => Via::Relay,
    };
    // The hop whose answer is missing when the descriptor does not come back.
    let answering = match route {
        Route::Direct => Hop::Server,
        Route::Relay => Hop::Tunnel,
    };
    let fail = |hop: Hop, reason: Reason, detail: String| {
        let mut failure = Failure::new(reason, detail);
        failure.hop = Some(hop);
        failure.via = Some(via);
        failure
    };
    let (http_base, credential) = match route {
        Route::Direct => {
            let saved = target
                .saved
                .as_ref()
                .ok_or_else(|| fail(Hop::Device, Reason::Configuration, "no saved pairing".into()))?;
            (
                saved.http_base.clone(),
                Credential::Bearer(saved.token.clone()),
            )
        }
        Route::Relay => {
            let listed = target.relay.as_ref().ok_or_else(|| {
                fail(
                    Hop::Relay,
                    Reason::Configuration,
                    "not listed by T3 Connect".into(),
                )
            })?;
            let jwt = cloud::service()
                .relay_jwt()
                .await
                .map_err(|message| fail(Hop::Relay, Reason::RelayUnavailable, message))?
                .ok_or_else(|| {
                    fail(
                        Hop::Relay,
                        Reason::Authentication,
                        "signed out of T3 Connect".into(),
                    )
                })?;
            let grant = relay
                .connect_environment(&jwt, &target.id, None)
                .await
                .map_err(|error| fail(Hop::Relay, Reason::RelayUnavailable, error.to_string()))?;
            let key = cloud::device_key();
            let token = crate::auth_dpop::exchange_dpop_token(
                client,
                &listed.endpoint.http_base_url,
                &key,
                &grant.credential,
            )
            .await
            .map_err(|error| fail(Hop::Tunnel, Reason::Network, error.to_string()))?;
            (
                listed.endpoint.http_base_url.clone(),
                Credential::Dpop(DpopSession {
                    key,
                    access_token: token.access_token,
                    ws_base: Some(listed.endpoint.ws_base_url.clone()),
                    connection_method: "relay",
                }),
            )
        }
    };
    let described = tokio::time::timeout(
        routes::DIRECT_TIMEOUT,
        server::fetch_environment(client, &http_base),
    )
    .await
    .map_err(|_| fail(answering, Reason::Timeout, "no answer".into()))?
    .map_err(|message| match message
        .strip_prefix("HTTP ")
        .and_then(|code| code.parse::<u16>().ok())
    {
        Some(code) => {
            let mut failure = fail(answering, Reason::EndpointUnavailable, message);
            failure.http_status = Some(code);
            failure
        }
        None => fail(answering, Reason::Network, message),
    })?;
    if described.id != target.id {
        return Err(fail(
            Hop::Server,
            Reason::Configuration,
            "answers as a different environment".into(),
        ));
    }
    let machine = described.machine;
    let environment = connection::Environment {
        label: target.label.clone(),
        ..described
    };
    Ok((
        Ready {
            http_base,
            credential,
            environment,
        },
        machine,
    ))
}

/// Waits until this environment is switched off or removed from the device.
async fn stopped(id: &str) {
    let mut changes = environment_state::subscribe();
    loop {
        if environment_state::get(id).is_none() || !environment_prefs::load().enabled(id) {
            return;
        }
        if changes.changed().await.is_err() {
            return std::future::pending().await;
        }
    }
}

/// Waits until this environment is switched on again; false when it was removed instead.
async fn resumed(id: &str) -> bool {
    let mut changes = environment_state::subscribe();
    loop {
        if environment_state::get(id).is_none() {
            return false;
        }
        if environment_prefs::load().enabled(id) {
            return true;
        }
        if changes.changed().await.is_err() {
            return false;
        }
    }
}

/// Mirrors one environment's projects into `status` for as long as the app runs.
async fn follow(store: Arc<dyn CredentialStore>, mut target: Target, status: StatusSender) {
    let client = auth::http_client();
    let relay = RelayClient::new(DEFAULT_RELAY_URL, cloud::device_key());
    let (own_tx, own_rx) = async_channel::unbounded();
    // The connection ends its session when the action channel closes, so keep the sender here.
    let (actions_tx, actions_rx) = async_channel::unbounded();
    if let Ok(mut senders) = senders().lock() {
        senders.insert(target.id.clone(), actions_tx.clone());
    }
    let machine = Arc::new(Mutex::new(MachineKind::Server));
    let id = target.id.clone();
    let label = target.label.clone();
    let url = Arc::new(Mutex::new(
        target
            .saved
            .as_ref()
            .map(|saved| saved.http_base.clone())
            .or_else(|| {
                target
                    .relay
                    .as_ref()
                    .map(|listed| listed.endpoint.http_base_url.clone())
            })
            .unwrap_or_default(),
    ));
    let mut initial = EnvState::new(id.clone(), label.clone());
    initial.url = url.lock().map(|url| url.clone()).unwrap_or_default();
    initial.saved = target.saved.is_some();
    initial.relay_listed = target.relay.is_some();
    if !environment_prefs::load().enabled(&id) {
        initial.phase = Phase::Available;
    }
    environment_state::put(initial);
    let forward = async {
        let mut last: Option<EnvironmentProjects> = None;
        while let Ok(update) = own_rx.recv().await {
            if matches!(update, Status::Call { .. }) {
                publish(&status, update);
                continue;
            }
            let (projects, connected) = match update {
                Status::Connected {
                    shell: Some(shell), ..
                } => (shell.project_entries, true),
                Status::Connected { .. } => continue,
                Status::Failed(message) => {
                    // The connection retries on its own, so the failure is its latest.
                    let detail = message
                        .strip_prefix("Reconnecting: ")
                        .unwrap_or(&message)
                        .to_owned();
                    environment_state::update(&id, |state| {
                        let mut failure = Failure::new(Reason::Network, detail);
                        failure.hop = Some(Hop::Server);
                        failure.via = state.via;
                        state.failure = Some(failure);
                        state.attempt += 1;
                        state.phase = Phase::Reconnecting;
                    });
                    (Vec::new(), false)
                }
                // A dropped socket must not leave a stale "Also on".
                _ => (Vec::new(), false),
            };
            if connected {
                environment_state::update(&id, |state| {
                    state.phase = Phase::Connected;
                    state.failure = None;
                    state.attempt = 0;
                    state.retry_at_ms = None;
                });
            }
            let next = EnvironmentProjects {
                id: id.clone(),
                label: label.clone(),
                url: url.lock().map(|url| url.clone()).unwrap_or_default(),
                connected,
                machine: machine
                    .lock()
                    .map(|machine| *machine)
                    .unwrap_or(MachineKind::Server),
                projects,
            };
            if last.as_ref() != Some(&next) {
                publish(&status, Status::Environment(next.clone()));
                last = Some(next);
            }
        }
    };
    let session = async {
        let mut backoff = connection::Backoff::new();
        let mut last_error: Option<String> = None;
        loop {
            let mut pause = None;
            if environment_state::get(&id).is_none() {
                break;
            }
            if !environment_prefs::load().enabled(&id) {
                environment_state::update(&id, |state| {
                    state.phase = Phase::Available;
                    state.failure = None;
                    state.attempt = 0;
                    state.retry_at_ms = None;
                });
                // Off: drop the badges until it is switched on again.
                publish(
                    &status,
                    Status::Environment(EnvironmentProjects {
                        id: id.clone(),
                        label: label.clone(),
                        url: url.lock().map(|url| url.clone()).unwrap_or_default(),
                        connected: false,
                        machine: machine
                            .lock()
                            .map(|machine| *machine)
                            .unwrap_or(MachineKind::Server),
                        projects: Vec::new(),
                    }),
                );
                if !resumed(&id).await {
                    break;
                }
                backoff.reset();
                continue;
            }
            let signed_in = cloud::service().subscribe().borrow().signed_in;
            let routes = routes::select(target.saved.is_some(), target.relay.is_some(), signed_in);
            if routes.is_empty() {
                break;
            }
            for route in routes {
                environment_state::update(&id, |state| {
                    state.phase = environment_state::pending_phase(state.attempt, &state.failure);
                });
                match open(route, &target, &client, &relay).await {
                    Ok((ready, described)) => {
                        backoff.reset();
                        last_error = None;
                        if let Ok(mut machine) = machine.lock() {
                            *machine = described;
                        }
                        if let Ok(mut url) = url.lock() {
                            url.clone_from(&ready.http_base);
                        }
                        environment_state::update(&id, |state| {
                            state.machine = described;
                            state.url.clone_from(&ready.http_base);
                            state.via = Some(match route {
                                Route::Direct => Via::Direct,
                                Route::Relay => Via::Relay,
                            });
                        });
                        let ended = tokio::select! {
                            ended = connection::run(
                                &ready.http_base,
                                &ready.credential,
                                &ready.environment,
                                &actions_rx,
                                &own_tx,
                            ) => Some(ended),
                            () = stopped(&id) => None,
                        };
                        let Some(Ended::SessionRejected) = ended else {
                            // Switched off or removed: the loop head handles which.
                            break;
                        };
                        if route == Route::Direct {
                            // The session is dead for good, so forget it; it shows again once paired.
                            let _ = pairing::forget(&store, &target.id).await;
                            target.saved = None;
                            environment_state::update(&id, |state| {
                                state.saved = false;
                                let mut failure = Failure::new(
                                    Reason::Authentication,
                                    "the saved pairing was rejected",
                                );
                                failure.hop = Some(Hop::Server);
                                failure.via = Some(Via::Direct);
                                state.failure = Some(failure);
                                state.phase = Phase::Error;
                            });
                            eprintln!(
                                "[environments] {} rejected its saved pairing; forgot it",
                                target.label
                            );
                        }
                        break;
                    }
                    Err(failure) => {
                        let message = format!("{} ({route:?}): {}", target.label, failure.detail);
                        if last_error.as_deref() != Some(&message) {
                            eprintln!("[environments] {message}");
                            last_error = Some(message);
                        }
                        let delay = backoff.next_delay();
                        pause = Some(delay);
                        environment_state::update(&id, |state| {
                            state.attempt += 1;
                            state.failure = Some(failure);
                            state.phase = Phase::Reconnecting;
                            state.retry_at_ms = Some(auth::now_ms() + delay.as_millis() as u64);
                        });
                    }
                }
            }
            sleep(pause.unwrap_or_else(|| backoff.next_delay())).await;
        }
        // Close the channel so `forward` ends and the badges clear.
        own_tx.close();
        if let Ok(mut senders) = senders().lock() {
            senders.remove(&target.id);
        }
        drop(actions_tx);
    };
    tokio::join!(forward, session);
}
