//! Where each other environment stands right now (`SupervisorConnectionState` on the web), so
//! Settings > Environments can show it. `environments::follow` writes it; the panel reads
//! `snapshot()` and wakes on `subscribe()`.

use crate::connection_diagnosis::{Failure, Via};
use crate::projects::MachineKind;
use std::sync::{Mutex, OnceLock};
use tokio::sync::watch;

/// `ConnectionPhase` of `presentConnectionState`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Available,
    Offline,
    Connecting,
    Reconnecting,
    Connected,
    Error,
    Unsupported,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvState {
    pub id: String,
    pub label: String,
    /// The http base this app reaches it at (`displayUrl`).
    pub url: String,
    pub machine: MachineKind,
    pub phase: Phase,
    pub failure: Option<Failure>,
    pub attempt: u32,
    pub retry_at_ms: Option<u64>,
    /// The route the live connection uses.
    pub via: Option<Via>,
    /// A pairing saved on this device (the direct route).
    pub saved: bool,
    /// T3 Connect lists it for the signed-in account (the relay route).
    pub relay_listed: bool,
    /// The T3 Connect endpoint's http base, when it lists this environment.
    pub relay_url: Option<String>,
    pub server_version: Option<String>,
}

impl EnvState {
    pub fn new(id: String, label: String) -> Self {
        Self {
            id,
            label,
            url: String::new(),
            machine: MachineKind::Server,
            phase: Phase::Connecting,
            failure: None,
            attempt: 0,
            retry_at_ms: None,
            via: None,
            saved: false,
            relay_listed: false,
            relay_url: None,
            server_version: None,
        }
    }
}

struct Registry {
    entries: Mutex<Vec<EnvState>>,
    version: watch::Sender<u64>,
}

fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(|| Registry {
        entries: Mutex::default(),
        version: watch::channel(0).0,
    })
}

fn changed() {
    registry().version.send_modify(|version| *version += 1);
}

pub fn subscribe() -> watch::Receiver<u64> {
    registry().version.subscribe()
}

pub fn snapshot() -> Vec<EnvState> {
    registry()
        .entries
        .lock()
        .map(|entries| entries.clone())
        .unwrap_or_default()
}

pub fn get(id: &str) -> Option<EnvState> {
    snapshot().into_iter().find(|entry| entry.id == id)
}

/// Adds or replaces an entry, keeping the order of the first insert.
pub fn put(state: EnvState) {
    if let Ok(mut entries) = registry().entries.lock() {
        match entries.iter_mut().find(|entry| entry.id == state.id) {
            Some(entry) => *entry = state,
            None => entries.push(state),
        }
    }
    changed();
}

/// Edits an entry; wakes listeners only when something differs.
pub fn update(id: &str, edit: impl FnOnce(&mut EnvState)) {
    let differs = registry().entries.lock().is_ok_and(|mut entries| {
        let Some(entry) = entries.iter_mut().find(|entry| entry.id == id) else {
            return false;
        };
        let before = entry.clone();
        edit(entry);
        *entry != before
    });
    if differs {
        changed();
    }
}

pub fn remove(id: &str) {
    if let Ok(mut entries) = registry().entries.lock() {
        entries.retain(|entry| entry.id != id);
    }
    changed();
}

/// `presentConnectionState`'s phase for a failure that has not connected yet: the first try with
/// no failure is still "connecting".
pub fn pending_phase(attempt: u32, failure: &Option<Failure>) -> Phase {
    if attempt <= 1 && failure.is_none() {
        Phase::Connecting
    } else {
        Phase::Reconnecting
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection_diagnosis::Reason;

    #[test]
    fn the_first_try_is_connecting_and_a_failed_one_reconnecting() {
        assert_eq!(pending_phase(0, &None), Phase::Connecting);
        assert_eq!(pending_phase(1, &None), Phase::Connecting);
        assert_eq!(pending_phase(2, &None), Phase::Reconnecting);
        let failure = Some(Failure::new(Reason::Network, "x"));
        assert_eq!(pending_phase(1, &failure), Phase::Reconnecting);
    }

    #[test]
    fn put_keeps_order_and_update_edits_in_place() {
        put(EnvState::new("env-order-a".into(), "A".into()));
        put(EnvState::new("env-order-b".into(), "B".into()));
        update("env-order-a", |entry| entry.phase = Phase::Connected);
        let ids: Vec<_> = snapshot()
            .into_iter()
            .filter(|e| e.id.starts_with("env-order-"))
            .collect();
        assert_eq!(ids[0].id, "env-order-a");
        assert_eq!(ids[0].phase, Phase::Connected);
        remove("env-order-a");
        remove("env-order-b");
        assert!(get("env-order-a").is_none());
    }
}
