//! What the composer's model picker trigger shows: the provider instances and the default model
//! selection of a fresh draft, ported from the web (`providerInstances.ts`, `ChatView.tsx`,
//! `ChatView.logic.ts`, `ComposerModelEffort.tsx`). Decoded by hand from `server.getConfig`,
//! which the generated wire types do not cover.

use crate::strip::Defaults;
use crate::usage::Limits;
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
    #[serde(default)]
    description: Option<String>,
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
        label: String,
        #[serde(default)]
        options: Vec<Choice>,
        #[serde(default)]
        current_value: Option<String>,
        #[serde(default)]
        role: Option<String>,
        #[serde(default)]
        prompt_injected_values: Vec<String>,
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
    badge: Option<String>,
    #[serde(default)]
    is_legacy: bool,
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
    #[serde(default)]
    message: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    models: Vec<Model>,
    /// Kept raw: `usage::Limits` decodes it for the details card.
    #[serde(default)]
    usage_limits: Option<Value>,
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

    /// The `{id, value}` selections, from the array form or the legacy object form.
    fn explicit(&self) -> Vec<Value> {
        match &self.options {
            Value::Array(items) => items.clone(),
            Value::Object(map) => map
                .iter()
                .map(|(id, value)| serde_json::json!({"id": id, "value": value}))
                .collect(),
            _ => Vec::new(),
        }
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

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct Favorite {
    provider: String,
    model: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Settings {
    /// `None` on servers that predate server-side favorites.
    #[serde(default)]
    favorites: Option<Vec<Favorite>>,
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
    #[serde(default, rename = "availableEditors", deserialize_with = "lenient")]
    available_editors: Vec<String>,
}

/// The provider snapshots and settings of one environment.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Catalog {
    providers: Vec<Provider>,
    settings: Settings,
    available_editors: Vec<String>,
}

/// The two-letter instance chip at the icon's bottom-right.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Badge {
    pub initials: String,
    /// Background and foreground as `0xRRGGBB`; `None` uses the card style.
    pub accent: Option<(u32, u32)>,
}

/// What a provider instance's icon shows: the driver's art or initials, plus the instance chip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mark {
    pub driver: String,
    pub display_name: String,
    pub badge: Option<Badge>,
}

/// A model chosen in the picker for one composer: the instance it runs on and its slug.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pick {
    pub instance_id: String,
    pub model: String,
    /// The `{id, value}` option selections the user set, as `modelSelection.options` carries them.
    pub options: Vec<Value>,
}

/// One model row of the picker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickerModel {
    pub slug: String,
    pub label: String,
    /// The model's full name, for the tooltip.
    pub title: String,
    /// The provider's raw name and short name, which search matches.
    pub name: String,
    pub short_name: Option<String>,
    pub sub_provider: Option<String>,
    pub favorite: bool,
    /// Older models sit behind the list's "Show more" unless starred.
    pub legacy: bool,
    pub new: bool,
}

/// One provider instance of the picker's sidebar, with its models.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickerInstance {
    pub instance_id: String,
    pub mark: Mark,
    pub name: String,
    /// What the sidebar says when the instance cannot take a turn.
    pub unavailable: Option<String>,
    pub models: Vec<PickerModel>,
    pub driver: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trigger {
    pub instance_id: String,
    pub slug: String,
    pub mark: Mark,
    pub model: String,
    pub effort: Option<String>,
    /// Whether the speed control is on a fast tier, which the chip marks with a bolt.
    pub fast: bool,
    /// The effort column and speed control of the model.
    pub controls: Controls,
    /// The `modelSelection.options` a turn on this model sends.
    pub options: Vec<Value>,
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
            available_editors,
        } = serde_json::from_value(value).map_err(|error| error.to_string())?;
        Ok(Self {
            providers,
            settings,
            available_editors,
        })
    }

    /// Whether `model` on `instance_id` is starred.
    pub fn is_favorite(&self, instance_id: &str, model: &str) -> bool {
        self.settings.favorites.as_ref().is_some_and(|favorites| {
            favorites
                .iter()
                .any(|fav| fav.provider == instance_id && fav.model == model)
        })
    }

    pub fn has_favorites(&self) -> bool {
        self.settings
            .favorites
            .as_ref()
            .is_some_and(|favorites| !favorites.is_empty())
    }

    /// Stars or unstars a model in the local copy, like the server's `setModelFavorites` reducer.
    pub fn set_favorite(&mut self, instance_id: &str, model: &str, favorite: bool) {
        let favorites = self.settings.favorites.get_or_insert_with(Vec::new);
        favorites.retain(|fav| !(fav.provider == instance_id && fav.model == model));
        if favorite {
            favorites.push(Favorite {
                provider: instance_id.to_owned(),
                model: model.to_owned(),
            });
        }
    }

    /// The editor ids the server found on its machine (`server.getConfig` `availableEditors`).
    pub fn available_editors(&self) -> &[String] {
        &self.available_editors
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

    /// The usage limits an instance reports, for the details card's limits row.
    pub fn usage_limits(&self, instance_id: &str) -> Option<Limits> {
        self.providers
            .iter()
            .find(|provider| provider.instance_id == instance_id)
            .and_then(|provider| Limits::decode(provider.usage_limits.as_ref()?))
    }

    /// A provider instance's icon, for the sidebar's thread cards.
    pub fn instance(&self, instance_id: &str) -> Option<Mark> {
        let entries = self.entries(None);
        let entry = entries
            .iter()
            .find(|entry| entry.provider.instance_id == instance_id)?;
        Some(mark(&entries, entry))
    }

    /// The model as the sidebar card names it: the instance's short name for the slug, else the slug.
    pub fn model_label(&self, instance_id: &str, slug: &str) -> String {
        self.providers
            .iter()
            .find(|provider| provider.instance_id == instance_id)
            .and_then(|provider| {
                let model = provider.models.iter().find(|model| model.slug == slug)?;
                Some(trigger_model_name(model, &provider.driver))
            })
            .unwrap_or_else(|| slug.to_owned())
    }

    /// The instances the picker lists, in the web's order: the ones enabled for `project`.
    pub fn picker(&self, project: Option<&str>) -> Vec<PickerInstance> {
        let entries = self.entries(project);
        entries
            .iter()
            .filter(|entry| entry.enabled)
            .map(|entry| {
                let provider = entry.provider;
                let mark = mark(&entries, entry);
                PickerInstance {
                    instance_id: provider.instance_id.clone(),
                    name: mark.display_name.clone(),
                    unavailable: (!entry.available() || provider.status != "ready")
                        .then(|| unavailable_note(&mark.display_name, provider)),
                    models: provider
                        .models
                        .iter()
                        .map(|model| PickerModel {
                            slug: model.slug.clone(),
                            label: trigger_model_name(model, &provider.driver),
                            title: display_model_name(&model.name, model.sub_provider.as_deref()),
                            name: model.name.clone(),
                            short_name: model.short_name.clone(),
                            sub_provider: model.sub_provider.clone(),
                            favorite: self.is_favorite(&provider.instance_id, &model.slug),
                            legacy: model.is_legacy,
                            new: model.badge.as_deref() == Some("new"),
                        })
                        .collect(),
                    driver: provider.driver.clone(),
                    mark,
                }
            })
            .collect()
    }

    /// The choice of `slug` on `instance_id`, when that instance can take a turn and has the model.
    pub fn pick(&self, project: Option<&str>, instance_id: &str, slug: &str) -> Option<Pick> {
        let entries = self.entries(project);
        let entry = entries.iter().find(|entry| {
            entry.provider.instance_id == instance_id
                && entry.enabled
                && entry.available()
                && entry.provider.status != "error"
        })?;
        entry
            .provider
            .models
            .iter()
            .any(|model| model.slug == slug)
            .then(|| Pick {
                instance_id: instance_id.to_owned(),
                model: slug.to_owned(),
                options: Vec::new(),
            })
    }

    /// The trigger a fresh draft in `project` shows; `None` when no provider is selectable.
    #[cfg(test)]
    pub fn trigger(&self, project: Option<&str>) -> Option<Trigger> {
        self.trigger_for(project, None)
    }

    /// The trigger of a draft whose user may have picked a model; a pick that no longer resolves
    /// (the instance was disabled, or went away) falls back to the draft's default.
    pub fn trigger_for(&self, project: Option<&str>, pick: Option<&Pick>) -> Option<Trigger> {
        let entries = self.entries(project);
        let picked = pick.and_then(|pick| {
            let entry = entries.iter().find(|entry| {
                entry.provider.instance_id == pick.instance_id && entry.enabled && entry.available()
            })?;
            let selection = Selection {
                instance_id: Some(pick.instance_id.clone()),
                provider: None,
                model: pick.model.clone(),
                options: Value::Array(pick.options.clone()),
            };
            Some((entry, selection))
        });
        let (entry, requested) = match &picked {
            Some((entry, selection)) => (*entry, Some(selection)),
            None => self.default_entry(&entries, project)?,
        };
        let provider = entry.provider;
        let requested = requested
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
        let requested = requested.filter(|selection| {
            resolve_model(&provider.models, &selection.model).map(|model| &model.slug)
                == Some(&slug)
        });
        let (model, title, effort, controls) = match shown {
            Some(model) => (
                trigger_model_name(model, &provider.driver),
                display_model_name(&model.name, model.sub_provider.as_deref()),
                effort_word(model, &provider.driver, requested),
                Controls::of(model, &provider.driver, requested),
            ),
            None => {
                let label = if slug.is_empty() {
                    "Choose model".to_owned()
                } else {
                    slug.clone()
                };
                (label.clone(), label, None, Controls::default())
            }
        };
        let options = shown
            .map(|model| sent_options(model, requested))
            .unwrap_or_default();
        Some(Trigger {
            instance_id: provider.instance_id.clone(),
            slug,
            mark: mark(&entries, entry),
            model,
            effort,
            fast: controls.fast(),
            controls,
            options,
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

/// `describeUnavailableInstance` for an instance the picker cannot select.
fn unavailable_note(name: &str, provider: &Provider) -> String {
    if provider.status == "disabled" {
        return format!("{name}: Disabled in settings.");
    }
    let kind = match provider.status.as_str() {
        "error" => "Unavailable",
        "warning" => "Limited",
        _ => "Not ready",
    };
    match provider.message.as_deref().map(str::trim) {
        Some(message) if !message.is_empty() => format!("{name}: {kind}. {message}"),
        _ => format!("{name}: {kind}."),
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

fn mark(entries: &[Entry], entry: &Entry) -> Mark {
    let display_name = display_name(entry.provider);
    Mark {
        driver: entry.provider.driver.clone(),
        badge: show_badge(entries, entry).then(|| badge(entry.provider, &display_name)),
        display_name,
    }
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

/// One row of the effort column, or one segment of the speed control.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Level {
    /// What `modelSelection.options` carries for this level: a choice id or a flag.
    pub value: Value,
    pub label: String,
    pub is_default: bool,
    pub selected: bool,
    pub description: Option<String>,
}

/// A model option the picker edits: a select's choices, or a flag as Off and On.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Control {
    pub id: String,
    pub label: String,
    pub flag: bool,
    pub levels: Vec<Level>,
}

impl Control {
    pub fn current(&self) -> Option<&Level> {
        self.levels.iter().find(|level| level.selected)
    }

    /// The level after the selected one, wrapping: what clicking a menu-less option row picks.
    pub fn next(&self) -> Option<&Level> {
        let at = self.levels.iter().position(|level| level.selected);
        self.levels
            .get(at.map_or(0, |index| (index + 1) % self.levels.len()))
    }
}

/// What the picker's effort column and footer offer for the selected model
/// (`ComposerModelEffort.tsx`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Controls {
    /// The reasoning control, listed under the column header.
    pub effort: Option<Control>,
    /// The remaining options, one row each below the levels.
    pub others: Vec<Control>,
    /// The footer's speed segments: a service tier, or Standard and Fast.
    pub speed: Option<Control>,
}

impl Controls {
    fn of(model: &Model, driver: &str, selection: Option<&Selection>) -> Self {
        let Some(capabilities) = &model.capabilities else {
            return Self::default();
        };
        let descriptors = &capabilities.option_descriptors;
        let selects = descriptors
            .iter()
            .filter(|descriptor| matches!(descriptor, Descriptor::Select { .. }));
        let flags = descriptors
            .iter()
            .filter(|descriptor| matches!(descriptor, Descriptor::Boolean { .. }));
        let (speeds, column): (Vec<&Descriptor>, Vec<&Descriptor>) = selects
            .chain(flags)
            .partition(|descriptor| is_speed_control(descriptor, driver));
        let effort_at = column
            .iter()
            .position(|descriptor| is_reasoning(descriptor));
        let mut controls: Vec<Option<Control>> = column
            .iter()
            .map(|descriptor| control(descriptor, selection, false))
            .collect();
        let effort = effort_at.and_then(|index| controls[index].take());
        Self {
            effort,
            others: controls.into_iter().flatten().collect(),
            speed: speeds
                .first()
                .and_then(|descriptor| control(descriptor, selection, true)),
        }
    }

    /// Whether the speed control sits on a fast tier.
    fn fast(&self) -> bool {
        self.speed
            .as_ref()
            .and_then(Control::current)
            .is_some_and(|level| is_fast_label(&level.label))
    }

    /// What each row of the effort column sets, in order: an option id and its value.
    pub fn actions(&self) -> Vec<(String, Value)> {
        let effort = self.effort.iter().flat_map(|control| {
            control
                .levels
                .iter()
                .map(|level| (control.id.clone(), level.value.clone()))
        });
        let others = self.others.iter().filter_map(|control| {
            control
                .next()
                .map(|level| (control.id.clone(), level.value.clone()))
        });
        effort.chain(others).collect()
    }

    /// The column header: the effort control's name, else "Options", else "Effort".
    pub fn title(&self) -> String {
        match &self.effort {
            Some(effort) if !effort.label.is_empty() => effort.label.clone(),
            None if !self.others.is_empty() => "Options".to_owned(),
            _ => "Effort".to_owned(),
        }
    }
}

pub fn is_fast_label(label: &str) -> bool {
    label == "Fast" || label == "Ultrafast"
}

/// A select's choices (minus the ones a prompt prefix selects) or a flag, with the current one marked.
fn control(descriptor: &Descriptor, selection: Option<&Selection>, speed: bool) -> Option<Control> {
    match descriptor {
        Descriptor::Select {
            id,
            label,
            options,
            current_value,
            prompt_injected_values,
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
                });
            let levels: Vec<Level> = options
                .iter()
                .filter(|option| !prompt_injected_values.contains(&option.id))
                .map(|option| Level {
                    value: Value::String(option.id.clone()),
                    label: option.label.clone(),
                    is_default: option.is_default,
                    selected: current.as_deref() == Some(option.id.as_str()),
                    description: option.description.clone().filter(|text| !text.is_empty()),
                })
                .collect();
            (!levels.is_empty()).then(|| Control {
                id: id.clone(),
                label: label.clone(),
                flag: false,
                levels,
            })
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
            let names = if speed {
                [(false, "Standard"), (true, "Fast")]
            } else {
                [(true, "On"), (false, "Off")]
            };
            Some(Control {
                id: id.clone(),
                label: label.clone(),
                flag: true,
                levels: names
                    .into_iter()
                    .map(|(value, name)| Level {
                        value: Value::Bool(value),
                        label: name.to_owned(),
                        is_default: false,
                        selected: on == value,
                        description: None,
                    })
                    .collect(),
            })
        }
    }
}

/// `options` with `id` set to `value`: the option selections after the user picks a level.
pub fn with_option(options: &[Value], id: &str, value: Value) -> Vec<Value> {
    let entry = serde_json::json!({"id": id, "value": value});
    let mut next: Vec<Value> = options
        .iter()
        .filter(|option| option["id"] != id)
        .cloned()
        .collect();
    next.push(entry);
    next
}

/// The options a turn sends: the user's selections for this model's options, plus Fast mode off
/// when the model has that flag and nothing set it (`withImplicitFastModeDefault`).
fn sent_options(model: &Model, selection: Option<&Selection>) -> Vec<Value> {
    let descriptors = model
        .capabilities
        .as_ref()
        .map(|capabilities| capabilities.option_descriptors.as_slice())
        .unwrap_or_default();
    let known = |id: &str| {
        descriptors.iter().any(|descriptor| match descriptor {
            Descriptor::Select { id: known, .. } | Descriptor::Boolean { id: known, .. } => {
                known == id
            }
        })
    };
    let mut options: Vec<Value> = selection
        .map(Selection::explicit)
        .unwrap_or_default()
        .into_iter()
        .filter(|option| option["id"].as_str().is_some_and(known))
        .collect();
    let has_fast_flag = descriptors
        .iter()
        .any(|descriptor| matches!(descriptor, Descriptor::Boolean { id, .. } if id == "fastMode"));
    if has_fast_flag && !options.iter().any(|option| option["id"] == "fastMode") {
        options.push(serde_json::json!({"id": "fastMode", "value": false}));
    }
    options
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
    fn the_installed_editors_ride_along_and_default_to_none() {
        let with = Catalog::decode(json!({
            "providers": [], "settings": {}, "availableEditors": ["zed", "file-manager"],
        }))
        .unwrap();
        assert_eq!(with.available_editors(), ["zed", "file-manager"]);
        assert!(catalog(vec![], json!({})).available_editors().is_empty());
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
            message: None,
            models: Vec::new(),
            usage_limits: None,
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
    fn unavailable_notes_follow_describe_unavailable_instance() {
        let note = |status: &str, message: Option<&str>| {
            let provider = Provider {
                instance_id: "claudeAgent".into(),
                driver: "claudeAgent".into(),
                display_name: None,
                accent_color: None,
                status: status.into(),
                availability: None,
                message: message.map(str::to_owned),
                models: Vec::new(),
                usage_limits: None,
            };
            unavailable_note("Claude", &provider)
        };
        assert_eq!(
            note("disabled", Some("ignored")),
            "Claude: Disabled in settings."
        );
        assert_eq!(note("error", None), "Claude: Unavailable.");
        assert_eq!(
            note("warning", Some(" Rate limited ")),
            "Claude: Limited. Rate limited"
        );
        assert_eq!(note("pending", Some("  ")), "Claude: Not ready.");
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
        assert_eq!(catalog.trigger(None).unwrap().mark.driver, "claudeAgent");
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
        assert_eq!(catalog.trigger(None).unwrap().mark.driver, "claudeAgent");
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
        assert_eq!(trigger.mark.driver, "codex");
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
        assert_eq!(
            enabled.trigger(Some("p")).unwrap().mark.driver,
            "claudeAgent"
        );
        assert_eq!(enabled.trigger(None).unwrap().mark.driver, "codex");
        let disabled = catalog(
            providers,
            json!({"projectSettingsOverrides": {"p": {
                "defaultModelSelection": {"instanceId": "claudeAgent", "model": "b"},
                "providerInstances": {"claudeAgent": {"enabled": false}}
            }}}),
        );
        assert_eq!(disabled.trigger(Some("p")).unwrap().mark.driver, "codex");
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

    fn two_instances() -> Catalog {
        catalog(
            vec![
                provider(
                    "codex",
                    "codex",
                    "ready",
                    vec![
                        model("gpt-5", "GPT-5", true),
                        model("gpt-5-mini", "GPT-5 Mini", false),
                    ],
                ),
                provider(
                    "claudeAgent",
                    "claudeAgent",
                    "ready",
                    vec![model("opus", "Claude Opus", true)],
                ),
            ],
            json!({"providerInstances": {"codex": {"enabled": true}, "claudeAgent": {"enabled": true}}}),
        )
    }

    #[test]
    fn a_pick_replaces_the_default_trigger() {
        let catalog = two_instances();
        let default = catalog.trigger(None).unwrap();
        assert_eq!(default.instance_id, "codex");
        assert_eq!(default.slug, "gpt-5");
        let pick = catalog.pick(None, "claudeAgent", "opus").unwrap();
        let picked = catalog.trigger_for(None, Some(&pick)).unwrap();
        assert_eq!(
            (picked.instance_id.as_str(), picked.slug.as_str()),
            ("claudeAgent", "opus")
        );
    }

    #[test]
    fn a_pick_that_no_longer_resolves_is_rejected_or_ignored() {
        let catalog = two_instances();
        assert_eq!(catalog.pick(None, "claudeAgent", "nope"), None);
        assert_eq!(catalog.pick(None, "ghost", "opus"), None);
        let stale = Pick {
            instance_id: "ghost".to_owned(),
            model: "x".to_owned(),
            options: Vec::new(),
        };
        assert_eq!(
            catalog.trigger_for(None, Some(&stale)),
            catalog.trigger(None)
        );
    }

    fn reasoning_model() -> Value {
        json!({
            "slug": "gpt-6", "name": "GPT-6", "isCustom": false, "isDefault": true,
            "capabilities": {"optionDescriptors": [
                {"id": "reasoningEffort", "label": "Reasoning", "type": "select", "options": [
                    {"id": "low", "label": "Low"},
                    {"id": "medium", "label": "Medium", "isDefault": true},
                    {"id": "high", "label": "High"},
                    {"id": "ultrathink", "label": "Ultrathink"},
                ], "promptInjectedValues": ["ultrathink"]},
                {"id": "fastMode", "label": "Fast Mode", "type": "boolean"},
                {"id": "contextWindow", "label": "Context", "type": "select", "options": [
                    {"id": "200k", "label": "200k", "isDefault": true},
                    {"id": "1m", "label": "1M"},
                ]},
            ]},
        })
    }

    fn reasoning_catalog(favorites: Value) -> Catalog {
        catalog(
            vec![provider("codex", "codex", "ready", vec![reasoning_model()])],
            json!({"favorites": favorites}),
        )
    }

    #[test]
    fn the_effort_column_lists_levels_and_marks_the_default_and_selection() {
        let trigger = reasoning_catalog(json!([])).trigger(None).unwrap();
        let effort = trigger.controls.effort.as_ref().unwrap();
        assert_eq!(trigger.controls.title(), "Reasoning");
        let levels: Vec<_> = effort
            .levels
            .iter()
            .map(|level| (level.label.as_str(), level.is_default, level.selected))
            .collect();
        assert_eq!(
            levels,
            [
                ("Low", false, false),
                ("Medium", true, true),
                ("High", false, false)
            ]
        );
        assert_eq!(trigger.effort.as_deref(), Some("Medium"));
        assert_eq!(trigger.controls.others.len(), 1);
        assert_eq!(trigger.controls.others[0].id, "contextWindow");
        let actions = trigger.controls.actions();
        assert_eq!(actions.len(), 3 + trigger.controls.others.len());
        assert_eq!(actions[0].1, effort.levels[0].value);
        let other = &trigger.controls.others[0];
        assert_eq!(
            actions[3],
            (other.id.clone(), other.next().unwrap().value.clone())
        );
    }

    #[test]
    fn fast_mode_is_a_footer_toggle_and_defaults_off_on_the_wire() {
        let catalog = reasoning_catalog(json!([]));
        let trigger = catalog.trigger(None).unwrap();
        let speed = trigger.controls.speed.as_ref().unwrap();
        let labels: Vec<_> = speed
            .levels
            .iter()
            .map(|level| level.label.as_str())
            .collect();
        assert_eq!(labels, ["Standard", "Fast"]);
        assert!(!trigger.fast);
        assert_eq!(trigger.options, [json!({"id": "fastMode", "value": false})]);

        let options = with_option(&trigger.options, "fastMode", json!(true));
        let pick = Pick {
            options: with_option(&options, "reasoningEffort", json!("high")),
            ..catalog.pick(None, "codex", "gpt-6").unwrap()
        };
        let picked = catalog.trigger_for(None, Some(&pick)).unwrap();
        assert!(picked.fast);
        assert_eq!(picked.effort.as_deref(), Some("High"));
        assert_eq!(
            picked.options,
            [
                json!({"id": "fastMode", "value": true}),
                json!({"id": "reasoningEffort", "value": "high"}),
            ]
        );
    }

    #[test]
    fn a_codex_service_tier_is_the_speed_control_and_a_bolt_marks_fast() {
        let tiers = json!({
            "slug": "gpt-6", "name": "GPT-6", "isCustom": false, "isDefault": true,
            "capabilities": {"optionDescriptors": [
                {"id": "serviceTier", "label": "Speed", "type": "select", "options": [
                    {"id": "default", "label": "Standard", "isDefault": true},
                    {"id": "fast", "label": "Fast", "description": "2x speed, increased usage"},
                ]},
            ]},
        });
        let catalog = catalog(
            vec![provider("codex", "codex", "ready", vec![tiers])],
            json!({}),
        );
        let trigger = catalog.trigger(None).unwrap();
        let speed = trigger.controls.speed.as_ref().unwrap();
        assert_eq!(
            speed.levels[1].description.as_deref(),
            Some("2x speed, increased usage")
        );
        assert!(trigger.controls.effort.is_none());
        assert_eq!(trigger.controls.title(), "Effort");
        let pick = Pick {
            options: with_option(&[], "serviceTier", json!("fast")),
            ..catalog.pick(None, "codex", "gpt-6").unwrap()
        };
        assert!(catalog.trigger_for(None, Some(&pick)).unwrap().fast);
    }

    #[test]
    fn options_a_model_does_not_have_are_not_sent() {
        let catalog = reasoning_catalog(json!([]));
        let pick = Pick {
            options: with_option(&[], "other", json!("x")),
            ..catalog.pick(None, "codex", "gpt-6").unwrap()
        };
        let sent = catalog.trigger_for(None, Some(&pick)).unwrap().options;
        assert!(sent.iter().all(|option| option["id"] != "other"));
    }

    #[test]
    fn a_model_without_options_has_empty_controls() {
        let trigger = two_instances().trigger(None).unwrap();
        assert_eq!(trigger.controls, Controls::default());
        assert!(trigger.options.is_empty());
        assert_eq!(trigger.controls.title(), "Effort");
    }

    #[test]
    fn favorites_come_from_the_server_settings_and_toggle_locally() {
        let mut starred = reasoning_catalog(json!([{"provider": "codex", "model": "gpt-6"}]));
        assert!(starred.has_favorites());
        assert!(starred.picker(None)[0].models[0].favorite);
        starred.set_favorite("codex", "gpt-6", false);
        assert!(!starred.is_favorite("codex", "gpt-6"));
        assert!(!starred.has_favorites());
        starred.set_favorite("codex", "gpt-6", true);
        starred.set_favorite("codex", "gpt-6", true);
        assert_eq!(starred.settings.favorites.as_ref().unwrap().len(), 1);
        assert!(!catalog(vec![], json!({})).has_favorites());
    }
}
