use crate::auth::{self, Credential};
use crate::auth_dpop::DpopSession;
use crate::cloud;
use crate::connection::{self, ActionSender, Ended};
use crate::credentials::{CredentialStore, StoredCredential};
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
    let mut followed = HashSet::from([primary_id]);
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
            if followed.insert(target.id.clone()) {
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
) -> Result<(Ready, MachineKind), String> {
    let (http_base, credential) = match route {
        Route::Direct => {
            let saved = target.saved.as_ref().ok_or("no saved pairing")?;
            (
                saved.http_base.clone(),
                Credential::Bearer(saved.token.clone()),
            )
        }
        Route::Relay => {
            let listed = target.relay.as_ref().ok_or("not listed by T3 Connect")?;
            let jwt = cloud::service()
                .relay_jwt()
                .await?
                .ok_or("signed out of T3 Connect")?;
            let grant = relay
                .connect_environment(&jwt, &target.id, None)
                .await
                .map_err(|error| error.to_string())?;
            let key = cloud::device_key();
            let token = crate::auth_dpop::exchange_dpop_token(
                client,
                &listed.endpoint.http_base_url,
                &key,
                &grant.credential,
            )
            .await
            .map_err(|error| error.to_string())?;
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
    .map_err(|_| "no answer".to_owned())??;
    if described.id != target.id {
        return Err("answers as a different environment".to_owned());
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
                // A dropped socket must not leave a stale "Also on".
                _ => (Vec::new(), false),
            };
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
            let signed_in = cloud::service().subscribe().borrow().signed_in;
            let routes = routes::select(target.saved.is_some(), target.relay.is_some(), signed_in);
            if routes.is_empty() {
                break;
            }
            for route in routes {
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
                        let Ended::SessionRejected = connection::run(
                            &ready.http_base,
                            &ready.credential,
                            &ready.environment,
                            &actions_rx,
                            &own_tx,
                        )
                        .await;
                        if route == Route::Direct {
                            // The session is dead for good, so forget it; it shows again once paired.
                            let _ = pairing::forget(&store, &target.id).await;
                            target.saved = None;
                            eprintln!(
                                "[environments] {} rejected its saved pairing; forgot it",
                                target.label
                            );
                        }
                        break;
                    }
                    Err(message) => {
                        let message = format!("{} ({route:?}): {message}", target.label);
                        if last_error.as_deref() != Some(&message) {
                            eprintln!("[environments] {message}");
                            last_error = Some(message);
                        }
                    }
                }
            }
            sleep(backoff.next_delay()).await;
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
