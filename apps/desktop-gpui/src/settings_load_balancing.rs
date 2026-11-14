//! The client settings behind Settings > Environments > Load balancing: `loadBalancingEnabled`
//! and `loadBalancingWeights` (packages/contracts/src/settings.ts), kept in
//! `client-settings.json` beside every other key, which this store leaves alone.

use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LoadBalancing {
    pub enabled: bool,
    pub weights: BTreeMap<String, u32>,
}

/// `LoadPreference` of LoadBalancingSettings.tsx.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preference {
    Prefer,
    Normal,
    Less,
    Manual,
}

impl Preference {
    pub const ALL: [Self; 4] = [Self::Prefer, Self::Normal, Self::Less, Self::Manual];

    pub fn label(self) -> &'static str {
        match self {
            Self::Prefer => "Prefer",
            Self::Normal => "Normal",
            Self::Less => "Less often",
            Self::Manual => "Manual only",
        }
    }

    pub fn weight(self) -> u32 {
        match self {
            Self::Prefer => 100,
            Self::Normal => 50,
            Self::Less => 25,
            Self::Manual => 0,
        }
    }

    /// `loadPreferenceForWeight`: unset and 50 read as normal, below 50 as less often.
    pub fn of_weight(weight: Option<u32>) -> Self {
        match weight {
            None | Some(50) => Self::Normal,
            Some(0) => Self::Manual,
            Some(w) if w < 50 => Self::Less,
            Some(_) => Self::Prefer,
        }
    }
}

fn fields(value: &Value) -> Option<&Map<String, Value>> {
    let object = value.as_object()?;
    match object.get("settings") {
        Some(Value::Object(inner)) => Some(inner),
        _ => Some(object),
    }
}

impl LoadBalancing {
    pub fn parse(text: &str) -> Option<Self> {
        let value: Value = serde_json::from_str(text).ok()?;
        let map = fields(&value)?;
        Some(Self {
            enabled: map
                .get("loadBalancingEnabled")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            weights: map
                .get("loadBalancingWeights")
                .and_then(Value::as_object)
                .map(|weights| {
                    weights
                        .iter()
                        .filter_map(|(id, w)| Some((id.clone(), u32::try_from(w.as_u64()?).ok()?)))
                        .collect()
                })
                .unwrap_or_default(),
        })
    }

    pub fn preference(&self, environment_id: &str) -> Preference {
        Preference::of_weight(self.weights.get(environment_id).copied())
    }

    /// Writes these keys into `existing`, keeping every key this store does not own.
    pub fn to_json(&self, existing: Option<&str>) -> String {
        let mut document = existing
            .and_then(|text| serde_json::from_str::<Value>(text).ok())
            .filter(Value::is_object)
            .unwrap_or_else(|| json!({}));
        let target = match document.get_mut("settings") {
            Some(inner) if inner.is_object() => inner,
            _ => &mut document,
        };
        if let Some(map) = target.as_object_mut() {
            map.insert("loadBalancingEnabled".into(), json!(self.enabled));
            map.insert("loadBalancingWeights".into(), json!(self.weights));
        }
        document.to_string()
    }

    fn path() -> Option<std::path::PathBuf> {
        crate::settings::DeviceSettings::path()
    }

    pub fn load() -> Self {
        Self::path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    /// Re-reads the file first, so keys other tabs saved in the meantime survive.
    pub fn save(&self) {
        if let Some(path) = Self::path() {
            let existing = std::fs::read_to_string(&path).ok();
            let _ = std::fs::write(path, self.to_json(existing.as_deref()));
        }
    }
}

/// `summarizeLoadPreferences`: "<label> <preference>" for each non-normal environment.
pub fn summarize(settings: &LoadBalancing, environments: &[(String, String)]) -> Option<String> {
    let parts: Vec<String> = environments
        .iter()
        .filter_map(|(id, label)| {
            let preference = settings.preference(id);
            (preference != Preference::Normal)
                .then(|| format!("{label} {}", preference.label().to_lowercase()))
        })
        .collect();
    (!parts.is_empty()).then(|| parts.join(" \u{b7} "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weights_read_as_the_web_buckets_them() {
        assert_eq!(Preference::of_weight(None), Preference::Normal);
        assert_eq!(Preference::of_weight(Some(50)), Preference::Normal);
        assert_eq!(Preference::of_weight(Some(0)), Preference::Manual);
        assert_eq!(Preference::of_weight(Some(10)), Preference::Less);
        assert_eq!(Preference::of_weight(Some(75)), Preference::Prefer);
        for preference in Preference::ALL {
            assert_eq!(Preference::of_weight(Some(preference.weight())), preference);
        }
    }

    #[test]
    fn saving_keeps_other_keys_and_the_settings_wrapper() {
        let mut settings = LoadBalancing::default();
        settings.enabled = true;
        settings.weights.insert("e1".into(), 100);
        let saved = settings.to_json(Some(r#"{"settings":{"theme":"dark"}}"#));
        let value: Value = serde_json::from_str(&saved).unwrap();
        assert_eq!(value["settings"]["theme"], "dark");
        assert_eq!(LoadBalancing::parse(&saved), Some(settings));
    }

    #[test]
    fn the_summary_lists_only_non_normal_environments() {
        let mut settings = LoadBalancing::default();
        settings.weights.insert("b".into(), 0);
        settings.weights.insert("c".into(), 50);
        let environments = [
            ("a".to_owned(), "A".to_owned()),
            ("b".to_owned(), "Mini".to_owned()),
            ("c".to_owned(), "C".to_owned()),
        ];
        assert_eq!(
            summarize(&settings, &environments).as_deref(),
            Some("Mini manual only")
        );
        assert_eq!(summarize(&LoadBalancing::default(), &environments), None);
    }
}
