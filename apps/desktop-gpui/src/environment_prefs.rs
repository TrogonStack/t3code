//! What this device remembers per saved environment beyond its pairing: whether it is switched
//! off (`savedEnvironmentRegistry` `enabled`) and the pull request sharing permission
//! (`pullRequestRoutingPermissions`). Kept in `environment-prefs.json` beside
//! `client-settings.json`.

use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// `PullRequestRoutingPermission` ("off" is the absence of an entry).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sharing {
    Off,
    Read,
    ReadWrite,
}

impl Sharing {
    pub const ALL: [Self; 3] = [Self::Off, Self::Read, Self::ReadWrite];

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Read => "Read PRs",
            Self::ReadWrite => "Read and act",
        }
    }

    fn wire(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Read => "read",
            Self::ReadWrite => "read-write",
        }
    }

    fn from_wire(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|value| value.wire() == text)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Prefs {
    pub disabled: BTreeSet<String>,
    pub sharing: BTreeMap<String, Sharing>,
}

impl Prefs {
    pub fn enabled(&self, id: &str) -> bool {
        !self.disabled.contains(id)
    }

    pub fn sharing_of(&self, id: &str) -> Sharing {
        self.sharing.get(id).copied().unwrap_or(Sharing::Off)
    }

    pub fn parse(text: &str) -> Self {
        let Ok(Value::Object(map)) = serde_json::from_str::<Value>(text) else {
            return Self::default();
        };
        let disabled = map
            .get("disabled")
            .and_then(Value::as_array)
            .map(|ids| {
                ids.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        let sharing = map
            .get("pullRequestRouting")
            .and_then(Value::as_object)
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|(id, value)| {
                        let sharing = Sharing::from_wire(value.as_str()?)?;
                        (sharing != Sharing::Off).then(|| (id.clone(), sharing))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self { disabled, sharing }
    }

    pub fn to_json(&self) -> String {
        let routing: Map<String, Value> = self
            .sharing
            .iter()
            .map(|(id, sharing)| (id.clone(), json!(sharing.wire())))
            .collect();
        json!({ "disabled": self.disabled, "pullRequestRouting": routing }).to_string()
    }
}

fn path() -> Option<PathBuf> {
    crate::settings::DeviceSettings::path()?
        .parent()
        .map(|dir| dir.join("environment-prefs.json"))
}

fn cache() -> &'static Mutex<Option<Prefs>> {
    static CACHE: OnceLock<Mutex<Option<Prefs>>> = OnceLock::new();
    CACHE.get_or_init(Mutex::default)
}

pub fn load() -> Prefs {
    let Ok(mut cached) = cache().lock() else {
        return Prefs::default();
    };
    cached
        .get_or_insert_with(|| {
            path()
                .and_then(|path| std::fs::read_to_string(path).ok())
                .map(|text| Prefs::parse(&text))
                .unwrap_or_default()
        })
        .clone()
}

pub fn update(edit: impl FnOnce(&mut Prefs)) {
    let mut prefs = load();
    edit(&mut prefs);
    if let Some(path) = path() {
        let _ = std::fs::write(path, prefs.to_json());
    }
    if let Ok(mut cached) = cache().lock() {
        *cached = Some(prefs);
    }
}

pub fn set_enabled(id: &str, enabled: bool) {
    update(|prefs| {
        if enabled {
            prefs.disabled.remove(id);
        } else {
            prefs.disabled.insert(id.to_owned());
        }
    });
}

pub fn set_sharing(id: &str, sharing: Sharing) {
    update(|prefs| {
        if sharing == Sharing::Off {
            prefs.sharing.remove(id);
        } else {
            prefs.sharing.insert(id.to_owned(), sharing);
        }
    });
}

/// Forgets everything kept for a removed environment.
pub fn forget(id: &str) {
    update(|prefs| {
        prefs.disabled.remove(id);
        prefs.sharing.remove(id);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_treats_off_as_absent() {
        let mut prefs = Prefs::default();
        prefs.disabled.insert("a".into());
        prefs.sharing.insert("b".into(), Sharing::ReadWrite);
        let back = Prefs::parse(&prefs.to_json());
        assert_eq!(back, prefs);
        assert!(!back.enabled("a"));
        assert!(back.enabled("b"));
        assert_eq!(back.sharing_of("zzz"), Sharing::Off);
        let parsed = Prefs::parse(r#"{"pullRequestRouting":{"x":"off","y":"read","z":"bogus"}}"#);
        assert_eq!(parsed.sharing.len(), 1);
        assert_eq!(parsed.sharing_of("y"), Sharing::Read);
    }

    #[test]
    fn garbage_reads_as_defaults() {
        assert_eq!(Prefs::parse("nope"), Prefs::default());
        assert_eq!(Prefs::parse("[]"), Prefs::default());
    }
}
