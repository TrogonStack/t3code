//! What the composer's model picker trigger shows: the provider instances and the default model
//! selection of a fresh draft, ported from the web (`providerInstances.ts`, `ChatView.tsx`,
//! `ChatView.logic.ts`, `ComposerModelEffort.tsx`). Decoded by hand from `server.getConfig`,
//! which the generated wire types do not cover.

use crate::strip::Defaults;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};
use serde_json::Value;
use std::collections::HashMap;

const DEFAULT_ENABLED_OFF_DRIVERS: &[&str] = &[
    "cursor",
    "grok",
    "fx",
    "kiro",
    "muse",
    "pi",
    "opencode",
    "antigravity",
];

const REASONING_IDS: &[&str] = &[
    "reasoningEffort",
    "effort",
    "reasoning",
    "variant",
    "reasoning_effort",
    "thinking_level",
    "thinking",
];

/// Variant words that name a tier of a family, not a model (`modelShortName.ts`).
const VARIANT_WORDS: &[&str] = &[
    "pro",
    "flash",
    "mini",
    "max",
    "lite",
    "nano",
    "ultra",
    "plus",
    "fast",
    "turbo",
    "preview",
    "latest",
    "thinking",
    "reasoning",
    "build",
    "code",
    "image",
    "vision",
    "audio",
    "live",
    "custom",
    "tools",
    "exp",
    "experimental",
    "chat",
    "tts",
    "stt",
    "non",
    "transcribe",
];

const FAMILY_PREFIXES: &[&str] = &["Claude ", "GPT-", "GPT ", "Gemini ", "Muse ", "Grok "];

fn lenient<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let items = Vec::<Value>::deserialize(deserializer)?;
    Ok(items
        .into_iter()
        .filter_map(|item| serde_json::from_value(item).ok())
        .collect())
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Choice {
    id: String,
    label: String,
    #[serde(default)]
    is_default: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "type",
    rename_all = "lowercase",
    rename_all_fields = "camelCase"
)]
enum Descriptor {
    Select {
        id: String,
        #[serde(default)]
        options: Vec<Choice>,
        #[serde(default)]
        current_value: Option<String>,
        #[serde(default)]
        role: Option<String>,
    },
    Boolean {
        id: String,
        label: String,
        #[serde(default)]
        current_value: Option<bool>,
    },
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Capabilities {
    #[serde(default, deserialize_with = "lenient")]
    option_descriptors: Vec<Descriptor>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Model {
    slug: String,
    name: String,
    #[serde(default)]
    short_name: Option<String>,
    #[serde(default)]
    sub_provider: Option<String>,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    is_custom: bool,
    #[serde(default)]
    is_default: bool,
    #[serde(default)]
    capabilities: Option<Capabilities>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Provider {
    instance_id: String,
    driver: String,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    accent_color: Option<String>,
    status: String,
    #[serde(default)]
    availability: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    models: Vec<Model>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
struct InstanceConfig {
    #[serde(default)]
    driver: Option<String>,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    config: Value,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
struct InstanceOverride {
    #[serde(default)]
    enabled: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct ProjectOverride {
    #[serde(default)]
    provider_instances: HashMap<String, InstanceOverride>,
    #[serde(default)]
    default_model_selection: Option<Selection>,
    #[serde(default)]
    default_runtime_mode: Option<String>,
    #[serde(default)]
    default_thread_env_mode: Option<String>,
    #[serde(default)]
    new_worktrees_start_from_origin: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Selection {
    #[serde(default)]
    instance_id: Option<String>,
    #[serde(default)]
    provider: Option<String>,
    model: String,
    #[serde(default)]
    options: Value,
}

impl Selection {
    fn instance(&self) -> Option<&str> {
        self.instance_id.as_deref().or(self.provider.as_deref())
    }

    /// Option values by id, from the array form or the legacy object form.
    fn option(&self, id: &str) -> Option<String> {
        let text = |value: &Value| match value {
            Value::String(text) => Some(text.clone()),
            Value::Bool(flag) => Some(flag.to_string()),
            _ => None,
        };
        match &self.options {
            Value::Array(items) => items
                .iter()
                .find(|item| item["id"] == id)
                .and_then(|item| text(&item["value"])),
            Value::Object(map) => map.get(id).and_then(text),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Settings {
    #[serde(default)]
    provider_instances: HashMap<String, InstanceConfig>,
    #[serde(default)]
    project_settings_overrides: HashMap<String, ProjectOverride>,
    #[serde(default)]
    default_model_selection: Option<Selection>,
    #[serde(default)]
    default_runtime_mode: Option<String>,
    #[serde(default)]
    default_thread_env_mode: Option<String>,
    #[serde(default)]
    new_worktrees_start_from_origin: Option<bool>,
}

#[derive(Deserialize)]
struct Config {
    #[serde(default, deserialize_with = "lenient")]
    providers: Vec<Provider>,
    #[serde(default)]
    settings: Settings,
}

/// The provider snapshots and settings of one environment.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Catalog {
    providers: Vec<Provider>,
    settings: Settings,
}

/// The two-letter instance chip at the icon's bottom-right.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Badge {
    pub initials: String,
    /// Background and foreground as `0xRRGGBB`; `None` uses the card style.
    pub accent: Option<(u32, u32)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trigger {
    pub driver: String,
    pub display_name: String,
    pub badge: Option<Badge>,
    pub model: String,
    pub effort: Option<String>,
    /// The model's full name, for the tooltip.
    pub title: String,
}

struct Entry<'a> {
    provider: &'a Provider,
    enabled: bool,
}

impl Entry<'_> {
    fn available(&self) -> bool {
        self.provider.availability.as_deref() != Some("unavailable")
    }
}

impl Catalog {
    pub fn decode(value: Value) -> Result<Self, String> {
        let Config {
            providers,
            settings,
        } = serde_json::from_value(value).map_err(|error| error.to_string())?;
        Ok(Self {
            providers,
            settings,
        })
    }

    /// The workspace and permission defaults of a fresh draft in `project`: its override, then the server settings.
    pub fn defaults(&self, project: Option<&str>) -> Defaults {
        let overrides = project.and_then(|id| self.settings.project_settings_overrides.get(id));
        let settings = &self.settings;
        Defaults::resolve(
            overrides
                .and_then(|o| o.default_thread_env_mode.as_deref())
                .or(settings.default_thread_env_mode.as_deref()),
            overrides
                .and_then(|o| o.default_runtime_mode.as_deref())
                .or(settings.default_runtime_mode.as_deref()),
            overrides
                .and_then(|o| o.new_worktrees_start_from_origin)
                .or(settings.new_worktrees_start_from_origin),
        )
    }

    /// The driver and display name of a provider instance, for the sidebar's provider icon.
    pub fn instance(&self, instance_id: &str) -> Option<(&str, String)> {
        let provider = self
            .providers
            .iter()
            .find(|provider| provider.instance_id == instance_id)?;
        Some((&provider.driver, display_name(provider)))
    }

    /// The trigger a fresh draft in `project` shows; `None` when no provider is selectable.
    pub fn trigger(&self, project: Option<&str>) -> Option<Trigger> {
        let entries = self.entries(project);
        let (entry, selection) = self.default_entry(&entries, project)?;
        let provider = entry.provider;
        let requested = selection
            .filter(|selection| selection.instance() == Some(provider.instance_id.as_str()));
        let slug = match requested {
            Some(selection) => resolve_model(&provider.models, &selection.model)
                .map(|model| model.slug.clone())
                .unwrap_or_else(|| default_model(provider)),
            None => default_model(provider),
        };
        let shown = provider
            .models
            .iter()
            .find(|model| model.slug == slug)
            .or_else(|| match provider.driver.as_str() {
                "opencode" | "antigravity" => None,
                _ => provider.models.first(),
            });
        let display_name = display_name(provider);
        let (model, title, effort) = match shown {
            Some(model) => (
                trigger_model_name(model, &provider.driver),
                display_model_name(&model.name, model.sub_provider.as_deref()),
                effort_word(model, &provider.driver, requested),
            ),
            None => {
                let label = if slug.is_empty() {
                    "Choose model".to_owned()
                } else {
                    slug
                };
                (label.clone(), label, None)
            }
        };
        Some(Trigger {
            driver: provider.driver.clone(),
            badge: show_badge(&entries, entry).then(|| badge(provider, &display_name)),
            display_name,
            model,
            effort,
            title,
        })
    }

    /// Provider instances in the order the web lists them: grouped by driver in first-seen
    /// order, each group's default instance first.
    fn entries(&self, project: Option<&str>) -> Vec<Entry<'_>> {
        let mut drivers: Vec<&str> = Vec::new();
        for provider in &self.providers {
            if !drivers.contains(&provider.driver.as_str()) {
                drivers.push(&provider.driver);
            }
        }
        drivers
            .into_iter()
            .flat_map(|driver| {
                let group = || self.providers.iter().filter(move |p| p.driver == driver);
                group()
                    .filter(move |p| p.instance_id == driver)
                    .chain(group().filter(move |p| p.instance_id != driver))
            })
            .map(|provider| Entry {
                provider,
                enabled: self.instance_enabled(project, provider),
            })
            .collect()
    }

    fn instance_enabled(&self, project: Option<&str>, provider: &Provider) -> bool {
        let id = &provider.instance_id;
        let overridden = project
            .and_then(|project| self.settings.project_settings_overrides.get(project))
            .and_then(|overrides| overrides.provider_instances.get(id))
            .and_then(|instance| instance.enabled);
        if let Some(enabled) = overridden {
            return enabled;
        }
        match self.settings.provider_instances.get(id) {
            Some(config) => {
                let driver = config.driver.as_deref().unwrap_or(&provider.driver);
                config_enabled(config, driver)
            }
            None => id == "codex" || id == "claudeAgent",
        }
    }

    /// The entry a fresh draft starts on, with the stored selection that chose it.
    fn default_entry<'a>(
        &'a self,
        entries: &'a [Entry<'a>],
        project: Option<&str>,
    ) -> Option<(&'a Entry<'a>, Option<&'a Selection>)> {
        let project_selection = project
            .and_then(|project| self.settings.project_settings_overrides.get(project))
            .and_then(|overrides| overrides.default_model_selection.as_ref())
            .filter(|selection| {
                entries.iter().any(|entry| {
                    Some(entry.provider.instance_id.as_str()) == selection.instance()
                        && entry.enabled
                })
            });
        let stored = project_selection.or(self.settings.default_model_selection.as_ref());
        if let Some(selection) = stored {
            return selectable(entries, selection.instance()).map(|entry| (entry, Some(selection)));
        }
        let first_driver = entries.first().map(|entry| entry.provider.driver.as_str());
        let group: Vec<&Entry> = entries
            .iter()
            .filter(|entry| Some(entry.provider.driver.as_str()) == first_driver)
            .collect();
        first_selectable(group.iter().copied())
            .or_else(|| first_selectable(entries.iter()))
            .map(|entry| (entry, None))
    }
}

fn config_enabled(config: &InstanceConfig, driver: &str) -> bool {
    let flag = config.config["enabled"].as_bool();
    if config.enabled == Some(false) || flag == Some(false) {
        return false;
    }
    config
        .enabled
        .or(flag)
        .unwrap_or(!DEFAULT_ENABLED_OFF_DRIVERS.contains(&driver))
}

fn first_selectable<'a>(
    entries: impl Iterator<Item = &'a Entry<'a>> + Clone,
) -> Option<&'a Entry<'a>> {
    let usable = |entry: &&Entry| entry.enabled && entry.available();
    entries
        .clone()
        .find(|entry| usable(entry) && entry.provider.status == "ready")
        .or_else(|| {
            entries
                .clone()
                .find(|entry| usable(entry) && entry.provider.status != "error")
        })
}

/// The requested instance when it can take a turn, else the first one that can.
fn selectable<'a>(entries: &'a [Entry<'a>], requested: Option<&str>) -> Option<&'a Entry<'a>> {
    entries
        .iter()
        .find(|entry| {
            Some(entry.provider.instance_id.as_str()) == requested
                && entry.enabled
                && entry.available()
        })
        .or_else(|| first_selectable(entries.iter()))
}

/// Slug, then name, then alias, each ignoring case; else the default model, else the first.
fn resolve_model<'a>(models: &'a [Model], requested: &str) -> Option<&'a Model> {
    let requested = requested.trim();
    models
        .iter()
        .find(|model| model.slug == requested)
        .or_else(|| {
            models
                .iter()
                .find(|model| model.name.eq_ignore_ascii_case(requested))
        })
        .or_else(|| {
            models.iter().find(|model| {
                model
                    .aliases
                    .iter()
                    .any(|alias| alias.eq_ignore_ascii_case(requested))
            })
        })
        .or_else(|| models.iter().find(|model| model.is_default))
        .or_else(|| models.first())
}

fn default_model(provider: &Provider) -> String {
    let models = &provider.models;
    models
        .iter()
        .find(|model| model.is_default && !model.is_custom)
        .or_else(|| models.iter().find(|model| !model.is_custom))
        .or_else(|| models.first())
        .map(|model| model.slug.clone())
        .unwrap_or_else(|| {
            match provider.driver.as_str() {
                "claudeAgent" => "claude-opus-5-5",
                "cursor" => "auto",
                "grok" => "grok-build",
                "fx" | "kiro" | "acpRegistry" | "pi" => "default",
                "opencode" => "openai/gpt-5",
                _ => "",
            }
            .to_owned()
        })
}

fn kind_label(driver: &str) -> String {
    match driver {
        "antigravity" => "Antigravity",
        "codex" => "Codex",
        "claudeAgent" => "Claude",
        "cursor" => "Cursor",
        "grok" => "Grok",
        "fx" => "fx",
        "kiro" => "Kiro",
        "muse" => "Muse Code",
        "acpRegistry" => "ACP Registry",
        "pi" => "Pi",
        "opencode" => "OpenCode",
        other => return humanize(other),
    }
    .to_owned()
}

fn humanize(slug: &str) -> String {
    let mut spaced = String::new();
    let mut previous = ' ';
    for ch in slug.chars() {
        if previous.is_ascii_lowercase() && ch.is_ascii_uppercase() {
            spaced.push(' ');
        }
        spaced.push(if matches!(ch, '_' | '-') { ' ' } else { ch });
        previous = ch;
    }
    let mut out = String::new();
    let mut word_start = true;
    for ch in spaced
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
    {
        out.extend(if word_start {
            ch.to_uppercase().collect::<Vec<_>>()
        } else {
            vec![ch]
        });
        word_start = !ch.is_alphanumeric();
    }
    out
}

fn display_name(provider: &Provider) -> String {
    let named = provider.display_name.as_deref().map(str::trim);
    let kind = kind_label(&provider.driver);
    if let Some(name) = named.filter(|name| !name.is_empty() && *name != kind) {
        return name.to_owned();
    }
    if provider.instance_id != provider.driver {
        let humanized = humanize(&provider.instance_id);
        if !humanized.is_empty() {
            return humanized;
        }
    }
    named
        .filter(|name| !name.is_empty())
        .map_or(kind, str::to_owned)
}

fn accent(provider: &Provider) -> Option<u32> {
    let color = provider.accent_color.as_deref()?.trim();
    let hex = color.strip_prefix('#')?;
    if hex.len() != 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(hex, 16).ok()
}

/// Black or white, whichever contrasts more with `color` (WCAG relative luminance).
fn accent_foreground(color: u32) -> u32 {
    let channel = |shift: u32| {
        let value = f64::from((color >> shift) & 0xff) / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    let luminance = 0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0);
    if (luminance + 0.05) / 0.05 >= 1.05 / (luminance + 0.05) {
        0x000000
    } else {
        0xffffff
    }
}

fn show_badge(entries: &[Entry], entry: &Entry) -> bool {
    if accent(entry.provider).is_some() {
        return true;
    }
    entries
        .iter()
        .filter(|candidate| candidate.provider.driver == entry.provider.driver)
        .filter(|candidate| !entry.enabled || candidate.enabled)
        .count()
        > 1
}

fn badge(provider: &Provider, display_name: &str) -> Badge {
    Badge {
        initials: initials(display_name),
        accent: accent(provider).map(|color| (color, accent_foreground(color))),
    }
}

fn is_separator(ch: char) -> bool {
    let ascii_punctuation = ch.is_ascii_punctuation() && !"$+<=>^`|~".contains(ch);
    let general_punctuation = matches!(ch, '\u{2010}'..='\u{2027}' | '\u{2030}'..='\u{205e}');
    (ascii_punctuation || general_punctuation) && ch != '\'' && ch != '\u{2019}'
}

/// First two letters of a one-word name, else the first letter of the first two words.
pub fn initials(label: &str) -> String {
    let spaced: String = label
        .chars()
        .map(|ch| if is_separator(ch) { ' ' } else { ch })
        .collect();
    let words: Vec<&str> = spaced
        .split_whitespace()
        .map(|word| word.trim_matches(|ch| ch == '\'' || ch == '\u{2019}'))
        .filter(|word| !word.is_empty())
        .collect();
    match words.as_slice() {
        [] => String::new(),
        [word] => word.chars().take(2).collect::<String>().to_uppercase(),
        [first, second, ..] => [first, second]
            .iter()
            .filter_map(|word| word.chars().next())
            .collect::<String>()
            .to_uppercase(),
    }
}

fn non_version_words(text: &str) -> Vec<String> {
    let mut stripped = String::new();
    let mut depth = 0;
    for ch in text.chars() {
        match ch {
            '(' => depth += 1,
            ')' if depth > 0 => {
                depth -= 1;
                stripped.push(' ');
            }
            _ if depth == 0 => stripped.push(ch),
            _ => {}
        }
    }
    stripped
        .split(|ch: char| ch.is_whitespace() || ch == '-')
        .filter(|word| !word.is_empty() && !is_version(word))
        .map(str::to_owned)
        .collect()
}

fn is_version(word: &str) -> bool {
    let word = word.strip_prefix(['v', 'V']).unwrap_or(word);
    let word = match word.strip_suffix(|ch: char| ch.is_ascii_alphabetic()) {
        Some(rest) if rest.ends_with(|ch: char| ch.is_ascii_digit()) => rest,
        _ => word,
    };
    !word.is_empty()
        && word
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
}

fn names_model(text: &str) -> bool {
    non_version_words(text)
        .iter()
        .any(|word| !VARIANT_WORDS.contains(&word.to_lowercase().as_str()))
}

fn native_families(driver: &str) -> &'static [&'static str] {
    match driver {
        "codex" => &["GPT-", "GPT "],
        "claudeAgent" => &["Claude "],
        "cursor" | "grok" => &["Grok "],
        "antigravity" => &["Gemini "],
        "muse" => &["Muse "],
        _ => &[],
    }
}

/// `^[A-Z][\w.&\- ]{0,24}: (?=\S)`
fn strip_vendor_prefix(name: &str) -> &str {
    let Some(colon) = name.find(": ") else {
        return name;
    };
    let head = &name[..colon];
    let valid = head.chars().count() <= 25
        && head.starts_with(|ch: char| ch.is_ascii_uppercase())
        && head
            .chars()
            .all(|ch| ch.is_alphanumeric() || matches!(ch, '_' | '.' | '&' | '-' | ' '));
    let rest = &name[colon + 2..];
    if valid && rest.starts_with(|ch: char| !ch.is_whitespace()) {
        rest
    } else {
        name
    }
}

fn model_short_name(name: &str, native: &[&str]) -> Option<String> {
    let unprefixed = strip_vendor_prefix(name);
    let without_family = FAMILY_PREFIXES
        .iter()
        .find(|prefix| unprefixed.starts_with(**prefix))
        .and_then(|prefix| {
            let rest = unprefixed[prefix.len()..].trim();
            let keep = if native.contains(prefix) {
                !non_version_words(rest).is_empty()
            } else {
                names_model(rest)
            };
            keep.then(|| {
                if rest.contains(char::is_whitespace) {
                    rest.to_owned()
                } else {
                    rest.replace('-', " ")
                }
            })
        });
    let short = without_family.unwrap_or_else(|| {
        if names_model(unprefixed) {
            unprefixed.to_owned()
        } else {
            name.to_owned()
        }
    });
    (short != name).then_some(short)
}

/// Drops a leading `qualifier` and its separator (`stripLeadingQualifier`).
fn display_model_name(name: &str, qualifier: Option<&str>) -> String {
    let Some(qualifier) = qualifier
        .map(str::trim)
        .filter(|qualifier| !qualifier.is_empty())
    else {
        return name.to_owned();
    };
    let lower_name = name.to_lowercase();
    let lower_qualifier = qualifier.to_lowercase();
    let Some(rest) = lower_name
        .strip_prefix(&lower_qualifier)
        .filter(|_| name.is_char_boundary(lower_qualifier.len()))
        .map(|_| &name[lower_qualifier.len()..])
    else {
        return name.to_owned();
    };
    let after = rest.trim_start();
    let separated = after.len() < rest.len() || after.starts_with(['.', ':', '/', '-']);
    if !separated {
        return name.to_owned();
    }
    let stripped = after
        .strip_prefix(['.', ':', '/', '-'])
        .unwrap_or(after)
        .trim();
    if stripped.is_empty() {
        name.to_owned()
    } else {
        stripped.to_owned()
    }
}

fn trigger_model_name(model: &Model, driver: &str) -> String {
    let short = model
        .short_name
        .clone()
        .or_else(|| model_short_name(&model.name, native_families(driver)));
    match short {
        Some(short) => display_model_name(&short, model.sub_provider.as_deref()),
        None => display_model_name(&model.name, model.sub_provider.as_deref()),
    }
}

fn is_reasoning(descriptor: &Descriptor) -> bool {
    match descriptor {
        Descriptor::Select { id, role, .. } => {
            role.as_deref() == Some("reasoning") || REASONING_IDS.contains(&id.as_str())
        }
        Descriptor::Boolean { id, .. } => REASONING_IDS.contains(&id.as_str()),
    }
}

fn is_speed_control(descriptor: &Descriptor, driver: &str) -> bool {
    match descriptor {
        Descriptor::Select { id, .. } => driver == "codex" && id == "serviceTier",
        Descriptor::Boolean { id, .. } => id == "fastMode",
    }
}

/// The reasoning control's current setting, such as `High`.
fn effort_word(model: &Model, driver: &str, selection: Option<&Selection>) -> Option<String> {
    let descriptors = &model.capabilities.as_ref()?.option_descriptors;
    let candidates = || {
        descriptors
            .iter()
            .filter(|descriptor| !is_speed_control(descriptor, driver))
    };
    let reasoning = candidates()
        .filter(|descriptor| matches!(descriptor, Descriptor::Select { .. }))
        .chain(candidates().filter(|descriptor| matches!(descriptor, Descriptor::Boolean { .. })))
        .find(|descriptor| is_reasoning(descriptor))?;
    match reasoning {
        Descriptor::Select {
            id,
            options,
            current_value,
            ..
        } => {
            let current = selection
                .and_then(|selection| selection.option(id))
                .or_else(|| current_value.clone())
                .or_else(|| {
                    options
                        .iter()
                        .find(|option| option.is_default)
                        .map(|option| option.id.clone())
                })?;
            options
                .iter()
                .find(|option| option.id == current)
                .map(|option| option.label.clone())
        }
        Descriptor::Boolean {
            id,
            label,
            current_value,
        } => {
            let on = selection
                .and_then(|selection| selection.option(id))
                .map(|value| value == "true")
                .or(*current_value)
                == Some(true);
            Some(format!("{label} {}", if on { "On" } else { "Off" }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strip::{EnvMode, RuntimeMode};
    use serde_json::json;

    fn model(slug: &str, name: &str, is_default: bool) -> Value {
        json!({"slug": slug, "name": name, "isCustom": false, "isDefault": is_default, "capabilities": null})
    }

    fn provider(instance: &str, driver: &str, status: &str, models: Vec<Value>) -> Value {
        json!({"instanceId": instance, "driver": driver, "status": status, "models": models})
    }

    fn catalog(providers: Vec<Value>, settings: Value) -> Catalog {
        Catalog::decode(json!({"providers": providers, "settings": settings})).unwrap()
    }

    #[test]
    fn defaults_prefer_the_project_override() {
        let catalog = catalog(
            vec![],
            json!({
                "defaultRuntimeMode": "auto",
                "defaultThreadEnvMode": "worktree",
                "projectSettingsOverrides": {"p": {"defaultThreadEnvMode": "local"}},
            }),
        );
        let overridden = catalog.defaults(Some("p"));
        assert_eq!(overridden.env_mode, EnvMode::Local);
        assert_eq!(overridden.runtime_mode, RuntimeMode::Auto);
        assert_eq!(catalog.defaults(Some("q")).env_mode, EnvMode::Worktree);
        assert_eq!(catalog.defaults(None).env_mode, EnvMode::Worktree);
    }

    #[test]
    fn initials_follow_the_web() {
        assert_eq!(initials("Codex"), "CO");
        assert_eq!(initials("Codex (hub)"), "CH");
        assert_eq!(initials("Theo's Codex"), "TC");
        assert_eq!(initials("Codex 'hub'"), "CH");
        assert_eq!(initials("work-claude personal"), "WC");
        assert_eq!(initials("  "), "");
    }

    #[test]
    fn accent_foreground_contrasts() {
        assert_eq!(accent_foreground(0xffffff), 0x000000);
        assert_eq!(accent_foreground(0x000000), 0xffffff);
        assert_eq!(accent_foreground(0x1d4ed8), 0xffffff);
        assert_eq!(accent_foreground(0xfacc15), 0x000000);
    }

    #[test]
    fn short_names_drop_the_native_family() {
        let claude = native_families("claudeAgent");
        assert_eq!(
            model_short_name("Claude Sonnet 4.5", claude).as_deref(),
            Some("Sonnet 4.5")
        );
        assert_eq!(model_short_name("Claude 4.5", claude), None);
        assert_eq!(model_short_name("GPT-5.5", native_families("codex")), None);
        assert_eq!(
            model_short_name("GPT-5.5-Codex", native_families("codex")).as_deref(),
            Some("5.5 Codex")
        );
        assert_eq!(model_short_name("GPT-5 Pro", &[]), None);
        assert_eq!(
            model_short_name("Anthropic: Claude Opus 5", &[]).as_deref(),
            Some("Opus 5")
        );
    }

    #[test]
    fn qualifiers_are_stripped_from_names() {
        assert_eq!(
            display_model_name("OpenAI / GPT-5", Some("OpenAI")),
            "GPT-5"
        );
        assert_eq!(display_model_name("OpenAI GPT-5", Some("openai")), "GPT-5");
        assert_eq!(display_model_name("OpenAIs", Some("OpenAI")), "OpenAIs");
        assert_eq!(display_model_name("OpenAI", Some("OpenAI")), "OpenAI");
    }

    #[test]
    fn display_names_prefer_the_snapshot_then_the_instance() {
        let named = |instance: &str, display: Option<&str>| Provider {
            instance_id: instance.into(),
            driver: "codex".into(),
            display_name: display.map(str::to_owned),
            accent_color: None,
            status: "ready".into(),
            availability: None,
            models: Vec::new(),
        };
        assert_eq!(display_name(&named("codex", Some("Codex"))), "Codex");
        assert_eq!(
            display_name(&named("codex_work", Some("Codex"))),
            "Codex Work"
        );
        assert_eq!(display_name(&named("codex_work", Some("Hub"))), "Hub");
        assert_eq!(humanize("claudeAgent"), "Claude Agent");
    }

    #[test]
    fn unconfigured_instances_enable_only_codex_and_claude() {
        let catalog = catalog(
            vec![
                provider("codex", "codex", "ready", vec![]),
                provider("cursor", "cursor", "ready", vec![]),
            ],
            json!({}),
        );
        let enabled: Vec<bool> = catalog
            .entries(None)
            .iter()
            .map(|entry| entry.enabled)
            .collect();
        assert_eq!(enabled, [true, false]);
    }

    #[test]
    fn configured_instances_follow_driver_defaults_and_flags() {
        let config = |value: Value| serde_json::from_value::<InstanceConfig>(value).unwrap();
        assert!(config_enabled(&config(json!({"driver": "codex"})), "codex"));
        assert!(!config_enabled(
            &config(json!({"driver": "cursor"})),
            "cursor"
        ));
        assert!(config_enabled(&config(json!({"enabled": true})), "cursor"));
        assert!(!config_enabled(
            &config(json!({"enabled": true, "config": {"enabled": false}})),
            "codex"
        ));
        assert!(config_enabled(
            &config(json!({"config": {"enabled": true}})),
            "cursor"
        ));
    }

    #[test]
    fn project_override_beats_instance_config() {
        let catalog = catalog(
            vec![provider("codex", "codex", "ready", vec![])],
            json!({"projectSettingsOverrides": {"p1": {"providerInstances": {"codex": {"enabled": false}}}}}),
        );
        assert!(catalog.entries(Some("p2"))[0].enabled);
        assert!(!catalog.entries(Some("p1"))[0].enabled);
    }

    #[test]
    fn entries_group_by_driver_with_defaults_first() {
        let catalog = catalog(
            vec![
                provider("codex_b", "codex", "ready", vec![]),
                provider("claudeAgent", "claudeAgent", "ready", vec![]),
                provider("codex", "codex", "ready", vec![]),
            ],
            json!({}),
        );
        let ids: Vec<String> = catalog
            .entries(None)
            .iter()
            .map(|entry| entry.provider.instance_id.clone())
            .collect();
        assert_eq!(ids, ["codex", "codex_b", "claudeAgent"]);
    }

    #[test]
    fn selection_falls_back_to_a_ready_instance() {
        let catalog = catalog(
            vec![
                provider("codex", "codex", "error", vec![model("a", "A", true)]),
                provider(
                    "claudeAgent",
                    "claudeAgent",
                    "ready",
                    vec![model("b", "B", true)],
                ),
            ],
            json!({}),
        );
        assert_eq!(catalog.trigger(None).unwrap().driver, "claudeAgent");
    }

    #[test]
    fn unavailable_instances_are_skipped() {
        let mut unavailable = provider("codex", "codex", "ready", vec![model("a", "A", true)]);
        unavailable["availability"] = json!("unavailable");
        let catalog = catalog(
            vec![
                unavailable,
                provider(
                    "claudeAgent",
                    "claudeAgent",
                    "warning",
                    vec![model("b", "B", true)],
                ),
            ],
            json!({}),
        );
        assert_eq!(catalog.trigger(None).unwrap().driver, "claudeAgent");
    }

    #[test]
    fn stored_default_selection_wins_and_resolves_the_model() {
        let models = vec![
            model("gpt-a", "GPT A", true),
            model("gpt-b", "GPT B", false),
        ];
        let catalog = catalog(
            vec![
                provider(
                    "claudeAgent",
                    "claudeAgent",
                    "ready",
                    vec![model("c", "C", true)],
                ),
                provider("codex", "codex", "ready", models),
            ],
            json!({"defaultModelSelection": {"instanceId": "codex", "model": "gpt b"}}),
        );
        let trigger = catalog.trigger(None).unwrap();
        assert_eq!(trigger.driver, "codex");
        assert_eq!(trigger.title, "GPT B");
    }

    #[test]
    fn project_default_applies_only_when_its_instance_is_enabled() {
        let providers = vec![
            provider("codex", "codex", "ready", vec![model("a", "A", true)]),
            provider(
                "claudeAgent",
                "claudeAgent",
                "ready",
                vec![model("b", "B", true)],
            ),
        ];
        let enabled = catalog(
            providers.clone(),
            json!({"projectSettingsOverrides": {"p": {"defaultModelSelection": {"instanceId": "claudeAgent", "model": "b"}}}}),
        );
        assert_eq!(enabled.trigger(Some("p")).unwrap().driver, "claudeAgent");
        assert_eq!(enabled.trigger(None).unwrap().driver, "codex");
        let disabled = catalog(
            providers,
            json!({"projectSettingsOverrides": {"p": {
                "defaultModelSelection": {"instanceId": "claudeAgent", "model": "b"},
                "providerInstances": {"claudeAgent": {"enabled": false}}
            }}}),
        );
        assert_eq!(disabled.trigger(Some("p")).unwrap().driver, "codex");
    }

    #[test]
    fn default_model_prefers_flagged_then_first_non_custom() {
        let mut custom = model("custom", "Custom", true);
        custom["isCustom"] = json!(true);
        let provider_of = |models: Vec<Value>| {
            catalog(vec![provider("codex", "codex", "ready", models)], json!({}))
                .providers
                .remove(0)
        };
        assert_eq!(
            default_model(&provider_of(vec![
                custom.clone(),
                model("a", "A", false),
                model("b", "B", true)
            ])),
            "b"
        );
        assert_eq!(
            default_model(&provider_of(vec![custom, model("a", "A", false)])),
            "a"
        );
        let cursor = catalog(
            vec![provider("cursor", "cursor", "ready", vec![])],
            json!({}),
        )
        .providers
        .remove(0);
        assert_eq!(default_model(&cursor), "auto");
    }

    #[test]
    fn badge_shows_for_accents_and_sibling_instances() {
        let mut accented = provider("codex", "codex", "ready", vec![]);
        accented["accentColor"] = json!("#1d4ed8");
        let alone = catalog(vec![accented], json!({}));
        let entries = alone.entries(None);
        assert!(show_badge(&entries, &entries[0]));
        let plain = catalog(vec![provider("codex", "codex", "ready", vec![])], json!({}));
        let entries = plain.entries(None);
        assert!(!show_badge(&entries, &entries[0]));
        let siblings = catalog(
            vec![
                provider("codex", "codex", "ready", vec![]),
                provider("codex_work", "codex", "ready", vec![]),
            ],
            json!({"providerInstances": {"codex_work": {"driver": "codex", "enabled": true}}}),
        );
        let entries = siblings.entries(None);
        assert!(show_badge(&entries, &entries[0]));
        assert_eq!(badge(entries[1].provider, "Codex Work").initials, "CW");
    }

    #[test]
    fn effort_reads_the_reasoning_select() {
        let mut with_effort = model("m", "M", true);
        with_effort["capabilities"] = json!({"optionDescriptors": [
            {"id": "serviceTier", "label": "Speed", "type": "select", "options": [{"id": "fast", "label": "Fast"}], "currentValue": "fast"},
            {"id": "reasoningEffort", "label": "Reasoning", "type": "select",
             "options": [{"id": "low", "label": "Low"}, {"id": "high", "label": "High", "isDefault": true}]},
            {"id": "fastMode", "label": "Fast", "type": "boolean", "currentValue": true}
        ]});
        let catalog = catalog(
            vec![provider("codex", "codex", "ready", vec![with_effort])],
            json!({}),
        );
        assert_eq!(
            catalog.trigger(None).unwrap().effort.as_deref(),
            Some("High")
        );
        let chosen =
            catalog_with_selection(&catalog, json!([{"id": "reasoningEffort", "value": "low"}]));
        assert_eq!(chosen.as_deref(), Some("Low"));
    }

    fn catalog_with_selection(catalog: &Catalog, options: Value) -> Option<String> {
        let selection: Selection = serde_json::from_value(
            json!({"instanceId": "codex", "model": "m", "options": options}),
        )
        .unwrap();
        effort_word(&catalog.providers[0].models[0], "codex", Some(&selection))
    }

    #[test]
    fn boolean_reasoning_reads_on_or_off() {
        let mut thinking = model("m", "M", true);
        thinking["capabilities"] = json!({"optionDescriptors": [
            {"id": "thinking", "label": "Thinking", "type": "boolean", "currentValue": true}
        ]});
        let catalog = catalog(
            vec![provider(
                "claudeAgent",
                "claudeAgent",
                "ready",
                vec![thinking],
            )],
            json!({}),
        );
        assert_eq!(
            catalog.trigger(None).unwrap().effort.as_deref(),
            Some("Thinking On")
        );
    }

    #[test]
    fn malformed_entries_are_dropped_not_fatal() {
        let catalog = catalog(
            vec![
                json!({"instanceId": 3}),
                provider(
                    "codex",
                    "codex",
                    "ready",
                    vec![json!("junk"), model("a", "A", true)],
                ),
            ],
            json!({}),
        );
        assert_eq!(catalog.providers.len(), 1);
        assert_eq!(catalog.providers[0].models.len(), 1);
    }

    #[test]
    fn no_selectable_provider_means_no_trigger() {
        assert_eq!(
            catalog(
                vec![provider("cursor", "cursor", "ready", vec![])],
                json!({})
            )
            .trigger(None),
            None
        );
    }
}
