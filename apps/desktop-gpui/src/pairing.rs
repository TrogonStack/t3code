//! Attaching to a server: use the session the Keychain kept, else pair once with a token.

use crate::auth::{self, AuthError, Credential};
use crate::config::Attach;
use crate::connection::{self, Action, Ended, Environment};
use crate::credentials::{CredentialStore, StoredCredential};
use crate::status::{Status, StatusSender, publish};
use async_channel::Receiver;
use std::sync::Arc;

/// What to do with what is saved and what the launch handed over.
#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    UseSaved(StoredCredential),
    Pair(String),
    NeedsPairing,
}

/// A saved session wins: the launch's token is typically the same one on every start and works
/// once, so it is only for an environment with nothing saved (or whose saved session was just dropped).
pub fn next_step(saved: Option<StoredCredential>, pairing_token: Option<String>) -> Step {
    match (saved, pairing_token) {
        (Some(saved), _) => Step::UseSaved(saved),
        (None, Some(token)) => Step::Pair(token),
        (None, None) => Step::NeedsPairing,
    }
}

fn needs_pairing_message(label: &str, after_rejection: bool) -> String {
    let lead = if after_rejection {
        format!("The saved pairing with {label} was rejected (revoked or expired).")
    } else {
        format!("Not paired with {label}.")
    };
    format!(
        "{lead} Mint a token with `t3 pair` and relaunch with {} set.",
        crate::config::ENV_PAIRING_TOKEN
    )
}

async fn blocking<T: Send + 'static>(
    store: &Arc<dyn CredentialStore>,
    work: impl FnOnce(&dyn CredentialStore) -> T + Send + 'static,
) -> Result<T, String> {
    let store = store.clone();
    tokio::task::spawn_blocking(move || work(store.as_ref()))
        .await
        .map_err(|error| error.to_string())
}

/// Forgets what is saved for `environment_id`. Deleting what is absent is fine.
pub async fn forget(store: &Arc<dyn CredentialStore>, environment_id: &str) -> Result<(), String> {
    let id = environment_id.to_owned();
    blocking(store, move |store| store.delete(&id)).await?
}

/// Connects to an attached server and stays connected, pairing first when nothing is saved.
/// Returns only when pairing is needed and cannot happen on its own.
pub async fn run(
    store: Arc<dyn CredentialStore>,
    attach: Attach,
    environment: &Environment,
    unpair: bool,
    actions: &Receiver<Action>,
    status: &StatusSender,
) {
    let id = environment.id.clone();
    if unpair {
        match forget(&store, &id).await {
            Ok(()) => eprintln!(
                "[pairing] forgot the saved pairing with {}",
                environment.label
            ),
            Err(message) => eprintln!("[pairing] could not forget it: {message}"),
        }
    }
    let mut pairing_token = attach.pairing_token;
    let mut paired: Option<StoredCredential> = None;
    let mut rejected = false;
    loop {
        let saved = match paired.take() {
            Some(credential) => Some(credential),
            None => {
                let load_id = id.clone();
                match blocking(&store, move |store| store.load(&load_id)).await {
                    Ok(Ok(saved)) => saved,
                    Ok(Err(message)) | Err(message) => {
                        eprintln!("[pairing] could not read the saved pairing: {message}");
                        None
                    }
                }
            }
        };
        match next_step(saved, pairing_token.clone()) {
            Step::UseSaved(saved) => {
                let credential = Credential::Bearer(saved.token);
                let ended =
                    connection::run(&attach.http_base, &credential, environment, actions, status)
                        .await;
                match ended {
                    Ended::SessionRejected => {
                        if let Err(message) = forget(&store, &id).await {
                            eprintln!("[pairing] could not delete the rejected pairing: {message}");
                        }
                        rejected = true;
                    }
                }
            }
            Step::Pair(token) => {
                pairing_token = None;
                let client = auth::http_client();
                let exchanged = auth::exchange_session(
                    &client,
                    &attach.http_base,
                    &Credential::Pairing(auth::pairing_token(&token)),
                )
                .await;
                match exchanged {
                    Ok(session) => {
                        let now = auth::now_ms();
                        let credential = StoredCredential {
                            environment_id: id.clone(),
                            label: environment.label.clone(),
                            http_base: attach.http_base.clone(),
                            token: session.token,
                            saved_at_ms: now,
                            expires_at_ms: session.expires_in_secs.map(|s| now + s * 1000),
                        };
                        let to_save = credential.clone();
                        match blocking(&store, move |store| store.save(&to_save)).await {
                            Ok(Ok(())) => {}
                            Ok(Err(message)) | Err(message) => eprintln!(
                                "[pairing] paired, but could not save it (you will pair again next launch): {message}"
                            ),
                        }
                        paired = Some(credential);
                    }
                    Err(AuthError::Rejected(_)) => {
                        return publish(
                            status,
                            Status::Failed(
                                "The pairing token was rejected. It works only once and expires; mint a new one with `t3 pair`.".to_owned(),
                            ),
                        );
                    }
                    Err(error) => {
                        return publish(status, Status::Failed(format!("Pairing failed: {error}")));
                    }
                }
            }
            Step::NeedsPairing => {
                return publish(
                    status,
                    Status::Failed(needs_pairing_message(&environment.label, rejected)),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saved() -> StoredCredential {
        StoredCredential {
            environment_id: "e".to_owned(),
            label: "l".to_owned(),
            http_base: "http://h".to_owned(),
            token: "t".to_owned(),
            saved_at_ms: 1,
            expires_at_ms: None,
        }
    }

    #[test]
    fn a_saved_session_wins_over_the_launch_token() {
        assert_eq!(
            next_step(Some(saved()), Some("tok".to_owned())),
            Step::UseSaved(saved())
        );
    }

    #[test]
    fn nothing_saved_pairs_with_the_token_or_asks_for_one() {
        assert_eq!(
            next_step(None, Some("tok".to_owned())),
            Step::Pair("tok".to_owned())
        );
        assert_eq!(next_step(None, None), Step::NeedsPairing);
    }

    #[test]
    fn a_rejected_saved_session_falls_back_to_asking() {
        // `run` deletes the entry on rejection and clears the launch token once it was tried,
        // so the next decision sees nothing saved and no token.
        assert_eq!(next_step(None, None), Step::NeedsPairing);
        let message = needs_pairing_message("Studio", true);
        assert!(message.contains("rejected"));
        assert!(message.contains("t3 pair"));
        assert!(needs_pairing_message("Studio", false).starts_with("Not paired with Studio."));
    }

    #[tokio::test]
    async fn forgetting_removes_the_entry_and_is_idempotent() {
        let store: Arc<dyn CredentialStore> = Arc::new(crate::credentials::MemoryStore::default());
        store.save(&saved()).expect("save");
        forget(&store, "e").await.expect("forget");
        forget(&store, "e").await.expect("forget again");
        assert_eq!(store.load("e"), Ok(None));
    }
}
