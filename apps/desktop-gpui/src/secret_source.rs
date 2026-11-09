//! The secret sources a setting can read from (`SecretSourceInput.tsx`): the server's configured
//! secret providers and the reference syntax of each kind.

use serde_json::{Map, Value, json};

/// `SECRET_REDACTED` (settingsSecrets.ts): six bullets stand in for a stored literal.
pub const REDACTED: &str = "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}";

/// One configured secret provider (`useSecretSources`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecretSource {
    pub id: String,
    pub kind: String,
    pub label: String,
}

/// The providers a setting can point at: none unless the environment reads references. A kind the
/// app does not know is dropped, as the web drops a config that does not classify.
pub fn sources(settings: &Value, references: bool) -> Vec<SecretSource> {
    if !references {
        return Vec::new();
    }
    let Some(providers) = settings.get("secretProviders").and_then(Value::as_object) else {
        return Vec::new();
    };
    providers
        .iter()
        .filter_map(|(id, config)| {
            let kind = config.get("kind")?.as_str()?;
            let label = config.get("label")?.as_str()?;
            known(kind).then(|| SecretSource {
                id: id.clone(),
                kind: kind.to_owned(),
                label: label.to_owned(),
            })
        })
        .collect()
}

fn known(kind: &str) -> bool {
    matches!(kind, "one_password" | "openbao" | "aws_secrets_manager")
}

/// `SettingsSecretValue` as stored: a reference is an object, anything else a literal.
pub fn reference_of(value: &Value) -> Option<(String, String)> {
    let object = value.as_object()?;
    Some((
        object.get("provider")?.as_str()?.to_owned(),
        object.get("reference")?.as_str()?.to_owned(),
    ))
}

pub fn reference_value(provider: &str, reference: &str) -> Value {
    json!({"provider": provider, "reference": reference})
}

/// Whether a stored value holds a secret (`hasToken`).
pub fn is_set(value: &Value) -> bool {
    match value {
        Value::String(text) => !text.is_empty(),
        Value::Object(_) => true,
        _ => false,
    }
}

/// `referencePlaceholder` of the provider kind.
pub fn placeholder(kind: &str) -> &'static str {
    match kind {
        "one_password" => "op://vault/item/field",
        "openbao" => "path/to/secret#field",
        "aws_secrets_manager" => "prod/openai#api_key",
        _ => "",
    }
}

/// `invalidReferenceMessage` of the provider kind.
pub fn invalid_message(kind: &str) -> &'static str {
    match kind {
        "one_password" => "Use a 1Password reference like op://vault/item/field.",
        "openbao" => "Use an OpenBao reference like path/to/secret#field.",
        "aws_secrets_manager" => {
            "Use an AWS Secrets Manager reference like name-or-arn or name-or-arn#key."
        }
        _ => "Enter a valid secret reference.",
    }
}

const MAX_REFERENCE: usize = 1024;

fn plain(reference: &str) -> bool {
    reference.chars().count() <= MAX_REFERENCE
        && reference == reference.trim()
        && !reference
            .chars()
            .any(|c| (c as u32) < 0x20 || c as u32 == 0x7f)
}

/// `isReference` of the provider kind: syntax the kind can read. An unknown kind accepts any
/// non-empty reference, as the web does when the definition is missing.
pub fn is_reference(kind: &str, reference: &str) -> bool {
    match kind {
        "one_password" => one_password(reference),
        "openbao" => plain(reference) && openbao(reference),
        "aws_secrets_manager" => plain(reference) && aws(reference),
        _ => !reference.is_empty(),
    }
}

/// `[A-Za-z0-9._-]` with spaces allowed inside, never at either end.
fn one_password_name(name: &str) -> bool {
    let word = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-');
    let mut chars = name.chars();
    let (Some(first), Some(last)) = (chars.next(), name.chars().next_back()) else {
        return false;
    };
    word(first) && word(last) && name.chars().all(|c| word(c) || c == ' ')
}

/// `^op://NAME(/NAME){2,3}(\?[a-z-]+=[A-Za-z0-9_-]+)?$`, at most 1024 characters.
fn one_password(reference: &str) -> bool {
    if reference.chars().count() > MAX_REFERENCE {
        return false;
    }
    let Some(rest) = reference.strip_prefix("op://") else {
        return false;
    };
    let (path, query) = match rest.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (rest, None),
    };
    let names: Vec<&str> = path.split('/').collect();
    if !(3..=4).contains(&names.len()) || !names.iter().all(|name| one_password_name(name)) {
        return false;
    }
    query.is_none_or(|query| {
        query.split_once('=').is_some_and(|(key, value)| {
            !key.is_empty()
                && key.chars().all(|c| c.is_ascii_lowercase() || c == '-')
                && !value.is_empty()
                && value
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
        })
    })
}

/// `path/to/secret#field`; the path never starts with `-`.
fn openbao(reference: &str) -> bool {
    reference.split_once('#').is_some_and(|(path, field)| {
        !path.is_empty() && !field.is_empty() && !path.starts_with('-')
    })
}

/// `name-or-arn` or `name-or-arn#key`.
fn aws(reference: &str) -> bool {
    let (id, key) = match reference.split_once('#') {
        Some((id, key)) => (id, Some(key)),
        None => (reference, None),
    };
    !id.is_empty() && !id.starts_with('-') && key != Some("")
}

/// A `Value` with the six-bullet marker, for a literal the server will store.
pub fn stored_literal() -> Value {
    Value::String(REDACTED.to_owned())
}

/// The `server.checkSecretSource` payload.
pub fn check_payload(provider: &str, reference: &str) -> Value {
    let mut source = Map::new();
    source.insert("provider".into(), json!(provider));
    source.insert("reference".into(), json!(reference));
    json!({"source": source})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_password_references() {
        for ok in [
            "op://vault/item/field",
            "op://My Vault/item/section/field",
            "op://vault/item/field?attribute=otp",
        ] {
            assert!(is_reference("one_password", ok), "{ok}");
        }
        for bad in [
            "op://vault/item",
            "op://vault/item/a/b/c",
            "op:// vault/item/field",
            "op://vault/item /field",
            "vault/item/field",
            "op://vault/item/field?Attr=x",
            "op://vault/item/field?a=",
        ] {
            assert!(!is_reference("one_password", bad), "{bad}");
        }
    }

    #[test]
    fn openbao_and_aws_references() {
        assert!(is_reference("openbao", "path/to/secret#field"));
        assert!(!is_reference("openbao", "path/to/secret"));
        assert!(!is_reference("openbao", "-path#field"));
        assert!(!is_reference("openbao", " path#field"));
        assert!(is_reference("aws_secrets_manager", "prod/openai"));
        assert!(is_reference("aws_secrets_manager", "prod/openai#api_key"));
        assert!(!is_reference("aws_secrets_manager", "prod/openai#"));
        assert!(!is_reference("aws_secrets_manager", "-prod"));
    }

    #[test]
    fn only_known_providers_are_offered_and_only_when_references_are_read() {
        let settings = json!({"secretProviders": {
            "work": {"kind": "one_password", "label": "Work vault"},
            "odd": {"kind": "something_new", "label": "Odd"},
        }});
        assert!(sources(&settings, false).is_empty());
        let listed = sources(&settings, true);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].label, "Work vault");
    }

    #[test]
    fn stored_values() {
        assert!(!is_set(&json!("")));
        assert!(is_set(&json!(REDACTED)));
        assert!(is_set(&reference_value("work", "op://a/b/c")));
        assert_eq!(
            reference_of(&reference_value("work", "op://a/b/c")),
            Some(("work".into(), "op://a/b/c".into()))
        );
    }
}
