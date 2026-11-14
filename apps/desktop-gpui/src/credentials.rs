//! The session this app holds for each environment it paired with, kept in the macOS Keychain.
//!
//! The server exchanges a one-time pairing token for a bearer session that lasts 30 days
//! (`DEFAULT_SESSION_TTL`, `apps/server/src/auth/SessionStore.ts`), and the same session is how
//! the web and Electron clients stay connected to a direct environment. There is no refresh
//! token: a revoked or expired session means pairing again. A DPoP-bound token is not an option
//! here, because it lives one hour and only another one-time credential can mint the next.

use serde::{Deserialize, Serialize};

/// Secret-store service every item is stored under; the account is the environment id.
const SERVICE: &str = "T3 Code (GPUI) Environment Sessions";

/// What `exchange_bearer` saved for one environment. The token is a secret: `Debug` hides it.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredCredential {
    pub environment_id: String,
    pub label: String,
    pub http_base: String,
    pub token: String,
    pub saved_at_ms: u64,
    /// When the server says the session ends; informational, the server decides.
    #[serde(default)]
    pub expires_at_ms: Option<u64>,
}

impl std::fmt::Debug for StoredCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoredCredential")
            .field("environment_id", &self.environment_id)
            .field("label", &self.label)
            .field("http_base", &self.http_base)
            .field("token", &"<redacted>")
            .field("saved_at_ms", &self.saved_at_ms)
            .field("expires_at_ms", &self.expires_at_ms)
            .finish()
    }
}

impl StoredCredential {
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        serde_json::to_vec(self)
            .map_err(|error| format!("could not encode the credential: {error}"))
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        serde_json::from_slice(bytes).map_err(|_| "the stored credential is unreadable".to_owned())
    }
}

/// Where credentials live. Blocking: the Keychain may prompt, so call from `spawn_blocking`.
pub trait CredentialStore: Send + Sync {
    fn load(&self, environment_id: &str) -> Result<Option<StoredCredential>, String>;
    fn save(&self, credential: &StoredCredential) -> Result<(), String>;
    /// Deleting what is not there succeeds: the reverse state must be idempotent.
    fn delete(&self, environment_id: &str) -> Result<(), String>;
    /// Every environment this app is paired with; unreadable entries are skipped.
    fn list(&self) -> Result<Vec<StoredCredential>, String>;
}

/// The platform secret store; see `secret_store` for where debug builds keep it.
pub struct Keychain;

impl CredentialStore for Keychain {
    fn load(&self, environment_id: &str) -> Result<Option<StoredCredential>, String> {
        crate::secret_store::get(SERVICE, environment_id)?
            .map(|bytes| StoredCredential::decode(&bytes))
            .transpose()
    }

    fn save(&self, credential: &StoredCredential) -> Result<(), String> {
        crate::secret_store::set(SERVICE, &credential.environment_id, &credential.encode()?)
    }

    fn delete(&self, environment_id: &str) -> Result<(), String> {
        crate::secret_store::delete(SERVICE, environment_id)
    }

    fn list(&self) -> Result<Vec<StoredCredential>, String> {
        Ok(crate::secret_store::accounts(SERVICE)?
            .iter()
            .filter_map(|account| self.load(account).ok().flatten())
            .collect())
    }
}

#[cfg(test)]
pub use memory::MemoryStore;

#[cfg(test)]
mod memory {
    use super::{CredentialStore, StoredCredential};
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    /// Round-trips through the encoded form so tests exercise (de)serialization too.
    #[derive(Default)]
    pub struct MemoryStore(Mutex<BTreeMap<String, Vec<u8>>>);

    impl CredentialStore for MemoryStore {
        fn load(&self, id: &str) -> Result<Option<StoredCredential>, String> {
            let items = self.0.lock().expect("store lock");
            items
                .get(id)
                .map(|bytes| StoredCredential::decode(bytes))
                .transpose()
        }
        fn save(&self, credential: &StoredCredential) -> Result<(), String> {
            let bytes = credential.encode()?;
            self.0
                .lock()
                .expect("store lock")
                .insert(credential.environment_id.clone(), bytes);
            Ok(())
        }
        fn delete(&self, id: &str) -> Result<(), String> {
            self.0.lock().expect("store lock").remove(id);
            Ok(())
        }
        fn list(&self) -> Result<Vec<StoredCredential>, String> {
            let items = self.0.lock().expect("store lock");
            Ok(items
                .values()
                .filter_map(|bytes| StoredCredential::decode(bytes).ok())
                .collect())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(id: &str) -> StoredCredential {
        StoredCredential {
            environment_id: id.to_owned(),
            label: "Studio".to_owned(),
            http_base: "http://127.0.0.1:3773".to_owned(),
            token: "secret-token".to_owned(),
            saved_at_ms: 10,
            expires_at_ms: Some(20),
        }
    }

    #[test]
    fn round_trips_through_json() {
        let credential = sample("env-1");
        assert_eq!(
            StoredCredential::decode(&credential.encode().expect("encode")),
            Ok(credential)
        );
    }

    #[test]
    fn old_entries_without_an_expiry_still_decode() {
        let json =
            br#"{"environment_id":"e","label":"l","http_base":"h","token":"t","saved_at_ms":1}"#;
        assert_eq!(
            StoredCredential::decode(json)
                .expect("decode")
                .expires_at_ms,
            None
        );
    }

    #[test]
    fn garbage_is_an_error_that_does_not_echo_the_bytes() {
        let error = StoredCredential::decode(b"not json secret").expect_err("garbage");
        assert!(!error.contains("secret"));
    }

    #[test]
    fn debug_hides_the_token() {
        assert!(!format!("{:?}", sample("e")).contains("secret-token"));
    }

    #[test]
    fn memory_store_saves_lists_and_deletes_idempotently() {
        let store = MemoryStore::default();
        assert_eq!(store.load("a"), Ok(None));
        store.save(&sample("a")).expect("save");
        store.save(&sample("b")).expect("save");
        assert_eq!(store.load("a"), Ok(Some(sample("a"))));
        assert_eq!(store.list().expect("list").len(), 2);
        store.delete("a").expect("delete");
        store.delete("a").expect("delete again");
        assert_eq!(store.load("a"), Ok(None));
        assert_eq!(store.list().expect("list"), vec![sample("b")]);
    }

    /// Talks to the real secret store: `cargo test -- --ignored keychain`.
    #[test]
    #[ignore = "touches the real secret store"]
    fn keychain_saves_lists_and_deletes() {
        let id = format!("smoke-test-{}", std::process::id());
        let store = Keychain;
        store.save(&sample(&id)).expect("save");
        assert_eq!(store.load(&id), Ok(Some(sample(&id))));
        assert!(store.list().expect("list").contains(&sample(&id)));
        store.delete(&id).expect("delete");
        store.delete(&id).expect("delete again");
        assert_eq!(store.load(&id), Ok(None));
        assert!(!store.list().expect("list").contains(&sample(&id)));
    }
}
