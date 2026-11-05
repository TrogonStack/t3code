//! The user's other saved environments (the Electron app's catalog): one connection each, whose
//! projects the picker uses to show where else a project lives.

use crate::auth;
use crate::catalog::{self, SavedEnvironment};
use crate::connection;
use crate::projects::EnvironmentProjects;
use crate::server;
use crate::status::{Status, StatusSender, publish};
use std::path::PathBuf;
use tokio::time::sleep;

/// Loads the catalog and keeps a connection to each reachable environment other than `primary_id`.
/// Any failure costs only the badges, and is logged once.
pub async fn run(primary_id: String, path: PathBuf, status: StatusSender) {
    let loaded = tokio::task::spawn_blocking(move || catalog::load(&path)).await;
    let catalog = match loaded {
        Ok(Ok(catalog)) => catalog,
        Ok(Err(message)) => return eprintln!("[environments] no environment badges: {message}"),
        Err(error) => return eprintln!("[environments] no environment badges: {error}"),
    };
    for (label, kind) in &catalog.skipped {
        eprintln!("[environments] skipping {label}: {kind} connections are not supported");
    }
    for notice in &catalog.notices {
        eprintln!("[environments] {notice}");
    }
    for saved in catalog
        .environments
        .into_iter()
        .filter(|saved| saved.environment_id != primary_id)
    {
        tokio::spawn(follow(saved, status.clone()));
    }
}

/// Mirrors one environment's projects into `status` for as long as the app runs.
async fn follow(saved: SavedEnvironment, status: StatusSender) {
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
            let projects = match update {
                Status::Connected {
                    shell: Some(shell), ..
                } => shell.project_entries,
                Status::Connected { .. } => continue,
                // A dropped socket must not leave a stale "Also on".
                _ => Vec::new(),
            };
            let next = EnvironmentProjects {
                id: saved.environment_id.clone(),
                label: saved.label.clone(),
                machine,
                projects,
            };
            if last.as_ref() != Some(&next) {
                publish(&status, Status::Environment(next.clone()));
                last = Some(next);
            }
        }
    };
    let session = connection::run(
        &saved.http_base,
        &saved.credential,
        &connection,
        &actions_rx,
        &own_tx,
    );
    tokio::join!(forward, session);
}
