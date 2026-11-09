//! The Providers tab's model (`ProviderInstanceCard.tsx`, `ProviderSettingsPanel.tsx`): the
//! environment variables of one provider instance, how a row edit changes them, which rows a
//! save publishes, and the `server.updateSettings` payload that stores an instance.

use serde_json::{Map, Value, json};

/// Call tokens, above every other range.
pub const TOKEN_BASE: u64 = 1 << 51;

const NAME_MAX_CHARS: usize = 128;

/// What a variable holds: a literal, or a reference the server resolves from a secret provider.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Content {
    Text(String),
    Secret { provider: String, reference: String },
}

/// `ProviderInstanceEnvironmentVariable` and `valueRedacted`, the marker for a stored literal the
/// server does not send back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Variable {
    pub name: String,
    pub content: Content,
    pub sensitive: bool,
    pub redacted: Option<bool>,
}

impl Variable {
    /// `{id: provider-env-N, name: "", value: "", sensitive: true}`, the row "Add variable" makes.
    pub fn blank() -> Self {
        Self {
            name: String::new(),
            content: Content::Text(String::new()),
            sensitive: true,
            redacted: None,
        }
    }

    /// A typed value replaces a redacted one.
    pub fn with_text(mut self, text: String) -> Self {
        self.content = Content::Text(text);
        self.redacted = Some(false);
        self
    }

    /// `setVariableSecret`: a reference (or `None` for Value) drops the redaction marker.
    pub fn with_secret(mut self, secret: Option<(String, String)>) -> Self {
        self.content = match secret {
            Some((provider, reference)) => Content::Secret {
                provider,
                reference,
            },
            None => Content::Text(String::new()),
        };
        self.redacted = None;
        self
    }

    /// The lock toggle. Going sensitive keeps the marker; going plain clears it, since the saved
    /// literal cannot be shown.
    pub fn with_sensitive(mut self, sensitive: bool) -> Self {
        self.sensitive = sensitive;
        if !(sensitive && self.redacted.is_none()) {
            self.redacted = if sensitive {
                self.redacted
            } else {
                Some(false)
            };
        }
        self
    }

    pub fn is_redacted(&self) -> bool {
        self.redacted == Some(true)
    }
}

/// `ENVIRONMENT_VARIABLE_NAME_PATTERN` with the trimmed, non-empty and 128 character limits.
pub fn valid_name(name: &str) -> bool {
    let name = name.trim();
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    name.chars().count() <= NAME_MAX_CHARS
        && (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// `publishRows`: the rows to store, or `None` while a row with a bad name holds something the
/// user typed. A blank untouched row is skipped.
pub fn publish(rows: &[Variable]) -> Option<Vec<Variable>> {
    let mut published = Vec::new();
    for row in rows {
        let name = row.name.trim();
        if !valid_name(name) {
            let blank = name.is_empty()
                && row.content == Content::Text(String::new())
                && row.sensitive
                && row.redacted.is_none();
            if blank {
                continue;
            }
            return None;
        }
        published.push(Variable {
            name: name.to_owned(),
            ..row.clone()
        });
    }
    Some(published)
}

/// Reads a stored variable; a `secret` (or a reference as `value`) is a sensitive reference.
pub fn decode(instance: &Value) -> Vec<Variable> {
    let Some(list) = instance.get("environment").and_then(Value::as_array) else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|entry| {
            let name = entry.get("name")?.as_str()?.to_owned();
            let reference = |value: &Value| {
                crate::secret_source::reference_of(value).map(|(provider, reference)| {
                    Content::Secret {
                        provider,
                        reference,
                    }
                })
            };
            let secret = entry
                .get("secret")
                .and_then(reference)
                .or_else(|| entry.get("value").and_then(reference));
            let (content, sensitive) = match secret {
                Some(content) => (content, true),
                None => (
                    Content::Text(entry.get("value")?.as_str().unwrap_or("").to_owned()),
                    entry.get("sensitive").and_then(Value::as_bool) == Some(true),
                ),
            };
            Some(Variable {
                name,
                content,
                sensitive,
                redacted: entry.get("valueRedacted").and_then(Value::as_bool),
            })
        })
        .collect()
}

/// The stored form: a reference is written beside an empty value.
pub fn encode(variable: &Variable) -> Value {
    let mut entry = Map::new();
    entry.insert("name".into(), json!(variable.name));
    match &variable.content {
        Content::Text(text) => {
            entry.insert("value".into(), json!(text));
        }
        Content::Secret {
            provider,
            reference,
        } => {
            entry.insert("value".into(), json!(""));
            entry.insert(
                "secret".into(),
                crate::secret_source::reference_value(provider, reference),
            );
        }
    }
    entry.insert("sensitive".into(), json!(variable.sensitive));
    if let Some(redacted) = variable.redacted
        && !matches!(variable.content, Content::Secret { .. })
    {
        entry.insert("valueRedacted".into(), json!(redacted));
    }
    Value::Object(entry)
}

/// The instance as stored once its variables are `variables`: `base` (the instance in the
/// settings, or `{driver}` for a default slot nothing configured) with `environment` replaced.
pub fn with_environment(base: &Value, variables: &[Variable]) -> Value {
    let mut instance = base.as_object().cloned().unwrap_or_default();
    if variables.is_empty() {
        instance.remove("environment");
    } else {
        instance.insert(
            "environment".into(),
            Value::Array(variables.iter().map(encode).collect()),
        );
    }
    Value::Object(instance)
}

/// The `server.updateSettings` payload that stores one instance (`persistInstance`).
pub fn upsert_payload(instance_id: &str, instance: &Value) -> Value {
    json!({
        "patch": {},
        "providerInstanceMutation": {
            "operation": "upsert",
            "instanceId": instance_id,
            "instance": instance,
        },
    })
}

/// The settings with `instance` stored, to show a save before the server answers.
pub fn apply_instance(settings: &mut Value, instance_id: &str, instance: &Value) {
    if !settings.is_object() {
        *settings = json!({});
    }
    let Some(root) = settings.as_object_mut() else {
        return;
    };
    let stored = root.entry("providerInstances").or_insert_with(|| json!({}));
    if !stored.is_object() {
        *stored = json!({});
    }
    if let Some(stored) = stored.as_object_mut() {
        stored.insert(instance_id.to_owned(), instance.clone());
    }
}

/// The instance in the settings, or `{driver}` while the default slot is unconfigured.
pub fn base_instance(settings: &Value, instance_id: &str, driver: &str) -> Value {
    settings
        .get("providerInstances")
        .and_then(|instances| instances.get(instance_id))
        .filter(|instance| instance.is_object())
        .cloned()
        .unwrap_or_else(|| json!({ "driver": driver }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(name: &str, value: &str) -> Variable {
        Variable {
            name: name.to_owned(),
            content: Content::Text(value.to_owned()),
            sensitive: false,
            redacted: None,
        }
    }

    #[test]
    fn names_follow_the_web_pattern() {
        for ok in ["API_KEY", "_x", "a1", " PADDED "] {
            assert!(valid_name(ok), "{ok}");
        }
        for bad in ["", "  ", "1ABC", "A-B", "A B", &"A".repeat(129)] {
            assert!(!valid_name(bad), "{bad}");
        }
        assert!(valid_name(&"A".repeat(128)));
    }

    #[test]
    fn a_literal_encodes_with_its_flags() {
        let secret = Variable {
            name: "API_KEY".into(),
            content: Content::Text("abc".into()),
            sensitive: true,
            redacted: Some(false),
        };
        assert_eq!(
            encode(&secret),
            json!({"name": "API_KEY", "value": "abc", "sensitive": true, "valueRedacted": false})
        );
        assert_eq!(
            encode(&plain("MODE", "fast")),
            json!({"name": "MODE", "value": "fast", "sensitive": false})
        );
    }

    #[test]
    fn a_reference_encodes_beside_an_empty_value() {
        let variable = Variable {
            name: "API_KEY".into(),
            content: Content::Secret {
                provider: "vault".into(),
                reference: "op://Vault/item/FIELD".into(),
            },
            sensitive: true,
            redacted: Some(true),
        };
        assert_eq!(
            encode(&variable),
            json!({
                "name": "API_KEY",
                "value": "",
                "sensitive": true,
                "secret": {"provider": "vault", "reference": "op://Vault/item/FIELD"},
            })
        );
    }

    #[test]
    fn decode_reads_the_stored_and_the_inline_reference() {
        let reference = json!({"provider": "vault", "reference": "op://Vault/item/FIELD"});
        let instance = json!({"driver": "codex", "environment": [
            {"name": "A", "value": "", "sensitive": false, "secret": reference},
            {"name": "B", "value": reference, "sensitive": false},
            {"name": "C", "value": "", "sensitive": true, "valueRedacted": true},
            {"name": "D", "value": "x"},
        ]});
        let rows = decode(&instance);
        let secret = Content::Secret {
            provider: "vault".into(),
            reference: "op://Vault/item/FIELD".into(),
        };
        assert_eq!((&rows[0].content, rows[0].sensitive), (&secret, true));
        assert_eq!((&rows[1].content, rows[1].sensitive), (&secret, true));
        assert!(rows[2].sensitive && rows[2].is_redacted());
        assert_eq!(rows[3], plain("D", "x"));
    }

    #[test]
    fn a_decoded_row_encodes_back_the_same() {
        let stored = json!({"driver": "codex", "environment": [
            {"name": "A", "value": "", "sensitive": true, "valueRedacted": true},
            {"name": "B", "value": "x", "sensitive": false},
        ]});
        assert_eq!(
            with_environment(&stored, &decode(&stored)),
            stored,
            "unchanged rows round-trip"
        );
    }

    #[test]
    fn typing_replaces_a_redacted_value() {
        let redacted = Variable {
            name: "API_KEY".into(),
            content: Content::Text(String::new()),
            sensitive: true,
            redacted: Some(true),
        };
        let typed = redacted.with_text("new".into());
        assert_eq!(typed.redacted, Some(false));
        assert_eq!(typed.content, Content::Text("new".into()));
    }

    #[test]
    fn the_lock_toggle_keeps_or_clears_the_marker() {
        let redacted = Variable {
            sensitive: true,
            redacted: Some(true),
            ..Variable::blank()
        };
        assert_eq!(redacted.clone().with_sensitive(true).redacted, Some(true));
        assert_eq!(redacted.with_sensitive(false).redacted, Some(false));
        let fresh = Variable::blank();
        assert_eq!(fresh.clone().with_sensitive(true).redacted, None);
        let plain = fresh.with_sensitive(false);
        assert_eq!((plain.sensitive, plain.redacted), (false, Some(false)));
        assert_eq!(plain.with_sensitive(true).redacted, Some(false));
    }

    #[test]
    fn a_secret_drops_the_marker_and_value_clears_it() {
        let row = Variable {
            redacted: Some(true),
            ..Variable::blank()
        };
        let picked = row
            .clone()
            .with_secret(Some(("vault".into(), "op://V/i/F".into())));
        assert_eq!(picked.redacted, None);
        assert!(matches!(picked.content, Content::Secret { .. }));
        let cleared = picked.with_secret(None);
        assert_eq!(cleared.content, Content::Text(String::new()));
    }

    #[test]
    fn publishing_skips_blank_rows_and_stops_on_a_bad_name() {
        let blank = Variable::blank();
        let good = plain(" MODE ", "fast");
        let rows = publish(&[blank.clone(), good.clone()]).expect("publishes");
        assert_eq!(rows, vec![plain("MODE", "fast")]);

        let typed_value = Variable {
            content: Content::Text("abc".into()),
            ..Variable::blank()
        };
        assert_eq!(publish(&[good.clone(), typed_value]), None);
        assert_eq!(
            publish(&[plain("", "")]),
            None,
            "a plain empty row was touched"
        );
        assert_eq!(publish(&[plain("1BAD", "")]), None);
        let marked = Variable {
            redacted: Some(false),
            ..Variable::blank()
        };
        assert_eq!(publish(&[marked]), None);
    }

    #[test]
    fn the_payload_replaces_the_environment_only() {
        let base = json!({"driver": "codex", "displayName": "Work", "config": {"k": 1},
            "environment": [{"name": "OLD", "value": "x", "sensitive": false}]});
        let instance = with_environment(&base, &[plain("MODE", "fast")]);
        assert_eq!(instance["displayName"], "Work");
        assert_eq!(instance["config"], json!({"k": 1}));
        assert_eq!(instance["environment"].as_array().map(Vec::len), Some(1));
        let payload = upsert_payload("codex_work", &instance);
        assert_eq!(payload["patch"], json!({}));
        assert_eq!(
            payload["providerInstanceMutation"]["operation"],
            json!("upsert")
        );
        assert_eq!(
            payload["providerInstanceMutation"]["instanceId"],
            json!("codex_work")
        );
        assert!(with_environment(&base, &[]).get("environment").is_none());
    }

    #[test]
    fn an_unconfigured_slot_starts_from_its_driver() {
        let settings =
            json!({"providerInstances": {"codex": {"driver": "codex", "enabled": false}}});
        assert_eq!(
            base_instance(&settings, "claudeAgent", "claudeAgent"),
            json!({"driver": "claudeAgent"})
        );
        assert_eq!(base_instance(&settings, "codex", "codex")["enabled"], false);
        let mut shown = settings.clone();
        apply_instance(&mut shown, "claudeAgent", &json!({"driver": "claudeAgent"}));
        assert!(shown["providerInstances"]["claudeAgent"].is_object());
    }
}
