//! The Providers tab's model (`ProviderInstanceCard.tsx`, `ProviderSettingsPanel.tsx`): the
//! environment variables of one provider instance, how a row edit changes them, which rows a
//! save publishes, and the `server.updateSettings` payload that stores an instance.

use serde_json::{Map, Value, json};
use std::collections::HashSet;

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

/// The drivers the Add provider dialog offers (the web's list minus the ACP registry flow).
pub const DRIVERS: [&str; 10] = [
    "codex",
    "claudeAgent",
    "cursor",
    "opencode",
    "grok",
    "antigravity",
    "kiro",
    "pi",
    "fx",
    "muse",
];

const INSTANCE_ID_MAX: usize = 64;
const SLUG_MAX: usize = 48;

/// The ids an instance may not take: the two built-in slots and every stored instance.
pub fn existing_ids(settings: &Value) -> HashSet<String> {
    let mut ids: HashSet<String> = ["codex", "claudeAgent"].map(str::to_owned).into();
    if let Some(stored) = settings.get("providerInstances").and_then(Value::as_object) {
        ids.extend(stored.keys().cloned());
    }
    ids
}

/// `validateInstanceId`: the message the dialog shows, or `None` when the id is usable.
pub fn validate_instance_id(id: &str, existing: &HashSet<String>) -> Option<String> {
    let id = id.trim();
    if id.is_empty() {
        return Some("Instance ID is required.".to_owned());
    }
    if id.chars().count() > INSTANCE_ID_MAX {
        return Some("Instance ID must be 64 characters or fewer.".to_owned());
    }
    let mut chars = id.chars();
    let starts = chars.next().is_some_and(|c| c.is_ascii_alphabetic());
    if !starts || !chars.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Some(
            "Instance ID must start with a letter and use only letters, digits, '-', or '_'."
                .to_owned(),
        );
    }
    existing
        .contains(id)
        .then(|| format!("An instance named '{id}' already exists."))
}

fn slug(label: &str) -> String {
    let mut out = String::new();
    let mut gap = false;
    for c in label.trim().to_lowercase().chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            if gap && !out.is_empty() {
                out.push('_');
            }
            gap = false;
            out.push(c);
        } else {
            gap = true;
        }
    }
    out.chars().take(SLUG_MAX).collect()
}

/// `deriveAvailableInstanceId`: the driver itself for a blank or default label, else
/// `<driver>_<slug>`, numbered until nothing stored has it.
pub fn derive_instance_id(
    driver: &str,
    driver_label: &str,
    label: &str,
    existing: &HashSet<String>,
) -> String {
    let base = if label.trim().is_empty() || label == driver_label {
        driver.to_owned()
    } else {
        let slug = slug(label);
        if slug.is_empty() {
            String::new()
        } else {
            format!("{driver}_{slug}")
        }
    };
    if base.is_empty() || !existing.contains(&base) {
        return base;
    }
    (2..)
        .map(|n| {
            let suffix = format!("_{n}");
            let keep: String = base.chars().take(INSTANCE_ID_MAX - suffix.len()).collect();
            format!("{keep}{suffix}")
        })
        .find(|candidate| !existing.contains(candidate))
        .unwrap_or(base)
}

/// The `server.updateSettings` payload that adds an instance.
pub fn create_payload(instance_id: &str, driver: &str, label: &str) -> Value {
    let mut instance = json!({ "driver": driver, "enabled": true });
    if !label.trim().is_empty() {
        instance["displayName"] = json!(label.trim());
    }
    if driver == "codex" {
        instance["config"] = json!({ "setupMode": "existing" });
    }
    json!({
        "patch": {},
        "providerInstanceMutation": {
            "operation": "create",
            "instanceId": instance_id,
            "instance": instance,
        },
    })
}

/// The `server.updateSettings` payload that removes an instance.
pub fn remove_payload(instance_id: &str) -> Value {
    json!({
        "patch": {},
        "providerInstanceMutation": { "operation": "remove", "instanceId": instance_id },
    })
}

/// The settings without `instance_id`, to show a removal before the server answers.
pub fn forget_instance(settings: &mut Value, instance_id: &str) {
    if let Some(stored) = settings
        .get_mut("providerInstances")
        .and_then(Value::as_object_mut)
    {
        stored.remove(instance_id);
    }
}

/// One text field of a driver's Runtime section.
pub struct RuntimeField {
    pub key: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub placeholder: &'static str,
}

const CODEX_FIELDS: [RuntimeField; 4] = [
    RuntimeField {
        key: "binaryPath",
        label: "Binary path",
        description: "Path to the Codex binary used by this instance.",
        placeholder: "codex",
    },
    RuntimeField {
        key: "homePath",
        label: "CODEX_HOME path",
        description: "Custom Codex home and config directory.",
        placeholder: "~/.codex",
    },
    RuntimeField {
        key: "shadowHomePath",
        label: "Shadow home path",
        description: "Account-specific Codex home. Keeps auth.json separate while sharing state from CODEX_HOME.",
        placeholder: "~/.codex-t3/personal",
    },
    RuntimeField {
        key: "launchArgs",
        label: "Launch arguments",
        description: "Additional CLI arguments passed to codex app-server on session start.",
        placeholder: "",
    },
];

const CLAUDE_FIELDS: [RuntimeField; 4] = [
    RuntimeField {
        key: "binaryPath",
        label: "Binary path",
        description: "Path to the Claude binary used by this instance.",
        placeholder: "claude",
    },
    RuntimeField {
        key: "homePath",
        label: "CLAUDE_CONFIG_DIR path",
        description: "Custom Claude home and config directory. Keeps .claude.json and .claude separate.",
        placeholder: "~/.claude",
    },
    RuntimeField {
        key: "autoCompactWindow",
        label: "Auto-compact after",
        description: "Compact after 100,000 to 1,000,000 tokens. Leave empty to use Claude's default.",
        placeholder: "e.g. 300000",
    },
    RuntimeField {
        key: "launchArgs",
        label: "Launch arguments",
        description: "Additional CLI arguments passed on session start.",
        placeholder: "e.g. --chrome",
    },
];

/// The Runtime fields this client knows; other drivers keep what the server stored.
pub fn runtime_fields(driver: &str) -> &'static [RuntimeField] {
    match driver {
        "codex" => &CODEX_FIELDS,
        "claudeAgent" => &CLAUDE_FIELDS,
        _ => &[],
    }
}

/// A Runtime field as stored, or empty.
pub fn runtime_value(instance: &Value, key: &str) -> String {
    instance["config"][key].as_str().unwrap_or("").to_owned()
}

/// `instance` with one Runtime field set; an empty value drops the key (`clearWhenEmpty: omit`).
pub fn with_runtime(instance: &Value, key: &str, value: &str) -> Value {
    let mut next = instance.clone();
    let Some(object) = next.as_object_mut() else {
        return next;
    };
    let config = object.entry("config").or_insert_with(|| json!({}));
    if !config.is_object() {
        *config = json!({});
    }
    if let Some(config) = config.as_object_mut() {
        let value = value.trim();
        if value.is_empty() {
            config.remove(key);
        } else {
            config.insert(key.to_owned(), json!(value));
        }
    }
    next
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

    fn ids(list: &[&str]) -> HashSet<String> {
        list.iter().map(|id| (*id).to_owned()).collect()
    }

    #[test]
    fn instance_ids_follow_the_web_rules() {
        let taken = ids(&["codex", "claudeAgent", "codex_work"]);
        assert_eq!(
            validate_instance_id(" ", &taken).as_deref(),
            Some("Instance ID is required.")
        );
        assert_eq!(
            validate_instance_id(&"a".repeat(65), &taken).as_deref(),
            Some("Instance ID must be 64 characters or fewer.")
        );
        for bad in ["1abc", "a b", "-a"] {
            assert!(validate_instance_id(bad, &taken).is_some_and(|m| m.contains("start with")));
        }
        assert_eq!(
            validate_instance_id("codex_work", &taken).as_deref(),
            Some("An instance named 'codex_work' already exists.")
        );
        assert_eq!(validate_instance_id("cursor_two-b", &taken), None);
    }

    #[test]
    fn derived_ids_number_past_taken_ones() {
        let taken = ids(&["codex", "codex_work", "codex_work_2"]);
        assert_eq!(derive_instance_id("cursor", "Cursor", "", &taken), "cursor");
        assert_eq!(
            derive_instance_id("cursor", "Cursor", "Cursor", &taken),
            "cursor"
        );
        assert_eq!(derive_instance_id("codex", "Codex", "", &taken), "codex_2");
        assert_eq!(
            derive_instance_id("codex", "Codex", "Work", &taken),
            "codex_work_3"
        );
        assert_eq!(
            derive_instance_id("codex", "Codex", "My  Team's Key!", &taken),
            "codex_my_team_s_key"
        );
        assert_eq!(derive_instance_id("codex", "Codex", "***", &taken), "");
    }

    #[test]
    fn create_and_remove_payloads_match_the_contract() {
        let create = create_payload("codex_work", "codex", " Work ");
        let mutation = &create["providerInstanceMutation"];
        assert_eq!(mutation["operation"], "create");
        assert_eq!(mutation["instanceId"], "codex_work");
        assert_eq!(mutation["instance"]["displayName"], "Work");
        assert_eq!(mutation["instance"]["config"]["setupMode"], "existing");
        let plain = create_payload("cursor", "cursor", "  ");
        assert!(plain["providerInstanceMutation"]["instance"]["displayName"].is_null());
        assert!(plain["providerInstanceMutation"]["instance"]["config"].is_null());
        let remove = remove_payload("codex_work");
        assert_eq!(remove["providerInstanceMutation"]["operation"], "remove");
        let mut settings = json!({"providerInstances": {"codex_work": {"driver": "codex"}}});
        forget_instance(&mut settings, "codex_work");
        assert_eq!(settings["providerInstances"], json!({}));
    }

    #[test]
    fn runtime_fields_drop_empty_values() {
        let base = json!({"driver": "codex", "config": {"binaryPath": "codex"}});
        let set = with_runtime(&base, "homePath", " ~/.codex ");
        assert_eq!(runtime_value(&set, "homePath"), "~/.codex");
        assert_eq!(runtime_value(&set, "binaryPath"), "codex");
        let cleared = with_runtime(&set, "binaryPath", "  ");
        assert!(cleared["config"].get("binaryPath").is_none());
        assert_eq!(
            runtime_fields("claudeAgent")[1].label,
            "CLAUDE_CONFIG_DIR path"
        );
        assert!(runtime_fields("cursor").is_empty());
    }
}
