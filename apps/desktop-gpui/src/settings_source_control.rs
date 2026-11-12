//! The Source control tab (`SourceControlSettings.tsx`, `SourceControlWritingSettings.tsx` and the
//! "Repositories" rows of `ProjectDefaultsSettings.tsx`): its server settings, the discovery
//! result that `server.discoverSourceControl` returns and the text of each discovery row. The
//! panel only draws this.
//!
//! Where each key lives (packages/contracts/src/settings.ts):
//! - `defaultAutoPull`, `removeAgentCreditsOnMerge`, `pullRequestMergeMethod`,
//!   `sourceControlWritingStyle`, `reviewGuideInstructions` and the two model selections are
//!   project-scoped: a project override lives in `projectSettingsOverrides[projectId]`, and the
//!   effective value is the override, else the environment's, else the default.
//! - `reviewGuidesEnabled` is environment-wide.
//! - A writing style is an object whose three fields override one by one; the legacy string form
//!   reads as custom instructions when it is not empty.

use crate::settings::{SearchItem, Section};
use crate::settings_devices::{self as devices, Layer, LayerKind, State};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashMap};

/// Call tokens of the Source control tab's requests start here.
pub const TOKEN_BASE: u64 = 1 << 46;

const fn item(id: &'static str, title: &'static str, terms: &'static str) -> SearchItem {
    SearchItem {
        id,
        title,
        section: Section::SourceControl,
        terms,
        desktop_only: false,
    }
}

/// `SOURCE_CONTROL_HOST_LABELS` (settingsSearch.ts:211).
macro_rules! host_labels {
    () => {
        "GitHub GitLab GitCafe Forgejo Bitbucket Azure DevOps"
    };
}

/// `SOURCE_CONTROL_HOST_TERMS` (settingsSearch.ts:214): each host's label and search keywords.
macro_rules! host_terms {
    () => {
        "GitHub ghes enterprise GitLab GitCafe Forgejo gitea tea codeberg Bitbucket Azure DevOps"
    };
}

/// The tab's `searchableSetting` entries (settingsSearch.ts:950-1050) and the hosts that have a
/// settings panel (`sourceControlHostSearchItems`, settingsSearch.ts:182-205).
pub const SOURCE_CONTROL_ITEMS: &[SearchItem] = &[
    item(
        "automatic-pull",
        "Automatically pull",
        "auto pull default branch current checkout fast forward upstream",
    ),
    item(
        "remove-agent-credits-on-merge",
        "Remove agent credits when merging",
        concat!(
            "pull request ",
            host_labels!(),
            " squash co-authored-by attribution claude codex generated"
        ),
    ),
    item(
        "pull-request-merge-method",
        "Default merge method",
        "pull request merge squash rebase last selected",
    ),
    item(
        "source-control",
        "Source control",
        concat!(
            "version control git ",
            host_terms!(),
            " hosting integrations credentials scan server environment"
        ),
    ),
    item(
        "source-control-host-github",
        "GitHub accounts and token",
        "GitHub ghes enterprise gh account login user host switch multiple accounts disable sign in token personal access token pat api key credential",
    ),
    item(
        "source-control-host-gitcafe",
        "GitCafe credentials",
        "GitCafe token token credentials sign in",
    ),
    item(
        "source-control-host-bitbucket",
        "Bitbucket credentials",
        "Bitbucket accessToken email apiToken token credentials sign in",
    ),
    item(
        "source-control-writing-style",
        "Source control writing style",
        "repository conventions conventional commits custom instructions change descriptions request titles",
    ),
    item(
        "follow-change-request-templates",
        "Follow change request templates",
        "repository pr pull request description structure",
    ),
    item(
        "source-control-writer-model",
        "Source control writer model",
        "override generated commit change request pr titles descriptions branch bookmark",
    ),
    item(
        "review-guides",
        "Review guides (beta)",
        "pull request pr guided review guide tour beta experimental",
    ),
    item(
        "review-guide-model",
        "Review guide model",
        "pull request pr guided review guide brief maintainer",
    ),
    item(
        "review-guide-instructions",
        "Review guide instructions",
        "pull request pr guided review guide preferences taste prompt custom",
    ),
];

/// A server setting of the tab. The three style fields are keys of `sourceControlWritingStyle`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Field {
    AutoPull,
    AgentCredits,
    MergeMethod,
    StyleMode,
    StyleInstructions,
    FollowTemplates,
    ReviewGuides,
    ReviewInstructions,
    ReviewModel,
    WriterModel,
    /// The Projects tab's New threads model, `defaultModelSelection`.
    DefaultModel,
    /// The Projects tab's New threads workspace, `defaultThreadEnvMode`.
    ThreadEnvMode,
    /// The Projects tab's actions, `defaultProjectScripts`.
    Scripts,
}

impl Field {
    /// The key in `settings` and in a project's override entry.
    pub fn wire(self) -> &'static str {
        match self {
            Self::AutoPull => "defaultAutoPull",
            Self::AgentCredits => "removeAgentCreditsOnMerge",
            Self::MergeMethod => "pullRequestMergeMethod",
            Self::StyleMode | Self::StyleInstructions | Self::FollowTemplates => {
                "sourceControlWritingStyle"
            }
            Self::ReviewGuides => "reviewGuidesEnabled",
            Self::ReviewInstructions => "reviewGuideInstructions",
            Self::ReviewModel => "reviewGuideModelSelection",
            Self::WriterModel => "sourceControlWriterModelSelection",
            Self::DefaultModel => "defaultModelSelection",
            Self::ThreadEnvMode => "defaultThreadEnvMode",
            Self::Scripts => "defaultProjectScripts",
        }
    }

    /// The key inside the writing style object.
    fn sub(self) -> Option<&'static str> {
        match self {
            Self::StyleMode => Some("mode"),
            Self::StyleInstructions => Some("customInstructions"),
            Self::FollowTemplates => Some("followChangeRequestTemplates"),
            _ => None,
        }
    }

    /// `isProjectScopedSettingKey`: every key but `reviewGuidesEnabled`.
    pub fn project_scoped(self) -> bool {
        !matches!(self, Self::ReviewGuides)
    }

    /// `DEFAULT_SERVER_SETTINGS`.
    pub fn default_value(self) -> Value {
        match self {
            Self::AutoPull | Self::AgentCredits | Self::ReviewGuides => json!(false),
            Self::FollowTemplates => json!(true),
            Self::StyleMode => json!("repo_conventions"),
            Self::StyleInstructions | Self::ReviewInstructions => json!(""),
            Self::MergeMethod | Self::ReviewModel | Self::WriterModel | Self::DefaultModel => {
                Value::Null
            }
            Self::ThreadEnvMode => json!("local"),
            Self::Scripts => json!([]),
        }
    }

    const ALL: [Self; 13] = [
        Self::AutoPull,
        Self::AgentCredits,
        Self::MergeMethod,
        Self::StyleMode,
        Self::StyleInstructions,
        Self::FollowTemplates,
        Self::ReviewGuides,
        Self::ReviewInstructions,
        Self::ReviewModel,
        Self::WriterModel,
        Self::DefaultModel,
        Self::ThreadEnvMode,
        Self::Scripts,
    ];
}

/// The keys of `server.getConfig`'s `settings` that the tab reads, kept as the server sent them
/// so that a project override write carries the other keys of the entry along.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SourceControlConfig {
    environment: Map<String, Value>,
    overrides: HashMap<String, Map<String, Value>>,
    /// `sourceControlHosts`: each provider's token and host settings.
    hosts: Map<String, Value>,
    secret_sources: Vec<crate::secret_source::SecretSource>,
}

const TEXT_GENERATION: &str = "textGenerationModelSelection";

/// `createModelSelection`: the instance, the model and the options when there are any.
fn selection_of(value: &Value) -> Option<Value> {
    let instance = value.get("instanceId")?.as_str()?;
    let model = value.get("model")?.as_str()?;
    let mut selection = json!({"instanceId": instance, "model": model});
    if let Some(options) = value
        .get("options")
        .filter(|o| o.as_array().is_some_and(|a| !a.is_empty()))
    {
        selection["options"] = options.clone();
    }
    Some(selection)
}

/// The model of a selection, for a row's value.
pub fn model_label(value: &Value) -> Option<String> {
    value.get("model")?.as_str().map(str::to_owned)
}

fn style_part(value: &Value, field: Field) -> Option<Value> {
    let sub = field.sub()?;
    match value {
        Value::Object(style) => style.get(sub).cloned(),
        Value::String(legacy) => Some(match field {
            Field::StyleMode => json!(if legacy.trim().is_empty() {
                "repo_conventions"
            } else {
                "custom"
            }),
            Field::StyleInstructions => json!(legacy),
            _ => json!(true),
        }),
        _ => None,
    }
}

fn read(map: &Map<String, Value>, field: Field) -> Option<Value> {
    let value = map.get(field.wire())?;
    if field.sub().is_some() {
        style_part(value, field)
    } else {
        Some(value.clone())
    }
}

/// `value` of `None` removes the key (and an emptied style object).
fn write(map: &mut Map<String, Value>, field: Field, value: Option<Value>) {
    let Some(sub) = field.sub() else {
        match value {
            Some(value) => map.insert(field.wire().to_owned(), value),
            None => map.remove(field.wire()),
        };
        return;
    };
    let mut style = match map.remove(field.wire()) {
        Some(Value::Object(style)) => style,
        Some(Value::String(legacy)) => {
            let mut style = Map::new();
            if let Some(mode) = style_part(&Value::String(legacy.clone()), Field::StyleMode) {
                style.insert("mode".to_owned(), mode);
            }
            style.insert("customInstructions".to_owned(), json!(legacy));
            style
        }
        _ => Map::new(),
    };
    match value {
        Some(value) => {
            style.insert(sub.to_owned(), value);
        }
        None => {
            style.remove(sub);
        }
    }
    if !style.is_empty() {
        map.insert(field.wire().to_owned(), Value::Object(style));
    }
}

impl SourceControlConfig {
    /// Reads the tab's keys out of a `settings` object; anything else gives the defaults.
    pub fn from_settings(settings: &Value) -> Self {
        let mut environment = Map::new();
        for key in Field::ALL
            .iter()
            .map(|field| field.wire())
            .chain([TEXT_GENERATION])
        {
            if let Some(value) = settings.get(key) {
                environment.insert(key.to_owned(), value.clone());
            }
        }
        let overrides = match settings.get("projectSettingsOverrides") {
            Some(Value::Object(entries)) => entries
                .iter()
                .filter_map(|(id, entry)| Some((id.clone(), entry.as_object()?.clone())))
                .collect(),
            _ => HashMap::new(),
        };
        Self {
            environment,
            overrides,
            hosts: settings
                .get("sourceControlHosts")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default(),
            secret_sources: Vec::new(),
        }
    }

    /// The secret providers a setting can read from, once the environment reads references.
    pub fn with_secret_sources(mut self, settings: &Value, references: bool) -> Self {
        self.secret_sources = crate::secret_source::sources(settings, references);
        self
    }

    pub fn secret_sources(&self) -> &[crate::secret_source::SecretSource] {
        &self.secret_sources
    }

    /// One saved key of a provider's host settings (`sourceControlHosts[kind][key]`).
    pub fn host_value(&self, kind: &str, key: &str) -> Value {
        self.hosts
            .get(kind)
            .and_then(|entry| entry.get(key))
            .cloned()
            .unwrap_or(Value::Null)
    }

    /// `readHostCredentialSettings`: the saved token of each host.
    pub fn tokens(&self, kind: &str) -> Vec<(String, Value)> {
        let mut tokens: Vec<_> = self
            .host_value(kind, "tokens")
            .as_object()
            .map(|map| map.iter().map(|(h, v)| (h.clone(), v.clone())).collect())
            .unwrap_or_default();
        tokens.sort_by(|a, b| a.0.cmp(&b.0));
        tokens
    }

    /// `readHostCredentialSettings`: which hosts are on and the login each is pinned to.
    pub fn host_choices(&self, kind: &str) -> BTreeMap<String, HostChoice> {
        self.host_value(kind, "hosts")
            .as_object()
            .map(|map| {
                map.iter()
                    .map(|(host, entry)| {
                        (
                            host.clone(),
                            HostChoice {
                                enabled: entry["enabled"].as_bool().unwrap_or(true),
                                account: text(&entry["account"]),
                            },
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The `settings.update` body for the keys of one provider's host settings.
    pub fn host_patch(kind: &str, fields: &Map<String, Value>) -> Value {
        json!({"patch": {"sourceControlHosts": {kind: fields}}})
    }

    /// Applies a host patch as the server would, so the tab shows it before the reply: an empty
    /// string removes the key, and a typed secret shows as its redaction marker.
    pub fn apply_host(&mut self, kind: &str, fields: &Map<String, Value>) {
        let entry = self
            .hosts
            .entry(kind.to_owned())
            .or_insert_with(|| Value::Object(Map::new()));
        let Some(entry) = entry.as_object_mut() else {
            return;
        };
        for (key, value) in fields {
            match value {
                Value::String(text) if text.is_empty() => {
                    entry.remove(key);
                }
                Value::Object(map) => {
                    let kept = map
                        .iter()
                        .filter(|(_, v)| v.as_str() != Some(""))
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                    entry.insert(key.clone(), Value::Object(kept));
                }
                value => {
                    entry.insert(key.clone(), value.clone());
                }
            }
        }
    }

    /// The environment's value, or the default when the server has none.
    pub fn environment_value(&self, field: Field) -> Value {
        read(&self.environment, field).unwrap_or_else(|| field.default_value())
    }

    /// Whether the environment holds a value other than the default (the layer reads as set).
    pub fn environment_set(&self, field: Field) -> bool {
        read(&self.environment, field).is_some_and(|value| value != field.default_value())
    }

    /// Every action id any layer holds, which a new action's id must not repeat.
    pub fn script_ids(&self) -> Vec<String> {
        let key = Field::Scripts.wire();
        std::iter::once(&self.environment)
            .chain(self.overrides.values())
            .filter_map(|map| map.get(key)?.as_array())
            .flatten()
            .filter_map(|script| script.get("id")?.as_str().map(str::to_owned))
            .collect()
    }

    /// The first project of the group that overrides the key.
    pub fn project_override(&self, field: Field, members: &[String]) -> Option<Value> {
        if !field.project_scoped() {
            return None;
        }
        members
            .iter()
            .find_map(|id| read(self.overrides.get(id)?, field))
    }

    /// `resolveProjectSettings` for one key: the override, else the environment's value.
    pub fn effective(&self, field: Field, members: &[String], in_project: bool) -> Value {
        if in_project && let Some(value) = self.project_override(field, members) {
            return value;
        }
        self.environment_value(field)
    }

    /// The text generation model the writer and review guide models start from: the project's
    /// override, else the environment's `textGenerationModelSelection`, reduced to what a model
    /// selection carries.
    pub fn text_generation(&self, members: &[String], in_project: bool) -> Option<Value> {
        let project = in_project
            .then(|| {
                members
                    .iter()
                    .find_map(|id| self.overrides.get(id)?.get(TEXT_GENERATION))
            })
            .flatten();
        let selection = project.or_else(|| self.environment.get(TEXT_GENERATION))?;
        selection_of(selection)
    }

    /// Puts the change in the environment's value, or in the entry of every member.
    pub fn apply(&mut self, changes: &[(Field, Option<Value>)], members: Option<&[String]>) {
        for (field, value) in changes {
            match members {
                Some(members) if field.project_scoped() => {
                    for id in members {
                        write(
                            self.overrides.entry(id.clone()).or_default(),
                            *field,
                            value.clone(),
                        );
                    }
                }
                _ => write(
                    &mut self.environment,
                    *field,
                    Some(value.clone().unwrap_or_else(|| field.default_value())),
                ),
            }
        }
    }

    /// The `server.updateSettings` payload for the changes, as `apply` would make them. An
    /// environment write sends only the changed keys; a project write replaces each entry.
    pub fn patch(&self, changes: &[(Field, Option<Value>)], members: Option<&[String]>) -> Value {
        match members {
            Some(members) if changes.iter().all(|(field, _)| field.project_scoped()) => {
                let mut after = self.clone();
                after.apply(changes, Some(members));
                devices::merge_override_patches(
                    members
                        .iter()
                        .map(|id| {
                            let entry = match after.overrides.get(id) {
                                Some(entry) if !entry.is_empty() => Value::Object(entry.clone()),
                                _ => Value::Null,
                            };
                            json!({"patch": {"projectSettingsOverrides": {id: entry}}})
                        })
                        .collect(),
                )
            }
            _ => {
                let mut patch = Map::new();
                for (field, value) in changes {
                    let value = value.clone().unwrap_or_else(|| field.default_value());
                    match field.sub() {
                        Some(sub) => {
                            let style = patch
                                .entry(field.wire().to_owned())
                                .or_insert_with(|| json!({}));
                            style[sub] = value;
                        }
                        None => {
                            patch.insert(field.wire().to_owned(), value);
                        }
                    }
                }
                json!({"patch": patch})
            }
        }
    }
}

/// Why a control is inert, or that its write reaches the attached environment
/// (settingsLayout.tsx:405-448): the same rule as the Devices tab's two switches.
pub fn reach(
    field: Field,
    search: &crate::settings_labs::ScopeSearch,
    data: &crate::settings_labs::ScopeData,
) -> Result<(), String> {
    devices::reach(proxy(field), search, data)
}

fn proxy(field: Field) -> devices::Key {
    if field.project_scoped() {
        devices::Key::Agent
    } else {
        devices::Key::Hub
    }
}

/// `mergeMethodLabel`: the select's options, "Last selected" first.
pub const MERGE_METHODS: [(&str, &str); 4] = [
    ("last", "Last selected"),
    ("merge", "Merge"),
    ("squash", "Squash and merge"),
    ("rebase", "Rebase and merge"),
];

pub const STYLE_MODES: [(&str, &str); 3] = [
    ("repo_conventions", "Repository conventions"),
    ("conventional_commits", "Conventional Commits"),
    ("custom", "Custom instructions"),
];

/// The index of a `pullRequestMergeMethod` value in `MERGE_METHODS`.
pub fn merge_index(value: &Value) -> usize {
    let wire = value.as_str().unwrap_or("last");
    MERGE_METHODS
        .iter()
        .position(|(key, _)| *key == wire)
        .unwrap_or(0)
}

pub fn style_index(value: &Value) -> usize {
    let wire = value.as_str().unwrap_or("repo_conventions");
    STYLE_MODES
        .iter()
        .position(|(key, _)| *key == wire)
        .unwrap_or(0)
}

/// What the picked merge option writes: "Last selected" is `null`.
pub fn merge_value(index: usize) -> Value {
    match MERGE_METHODS[index].0 {
        "last" => Value::Null,
        method => json!(method),
    }
}

/// The description under the writing style row.
pub fn style_description(mode: &str) -> &'static str {
    match mode {
        "conventional_commits" => {
            "Use Conventional Commit prefixes and keep change request text concise."
        }
        "custom" => {
            "Use your instructions for change descriptions and change requests in every project."
        }
        _ => "In each project, matches recent change descriptions and change request titles.",
    }
}

/// The value of a layer row.
/// `resolveEnvModeLabel`.
pub fn env_mode_label(mode: &str) -> &'static str {
    if mode == "worktree" {
        "New worktree"
    } else {
        "Local"
    }
}

fn describe(field: Field, value: &Value) -> String {
    match field {
        Field::MergeMethod => MERGE_METHODS[merge_index(value)].1.to_owned(),
        Field::StyleMode => STYLE_MODES[style_index(value)].1.to_owned(),
        Field::ThreadEnvMode => env_mode_label(value.as_str().unwrap_or("local")).to_owned(),
        Field::Scripts => match value.as_array().map_or(0, Vec::len) {
            1 => "1 action".to_owned(),
            count => format!("{count} actions"),
        },
        Field::DefaultModel if value.is_null() => "Automatic".to_owned(),
        _ => match value {
            Value::Bool(true) => "On".to_owned(),
            Value::Bool(false) => "Off".to_owned(),
            Value::String(text) if text.is_empty() => "Off".to_owned(),
            Value::String(text) => text.clone(),
            Value::Object(_) => model_label(value).unwrap_or_else(|| "On".to_owned()),
            Value::Null => "Off".to_owned(),
            _ => "Inherits".to_owned(),
        },
    }
}

/// `settingInheritanceLayers` (SettingInheritance.tsx:105-147) for a key with any default.
pub fn layers(
    field: Field,
    environment_label: &str,
    in_project: bool,
    project_override: Option<&Value>,
    config: &SourceControlConfig,
) -> Vec<Layer> {
    let from_project = field.project_scoped() && project_override.is_some();
    let environment_set = config.environment_set(field);
    let mut out = Vec::new();
    if in_project && field.project_scoped() {
        out.push(Layer {
            kind: LayerKind::Project,
            label: "Project".to_owned(),
            value: project_override
                .map_or_else(|| "Inherits".to_owned(), |value| describe(field, value)),
            effective: from_project,
            set: from_project,
        });
    }
    out.push(Layer {
        kind: LayerKind::Environment,
        label: environment_label.to_owned(),
        value: if environment_set {
            describe(field, &config.environment_value(field))
        } else {
            "Inherits".to_owned()
        },
        effective: !from_project && environment_set,
        set: environment_set,
    });
    out.push(Layer {
        kind: LayerKind::Default,
        label: "Default".to_owned(),
        value: describe(field, &field.default_value()),
        effective: !from_project && !environment_set,
        set: true,
    });
    out
}

/// The tooltip and marker state of a row (settingsLayout.tsx:381-395).
pub fn inheritance(
    field: Field,
    in_project: bool,
    has_override: bool,
    environment_set: bool,
    environment_name: &str,
) -> (State, String) {
    devices::inheritance(
        proxy(field),
        in_project,
        has_override.then_some(true),
        environment_set,
        environment_name,
    )
}

/// `NullOr(String)` in the wire's two shapes: a plain string or null, and effect's `Option`
/// (`{"_tag":"Some","value":...}`).
fn text(value: &Value) -> Option<String> {
    let value = match value {
        Value::Object(option) if option.get("_tag").and_then(Value::as_str) == Some("Some") => {
            option.get("value")?
        }
        other => other,
    };
    value
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthStatus {
    Authenticated,
    Unauthenticated,
    Unknown,
}

/// `SourceControlProviderAuth` (sourceControl.ts:115-135), reduced to what a row reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Auth {
    pub status: AuthStatus,
    pub account: Option<String>,
    pub detail: Option<String>,
    pub accounts: Vec<AuthAccount>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthAccount {
    pub host: String,
    pub account: String,
    pub active: bool,
    pub authenticated: bool,
    pub environment_variable: Option<String>,
    pub error: Option<String>,
}

/// Whether the provider has an expanded panel (`definition.settings` in the web's registry).
pub fn has_host_settings(kind: &str) -> bool {
    matches!(kind, "github" | "gitcafe" | "bitbucket")
}

/// `HostCredentialSettings["hosts"][host]`: whether the host is on, and the login it is pinned to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostChoice {
    pub enabled: bool,
    pub account: Option<String>,
}

/// `HostAccountGroup` (SourceControlAccountSettings.logic.ts).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostAccountGroup {
    pub host: String,
    pub active_account: Option<String>,
    pub selectable: Vec<String>,
    pub broken: Vec<AuthAccount>,
    pub environment_variable: Option<String>,
}

/// `groupHostAccounts`.
pub fn group_host_accounts(accounts: &[AuthAccount]) -> Vec<HostAccountGroup> {
    let mut hosts: Vec<&str> = Vec::new();
    for entry in accounts {
        if !hosts.contains(&entry.host.as_str()) {
            hosts.push(&entry.host);
        }
    }
    hosts
        .into_iter()
        .map(|host| {
            let entries: Vec<&AuthAccount> = accounts.iter().filter(|e| e.host == host).collect();
            let stored: Vec<&&AuthAccount> = entries
                .iter()
                .filter(|e| e.environment_variable.is_none())
                .collect();
            let selectable: Vec<&&AuthAccount> =
                stored.iter().copied().filter(|e| e.authenticated).collect();
            HostAccountGroup {
                host: host.to_owned(),
                active_account: selectable
                    .iter()
                    .find(|e| e.active)
                    .or(selectable.first())
                    .map(|e| e.account.clone()),
                selectable: selectable.iter().map(|e| e.account.clone()).collect(),
                broken: stored
                    .iter()
                    .filter(|e| !e.authenticated)
                    .map(|e| (***e).clone())
                    .collect(),
                environment_variable: entries.iter().find_map(|e| e.environment_variable.clone()),
            }
        })
        .collect()
}

/// `nextHostAccountChoices`: a host that is on and follows the active login is not stored.
pub fn next_host_account_choices(
    current: &BTreeMap<String, HostChoice>,
    host: &str,
    enabled: Option<bool>,
    account: Option<Option<&str>>,
) -> BTreeMap<String, HostChoice> {
    let previous = current.get(host);
    let enabled = enabled.or(previous.map(|p| p.enabled)).unwrap_or(true);
    let account = match account {
        None => previous.and_then(|p| p.account.clone()),
        Some(next) => next.map(str::to_owned),
    };
    let mut rest = current.clone();
    rest.remove(host);
    if !(enabled && account.is_none()) {
        rest.insert(host.to_owned(), HostChoice { enabled, account });
    }
    rest
}

/// The `hosts` map as the settings patch carries it.
pub fn host_choices_value(choices: &BTreeMap<String, HostChoice>) -> Value {
    Value::Object(
        choices
            .iter()
            .map(|(host, choice)| {
                let mut entry = Map::new();
                entry.insert("enabled".into(), json!(choice.enabled));
                if let Some(account) = &choice.account {
                    entry.insert("account".into(), json!(account));
                }
                (host.clone(), Value::Object(entry))
            })
            .collect(),
    )
}

/// A `VcsDiscoveryItem` or a `SourceControlProviderDiscoveryItem`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub kind: String,
    pub label: String,
    pub executable: Option<String>,
    pub available: bool,
    pub version: Option<String>,
    pub install_hint: String,
    /// A version control system the server does not drive yet (`implemented: false`).
    pub implemented: bool,
    /// Present for a provider.
    pub auth: Option<Auth>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Discovery {
    pub version_control: Vec<Item>,
    pub providers: Vec<Item>,
}

impl Discovery {
    pub fn is_empty(&self) -> bool {
        self.version_control.is_empty() && self.providers.is_empty()
    }
}

fn item_of(value: &Value) -> Option<Item> {
    let auth = value.get("auth").map(|auth| Auth {
        status: match auth["status"].as_str() {
            Some("authenticated") => AuthStatus::Authenticated,
            Some("unauthenticated") => AuthStatus::Unauthenticated,
            _ => AuthStatus::Unknown,
        },
        account: text(&auth["account"]),
        detail: text(&auth["detail"]),
        accounts: auth["accounts"]
            .as_array()
            .map(|accounts| {
                accounts
                    .iter()
                    .filter_map(|entry| {
                        Some(AuthAccount {
                            host: entry["host"].as_str()?.to_owned(),
                            account: entry["account"].as_str()?.to_owned(),
                            active: entry["active"].as_bool().unwrap_or(false),
                            authenticated: entry["authenticated"].as_bool().unwrap_or(false),
                            environment_variable: text(&entry["environmentVariable"]),
                            error: text(&entry["error"]),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),
    });
    Some(Item {
        kind: value["kind"].as_str()?.to_owned(),
        label: value["label"].as_str()?.to_owned(),
        executable: text(&value["executable"]),
        available: value["status"].as_str() == Some("available"),
        version: text(&value["version"]),
        install_hint: value["installHint"].as_str().unwrap_or_default().to_owned(),
        implemented: value["implemented"].as_bool().unwrap_or(true),
        auth,
    })
}

impl Discovery {
    /// Reads a `SourceControlDiscoveryResult`; an item that does not decode is dropped.
    pub fn parse(value: &Value) -> Self {
        let items = |key: &str| -> Vec<Item> {
            value[key]
                .as_array()
                .map(|items| items.iter().filter_map(item_of).collect())
                .unwrap_or_default()
        };
        Self {
            version_control: items("versionControlSystems"),
            providers: items("sourceControlProviders"),
        }
    }
}

impl Item {
    /// `isVcsNotReady`.
    pub fn not_ready(&self) -> bool {
        self.auth.is_none() && !self.implemented
    }

    /// The switch of the row, which only reports (`enabled` in `DiscoveryItemRow`).
    pub fn enabled(&self) -> bool {
        match &self.auth {
            Some(auth) => self.available && auth.status == AuthStatus::Authenticated,
            None => self.available && self.implemented,
        }
    }

    pub fn dot(&self) -> Dot {
        if self.not_ready() {
            return Dot::Muted;
        }
        match &self.auth {
            Some(auth) if self.available && auth.status == AuthStatus::Authenticated => {
                Dot::Success
            }
            Some(_) => Dot::Muted,
            None if !self.available => Dot::Warning,
            None => Dot::Success,
        }
    }

    /// The badge after the version: "Coming Soon", or "Not authenticated".
    pub fn badge(&self) -> Option<Badge> {
        if self.not_ready() {
            return Some(Badge::ComingSoon);
        }
        (self.auth.as_ref()?.status == AuthStatus::Unauthenticated)
            .then_some(Badge::NotAuthenticated)
    }

    /// `itemSummary`: one piece per flex child of the row's summary line.
    pub fn summary(&self) -> Vec<Span> {
        let text = |text: String| vec![Span::Text(text)];
        if self.not_ready() {
            return text(format!("Support for {} is coming soon.", self.label));
        }
        if !self.available {
            return text(format!(
                "Not available on this server: {}",
                self.install_hint
            ));
        }
        let Some(auth) = &self.auth else {
            return text("Available".to_owned());
        };
        match auth.status {
            AuthStatus::Authenticated => {
                let mut spans = vec![Span::Text("Authenticated".to_owned())];
                if let Some(account) = &auth.account {
                    spans.push(Span::Text("as".to_owned()));
                    spans.push(Span::Account(account.clone()));
                }
                if let Some(detail) = &auth.detail {
                    spans.push(Span::Text(format!("\u{b7} {detail}")));
                }
                spans
            }
            AuthStatus::Unauthenticated if self.executable.is_none() => {
                text(format!("Available. {}", self.install_hint))
            }
            AuthStatus::Unauthenticated if auth.accounts.iter().any(|a| a.authenticated) => text(
                auth.detail
                    .clone()
                    .unwrap_or_else(|| format!("Every {} host is turned off.", self.label)),
            ),
            AuthStatus::Unauthenticated => vec![
                Span::Text(format!(
                    "{} is not authenticated on this server. Sign in or configure credentials using the",
                    self.label
                )),
                Span::Code(self.executable.clone().unwrap_or_default()),
                Span::Text("tool on the server host to enable change request features.".to_owned()),
            ],
            AuthStatus::Unknown => text(format!(
                "Could not verify {}. {}",
                self.label,
                auth.detail.as_deref().unwrap_or(&self.install_hint)
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dot {
    Success,
    Warning,
    Muted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Badge {
    ComingSoon,
    NotAuthenticated,
}

impl Badge {
    pub fn label(self) -> &'static str {
        match self {
            Self::ComingSoon => "Coming Soon",
            Self::NotAuthenticated => "Not authenticated",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Span {
    Text(String),
    Code(String),
    /// An account name, redacted until clicked.
    Account(String),
}

/// `redactedPlaceholder` (RedactedSensitiveText.tsx:7-24): the same letters for the same name.
pub fn redacted(value: &str) -> String {
    const ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
    let mut state: i32 = 0x811c9dc5_u32 as i32;
    for unit in value.encode_utf16() {
        state ^= i32::from(unit);
        state = state.wrapping_mul(0x0100_0193);
    }
    let mut next = || {
        state = (state ^ ((state as u32) >> 13) as i32).wrapping_mul(0x85eb_ca6b_u32 as i32);
        state = (state ^ ((state as u32) >> 16) as i32).wrapping_mul(0xc2b2_ae35_u32 as i32);
        ALPHABET[(i64::from(state).unsigned_abs() % ALPHABET.len() as u64) as usize] as char
    };
    value
        .chars()
        .map(|c| {
            if matches!(c, '@' | '.' | '-' | '_') {
                c
            } else {
                next()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(members: &[&str]) -> Vec<String> {
        members.iter().map(|id| (*id).to_owned()).collect()
    }

    #[test]
    fn defaults_when_the_server_sends_nothing() {
        let config = SourceControlConfig::from_settings(&json!({}));
        assert_eq!(config.environment_value(Field::AutoPull), json!(false));
        assert_eq!(config.environment_value(Field::MergeMethod), Value::Null);
        assert_eq!(
            config.environment_value(Field::FollowTemplates),
            json!(true)
        );
        assert_eq!(
            config.environment_value(Field::StyleMode),
            json!("repo_conventions")
        );
        assert!(!config.environment_set(Field::FollowTemplates));
    }

    #[test]
    fn a_legacy_style_string_reads_as_custom_instructions() {
        let config =
            SourceControlConfig::from_settings(&json!({"sourceControlWritingStyle": "Be brief."}));
        assert_eq!(config.environment_value(Field::StyleMode), json!("custom"));
        assert_eq!(
            config.environment_value(Field::StyleInstructions),
            json!("Be brief.")
        );
        let empty = SourceControlConfig::from_settings(&json!({"sourceControlWritingStyle": " "}));
        assert_eq!(
            empty.environment_value(Field::StyleMode),
            json!("repo_conventions")
        );
    }

    #[test]
    fn a_project_override_wins_and_style_fields_override_one_by_one() {
        let config = SourceControlConfig::from_settings(&json!({
            "defaultAutoPull": true,
            "sourceControlWritingStyle": {"mode": "conventional_commits", "customInstructions": "x", "followChangeRequestTemplates": true},
            "projectSettingsOverrides": {"p1": {"defaultAutoPull": false, "sourceControlWritingStyle": {"mode": "custom"}}}
        }));
        let members = ids(&["p1"]);
        assert_eq!(
            config.effective(Field::AutoPull, &members, true),
            json!(false)
        );
        assert_eq!(
            config.effective(Field::AutoPull, &members, false),
            json!(true)
        );
        assert_eq!(
            config.effective(Field::StyleMode, &members, true),
            json!("custom")
        );
        assert_eq!(
            config.effective(Field::StyleInstructions, &members, true),
            json!("x")
        );
    }

    #[test]
    fn a_null_override_is_an_override_to_last_selected() {
        let config = SourceControlConfig::from_settings(&json!({
            "pullRequestMergeMethod": "squash",
            "projectSettingsOverrides": {"p1": {"pullRequestMergeMethod": null}}
        }));
        let members = ids(&["p1"]);
        assert_eq!(
            config.project_override(Field::MergeMethod, &members),
            Some(Value::Null)
        );
        assert_eq!(
            config.effective(Field::MergeMethod, &members, true),
            Value::Null
        );
        assert_eq!(
            config.effective(Field::MergeMethod, &[], false),
            json!("squash")
        );
    }

    #[test]
    fn an_environment_write_sends_only_the_changed_style_field() {
        let config = SourceControlConfig::default();
        let patch = config.patch(
            &[
                (Field::StyleMode, Some(json!("custom"))),
                (Field::AutoPull, Some(json!(true))),
            ],
            None,
        );
        assert_eq!(
            patch,
            json!({"patch": {"sourceControlWritingStyle": {"mode": "custom"}, "defaultAutoPull": true}})
        );
    }

    #[test]
    fn resetting_in_the_environment_writes_the_default() {
        let config = SourceControlConfig::default();
        let patch = config.patch(
            &[(Field::MergeMethod, None), (Field::FollowTemplates, None)],
            None,
        );
        assert_eq!(
            patch,
            json!({"patch": {"pullRequestMergeMethod": null, "sourceControlWritingStyle": {"followChangeRequestTemplates": true}}})
        );
    }

    #[test]
    fn a_project_write_replaces_the_entry_and_keeps_its_other_keys() {
        let config = SourceControlConfig::from_settings(&json!({
            "projectSettingsOverrides": {"p1": {"enableAgentDeviceAccess": true}}
        }));
        let patch = config.patch(&[(Field::AutoPull, Some(json!(true)))], Some(&ids(&["p1"])));
        assert_eq!(
            patch,
            json!({"patch": {"projectSettingsOverrides": {"p1": {"enableAgentDeviceAccess": true, "defaultAutoPull": true}}}})
        );
    }

    #[test]
    fn clearing_the_last_override_of_a_project_writes_null() {
        let config = SourceControlConfig::from_settings(&json!({
            "projectSettingsOverrides": {"p1": {"defaultAutoPull": true}}
        }));
        let patch = config.patch(&[(Field::AutoPull, None)], Some(&ids(&["p1"])));
        assert_eq!(
            patch,
            json!({"patch": {"projectSettingsOverrides": {"p1": null}}})
        );
    }

    #[test]
    fn an_environment_wide_key_ignores_the_project() {
        let mut config = SourceControlConfig::default();
        config.apply(
            &[(Field::ReviewGuides, Some(json!(true)))],
            Some(&ids(&["p1"])),
        );
        assert_eq!(config.environment_value(Field::ReviewGuides), json!(true));
        assert_eq!(
            config.project_override(Field::ReviewGuides, &ids(&["p1"])),
            None
        );
    }

    #[test]
    fn apply_puts_style_fields_in_the_override_entry() {
        let mut config = SourceControlConfig::default();
        let members = ids(&["p1"]);
        config.apply(&[(Field::StyleMode, Some(json!("custom")))], Some(&members));
        assert_eq!(
            config.project_override(Field::StyleMode, &members),
            Some(json!("custom"))
        );
        config.apply(&[(Field::StyleMode, None)], Some(&members));
        assert_eq!(config.project_override(Field::StyleMode, &members), None);
    }

    #[test]
    fn merge_options_map_to_wire_values() {
        assert_eq!(merge_value(0), Value::Null);
        assert_eq!(merge_value(2), json!("squash"));
        assert_eq!(merge_index(&Value::Null), 0);
        assert_eq!(merge_index(&json!("rebase")), 3);
        assert_eq!(style_index(&json!("custom")), 2);
    }

    #[test]
    fn layers_mark_the_effective_one() {
        let config = SourceControlConfig::from_settings(&json!({"defaultAutoPull": true}));
        let layers = layers(Field::AutoPull, "Mac", true, None, &config);
        assert_eq!(layers.len(), 3);
        assert!(layers[1].effective && layers[1].value == "On");
        assert!(!layers[2].effective && layers[2].value == "Off");
        let (state, _) = inheritance(Field::AutoPull, true, false, true, "Mac");
        assert_eq!(state, State::Inherited);
    }

    fn discovery() -> Discovery {
        Discovery::parse(&json!({
            "versionControlSystems": [
                {"kind": "git", "label": "Git", "executable": "git", "status": "available", "version": "2.51.0", "installHint": "Install Git", "implemented": true},
                {"kind": "jj", "label": "Jujutsu", "executable": null, "status": "missing", "version": {"_tag": "None"}, "installHint": "Install jj", "implemented": false}
            ],
            "sourceControlProviders": [
                {"kind": "github", "label": "GitHub", "executable": "gh", "status": "available", "version": {"_tag": "Some", "value": "2.80.0"}, "installHint": "Install gh",
                 "auth": {"status": "authenticated", "account": {"_tag": "Some", "value": "octo"}, "host": null, "detail": null}},
                {"kind": "gitlab", "label": "GitLab", "executable": "glab", "status": "available", "version": null, "installHint": "Install glab",
                 "auth": {"status": "unauthenticated", "account": null, "host": null, "detail": null}},
                {"kind": "bitbucket", "label": "Bitbucket", "executable": null, "status": "available", "version": null, "installHint": "Add a token.",
                 "auth": {"status": "unauthenticated", "account": null, "host": null, "detail": null}},
                {"kind": "forgejo", "label": "Forgejo / Gitea", "executable": "tea", "status": "missing", "version": null, "installHint": "Install tea",
                 "auth": {"status": "unknown", "account": null, "host": null, "detail": null}}
            ]
        }))
    }

    #[test]
    fn discovery_parses_both_option_shapes() {
        let found = discovery();
        assert_eq!(found.version_control.len(), 2);
        assert_eq!(found.version_control[0].version.as_deref(), Some("2.51.0"));
        assert_eq!(found.version_control[1].version, None);
        assert_eq!(found.providers[0].version.as_deref(), Some("2.80.0"));
        assert_eq!(
            found.providers[0]
                .auth
                .as_ref()
                .and_then(|a| a.account.as_deref()),
            Some("octo")
        );
        assert!(Discovery::parse(&json!({})).is_empty());
    }

    #[test]
    fn rows_report_dots_badges_and_switches() {
        let found = discovery();
        let [git, jj] = &found.version_control[..] else {
            panic!()
        };
        assert_eq!(
            (git.dot(), git.enabled(), git.badge()),
            (Dot::Success, true, None)
        );
        assert_eq!(
            (jj.dot(), jj.badge()),
            (Dot::Muted, Some(Badge::ComingSoon))
        );
        let [github, gitlab, _, forgejo] = &found.providers[..] else {
            panic!()
        };
        assert_eq!((github.dot(), github.enabled()), (Dot::Success, true));
        assert_eq!(
            (gitlab.dot(), gitlab.badge()),
            (Dot::Muted, Some(Badge::NotAuthenticated))
        );
        assert!(!forgejo.enabled());
    }

    #[test]
    fn a_missing_version_control_tool_warns() {
        let item = Item {
            kind: "git".into(),
            label: "Git".into(),
            executable: Some("git".into()),
            available: false,
            version: None,
            install_hint: "Install Git".into(),
            implemented: true,
            auth: None,
        };
        assert_eq!(item.dot(), Dot::Warning);
    }

    #[test]
    fn summaries_follow_the_web_order() {
        let found = discovery();
        assert_eq!(
            found.version_control[1].summary(),
            vec![Span::Text("Support for Jujutsu is coming soon.".into())]
        );
        assert_eq!(
            found.providers[0].summary(),
            vec![
                Span::Text("Authenticated".into()),
                Span::Text("as".into()),
                Span::Account("octo".into())
            ]
        );
        assert_eq!(found.providers[1].summary()[1], Span::Code("glab".into()));
        assert_eq!(
            found.providers[2].summary(),
            vec![Span::Text("Available. Add a token.".into())]
        );
        assert_eq!(
            found.providers[3].summary(),
            vec![Span::Text(
                "Not available on this server: Install tea".into()
            )]
        );
        assert_eq!(
            found.version_control[0].summary(),
            vec![Span::Text("Available".into())]
        );
    }

    #[test]
    fn redaction_keeps_punctuation_and_is_stable() {
        let masked = redacted("a.b-c@d_e");
        assert_eq!(masked.len(), 9);
        assert_eq!(
            masked
                .chars()
                .filter(|c| matches!(c, '.' | '-' | '@' | '_'))
                .count(),
            4
        );
        assert_eq!(masked, redacted("a.b-c@d_e"));
        assert_ne!(redacted("octocat"), "octocat");
    }

    fn login(host: &str, account: &str, active: bool, ok: bool, env: Option<&str>) -> AuthAccount {
        AuthAccount {
            host: host.to_owned(),
            account: account.to_owned(),
            active,
            authenticated: ok,
            environment_variable: env.map(str::to_owned),
            error: None,
        }
    }

    #[test]
    fn groups_logins_by_host() {
        let groups = group_host_accounts(&[
            login("github.com", "ada", false, true, None),
            login("github.com", "bob", true, true, None),
            login("github.com", "old", false, false, None),
            login(
                "ghe.example",
                "eve",
                false,
                true,
                Some("GH_ENTERPRISE_TOKEN"),
            ),
        ]);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].selectable, ids(&["ada", "bob"]));
        assert_eq!(groups[0].active_account.as_deref(), Some("bob"));
        assert_eq!(groups[0].broken.len(), 1);
        assert!(groups[1].selectable.is_empty());
        assert_eq!(
            groups[1].environment_variable.as_deref(),
            Some("GH_ENTERPRISE_TOKEN")
        );
    }

    #[test]
    fn host_choices_drop_the_default_and_keep_the_rest() {
        let mut current = BTreeMap::new();
        let off = next_host_account_choices(&current, "github.com", Some(false), None);
        assert_eq!(
            off["github.com"],
            HostChoice {
                enabled: false,
                account: None
            }
        );
        let pinned = next_host_account_choices(&off, "github.com", Some(true), Some(Some("ada")));
        assert_eq!(pinned["github.com"].account.as_deref(), Some("ada"));
        current = pinned;
        let back = next_host_account_choices(&current, "github.com", None, Some(None));
        assert!(back.is_empty());
    }

    #[test]
    fn applying_a_host_patch_drops_cleared_keys() {
        let mut config = SourceControlConfig::default();
        let fields = |value: Value| match value {
            Value::Object(map) => map,
            _ => Map::new(),
        };
        config.apply_host(
            "bitbucket",
            &fields(json!({"email": "a@b.c", "accessToken": "******"})),
        );
        assert_eq!(config.host_value("bitbucket", "email"), json!("a@b.c"));
        config.apply_host("bitbucket", &fields(json!({"email": ""})));
        assert_eq!(config.host_value("bitbucket", "email"), Value::Null);
        assert_eq!(
            config.host_value("bitbucket", "accessToken"),
            json!("******")
        );
    }

    #[test]
    fn redacted_keeps_separators_and_length() {
        let shown = redacted("ada-lovelace@x.io");
        assert_eq!(shown.chars().count(), 17);
        for (a, b) in "ada-lovelace@x.io".chars().zip(shown.chars()) {
            if matches!(a, '@' | '.' | '-' | '_') {
                assert_eq!(a, b);
            } else {
                assert!(b.is_ascii_alphanumeric());
            }
        }
        assert_eq!(shown, redacted("ada-lovelace@x.io"));
    }
}
