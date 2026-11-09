//! The Secrets tab's model (`SecretsSettings.tsx`, `SecretProviderDialog.logic.ts` and the three
//! `secret-provider-*` definitions): how a stored provider classifies, the one-line summary of a
//! row, the draft a dialog edits with its validation, and the settings patches a save sends.

use serde_json::{Map, Value, json};

/// Call tokens, above every other range.
pub const TOKEN_BASE: u64 = 1 << 50;

const DEFAULT_MOUNT: &str = "secret";

/// The attached environment's settings and whether it lets a client manage its providers
/// (`serverConfig.secretProviderSettings`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Config {
    pub settings: Value,
    pub manageable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    OnePassword,
    OpenBao,
    Aws,
}

impl Kind {
    pub const ALL: [Self; 3] = [Self::OnePassword, Self::OpenBao, Self::Aws];

    pub fn wire(self) -> &'static str {
        match self {
            Self::OnePassword => "one_password",
            Self::OpenBao => "openbao",
            Self::Aws => "aws_secrets_manager",
        }
    }

    pub fn from_wire(kind: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|known| known.wire() == kind)
    }

    /// `SECRET_PROVIDER_KIND_LABELS`.
    pub fn label(self) -> &'static str {
        match self {
            Self::OnePassword => "1Password",
            Self::OpenBao => "OpenBao",
            Self::Aws => "AWS Secrets Manager",
        }
    }

    /// `NAME_PLACEHOLDERS`.
    pub fn name_placeholder(self) -> &'static str {
        match self {
            Self::OnePassword => "Personal",
            Self::OpenBao => "Homelab",
            Self::Aws => "Production",
        }
    }
}

/// `OpenBaoAuth`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Auth {
    Token,
    Env,
    Cli,
}

impl Auth {
    pub const ALL: [Self; 3] = [Self::Token, Self::Env, Self::Cli];

    pub fn wire(self) -> &'static str {
        match self {
            Self::Token => "token",
            Self::Env => "env",
            Self::Cli => "cli",
        }
    }

    fn from_wire(auth: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|known| known.wire() == auth)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Token => "Token",
            Self::Env => "BAO_TOKEN on the server",
            Self::Cli => "bao CLI login",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Self::Token => "The token below, stored with the provider.",
            Self::Env => "The server process must have BAO_TOKEN set.",
            Self::Cli => {
                "Runs bao print token on the server, so bao must be installed and logged in there."
            }
        }
    }
}

/// `ClassifiedSecretProvider`: a stored provider the app can edit, or one it only lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Classified {
    OnePassword {
        account: String,
    },
    OpenBao {
        address: String,
        mount: String,
        namespace: Option<String>,
        auth: Auth,
        token: Option<String>,
    },
    Aws {
        region: String,
        profile: Option<String>,
    },
    Unsupported,
}

/// A configured provider as the settings hold it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Provider {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub classified: Classified,
}

impl Provider {
    pub fn supported(&self) -> bool {
        self.classified != Classified::Unsupported
    }

    /// `secretProviderDescription`: `None` for 1Password, whose row draws the account itself.
    pub fn summary(&self) -> Option<String> {
        describe(&self.classified)
    }
}

fn has_control(text: &str) -> bool {
    text.chars().any(|c| (c as u32) < 0x20 || c as u32 == 0x7f)
}

/// `OnePasswordAccount`.
fn account_issue(account: &str) -> Option<&'static str> {
    let word = |c: char| c.is_ascii_alphanumeric();
    let mut chars = account.chars();
    let valid = account.chars().count() <= 253
        && chars.next().is_some_and(word)
        && chars.all(|c| word(c) || matches!(c, '.' | '_' | '-'));
    (!valid)
        .then_some("1Password account must be a shorthand, sign-in address, or ID with no spaces.")
}

/// `isHttpUrl` of the OpenBao definition: `new URL` with an http or https scheme and a host.
fn is_http_url(address: &str) -> bool {
    let lower = address.to_ascii_lowercase();
    let Some(rest) = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))
    else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = authority.rsplit('@').next().unwrap_or_default();
    !host.is_empty() && !host.starts_with(':') && !address.chars().any(char::is_whitespace)
}

fn address_issue(address: &str) -> Option<&'static str> {
    let valid = address.chars().count() <= 1024 && is_http_url(address);
    (!valid).then_some("OpenBao address must be an http or https URL.")
}

/// `OpenBaoMount`: `^[A-Za-z0-9][A-Za-z0-9._/-]*$`, at most 253 characters.
fn mount_issue(mount: &str) -> Option<&'static str> {
    let mut chars = mount.chars();
    let valid = mount.chars().count() <= 253
        && chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '/' | '-'));
    (!valid).then_some("OpenBao mount must start with a letter or digit and contain no spaces.")
}

/// `OpenBaoNamespace`: non-empty, at most 253 characters.
fn namespace_issue(namespace: &str) -> Option<&'static str> {
    (namespace.chars().count() > 253).then_some("Enter at most 253 characters.")
}

/// `AwsRegion`: `^[a-z]{2}(-[a-z]+)+-\d+$`.
fn region_issue(region: &str) -> Option<&'static str> {
    let parts: Vec<&str> = region.split('-').collect();
    let lower = |part: &str| !part.is_empty() && part.chars().all(|c| c.is_ascii_lowercase());
    let valid = parts.len() >= 3
        && parts[0].len() == 2
        && lower(parts[0])
        && parts[1..parts.len() - 1].iter().all(|part| lower(part))
        && parts
            .last()
            .is_some_and(|last| !last.is_empty() && last.chars().all(|c| c.is_ascii_digit()));
    (!valid).then_some("AWS region must look like us-east-1.")
}

/// `AwsProfile`: at most 64 characters, no leading `-`, whitespace or control characters.
fn profile_issue(profile: &str) -> Option<&'static str> {
    let valid = profile.chars().count() <= 64
        && !profile.starts_with('-')
        && !profile.chars().any(char::is_whitespace)
        && !has_control(profile);
    (!valid).then_some("AWS profile must not start with - or contain whitespace.")
}

fn text<'a>(config: &'a Value, key: &str) -> Option<&'a str> {
    config.get(key).and_then(Value::as_str)
}

/// `classifySecretProvider`: a config that no longer decodes, or an unknown kind, is unsupported.
pub fn classify(kind: &str, config: Option<&Value>) -> Classified {
    let empty = Value::Null;
    let config = config.unwrap_or(&empty);
    let trimmed = |value: &str| value == value.trim();
    match Kind::from_wire(kind) {
        Some(Kind::OnePassword) => match text(config, "account") {
            Some(account) if account_issue(account).is_none() => Classified::OnePassword {
                account: account.to_owned(),
            },
            _ => Classified::Unsupported,
        },
        Some(Kind::OpenBao) => {
            let Some(address) = text(config, "address")
                .filter(|a| !a.is_empty() && trimmed(a) && address_issue(a).is_none())
            else {
                return Classified::Unsupported;
            };
            let mount = match config.get("mount") {
                None => DEFAULT_MOUNT,
                Some(Value::String(mount)) if mount_issue(mount).is_none() => mount,
                Some(_) => return Classified::Unsupported,
            };
            let namespace = match config.get("namespace") {
                None => None,
                Some(Value::String(ns)) if !ns.is_empty() && trimmed(ns) => Some(ns.clone()),
                Some(_) => return Classified::Unsupported,
            };
            let auth = match config.get("auth") {
                None => Some(Auth::Token),
                Some(Value::String(auth)) => Auth::from_wire(auth),
                Some(_) => None,
            };
            let token = match config.get("token") {
                None => Some(None),
                Some(Value::String(token)) => Some(Some(token.clone())),
                Some(_) => None,
            };
            match (auth, token) {
                (Some(auth), Some(token)) => Classified::OpenBao {
                    address: address.to_owned(),
                    mount: mount.to_owned(),
                    namespace,
                    auth,
                    token,
                },
                _ => Classified::Unsupported,
            }
        }
        Some(Kind::Aws) => {
            let Some(region) =
                text(config, "region").filter(|r| trimmed(r) && region_issue(r).is_none())
            else {
                return Classified::Unsupported;
            };
            let profile = match config.get("profile") {
                None => None,
                Some(Value::String(p))
                    if !p.is_empty() && trimmed(p) && profile_issue(p).is_none() =>
                {
                    Some(p.clone())
                }
                Some(_) => return Classified::Unsupported,
            };
            Classified::Aws {
                region: region.to_owned(),
                profile,
            }
        }
        None => Classified::Unsupported,
    }
}

/// The providers of `settings.secretProviders`, in the order the settings hold them.
pub fn providers(settings: &Value) -> Vec<Provider> {
    let Some(entries) = settings.get("secretProviders").and_then(Value::as_object) else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|(id, entry)| {
            let kind = entry.get("kind")?.as_str()?;
            let label = entry.get("label")?.as_str()?;
            Some(Provider {
                id: id.clone(),
                kind: kind.to_owned(),
                label: label.to_owned(),
                classified: classify(kind, entry.get("config")),
            })
        })
        .collect()
}

/// `secretProviderDescription`.
pub fn describe(classified: &Classified) -> Option<String> {
    match classified {
        Classified::OnePassword { .. } | Classified::Unsupported => None,
        Classified::OpenBao {
            address,
            mount,
            namespace,
            auth,
            ..
        } => {
            let mut parts = vec![
                format!("OpenBao \u{b7} {address}"),
                format!("mount {mount}"),
            ];
            parts.extend(namespace.iter().map(|ns| format!("namespace {ns}")));
            match auth {
                Auth::Env => parts.push("BAO_TOKEN".to_owned()),
                Auth::Cli => parts.push("bao CLI login".to_owned()),
                Auth::Token => {}
            }
            Some(parts.join(" \u{b7} "))
        }
        Classified::Aws { region, profile } => {
            let mut parts = vec![format!("AWS Secrets Manager \u{b7} {region}")];
            parts.extend(profile.iter().map(|p| format!("profile {p}")));
            Some(parts.join(" \u{b7} "))
        }
    }
}

/// `secretProviderIdFromName`: stable and readable in settings.json.
pub fn id_from_name(name: &str) -> Option<String> {
    let mut slug = String::new();
    for c in name.to_lowercase().chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            slug.push(c);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let trimmed = slug.trim_matches('-');
    let mut id: String = trimmed.chars().take(64).collect();
    while id.ends_with('-') {
        id.pop();
    }
    let mut chars = id.chars();
    let valid = chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'));
    valid.then_some(id)
}

/// `SecretProviderDraft`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Draft {
    pub kind: Kind,
    pub name: String,
    pub account: String,
    pub address: String,
    pub mount: String,
    pub namespace: String,
    pub auth: Auth,
    pub token: String,
    /// The token already stored with the provider; kept when none is typed.
    pub stored_token: String,
    pub region: String,
    pub profile: String,
}

impl Draft {
    pub fn empty() -> Self {
        Self {
            kind: Kind::OnePassword,
            name: String::new(),
            account: String::new(),
            address: String::new(),
            mount: DEFAULT_MOUNT.to_owned(),
            namespace: String::new(),
            auth: Auth::Token,
            token: String::new(),
            stored_token: String::new(),
            region: String::new(),
            profile: String::new(),
        }
    }

    /// `draftFromSecretProvider`.
    pub fn from_provider(provider: &Provider) -> Self {
        let base = Self {
            name: provider.label.clone(),
            ..Self::empty()
        };
        match &provider.classified {
            Classified::OnePassword { account } => Self {
                account: account.clone(),
                ..base
            },
            Classified::OpenBao {
                address,
                mount,
                namespace,
                auth,
                token,
            } => Self {
                kind: Kind::OpenBao,
                address: address.clone(),
                mount: mount.clone(),
                namespace: namespace.clone().unwrap_or_default(),
                auth: *auth,
                stored_token: token.clone().unwrap_or_default(),
                ..base
            },
            Classified::Aws { region, profile } => Self {
                kind: Kind::Aws,
                region: region.clone(),
                profile: profile.clone().unwrap_or_default(),
                ..base
            },
            Classified::Unsupported => base,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Add,
    Edit(String),
}

/// `SecretProviderDraftIssues`: one message per field that blocks the save.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Issues {
    pub name: Option<String>,
    pub account: Option<String>,
    pub address: Option<String>,
    pub mount: Option<String>,
    pub namespace: Option<String>,
    pub token: Option<String>,
    pub region: Option<String>,
    pub profile: Option<String>,
}

impl Issues {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// `decodeField`: trimmed, empty is `empty`, anything else is checked by `check`.
fn required(
    value: &str,
    empty: &'static str,
    check: impl Fn(&str) -> Option<&'static str>,
) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(empty.to_owned());
    }
    check(trimmed).map_or_else(|| Ok(trimmed.to_owned()), |issue| Err(issue.to_owned()))
}

/// `optionalField`: an empty value is absent.
fn optional(
    value: &str,
    check: impl Fn(&str) -> Option<&'static str>,
) -> Result<Option<String>, String> {
    if value.trim().is_empty() {
        return Ok(None);
    }
    required(value, "", check).map(Some)
}

fn keep<T>(slot: &mut Option<String>, result: &Result<T, String>) {
    if let Err(message) = result {
        *slot = Some(message.clone());
    }
}

/// The id and stored entry a draft adds or replaces: `secretProviderFromDraft`.
pub fn from_draft(
    draft: &Draft,
    existing_ids: &[String],
    target: &Target,
) -> Result<(String, Value), Box<Issues>> {
    let mut issues = Issues::default();
    let label = required(&draft.name, "Enter a name.", |name| {
        (name.chars().count() > 64).then_some("Enter at most 64 characters.")
    });
    let mut id = None;
    match &label {
        Err(message) => issues.name = Some(message.clone()),
        Ok(label) => {
            id = match target {
                Target::Edit(id) => Some(id.clone()),
                Target::Add => id_from_name(label),
            };
            match (&id, target) {
                (None, _) => {
                    issues.name = Some("The name needs at least one letter or digit.".to_owned());
                }
                (Some(id), Target::Add) if existing_ids.contains(id) => {
                    issues.name = Some("A secret provider with this name exists.".to_owned());
                }
                _ => {}
            }
        }
    }
    let config = match draft.kind {
        Kind::OnePassword => {
            let account = required(&draft.account, "Enter the 1Password account.", |a| {
                account_issue(a)
            });
            keep(&mut issues.account, &account);
            account.map(|account| json!({"account": account}))
        }
        Kind::OpenBao => {
            let address = required(&draft.address, "Enter the OpenBao address.", |a| {
                address_issue(a)
            });
            let mount_text = draft.mount.trim();
            let mount = required(
                if mount_text.is_empty() {
                    DEFAULT_MOUNT
                } else {
                    mount_text
                },
                "Enter the mount.",
                mount_issue,
            );
            let namespace = optional(&draft.namespace, namespace_issue);
            let token = match draft.token.trim() {
                "" => draft.stored_token.clone(),
                typed => typed.to_owned(),
            };
            keep(&mut issues.address, &address);
            keep(&mut issues.mount, &mount);
            keep(&mut issues.namespace, &namespace);
            if draft.auth == Auth::Token && token.is_empty() {
                issues.token = Some("Enter the OpenBao token.".to_owned());
            }
            match (address, mount, namespace) {
                (Ok(address), Ok(mount), Ok(namespace)) => {
                    let mut config = Map::new();
                    config.insert("address".into(), address.into());
                    config.insert("mount".into(), mount.into());
                    if let Some(namespace) = namespace {
                        config.insert("namespace".into(), namespace.into());
                    }
                    config.insert("auth".into(), draft.auth.wire().into());
                    if draft.auth == Auth::Token {
                        config.insert("token".into(), token.into());
                    }
                    Ok(Value::Object(config))
                }
                _ => Err(String::new()),
            }
        }
        Kind::Aws => {
            let region = required(&draft.region, "Enter the AWS region.", region_issue);
            let profile = optional(&draft.profile, profile_issue);
            keep(&mut issues.region, &region);
            keep(&mut issues.profile, &profile);
            match (region, profile) {
                (Ok(region), Ok(profile)) => {
                    let mut config = Map::new();
                    config.insert("region".into(), region.into());
                    if let Some(profile) = profile {
                        config.insert("profile".into(), profile.into());
                    }
                    Ok(Value::Object(config))
                }
                _ => Err(String::new()),
            }
        }
    };
    match (id, label, config) {
        (Some(id), Ok(label), Ok(config)) if issues.is_empty() => Ok((
            id,
            json!({"kind": draft.kind.wire(), "label": label, "config": config}),
        )),
        _ => Err(Box::new(issues)),
    }
}

/// The settings patch that adds or replaces a provider.
pub fn save_patch(id: &str, entry: Value) -> Value {
    json!({"secretProviders": {id: entry}})
}

/// Shows a patch in the settings at once, as the server will store it.
pub fn apply_patch(settings: &mut Value, patch: &Value) {
    let Some(changes) = patch.get("secretProviders").and_then(Value::as_object) else {
        return;
    };
    if !settings.is_object() {
        *settings = json!({});
    }
    let Some(root) = settings.as_object_mut() else {
        return;
    };
    let stored = root.entry("secretProviders").or_insert_with(|| json!({}));
    if !stored.is_object() {
        *stored = json!({});
    }
    if let Some(stored) = stored.as_object_mut() {
        for (id, entry) in changes {
            if entry.is_null() {
                stored.remove(id);
            } else {
                stored.insert(id.clone(), entry.clone());
            }
        }
    }
}

/// The settings patch that removes a provider.
pub fn remove_patch(id: &str) -> Value {
    json!({"secretProviders": {id: Value::Null}})
}

/// The `server.listOnePasswordAccounts` answer: the accounts the `op` CLI is signed into.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    pub account: String,
    pub email: String,
}

pub fn decode_accounts(value: &Value) -> Vec<Account> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            Some(Account {
                account: entry.get("account")?.as_str()?.to_owned(),
                email: entry.get("email")?.as_str()?.to_owned(),
            })
        })
        .collect()
}

fn references(value: &Value, provider: &str, path: &str, found: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            if map.get("provider").and_then(Value::as_str) == Some(provider)
                && map.get("reference").is_some_and(Value::is_string)
            {
                found.push(path.to_owned());
                return;
            }
            for (key, inner) in map {
                references(inner, provider, key, found);
            }
        }
        Value::Array(items) => {
            for item in items {
                references(item, provider, path, found);
            }
        }
        _ => {}
    }
}

/// `secretProviderUses`: what reads from provider `id`, as "owner \u{b7} field". The web names a field
/// by its form label; the stored key stands in for it here.
pub fn uses(settings: &Value, id: &str) -> Vec<String> {
    let mut uses = Vec::new();
    let owner = |name: &str, value: &Value, uses: &mut Vec<String>| {
        let mut found = Vec::new();
        references(value, id, "", &mut found);
        uses.extend(found.into_iter().map(|field| {
            if field.is_empty() {
                name.to_owned()
            } else {
                format!("{name} \u{b7} {field}")
            }
        }));
    };
    if let Some(instances) = settings.get("providerInstances").and_then(Value::as_object) {
        for (instance, config) in instances {
            let name = config
                .get("displayName")
                .and_then(Value::as_str)
                .or_else(|| config.get("driver").and_then(Value::as_str))
                .unwrap_or(instance);
            if let Some(inner) = config.get("config") {
                owner(name, inner, &mut uses);
            }
            for variable in config
                .get("environment")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let key = variable.get("name").and_then(Value::as_str).unwrap_or("");
                if let Some(value) = variable.get("value") {
                    let mut found = Vec::new();
                    references(value, id, "", &mut found);
                    uses.extend(found.into_iter().map(|_| format!("{name} \u{b7} {key}")));
                }
            }
        }
    }
    if let Some(hosts) = settings
        .get("sourceControlHosts")
        .and_then(Value::as_object)
    {
        for (kind, host) in hosts {
            owner(kind, host, &mut uses);
        }
    }
    if let Some(sources) = settings.get("usageLimitSources").and_then(Value::as_object) {
        for (source_id, source) in sources {
            let name = source
                .get("label")
                .and_then(Value::as_str)
                .or_else(|| source.get("url").and_then(Value::as_str))
                .unwrap_or(source_id);
            owner(name, source, &mut uses);
        }
    }
    for host in settings
        .get("easDeviceHosts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let name = host.get("label").and_then(Value::as_str).unwrap_or("EAS");
        owner(name, host, &mut uses);
    }
    if let Some(credentials) = settings
        .get("cloudSpeechCredentials")
        .and_then(Value::as_object)
    {
        for (provider, credential) in credentials {
            let mut found = Vec::new();
            references(credential, id, "", &mut found);
            uses.extend(
                found
                    .into_iter()
                    .map(|_| format!("Voice \u{b7} {provider}")),
            );
        }
    }
    uses
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(kind: Kind) -> Draft {
        Draft {
            kind,
            name: "Homelab".to_owned(),
            ..Draft::empty()
        }
    }

    fn built(draft: &Draft) -> (String, Value) {
        from_draft(draft, &[], &Target::Add).expect("a valid draft")
    }

    #[test]
    fn ids_follow_the_name() {
        assert_eq!(id_from_name("My Vault!").as_deref(), Some("my-vault"));
        assert_eq!(id_from_name("  --A--b__c ").as_deref(), Some("a-b-c"));
        assert_eq!(id_from_name("!!!"), None);
        assert_eq!(id_from_name(&"a".repeat(80)).map(|id| id.len()), Some(64));
    }

    #[test]
    fn one_password_encodes_and_decodes() {
        let mut one = draft(Kind::OnePassword);
        one.account = " my.1password.com ".to_owned();
        let (id, entry) = built(&one);
        assert_eq!(id, "homelab");
        assert_eq!(
            entry,
            json!({"kind": "one_password", "label": "Homelab", "config": {"account": "my.1password.com"}})
        );
        let stored = json!({"secretProviders": {id: entry}});
        let listed = providers(&stored);
        assert_eq!(
            listed[0].classified,
            Classified::OnePassword {
                account: "my.1password.com".to_owned()
            }
        );
        assert_eq!(Draft::from_provider(&listed[0]).account, "my.1password.com");
    }

    #[test]
    fn open_bao_encodes_per_auth() {
        let mut bao = draft(Kind::OpenBao);
        bao.address = "https://bao.example:8200".to_owned();
        bao.namespace = " admin/platform ".to_owned();
        bao.mount = String::new();
        bao.token = "s.token".to_owned();
        let (_, entry) = built(&bao);
        assert_eq!(
            entry["config"],
            json!({"address": "https://bao.example:8200", "mount": "secret",
                   "namespace": "admin/platform", "auth": "token", "token": "s.token"})
        );
        bao.auth = Auth::Env;
        let (_, entry) = built(&bao);
        assert_eq!(
            entry["config"],
            json!({"address": "https://bao.example:8200", "mount": "secret",
                   "namespace": "admin/platform", "auth": "env"})
        );
    }

    #[test]
    fn open_bao_keeps_the_stored_token_unless_one_is_typed() {
        let stored = json!({"secretProviders": {"vault": {"kind": "openbao", "label": "Vault",
            "config": {"address": "http://bao.example", "token": "stored"}}}});
        let provider = &providers(&stored)[0];
        let mut edit = Draft::from_provider(provider);
        assert_eq!(edit.mount, "secret");
        let target = Target::Edit("vault".to_owned());
        let (_, entry) = from_draft(&edit, &[], &target).unwrap();
        assert_eq!(entry["config"]["token"], "stored");
        edit.token = "fresh".to_owned();
        let (_, entry) = from_draft(&edit, &[], &target).unwrap();
        assert_eq!(entry["config"]["token"], "fresh");
    }

    #[test]
    fn aws_encodes_and_summarizes() {
        let mut aws = draft(Kind::Aws);
        aws.region = "us-east-1".to_owned();
        aws.profile = "production".to_owned();
        let (id, entry) = built(&aws);
        let listed = providers(&json!({"secretProviders": {id: entry}}));
        assert_eq!(
            listed[0].summary().as_deref(),
            Some("AWS Secrets Manager \u{b7} us-east-1 \u{b7} profile production")
        );
    }

    #[test]
    fn summaries_follow_the_web() {
        let bao = Classified::OpenBao {
            address: "https://bao.example:8200".to_owned(),
            mount: "secret".to_owned(),
            namespace: Some("admin".to_owned()),
            auth: Auth::Cli,
            token: None,
        };
        assert_eq!(
            describe(&bao).as_deref(),
            Some(
                "OpenBao \u{b7} https://bao.example:8200 \u{b7} mount secret \u{b7} namespace admin \u{b7} bao CLI login"
            )
        );
        assert_eq!(describe(&Classified::Unsupported), None);
    }

    #[test]
    fn unknown_kinds_and_broken_configs_are_unsupported() {
        let stored = json!({"secretProviders": {
            "a": {"kind": "vault9", "label": "A"},
            "b": {"kind": "aws_secrets_manager", "label": "B", "config": {"region": "nowhere"}},
            "c": {"kind": "one_password", "label": "C"}}});
        assert!(providers(&stored).iter().all(|p| !p.supported()));
    }

    #[test]
    fn each_kind_reports_its_own_issues() {
        let none: [String; 0] = [];
        let mut one = draft(Kind::OnePassword);
        one.name = "  ".to_owned();
        let issues = from_draft(&one, &none, &Target::Add).unwrap_err();
        assert_eq!(issues.name.as_deref(), Some("Enter a name."));
        assert_eq!(
            issues.account.as_deref(),
            Some("Enter the 1Password account.")
        );
        one.account = "has space".to_owned();
        assert_eq!(
            from_draft(&one, &none, &Target::Add)
                .unwrap_err()
                .account
                .as_deref(),
            Some("1Password account must be a shorthand, sign-in address, or ID with no spaces.")
        );

        let mut bao = draft(Kind::OpenBao);
        bao.address = "ftp://bao".to_owned();
        bao.mount = "-bad".to_owned();
        let issues = from_draft(&bao, &none, &Target::Add).unwrap_err();
        assert_eq!(
            issues.address.as_deref(),
            Some("OpenBao address must be an http or https URL.")
        );
        assert_eq!(
            issues.mount.as_deref(),
            Some("OpenBao mount must start with a letter or digit and contain no spaces.")
        );
        assert_eq!(issues.token.as_deref(), Some("Enter the OpenBao token."));
        bao.auth = Auth::Cli;
        assert_eq!(
            from_draft(&bao, &none, &Target::Add).unwrap_err().token,
            None
        );

        let mut aws = draft(Kind::Aws);
        aws.region = "us_east_1".to_owned();
        aws.profile = "-p".to_owned();
        let issues = from_draft(&aws, &none, &Target::Add).unwrap_err();
        assert_eq!(
            issues.region.as_deref(),
            Some("AWS region must look like us-east-1.")
        );
        assert_eq!(
            issues.profile.as_deref(),
            Some("AWS profile must not start with - or contain whitespace.")
        );
        aws.region = "eu-central-1".to_owned();
        aws.profile = String::new();
        assert!(from_draft(&aws, &none, &Target::Add).is_ok());
    }

    #[test]
    fn names_must_be_new_on_add_and_keep_their_id_on_edit() {
        let mut one = draft(Kind::OnePassword);
        one.account = "my.1password.com".to_owned();
        let taken = ["homelab".to_owned()];
        assert_eq!(
            from_draft(&one, &taken, &Target::Add)
                .unwrap_err()
                .name
                .as_deref(),
            Some("A secret provider with this name exists.")
        );
        let (id, _) = from_draft(&one, &taken, &Target::Edit("homelab".to_owned())).unwrap();
        assert_eq!(id, "homelab");
        one.name = "!!".to_owned();
        assert_eq!(
            from_draft(&one, &[], &Target::Add)
                .unwrap_err()
                .name
                .as_deref(),
            Some("The name needs at least one letter or digit.")
        );
    }

    #[test]
    fn region_and_address_shapes() {
        assert!(region_issue("us-east-1").is_none());
        assert!(region_issue("us-gov-west-1").is_none());
        assert!(region_issue("us-east").is_some());
        assert!(region_issue("USA-east-1").is_some());
        assert!(is_http_url("http://localhost:8200"));
        assert!(!is_http_url("https://"));
        assert!(!is_http_url("bao.example"));
    }

    #[test]
    fn patches_add_and_remove() {
        let entry =
            json!({"kind": "aws_secrets_manager", "label": "X", "config": {"region": "us-east-1"}});
        assert_eq!(
            save_patch("x", entry.clone()),
            json!({"secretProviders": {"x": entry}})
        );
        assert_eq!(remove_patch("x"), json!({"secretProviders": {"x": null}}));
    }

    #[test]
    fn accounts_decode() {
        let value = json!([{"account": "my.1password.com", "email": "a@example.com"}, {"x": 1}]);
        assert_eq!(
            decode_accounts(&value),
            vec![Account {
                account: "my.1password.com".to_owned(),
                email: "a@example.com".to_owned()
            }]
        );
    }

    #[test]
    fn a_patch_shows_at_once() {
        let mut settings = json!({"secretProviders": {"a": {"label": "A"}}});
        apply_patch(&mut settings, &save_patch("b", json!({"label": "B"})));
        apply_patch(&mut settings, &remove_patch("a"));
        assert_eq!(settings, json!({"secretProviders": {"b": {"label": "B"}}}));
    }

    #[test]
    fn uses_name_the_readers() {
        let reference = json!({"provider": "vault", "reference": "path#field"});
        let settings = json!({
            "providerInstances": {"codex": {"driver": "codex", "displayName": "Codex",
                "environment": [{"name": "API_KEY", "value": reference}]}},
            "sourceControlHosts": {"github": {"token": reference}},
            "easDeviceHosts": [{"id": "e", "label": "Builder", "token": reference}],
            "cloudSpeechCredentials": {"openai": reference},
        });
        assert_eq!(
            uses(&settings, "vault"),
            vec![
                "Codex \u{b7} API_KEY",
                "github \u{b7} token",
                "Builder \u{b7} token",
                "Voice \u{b7} openai"
            ]
        );
        assert!(uses(&settings, "other").is_empty());
    }
}
