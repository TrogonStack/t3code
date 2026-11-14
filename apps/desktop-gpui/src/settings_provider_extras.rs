//! The Providers tab's lower sections (`UsageProviderSettings.tsx`, `TextGeneratorsSettings.tsx`,
//! `BackgroundActivitySettings.tsx`, `SettingsPanels.logic.ts`): how the server settings read as
//! usage hubs, text generators, the provider health interval and the idle compaction mode, and the
//! `server.updateSettings` patches each edit sends.

use serde_json::{Map, Value, json};
use std::collections::HashSet;

pub const HEALTH_INTERVAL_STEP_SECONDS: i64 = 30;

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn flag(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(true)
}

fn entries(settings: &Value, key: &str) -> Vec<(String, Value)> {
    settings
        .get(key)
        .and_then(Value::as_object)
        .map(|map| map.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsageSource {
    pub id: String,
    pub label: String,
    pub url: String,
    pub enabled: bool,
}

impl UsageSource {
    /// `label.trim() || url`.
    pub fn title(&self) -> String {
        let label = self.label.trim();
        if label.is_empty() { &self.url } else { label }.to_owned()
    }

    /// `CLI Proxy`, then ` · Disabled`, then the URL unless the title already is it.
    pub fn description(&self) -> String {
        let mut out = String::from("CLI Proxy");
        if !self.enabled {
            out.push_str(" \u{b7} Disabled");
        }
        if self.title() != self.url {
            out.push_str(" \u{b7} ");
            out.push_str(&self.url);
        }
        out
    }
}

pub fn usage_sources(settings: &Value) -> Vec<UsageSource> {
    entries(settings, "usageLimitSources")
        .into_iter()
        .map(|(id, source)| UsageSource {
            label: text(&source, "label").to_owned(),
            url: text(&source, "url").to_owned(),
            enabled: flag(&source, "enabled"),
            id,
        })
        .collect()
}

/// `new URL(url).host`: lowercased, without credentials or a default port.
fn host_of(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    if scheme.is_empty() || !scheme.chars().all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
    {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let authority = authority.rsplit('@').next().unwrap_or_default().to_lowercase();
    let default_port = match scheme.to_lowercase().as_str() {
        "http" | "ws" => ":80",
        "https" | "wss" => ":443",
        _ => "",
    };
    let host = match authority.strip_suffix(default_port) {
        Some(host) if !default_port.is_empty() => host.to_owned(),
        _ => authority,
    };
    (!host.is_empty()).then_some(host)
}

/// `sourceIdFromUrl`: the id of the hub already at `url`, else a free `cliproxy-<host>` id.
pub fn source_id_from_url(url: &str, sources: &[UsageSource]) -> String {
    if let Some(existing) = sources.iter().find(|source| source.url == url) {
        return existing.id.clone();
    }
    let host = host_of(url).unwrap_or_else(|| url.to_owned()).to_lowercase();
    let slug = collapse_runs(&host);
    let slug = slug.trim_matches('-');
    let base = format!("cliproxy-{}", if slug.is_empty() { "hub" } else { slug });
    let taken: HashSet<&str> = sources.iter().map(|source| source.id.as_str()).collect();
    let mut id = base.clone();
    let mut n = 2;
    while taken.contains(id.as_str()) {
        id = format!("{base}-{n}");
        n += 1;
    }
    id
}

/// `replace(/[^a-z0-9.-]+/g, "-")`.
fn collapse_runs(text: &str) -> String {
    let mut out = String::new();
    let mut in_run = false;
    for c in text.chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-' {
            out.push(c);
            in_run = false;
        } else if !in_run {
            out.push('-');
            in_run = true;
        }
    }
    out
}

pub fn add_usage_source_patch(
    id: &str,
    url: &str,
    label: &str,
    management_key: Value,
) -> Value {
    let mut source = Map::new();
    source.insert("kind".into(), json!("cliproxy"));
    let label = label.trim();
    if !label.is_empty() {
        source.insert("label".into(), json!(label));
    }
    source.insert("url".into(), json!(url));
    source.insert("managementKey".into(), management_key);
    source.insert("enabled".into(), json!(true));
    json!({"usageLimitSources": {id: source}})
}

pub fn remove_usage_source_patch(id: &str) -> Value {
    json!({"usageLimitSources": {id: null}})
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Preset {
    pub id: &'static str,
    pub label: &'static str,
    pub kind: &'static str,
    pub base_url: &'static str,
}

pub const PRESETS: [Preset; 4] = [
    Preset {
        id: "openai",
        label: "OpenAI",
        kind: "openaiCompatible",
        base_url: "https://api.openai.com/v1",
    },
    Preset {
        id: "openrouter",
        label: "OpenRouter",
        kind: "openaiCompatible",
        base_url: "https://openrouter.ai/api/v1",
    },
    Preset {
        id: "anthropic",
        label: "Anthropic",
        kind: "anthropic",
        base_url: "https://api.anthropic.com/v1",
    },
    Preset {
        id: "custom",
        label: "Custom (OpenAI-compatible)",
        kind: "openaiCompatible",
        base_url: "",
    },
];

#[derive(Clone, Debug, PartialEq)]
pub struct TextGenerator {
    pub id: String,
    pub config: Value,
}

impl TextGenerator {
    pub fn kind(&self) -> &str {
        text(&self.config, "kind")
    }
    pub fn base_url(&self) -> &str {
        text(&self.config, "baseUrl")
    }
    pub fn enabled(&self) -> bool {
        flag(&self.config, "enabled")
    }
    pub fn name(&self) -> String {
        display_name(&self.config)
    }
    pub fn brand(&self) -> Brand {
        brand(&self.config)
    }
    /// `Off · ` then the URL without its scheme.
    pub fn description(&self) -> String {
        let url = self.base_url();
        let url = url
            .strip_prefix("https://")
            .or_else(|| url.strip_prefix("http://"))
            .unwrap_or(url);
        format!("{}{url}", if self.enabled() { "" } else { "Off \u{b7} " })
    }
}

pub fn text_generators(settings: &Value) -> Vec<TextGenerator> {
    entries(settings, "textGenerators")
        .into_iter()
        .map(|(id, config)| TextGenerator { id, config })
        .collect()
}

pub fn display_name(config: &Value) -> String {
    let label = text(config, "label");
    if !label.is_empty() {
        return label.to_owned();
    }
    let (url, kind) = (text(config, "baseUrl"), text(config, "kind"));
    if let Some(preset) = PRESETS
        .iter()
        .find(|preset| preset.base_url == url && preset.kind == kind)
    {
        return preset.label.to_owned();
    }
    host_of(url).unwrap_or_else(|| url.to_owned())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Brand {
    Anthropic,
    OpenRouter,
    OpenAi,
    Ollama,
    OpenAiCompatible,
}

pub fn brand(config: &Value) -> Brand {
    if text(config, "kind") == "anthropic" {
        return Brand::Anthropic;
    }
    let url = text(config, "baseUrl");
    let host = host_of(url).unwrap_or_default();
    let (name, port) = match host.rsplit_once(':') {
        Some((name, port)) if port.chars().all(|c| c.is_ascii_digit()) => (name, port),
        _ => (host.as_str(), ""),
    };
    if name == "openrouter.ai" || name.ends_with(".openrouter.ai") {
        Brand::OpenRouter
    } else if name == "api.openai.com" {
        Brand::OpenAi
    } else if port == "11434" || name.contains("ollama") {
        Brand::Ollama
    } else {
        Brand::OpenAiCompatible
    }
}

/// `nextTextGeneratorId`: `base`, then `base-2`, `base-3`, past every id a selection could mean.
pub fn next_text_generator_id(
    base: &str,
    settings: &Value,
    instance_ids: &[String],
) -> String {
    let mut taken: HashSet<String> = crate::settings_providers::DRIVERS
        .iter()
        .map(|driver| (*driver).to_owned())
        .collect();
    for key in ["providerInstances", "textGenerators"] {
        taken.extend(entries(settings, key).into_iter().map(|(id, _)| id));
    }
    taken.extend(instance_ids.iter().cloned());
    let mut id = base.to_owned();
    let mut n = 2;
    while taken.contains(&id) {
        id = format!("{base}-{n}");
        n += 1;
    }
    id
}

pub fn text_generator_patch(id: &str, config: Value) -> Value {
    json!({"textGenerators": {id: config}})
}

pub fn remove_text_generator_patch(id: &str) -> Value {
    json!({"textGenerators": {id: null}})
}

pub fn with_enabled(config: &Value, enabled: bool) -> Value {
    let mut config = config.clone();
    if let Some(map) = config.as_object_mut() {
        map.insert("enabled".into(), json!(enabled));
    }
    config
}

/// The config a wizard saves: preset kind and URL (or the typed URL), the key and the label.
pub fn wizard_config(
    kind: &str,
    base_url: &str,
    api_key: Option<Value>,
    label: &str,
    enabled: bool,
) -> Value {
    let mut config = Map::new();
    config.insert("kind".into(), json!(kind));
    let label = label.trim();
    if !label.is_empty() {
        config.insert("label".into(), json!(label));
    }
    config.insert("baseUrl".into(), json!(base_url.trim()));
    if let Some(key) = api_key {
        config.insert("apiKey".into(), key);
    }
    config.insert("enabled".into(), json!(enabled));
    Value::Object(config)
}

pub fn list_models_input(id: &str, config: Option<&Value>) -> Value {
    match config {
        Some(config) => json!({"id": id, "config": config}),
        None => json!({"id": id}),
    }
}

pub fn refresh_models_input(id: &str) -> Value {
    json!({"id": id, "refresh": true})
}

pub fn test_model_input(id: &str, model: &str) -> Value {
    json!({"id": id, "test": true, "model": model})
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Model {
    pub id: String,
    pub name: String,
}

pub fn parse_models(result: &Value) -> Vec<Model> {
    result
        .get("models")
        .and_then(Value::as_array)
        .map(|models| {
            models
                .iter()
                .map(|model| {
                    let id = text(model, "id").to_owned();
                    let name = text(model, "name");
                    Model {
                        name: if name.is_empty() { id.clone() } else { name.to_owned() },
                        id,
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The three tasks a connected generator can take over, as `(setting key, title)`.
pub const TASKS: [(&str, &str); 3] = [
    (
        "textGenerationModelSelection",
        "Thread titles and other generated text",
    ),
    (
        "sourceControlWriterModelSelection",
        "Commit messages, pull requests, and branch names",
    ),
    (
        "speechPostProcessingModelSelection",
        "Voice transcript cleanup",
    ),
];

pub fn task_patch(key: &str, id: &str, model: &str) -> Value {
    json!({key: {"instanceId": id, "model": model}})
}

/// The `(instance, model)` a task uses: its own selection, else the text generation model's.
pub fn task_selection(settings: &Value, key: &str) -> Option<(String, String)> {
    [key, "textGenerationModelSelection"]
        .into_iter()
        .filter_map(|key| settings.get(key))
        .find(|selection| !selection.is_null())
        .map(|selection| {
            (
                text(selection, "instanceId").to_owned(),
                text(selection, "model").to_owned(),
            )
        })
}

/// A task that follows the generation model has no selection of its own.
pub fn task_follows(settings: &Value, key: &str) -> bool {
    settings.get(key).is_none_or(Value::is_null)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Preset3 {
    git: i64,
    health: i64,
    active: i64,
    idle: i64,
    locked: bool,
    host_low: bool,
    client_low: bool,
    battery: bool,
}

const MIN: i64 = 60_000;

fn preset(profile: &str) -> Preset3 {
    match profile {
        "performance" => Preset3 {
            git: 15_000,
            health: MIN,
            active: 30_000,
            idle: 2 * MIN,
            locked: true,
            host_low: false,
            client_low: false,
            battery: false,
        },
        "battery-saver" => Preset3 {
            git: 0,
            health: 15 * MIN,
            active: MIN,
            idle: 10 * MIN,
            locked: true,
            host_low: true,
            client_low: true,
            battery: true,
        },
        _ => Preset3 {
            git: 30_000,
            health: 5 * MIN,
            active: 30_000,
            idle: 5 * MIN,
            locked: true,
            host_low: true,
            client_low: true,
            battery: false,
        },
    }
}

fn known_profile(value: Option<&str>) -> &'static str {
    match value {
        Some("performance") => "performance",
        Some("battery-saver") => "battery-saver",
        _ => "balanced",
    }
}

/// `getBackgroundActivityBaseProfile` for a `backgroundActivity` value.
pub fn base_profile(activity: &Value) -> &'static str {
    let profile = activity.get("profile").and_then(Value::as_str);
    if profile == Some("custom") {
        known_profile(activity.get("baseProfile").and_then(Value::as_str))
    } else {
        known_profile(profile)
    }
}

const OVERRIDE_KEYS: [&str; 8] = [
    "automaticGitFetchInterval",
    "providerHealthRefreshInterval",
    "hostPowerMonitorActiveInterval",
    "hostPowerMonitorIdleInterval",
    "pauseWhenHostLocked",
    "pauseWhenHostLowPower",
    "pauseWhenClientLowPower",
    "pauseWhenOnBattery",
];

/// `resolveServerBackgroundActivitySettings` as `(base profile, the eight resolved fields)`.
pub fn resolve_background(settings: &Value) -> (&'static str, Map<String, Value>) {
    let empty = json!({});
    let activity = settings.get("backgroundActivity").unwrap_or(&empty);
    let is_default = known_profile(activity.get("profile").and_then(Value::as_str)) == "balanced"
        && activity.get("profile").is_none_or(|p| p == "balanced")
        && activity.get("baseProfile").is_none()
        && activity
            .get("overrides")
            .and_then(Value::as_object)
            .is_none_or(Map::is_empty);
    let legacy_profile = known_profile(settings.get("backgroundActivityProfile").and_then(Value::as_str));
    let git = settings
        .get("automaticGitFetchInterval")
        .and_then(Value::as_i64)
        .unwrap_or(30_000);
    let health = settings
        .get("providerHealthRefreshInterval")
        .and_then(Value::as_i64)
        .unwrap_or(5 * MIN);
    let legacy_base = preset(legacy_profile);
    let has_legacy = legacy_profile != "balanced" || git != 30_000 || health != 5 * MIN;
    let (base, overrides) = if is_default && has_legacy {
        let mut overrides = Map::new();
        if git != legacy_base.git {
            overrides.insert("automaticGitFetchInterval".into(), json!(git));
        }
        if health != legacy_base.health {
            overrides.insert("providerHealthRefreshInterval".into(), json!(health));
        }
        (legacy_profile, overrides)
    } else {
        let overrides = if activity.get("profile").and_then(Value::as_str) == Some("custom") {
            activity
                .get("overrides")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default()
        } else {
            Map::new()
        };
        (base_profile(activity), overrides)
    };
    let base_preset = preset(base);
    let fallback = [
        json!(base_preset.git),
        json!(base_preset.health),
        json!(base_preset.active),
        json!(base_preset.idle),
        json!(base_preset.locked),
        json!(base_preset.host_low),
        json!(base_preset.client_low),
        json!(base_preset.battery),
    ];
    let mut resolved = Map::new();
    for (key, default) in OVERRIDE_KEYS.iter().zip(fallback) {
        resolved.insert((*key).to_owned(), overrides.get(*key).cloned().unwrap_or(default));
    }
    (base, resolved)
}

pub fn health_seconds(settings: &Value) -> i64 {
    let (_, resolved) = resolve_background(settings);
    seconds_of(&resolved["providerHealthRefreshInterval"])
}

pub fn health_preset_seconds(settings: &Value) -> i64 {
    preset(resolve_background(settings).0).health / 1000
}

fn seconds_of(ms: &Value) -> i64 {
    (ms.as_f64().unwrap_or(0.) / 1000.).round() as i64
}

/// `normalizeIntervalSeconds`.
pub fn normalize_seconds(value: f64, minimum: i64) -> i64 {
    (value.round() as i64).max(minimum)
}

/// `backgroundActivityOverrideSettings`: a custom profile over the current base, every resolved
/// field kept as an override, and `patch` merged in (`None` drops that override).
pub fn override_patch(settings: &Value, patch: &[(&str, Option<Value>)]) -> Value {
    let (_, resolved) = resolve_background(settings);
    let mut overrides = resolved;
    for (key, value) in patch {
        match value {
            Some(value) => {
                overrides.insert((*key).to_owned(), value.clone());
            }
            None => {
                overrides.remove(*key);
            }
        }
    }
    let empty = json!({});
    let current = settings.get("backgroundActivity").unwrap_or(&empty);
    json!({"backgroundActivity": {
        "schemaVersion": 1,
        "profile": "custom",
        "baseProfile": base_profile(current),
        "overrides": overrides,
    }})
}

pub fn health_interval_patch(settings: &Value, seconds: i64) -> Value {
    override_patch(
        settings,
        &[("providerHealthRefreshInterval", Some(json!(seconds * 1000)))],
    )
}

pub fn health_reset_patch(settings: &Value) -> Value {
    override_patch(settings, &[("providerHealthRefreshInterval", None)])
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdleMode {
    OnReturn,
    BeforeCacheExpires,
    Off,
}

impl IdleMode {
    pub const ALL: [Self; 3] = [Self::OnReturn, Self::BeforeCacheExpires, Self::Off];

    pub fn wire(self) -> &'static str {
        match self {
            Self::OnReturn => "onReturn",
            Self::BeforeCacheExpires => "beforeCacheExpires",
            Self::Off => "off",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::OnReturn => "When you return",
            Self::BeforeCacheExpires => "Before the cache expires (beta)",
            Self::Off => "Never",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.wire() == value)
    }
}

/// `globalIdleCompaction`: the new field, else the old string or boolean.
pub fn global_idle(settings: &Value) -> IdleMode {
    if let Some(mode) = settings
        .get("idleCompaction")
        .and_then(Value::as_str)
        .and_then(IdleMode::parse)
    {
        return mode;
    }
    match settings.get("compactIdleThreads") {
        Some(Value::String(mode)) => IdleMode::parse(mode).unwrap_or(IdleMode::OnReturn),
        Some(Value::Bool(true)) => IdleMode::BeforeCacheExpires,
        _ => IdleMode::OnReturn,
    }
}

/// Whether the server accepts the three modes, so "Never" can be stored.
pub fn reads_idle_modes(settings: &Value) -> bool {
    settings.get("idleCompaction").is_some_and(|v| !v.is_null())
        || settings.get("compactIdleThreads").is_some_and(Value::is_string)
}

/// `idleCompactionPatch`: the key and shape this server stores the mode in.
pub fn idle_patch(settings: &Value, mode: IdleMode) -> Value {
    if settings.get("idleCompaction").is_some_and(|v| !v.is_null()) {
        json!({"idleCompaction": mode.wire()})
    } else if reads_idle_modes(settings) {
        json!({"compactIdleThreads": mode.wire()})
    } else {
        json!({"compactIdleThreads": mode == IdleMode::BeforeCacheExpires})
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IdleState {
    pub mode: IdleMode,
    pub mixed: bool,
    pub never_available: bool,
}

/// `scopedIdleCompaction` over the targets a write reaches.
pub fn idle_state(targets: &[&Value]) -> IdleState {
    let modes: Vec<IdleMode> = targets.iter().map(|target| global_idle(target)).collect();
    let mode = modes.first().copied().unwrap_or(IdleMode::OnReturn);
    IdleState {
        mode,
        mixed: modes.iter().any(|other| *other != mode),
        never_available: targets.iter().all(|target| reads_idle_modes(target)),
    }
}

/// The settings as a patch leaves them, to show an edit before the server answers: objects merge,
/// `null` deletes, `overrides` is replaced, and a literal key shows as the redaction marker.
pub fn apply_patch(settings: &mut Value, patch: &Value) {
    merge(settings, patch, None);
}

fn merge(target: &mut Value, patch: &Value, key: Option<&str>) {
    let Some(patch_map) = patch.as_object() else {
        *target = match (key, patch) {
            (Some("apiKey" | "managementKey"), Value::String(_)) => {
                json!(crate::secret_source::REDACTED)
            }
            _ => patch.clone(),
        };
        return;
    };
    if key == Some("overrides") || !target.is_object() {
        *target = json!({});
    }
    let Some(map) = target.as_object_mut() else {
        return;
    };
    for (name, value) in patch_map {
        if value.is_null() {
            map.remove(name);
        } else {
            merge(map.entry(name.clone()).or_insert(Value::Null), value, Some(name));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(id: &str, url: &str) -> UsageSource {
        UsageSource {
            id: id.into(),
            label: String::new(),
            url: url.into(),
            enabled: true,
        }
    }

    #[test]
    fn hub_ids_follow_the_host() {
        assert_eq!(
            source_id_from_url("https://Hub.Example.ts.net:8318/x", &[]),
            "cliproxy-hub.example.ts.net-8318"
        );
        assert_eq!(source_id_from_url("https://hub.test/", &[]), "cliproxy-hub.test");
        assert_eq!(source_id_from_url("not a url", &[]), "cliproxy-not-a-url");
        assert_eq!(source_id_from_url("", &[]), "cliproxy-hub");
    }

    #[test]
    fn hub_ids_reuse_and_dedupe() {
        let known = [source("mine", "https://a.test"), source("cliproxy-b.test", "https://x")];
        assert_eq!(source_id_from_url("https://a.test", &known), "mine");
        assert_eq!(
            source_id_from_url("https://b.test", &known),
            "cliproxy-b.test-2"
        );
    }

    #[test]
    fn hub_rows_read_like_the_web() {
        let mut hub = source("a", "https://a.test");
        assert_eq!(hub.title(), "https://a.test");
        assert_eq!(hub.description(), "CLI Proxy");
        hub.label = " Home ".into();
        hub.enabled = false;
        assert_eq!(hub.title(), "Home");
        assert_eq!(hub.description(), "CLI Proxy \u{b7} Disabled \u{b7} https://a.test");
    }

    #[test]
    fn hub_patches() {
        assert_eq!(
            add_usage_source_patch("h", "https://a.test", "  ", json!("k")),
            json!({"usageLimitSources": {"h": {"kind": "cliproxy", "url": "https://a.test", "managementKey": "k", "enabled": true}}})
        );
        assert_eq!(
            add_usage_source_patch("h", "u", " L ", json!("k"))["usageLimitSources"]["h"]["label"],
            "L"
        );
        assert_eq!(
            remove_usage_source_patch("h"),
            json!({"usageLimitSources": {"h": null}})
        );
    }

    #[test]
    fn generator_names_and_brands() {
        let openai = json!({"kind": "openaiCompatible", "baseUrl": "https://api.openai.com/v1"});
        assert_eq!(display_name(&openai), "OpenAI");
        assert_eq!(brand(&openai), Brand::OpenAi);
        let local = json!({"kind": "openaiCompatible", "baseUrl": "http://localhost:11434/v1"});
        assert_eq!(display_name(&local), "localhost:11434");
        assert_eq!(brand(&local), Brand::Ollama);
        assert_eq!(
            display_name(&json!({"label": "Mine", "baseUrl": "x", "kind": "anthropic"})),
            "Mine"
        );
        assert_eq!(
            brand(&json!({"kind": "anthropic", "baseUrl": "https://x.test"})),
            Brand::Anthropic
        );
        assert_eq!(
            brand(&json!({"kind": "openaiCompatible", "baseUrl": "https://eu.openrouter.ai/v1"})),
            Brand::OpenRouter
        );
        assert_eq!(
            brand(&json!({"kind": "openaiCompatible", "baseUrl": "https://x.test"})),
            Brand::OpenAiCompatible
        );
        assert_eq!(display_name(&json!({"kind": "anthropic", "baseUrl": "junk"})), "junk");
    }

    #[test]
    fn generator_rows_and_ids() {
        let generator = TextGenerator {
            id: "g".into(),
            config: json!({"kind": "openaiCompatible", "baseUrl": "https://api.openai.com/v1", "enabled": false}),
        };
        assert_eq!(generator.description(), "Off \u{b7} api.openai.com/v1");
        let settings = json!({"providerInstances": {"openai": {}}, "textGenerators": {"openai-2": {}}});
        assert_eq!(
            next_text_generator_id("openai", &settings, &["openai-3".into()]),
            "openai-4"
        );
        assert_eq!(next_text_generator_id("codex", &json!({}), &[]), "codex-2");
        assert_eq!(next_text_generator_id("fresh", &json!({}), &[]), "fresh");
    }

    #[test]
    fn generator_config_and_toggle() {
        let config = wizard_config("anthropic", " https://a.test/v1 ", Some(json!("k")), " ", true);
        assert_eq!(
            config,
            json!({"kind": "anthropic", "baseUrl": "https://a.test/v1", "apiKey": "k", "enabled": true})
        );
        assert_eq!(with_enabled(&config, false)["enabled"], false);
        assert_eq!(
            remove_text_generator_patch("g"),
            json!({"textGenerators": {"g": null}})
        );
    }

    #[test]
    fn models_parse() {
        let models = parse_models(&json!({"models": [{"id": "a", "name": "A"}, {"id": "b"}]}));
        assert_eq!(models[0].name, "A");
        assert_eq!(models[1].name, "b");
        assert!(parse_models(&json!({})).is_empty());
    }

    #[test]
    fn task_selections() {
        let settings = json!({"textGenerationModelSelection": {"instanceId": "g", "model": "m"}});
        assert!(task_follows(&settings, TASKS[1].0));
        assert!(!task_follows(&settings, TASKS[0].0));
        assert_eq!(
            task_selection(&settings, TASKS[1].0),
            Some(("g".into(), "m".into()))
        );
        let own = json!({TASKS[1].0: {"instanceId": "h", "model": "n"}});
        assert_eq!(
            task_selection(&own, TASKS[1].0),
            Some(("h".into(), "n".into()))
        );
        assert_eq!(task_selection(&json!({}), TASKS[1].0), None);
    }

    #[test]
    fn health_interval_defaults_and_presets() {
        assert_eq!(health_seconds(&json!({})), 300);
        let battery = json!({"backgroundActivity": {"schemaVersion": 1, "profile": "battery-saver", "overrides": {}}});
        assert_eq!(health_seconds(&battery), 900);
        assert_eq!(health_preset_seconds(&battery), 900);
        let custom = json!({"backgroundActivity": {"schemaVersion": 1, "profile": "custom", "baseProfile": "performance", "overrides": {"providerHealthRefreshInterval": 90000}}});
        assert_eq!(health_seconds(&custom), 90);
        assert_eq!(health_preset_seconds(&custom), 60);
    }

    #[test]
    fn legacy_intervals_still_resolve() {
        let legacy = json!({"providerHealthRefreshInterval": 120000});
        assert_eq!(health_seconds(&legacy), 120);
        assert_eq!(health_preset_seconds(&legacy), 300);
        let profile = json!({"backgroundActivityProfile": "performance", "providerHealthRefreshInterval": 60000});
        assert_eq!(health_seconds(&profile), 60);
        assert_eq!(resolve_background(&profile).1["automaticGitFetchInterval"], 30000);
    }

    #[test]
    fn health_patch_keeps_every_resolved_field() {
        let patch = health_interval_patch(&json!({}), 90);
        let activity = &patch["backgroundActivity"];
        assert_eq!(activity["profile"], "custom");
        assert_eq!(activity["baseProfile"], "balanced");
        assert_eq!(activity["overrides"]["providerHealthRefreshInterval"], 90000);
        assert_eq!(activity["overrides"]["automaticGitFetchInterval"], 30000);
        assert_eq!(activity["overrides"].as_object().unwrap().len(), 8);
    }

    #[test]
    fn health_reset_drops_only_that_override() {
        let reset = health_reset_patch(&json!({}));
        let overrides = reset["backgroundActivity"]["overrides"].as_object().unwrap();
        assert_eq!(overrides.len(), 7);
        assert!(!overrides.contains_key("providerHealthRefreshInterval"));
    }

    #[test]
    fn interval_seconds_normalize() {
        assert_eq!(normalize_seconds(-4.0, 0), 0);
        assert_eq!(normalize_seconds(44.6, 0), 45);
    }

    #[test]
    fn idle_mode_reads_the_old_and_new_fields() {
        assert_eq!(global_idle(&json!({})), IdleMode::OnReturn);
        assert_eq!(global_idle(&json!({"compactIdleThreads": true})), IdleMode::BeforeCacheExpires);
        assert_eq!(global_idle(&json!({"compactIdleThreads": "off"})), IdleMode::Off);
        assert_eq!(
            global_idle(&json!({"idleCompaction": "off", "compactIdleThreads": true})),
            IdleMode::Off
        );
    }

    #[test]
    fn idle_patch_follows_the_server_shape() {
        assert_eq!(idle_patch(&json!({"idleCompaction": "onReturn"}), IdleMode::Off), json!({"idleCompaction": "off"}));
        assert_eq!(idle_patch(&json!({"compactIdleThreads": "off"}), IdleMode::OnReturn), json!({"compactIdleThreads": "onReturn"}));
        assert_eq!(idle_patch(&json!({"compactIdleThreads": false}), IdleMode::BeforeCacheExpires), json!({"compactIdleThreads": true}));
        assert_eq!(idle_patch(&json!({}), IdleMode::OnReturn), json!({"compactIdleThreads": false}));
    }

    #[test]
    fn idle_state_flags_mixed_and_never() {
        let old = json!({"compactIdleThreads": true});
        let new = json!({"idleCompaction": "onReturn"});
        let state = idle_state(&[&old]);
        assert!(!state.mixed && !state.never_available);
        let state = idle_state(&[&old, &new]);
        assert!(state.mixed && !state.never_available);
        assert!(idle_state(&[&new]).never_available);
    }

    #[test]
    fn apply_patch_merges_deletes_and_hides_keys() {
        let mut settings = json!({
            "textGenerators": {"a": {"kind": "anthropic", "apiKey": "x"}},
            "usageLimitSources": {"hub": {"url": "u"}},
            "backgroundActivity": {"overrides": {"providerHealthRefreshInterval": 1, "automaticGitFetchInterval": 2}},
        });
        apply_patch(&mut settings, &remove_usage_source_patch("hub"));
        assert!(settings["usageLimitSources"].get("hub").is_none());
        apply_patch(
            &mut settings,
            &json!({"textGenerators": {"b": {"kind": "openaiCompatible", "apiKey": "secret"}}}),
        );
        assert_eq!(settings["textGenerators"]["a"]["apiKey"], "x");
        assert_eq!(settings["textGenerators"]["b"]["apiKey"], crate::secret_source::REDACTED);
        apply_patch(
            &mut settings,
            &json!({"backgroundActivity": {"overrides": {"automaticGitFetchInterval": 2}}}),
        );
        assert_eq!(
            settings["backgroundActivity"]["overrides"],
            json!({"automaticGitFetchInterval": 2})
        );
    }
}
