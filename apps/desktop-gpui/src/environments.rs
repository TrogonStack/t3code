use crate::auth::{self, Credential};
use crate::connection::{self, Ended};
use crate::credentials::{CredentialStore, StoredCredential};
use crate::pairing;
use crate::projects::EnvironmentProjects;
use crate::server;
use crate::status::{Status, StatusSender, publish};
use std::sync::Arc;
use tokio::time::sleep;

/// Lists what the Keychain holds for environments other than `primary_id` and keeps a connection
/// to each reachable one. Any failure costs only the badges, and is logged once.
pub async fn run(primary_id: String, store: Arc<dyn CredentialStore>, status: StatusSender) {
    let listed = {
        let store = store.clone();
        tokio::task::spawn_blocking(move || store.list()).await
    };
    let saved = match listed {
        Ok(Ok(saved)) => saved,
        Ok(Err(message)) => return eprintln!("[environments] no environment badges: {message}"),
        Err(error) => return eprintln!("[environments] no environment badges: {error}"),
    };
    for saved in saved
        .into_iter()
        .filter(|saved| saved.environment_id != primary_id)
    {
        tokio::spawn(follow(store.clone(), saved, status.clone()));
    }
}

/// Mirrors one environment's projects into `status` for as long as the app runs.
async fn follow(store: Arc<dyn CredentialStore>, saved: StoredCredential, status: StatusSender) {
    let client = auth::http_client();
    let (own_tx, own_rx) = async_channel::unbounded();
    // The connection ends its session when the action channel closes, so keep the sender here.
    let (_actions_tx, actions_rx) = async_channel::unbounded();
    let mut backoff = connection::Backoff::new();
    let environment = loop {
        match server::fetch_environment(&client, &saved.http_base).await {
            Ok(environment) if environment.id == saved.environment_id => break environment,
            Ok(_) => {
                return eprintln!(
                    "[environments] {} answers as a different environment; not connecting",
                    saved.label
                );
            }
            Err(_) => sleep(backoff.next_delay()).await,
        }
    };
    let machine = environment.machine;
    let connection = connection::Environment {
        label: saved.label.clone(),
        ..environment
    };
    let forward = async {
        let mut last: Option<EnvironmentProjects> = None;
        while let Ok(update) = own_rx.recv().await {
            let (projects, connected) = match update {
                Status::Connected {
                    shell: Some(shell), ..
                } => (shell.project_entries, true),
                Status::Connected { .. } => continue,
                // A dropped socket must not leave a stale "Also on".
                _ => (Vec::new(), false),
            };
            let next = EnvironmentProjects {
                id: saved.environment_id.clone(),
                label: saved.label.clone(),
                url: saved.http_base.clone(),
                connected,
                machine,
                projects,
            };
            if last.as_ref() != Some(&next) {
                publish(&status, Status::Environment(next.clone()));
                last = Some(next);
            }
        }
    };
    let session = async {
        let credential = Credential::Bearer(saved.token.clone());
        let ended = connection::run(
            &saved.http_base,
            &credential,
            &connection,
            &actions_rx,
            &own_tx,
        )
        .await;
        let Ended::SessionRejected = ended;
        // The session is dead for good, so forget it; it shows again once paired.
        let _ = pairing::forget(&store, &saved.environment_id).await;
        eprintln!(
            "[environments] {} rejected its saved pairing; forgot it",
            saved.label
        );
        // Close the channel so `forward` ends and the badges clear.
        own_tx.close();
    };
    tokio::join!(forward, session);
}
