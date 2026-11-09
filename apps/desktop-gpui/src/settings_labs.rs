//! The Labs tab (`LabsSettingsPanel.tsx`, `LegacyFeaturesSection` in `SettingsPanels.tsx`): its
//! device settings, the settings scope bar's model (`settingsScope.ts`, `settingsScopeAxis.ts`),
//! the copy links and the search entries. The panel and the pickers only draw this.
//!
//! Where each Labs key lives (packages/contracts/src/settings.ts):
//! - Device settings, `client-settings.json` (`ClientSettingsSchema`): `boringSidebarEnabled`,
//!   `boringSidebarMultilineRows`, `toolCallVisibility`, `legacySidebarEnabled` (hidden, cleared
//!   by the boring sidebar switch), `planModeEnabled`, `contextWindowMeterEnabled`. A device
//!   setting ignores the scope bar.
//! - Server settings, environment-wide: `guestSharingEnabled`, `linkedEnvironmentsEnabled`,
//!   `simlockDeviceHostsEnabled`. None is in `PROJECT_SCOPED_SERVER_SETTING_KEYS`, so a project
//!   scope cannot write them.

use crate::projects::{COLORS, MachineKind};
use crate::settings::{SearchItem, Section};
use serde_json::{Map, Value, json};
use std::path::PathBuf;

/// Call tokens of the Labs tab's server writes start here.
pub const TOKEN_BASE: u64 = 1 << 44;

/// `ToolCallVisibility` (settings.ts:346); labels from `LabsSettingsPanel.tsx`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolCalls {
    ShowAll,
    HideSuccessful,
    HideAll,
    Zen,
}

impl ToolCalls {
    pub const ALL: &'static [Self] = &[
        Self::ShowAll,
        Self::HideSuccessful,
        Self::HideAll,
        Self::Zen,
    ];

    pub fn wire(self) -> &'static str {
        match self {
            Self::ShowAll => "showAll",
            Self::HideSuccessful => "hideSuccessful",
            Self::HideAll => "hideAll",
            Self::Zen => "zen",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::ShowAll => "Show all",
            Self::HideSuccessful => "Hide successful",
            Self::HideAll => "Hide all",
            Self::Zen => "Zen",
        }
    }

    pub fn from_wire(value: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|v| v.wire() == value)
    }
}

/// The Labs keys of `ClientSettingsSchema`, with the schema's defaults.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LabsSettings {
    pub boring_sidebar: bool,
    pub boring_multiline: bool,
    pub tool_calls: ToolCallsDefault,
    pub legacy_sidebar: bool,
    pub plan_mode: bool,
    pub context_meter: bool,
}

/// `ToolCalls` with `showAll` as its `Default`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToolCallsDefault(pub ToolCalls);

impl Default for ToolCallsDefault {
    fn default() -> Self {
        Self(ToolCalls::ShowAll)
    }
}

/// The document is bare or wrapped as `{"settings": {...}}` (`decodeClientSettingsJson`).
fn fields(value: &Value) -> Option<&Map<String, Value>> {
    let object = value.as_object()?;
    match object.get("settings") {
        Some(Value::Object(inner)) => Some(inner),
        _ => Some(object),
    }
}

impl LabsSettings {
    /// `boringSidebarEnabled || legacySidebarEnabled`: the Boring sidebar switch's state.
    pub fn boring_checked(self) -> bool {
        self.boring_sidebar || self.legacy_sidebar
    }

    /// The switch writes `{boringSidebarEnabled: checked, legacySidebarEnabled: false}`.
    pub fn set_boring(&mut self, checked: bool) {
        self.boring_sidebar = checked;
        self.legacy_sidebar = false;
    }

    pub fn parse(text: &str) -> Option<Self> {
        let value: Value = serde_json::from_str(text).ok()?;
        let map = fields(&value)?;
        let flag = |key: &str| map.get(key).and_then(Value::as_bool).unwrap_or(false);
        Some(Self {
            boring_sidebar: flag("boringSidebarEnabled"),
            boring_multiline: flag("boringSidebarMultilineRows"),
            tool_calls: ToolCallsDefault(
                map.get("toolCallVisibility")
                    .and_then(Value::as_str)
                    .and_then(ToolCalls::from_wire)
                    .unwrap_or(ToolCalls::ShowAll),
            ),
            legacy_sidebar: flag("legacySidebarEnabled"),
            plan_mode: flag("planModeEnabled"),
            context_meter: flag("contextWindowMeterEnabled"),
        })
    }

    pub fn entries(self) -> [(&'static str, Value); 6] {
        [
            ("boringSidebarEnabled", json!(self.boring_sidebar)),
            ("boringSidebarMultilineRows", json!(self.boring_multiline)),
            ("toolCallVisibility", json!(self.tool_calls.0.wire())),
            ("legacySidebarEnabled", json!(self.legacy_sidebar)),
            ("planModeEnabled", json!(self.plan_mode)),
            ("contextWindowMeterEnabled", json!(self.context_meter)),
        ]
    }

    /// Writes these keys into `existing`, keeping every key this tab does not own.
    pub fn to_json(self, existing: Option<&str>) -> String {
        let mut document = existing
            .and_then(|text| serde_json::from_str::<Value>(text).ok())
            .filter(Value::is_object)
            .unwrap_or_else(|| json!({}));
        let target = match document.get_mut("settings") {
            Some(inner) if inner.is_object() => inner,
            _ => &mut document,
        };
        if let Some(map) = target.as_object_mut() {
            for (key, value) in self.entries() {
                map.insert(key.to_owned(), value);
            }
        }
        document.to_string()
    }

    fn path() -> Option<PathBuf> {
        crate::settings::DeviceSettings::path()
    }

    pub fn load() -> Self {
        Self::path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    /// Re-reads the file first, so keys the other tabs saved in the meantime survive.
    pub fn save(self) {
        if let Some(path) = Self::path() {
            let existing = std::fs::read_to_string(&path).ok();
            let _ = std::fs::write(path, self.to_json(existing.as_deref()));
        }
    }
}

/// The three environment-wide server settings of the tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ServerKey {
    GuestSharing,
    LinkedEnvironments,
    SimlockHosts,
}

impl ServerKey {
    #[cfg(test)]
    pub const ALL: [Self; 3] = [
        Self::GuestSharing,
        Self::LinkedEnvironments,
        Self::SimlockHosts,
    ];

    pub fn wire(self) -> &'static str {
        match self {
            Self::GuestSharing => "guestSharingEnabled",
            Self::LinkedEnvironments => "linkedEnvironmentsEnabled",
            Self::SimlockHosts => "simlockDeviceHostsEnabled",
        }
    }

    /// The `settings.update` payload: `{"patch": {key: enabled}}`.
    pub fn patch(self, enabled: bool) -> Value {
        json!({ "patch": { self.wire(): enabled } })
    }

    /// Nothing is project-scoped here, so a project scope can never write it.
    pub fn project_scoped(self) -> bool {
        false
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ServerFlags {
    pub guest_sharing: bool,
    pub linked_environments: bool,
    pub simlock_hosts: bool,
}

impl ServerFlags {
    pub fn get(self, key: ServerKey) -> bool {
        match key {
            ServerKey::GuestSharing => self.guest_sharing,
            ServerKey::LinkedEnvironments => self.linked_environments,
            ServerKey::SimlockHosts => self.simlock_hosts,
        }
    }

    pub fn set(&mut self, key: ServerKey, enabled: bool) {
        match key {
            ServerKey::GuestSharing => self.guest_sharing = enabled,
            ServerKey::LinkedEnvironments => self.linked_environments = enabled,
            ServerKey::SimlockHosts => self.simlock_hosts = enabled,
        }
    }
}

/// Where an inline link in a row description goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Link {
    /// Another settings section: the modal switches to it.
    Section(Section),
    /// Opens in the browser.
    External(&'static str),
}

/// A run of a row description: plain text or a link (`underline underline-offset-2`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Piece {
    Text(&'static str),
    Link(&'static str, Link),
}

/// `LabsSettingsPanel.tsx:155-162`. `/settings/access` is "Sharing & access".
pub const GUEST_LINKS: &[Piece] = &[
    Piece::Text(
        "Share a thread with a guest who watches it live in a browser and sends messages you approve. Needs T3 Connect: sign in and link this machine in ",
    ),
    Piece::Link("Sharing & access", Link::Section(Section::Access)),
    Piece::Text(". Turning it off stops every guest link."),
];

/// `LabsSettingsPanel.tsx:182-189`.
pub const LINKED_ENVIRONMENTS: &[Piece] = &[
    Piece::Text(
        "Let agents here work on your other machines, and theirs here. Turn it on for each machine, then link them in ",
    ),
    Piece::Link("Connections", Link::Section(Section::Connections)),
    Piece::Text(". While off, nothing new starts over a link; running tasks finish."),
];

/// `LabsSettingsPanel.tsx:208-225`.
pub const SIMLOCK_HOSTS: &[Piece] = &[
    Piece::Text("Let the Device panel lease simulators and emulators from your own "),
    Piece::Link(
        "Simlock",
        Link::External("https://incubator.callstack.com/simlock/"),
    ),
    Piece::Text(" pool, added in "),
    Piece::Link("Devices", Link::Section(Section::Devices)),
    Piece::Text(". An alpha that may change or break. T3 Code never installs Simlock."),
];

const fn item(id: &'static str, title: &'static str, terms: &'static str) -> SearchItem {
    SearchItem {
        id,
        title,
        section: Section::Labs,
        terms,
        desktop_only: false,
    }
}

/// The Labs tab's `searchableSetting` entries (settingsSearch.ts:458-497, 863-873).
pub const LABS_ITEMS: &[SearchItem] = &[
    item(
        "boring-sidebar",
        "Boring sidebar (group by project)",
        "beta folders project tree legacy old classic sidebar",
    ),
    item(
        "boring-sidebar-multiline",
        "Two-line folder rows",
        "beta boring sidebar folder rows multiline branch pull request details",
    ),
    item(
        "tool-call-visibility",
        "Tool calls in chat",
        "beta hide tool calls commands edits searches work log activity quiet chat zen final answers only",
    ),
    item(
        "guest-links",
        "Guest links",
        "beta share guest invite link teammate watch t3 connect tunnel",
    ),
    item(
        "linked-environments-beta",
        "Linked environments",
        "beta link another machine vps peer agents work there delegate",
    ),
    item(
        "simlock-hosts",
        "Simlock device hosts (alpha)",
        "beta alpha simlock callstack simulator emulator ios android pool lease gateway",
    ),
    item(
        "legacy-plan-mode",
        "Plan mode (legacy)",
        "build plan composer old",
    ),
    item(
        "legacy-context-window-indicator",
        "Context window indicator (legacy)",
        "composer meter usage tokens circle old",
    ),
];

/// The id the scope pickers give their "All ..." entry (`ALL_PROJECTS_VALUE`).
pub const ALL: &str = "all";

/// `SettingsScopeSearch` without the checkout axis, which this app has no picker for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScopeSearch {
    pub project: Option<String>,
    pub machine: Option<String>,
}

impl ScopeSearch {
    /// `selectEnvironmentAxis`: keeps the project.
    pub fn select_environment(&self, value: &str) -> Self {
        Self {
            project: self.project.clone(),
            machine: (value != ALL).then(|| value.to_owned()),
        }
    }

    /// `selectProjectAxis`: keeps the environment.
    pub fn select_project(&self, value: &str) -> Self {
        Self {
            project: (value != ALL).then(|| value.to_owned()),
            machine: self.machine.clone(),
        }
    }

    /// The sentence's middle word (SettingsScopeSentence.tsx:84).
    pub fn joiner(&self) -> &'static str {
        if self.machine.is_some() {
            "on"
        } else {
            "across"
        }
    }
}

/// An environment as the scope bar sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Environment {
    pub id: String,
    pub label: String,
    /// `host:port`, else the environment id (`settingsScopeEnvironmentAddress`).
    pub address: String,
    /// The URL the address comes from; empty when none is known.
    pub display_url: String,
    pub machine: MachineKind,
    pub connected: bool,
    /// Whether this app holds the connection that can write its server settings.
    pub writable: bool,
}

/// A project group as the scope bar sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Project {
    pub key: String,
    pub name: String,
    pub root: String,
    pub initials: String,
    /// Index into the shared palette.
    pub color: usize,
    pub archived: bool,
    /// The environments with a member of the group.
    pub environments: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScopeData {
    pub environments: Vec<Environment>,
    pub projects: Vec<Project>,
}

/// `ResolvedSettingsScope`, reduced to what the tab reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Scope {
    All,
    Environment(String),
    /// A project, on the listed environments.
    Project(Vec<String>),
    Unavailable(&'static str),
}

pub fn resolve(search: &ScopeSearch, data: &ScopeData) -> Scope {
    let environment = search
        .machine
        .as_ref()
        .map(|id| data.environments.iter().any(|e| &e.id == id));
    if environment == Some(false) {
        return Scope::Unavailable("This environment is no longer available.");
    }
    if let Some(key) = &search.project {
        let Some(project) = data.projects.iter().find(|p| &p.key == key) else {
            return Scope::Unavailable("This project is no longer available.");
        };
        let members: Vec<String> = project
            .environments
            .iter()
            .filter(|id| search.machine.as_ref().is_none_or(|machine| machine == *id))
            .cloned()
            .collect();
        if members.is_empty() {
            return Scope::Unavailable("This project has no checkout on this environment.");
        }
        return Scope::Project(members);
    }
    match &search.machine {
        Some(id) => Scope::Environment(id.clone()),
        None => Scope::All,
    }
}

/// Why a server row's control is inert, or the environments its write reaches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reach {
    Targets(Vec<String>),
    Inert(String),
}

/// `planScopedSettingsPatch` for one environment-wide server key: all and environment scopes
/// write every writable connected environment in scope; a project scope cannot write it
/// (settingsLayout.tsx:448); an unavailable scope shows its message.
pub fn reach(key: ServerKey, search: &ScopeSearch, data: &ScopeData) -> Reach {
    let writable = |ids: &[String]| -> Vec<String> {
        ids.iter()
            .filter(|id| {
                data.environments
                    .iter()
                    .any(|e| &e.id == *id && e.connected && e.writable)
            })
            .cloned()
            .collect()
    };
    match resolve(search, data) {
        Scope::Unavailable(message) => Reach::Inert(message.to_owned()),
        Scope::Project(_) if !key.project_scoped() => {
            Reach::Inert("Environment-wide setting. Select an environment to change it.".to_owned())
        }
        Scope::Project(ids) => targets_or_inert(writable(&ids)),
        Scope::Environment(id) => match data.environments.iter().find(|e| e.id == id) {
            Some(e) if !e.connected => Reach::Inert(
                "Reconnect the selected environment to change this setting.".to_owned(),
            ),
            Some(e) if !e.writable => Reach::Inert(format!(
                "Settings of {} cannot be changed from this app yet.",
                e.label
            )),
            _ => targets_or_inert(writable(&[id])),
        },
        Scope::All => {
            let ids: Vec<String> = data.environments.iter().map(|e| e.id.clone()).collect();
            targets_or_inert(writable(&ids))
        }
    }
}

fn targets_or_inert(targets: Vec<String>) -> Reach {
    if targets.is_empty() {
        Reach::Inert("Reconnect the selected environment to change this setting.".to_owned())
    } else {
        Reach::Targets(targets)
    }
}

/// One picker row (`ScopeOption`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub value: String,
    pub label: String,
    pub secondary: String,
    pub offline: bool,
    pub icon: ChoiceIcon,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChoiceIcon {
    None,
    Layers,
    Machine(MachineKind),
    Project { initials: String, color: usize },
}

/// `settingsScopeEnvironmentAddress`: the URL's host (`new URL(url).host`), else the URL as given,
/// else the environment id.
pub fn environment_address(display_url: &str, id: &str) -> String {
    if display_url.is_empty() {
        return id.to_owned();
    }
    let Some((scheme, rest)) = display_url.split_once("://") else {
        return display_url.to_owned();
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = authority
        .rsplit('@')
        .next()
        .unwrap_or_default()
        .to_lowercase();
    let default_port = match scheme.to_lowercase().as_str() {
        "http" | "ws" => ":80",
        "https" | "wss" => ":443",
        _ => "",
    };
    let host = match host.strip_suffix(default_port) {
        Some(bare) if !default_port.is_empty() => bare,
        _ => &host,
    };
    if host.is_empty() {
        display_url.to_owned()
    } else {
        host.to_owned()
    }
}

/// `settingsScopeEnvironmentLabels`: the secondary text only for a label another environment
/// shares, the full URL when a duplicate also shares the address.
fn environment_secondary(environment: &Environment, all: &[Environment]) -> String {
    let duplicates: Vec<&Environment> = all
        .iter()
        .filter(|other| other.id != environment.id && other.label == environment.label)
        .collect();
    if duplicates.is_empty() {
        return String::new();
    }
    if duplicates
        .iter()
        .any(|other| other.address == environment.address)
    {
        return if environment.display_url.is_empty() {
            environment.id.clone()
        } else {
            environment.display_url.clone()
        };
    }
    environment.address.clone()
}

/// `settingsScopeEnvironmentLabel`: `label · secondary` when the label is shared.
pub fn environment_label(environment: &Environment, all: &[Environment]) -> String {
    let secondary = environment_secondary(environment, all);
    if secondary.is_empty() {
        environment.label.clone()
    } else {
        format!("{} · {secondary}", environment.label)
    }
}

/// The project picker's rows after the "All projects" entry (`ProjectScopeMenu`,
/// `projectScopeMenuGroups`).
pub fn project_choices(search: &ScopeSearch, data: &ScopeData) -> Vec<Choice> {
    let shown: Vec<&Project> = data
        .projects
        .iter()
        .filter(|project| {
            search
                .machine
                .as_ref()
                .is_none_or(|machine| project.environments.contains(machine))
        })
        .collect();
    let machines = |project: &Project| -> String {
        let mut names: Vec<String> = Vec::new();
        for id in &project.environments {
            let name = data.environments.iter().find(|e| &e.id == id).map_or_else(
                || id.clone(),
                |e| format!("{} \u{b7} {}", e.label, e.address),
            );
            if !names.contains(&name) {
                names.push(name);
            }
        }
        names.join(", ")
    };
    shown
        .iter()
        .map(|project| {
            let same_name: Vec<&&Project> = shown
                .iter()
                .filter(|other| other.key != project.key && other.name == project.name)
                .collect();
            let location = if same_name.is_empty() {
                if data.environments.len() > 1 {
                    project.root.clone()
                } else {
                    String::new()
                }
            } else if same_name
                .iter()
                .any(|other| machines(other) == machines(project))
            {
                format!("{} \u{b7} {}", machines(project), project.root)
            } else {
                machines(project)
            };
            let secondary = if project.archived {
                ["Archived", location.as_str()]
                    .iter()
                    .filter(|part| !part.is_empty())
                    .copied()
                    .collect::<Vec<_>>()
                    .join(" \u{b7} ")
            } else {
                location
            };
            Choice {
                value: project.key.clone(),
                label: project.name.clone(),
                secondary,
                offline: false,
                icon: ChoiceIcon::Project {
                    initials: project.initials.clone(),
                    color: project.color,
                },
            }
        })
        .collect()
}

/// The environment picker's rows after "All environments" (`EnvironmentScopeMenu`): narrowed to
/// the selected project's environments.
pub fn environment_choices(search: &ScopeSearch, data: &ScopeData) -> Vec<Choice> {
    let members = search
        .project
        .as_ref()
        .and_then(|key| data.projects.iter().find(|p| &p.key == key))
        .map(|project| &project.environments);
    data.environments
        .iter()
        .filter(|e| members.is_none_or(|ids| ids.contains(&e.id)))
        .map(|e| Choice {
            value: e.id.clone(),
            label: e.label.clone(),
            secondary: if data.environments.len() <= 1 {
                String::new()
            } else {
                let secondary = environment_secondary(e, &data.environments);
                if secondary.is_empty() {
                    e.address.clone()
                } else {
                    secondary
                }
            },
            offline: !e.connected,
            icon: ChoiceIcon::Machine(e.machine),
        })
        .collect()
}

/// The project picker's trigger label.
pub fn project_label(search: &ScopeSearch, data: &ScopeData) -> String {
    match &search.project {
        None => "All projects".to_owned(),
        Some(key) => data
            .projects
            .iter()
            .find(|p| &p.key == key)
            .map_or_else(|| "Unavailable project".to_owned(), |p| p.name.clone()),
    }
}

/// The environment picker's trigger label.
pub fn environment_trigger_label(search: &ScopeSearch, data: &ScopeData) -> String {
    match &search.machine {
        None => "All environments".to_owned(),
        Some(id) => data.environments.iter().find(|e| &e.id == id).map_or_else(
            || "Unavailable environment".to_owned(),
            |e| environment_label(e, &data.environments),
        ),
    }
}

/// A picker opens a search field past six options (`options.length > 6`).
pub fn searchable(options: usize) -> bool {
    options > 6
}

/// The combobox filter: `label secondary value`, lowercased, contains the trimmed query.
pub fn filter<'a>(choices: &'a [Choice], query: &str) -> Vec<&'a Choice> {
    let needle = query.trim().to_lowercase();
    choices
        .iter()
        .filter(|c| {
            format!("{} {} {}", c.label, c.secondary, c.value)
                .to_lowercase()
                .contains(&needle)
        })
        .collect()
}

/// The tile's color: palette index to its `(light, dark)` Tailwind shades.
pub fn avatar_color(index: usize, dark: bool) -> u32 {
    let (_, light, night) = COLORS[index % COLORS.len()];
    if dark { night } else { light }
}

#[cfg(test)]
/// The Labs search entries that match `query`, in rank order.
pub fn search_hits(query: &str) -> Vec<&'static str> {
    crate::settings::search(query, LABS_ITEMS)
        .into_iter()
        .map(|item| item.id)
        .collect()
}

/// The scope bar's data from what the status carries: the attached environment (the only one
/// this app can write) and the other saved ones, whose projects arrive without a connection.
pub fn scope_data(
    primary_label: &str,
    primary_machine: MachineKind,
    primary_url: &str,
    primary: &[crate::projects::ProjectEntry],
    others: &[crate::projects::EnvironmentProjects],
) -> ScopeData {
    const PRIMARY: &str = "primary";
    let mut data = ScopeData::default();
    data.environments.push(Environment {
        id: PRIMARY.to_owned(),
        label: primary_label.to_owned(),
        address: environment_address(primary_url, PRIMARY),
        display_url: primary_url.to_owned(),
        machine: primary_machine,
        connected: true,
        writable: true,
    });
    let mut add = |env: &str, entries: &[crate::projects::ProjectEntry]| {
        for entry in entries.iter().filter(|e| !e.scratch) {
            let key = entry
                .logical_key
                .clone()
                .unwrap_or_else(|| entry.key.clone());
            match data.projects.iter_mut().find(|p| p.key == key) {
                Some(project) => {
                    if !project.environments.iter().any(|e| e == env) {
                        project.environments.push(env.to_owned());
                    }
                }
                None => data.projects.push(Project {
                    key,
                    name: entry.title.clone(),
                    root: entry.root.clone(),
                    initials: entry.initials.clone(),
                    color: entry.color,
                    archived: false,
                    environments: vec![env.to_owned()],
                }),
            }
        }
    };
    add(PRIMARY, primary);
    for other in others {
        add(&other.id, &other.projects);
    }
    for other in others {
        data.environments.push(Environment {
            id: other.id.clone(),
            label: other.label.clone(),
            address: environment_address(&other.url, &other.id),
            display_url: other.url.clone(),
            machine: other.machine,
            connected: other.connected,
            writable: false,
        });
    }
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(id: &str, label: &str, connected: bool, writable: bool) -> Environment {
        Environment {
            id: id.to_owned(),
            label: label.to_owned(),
            address: format!("{id}.example:3773"),
            display_url: format!("https://{id}.example:3773"),
            machine: MachineKind::Laptop,
            connected,
            writable,
        }
    }

    fn project(key: &str, name: &str, environments: &[&str]) -> Project {
        Project {
            key: key.to_owned(),
            name: name.to_owned(),
            root: format!("/home/user/projects/{key}"),
            initials: crate::projects::initials(name),
            color: crate::projects::color_index(name),
            archived: false,
            environments: environments.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    fn data() -> ScopeData {
        ScopeData {
            environments: vec![
                env("a", "Example Mac", true, true),
                env("b", "Example VPS", true, false),
                env("c", "Example Box", false, false),
            ],
            projects: vec![
                project("one", "Example One", &["a", "b"]),
                project("two", "Example Two", &["b"]),
            ],
        }
    }

    fn search(project: Option<&str>, machine: Option<&str>) -> ScopeSearch {
        ScopeSearch {
            project: project.map(str::to_owned),
            machine: machine.map(str::to_owned),
        }
    }

    #[test]
    fn defaults_match_the_contract() {
        let d = LabsSettings::default();
        assert!(!d.boring_sidebar && !d.boring_multiline && !d.legacy_sidebar);
        assert!(!d.plan_mode && !d.context_meter);
        assert_eq!(d.tool_calls.0, ToolCalls::ShowAll);
    }

    #[test]
    fn parses_bare_and_wrapped_documents_and_keeps_foreign_keys() {
        let wrapped = LabsSettings::parse(
            r#"{"settings":{"boringSidebarEnabled":true,"toolCallVisibility":"zen","theme":"dark"}}"#,
        )
        .unwrap();
        assert!(wrapped.boring_sidebar);
        assert_eq!(wrapped.tool_calls.0, ToolCalls::Zen);
        let bad = LabsSettings::parse(r#"{"toolCallVisibility":"loud"}"#).unwrap();
        assert_eq!(bad.tool_calls.0, ToolCalls::ShowAll);
        assert!(LabsSettings::parse("[]").is_none());
        let written = LabsSettings::default().to_json(Some(r#"{"settings":{"theme":"dark"}}"#));
        let value: Value = serde_json::from_str(&written).unwrap();
        assert_eq!(value["settings"]["theme"], "dark");
        assert_eq!(value["settings"]["toolCallVisibility"], "showAll");
    }

    #[test]
    fn the_boring_switch_reads_either_flag_and_clears_the_legacy_one() {
        let mut s = LabsSettings {
            legacy_sidebar: true,
            ..LabsSettings::default()
        };
        assert!(s.boring_checked());
        s.set_boring(false);
        assert!(!s.boring_checked() && !s.legacy_sidebar);
        s.set_boring(true);
        assert!(s.boring_sidebar && !s.legacy_sidebar);
    }

    #[test]
    fn tool_call_labels_follow_the_web_order() {
        let labels: Vec<_> = ToolCalls::ALL.iter().map(|v| v.label()).collect();
        assert_eq!(labels, ["Show all", "Hide successful", "Hide all", "Zen"]);
    }

    #[test]
    fn scope_resolves_like_the_web() {
        let d = data();
        assert_eq!(resolve(&search(None, None), &d), Scope::All);
        assert_eq!(
            resolve(&search(None, Some("b")), &d),
            Scope::Environment("b".into())
        );
        assert_eq!(
            resolve(&search(Some("one"), None), &d),
            Scope::Project(vec!["a".into(), "b".into()])
        );
        assert_eq!(
            resolve(&search(Some("one"), Some("b")), &d),
            Scope::Project(vec!["b".into()])
        );
        assert!(matches!(
            resolve(&search(Some("two"), Some("a")), &d),
            Scope::Unavailable("This project has no checkout on this environment.")
        ));
        assert!(matches!(
            resolve(&search(Some("gone"), None), &d),
            Scope::Unavailable("This project is no longer available.")
        ));
        assert!(matches!(
            resolve(&search(None, Some("gone")), &d),
            Scope::Unavailable("This environment is no longer available.")
        ));
    }

    #[test]
    fn axis_selection_keeps_the_other_axis() {
        let s = search(Some("one"), Some("a"));
        assert_eq!(s.select_project(ALL), search(None, Some("a")));
        assert_eq!(s.select_environment(ALL), search(Some("one"), None));
        assert_eq!(s.select_environment("b"), search(Some("one"), Some("b")));
        assert_eq!(s.joiner(), "on");
        assert_eq!(search(Some("one"), None).joiner(), "across");
    }

    #[test]
    fn server_rows_write_only_writable_connected_environments() {
        let d = data();
        let key = ServerKey::GuestSharing;
        assert_eq!(
            reach(key, &search(None, None), &d),
            Reach::Targets(vec!["a".into()])
        );
        assert_eq!(
            reach(key, &search(None, Some("a")), &d),
            Reach::Targets(vec!["a".into()])
        );
        assert!(
            matches!(reach(key, &search(None, Some("c")), &d), Reach::Inert(m) if m.starts_with("Reconnect"))
        );
        assert!(
            matches!(reach(key, &search(None, Some("b")), &d), Reach::Inert(m) if m.contains("Example VPS"))
        );
        assert_eq!(
            reach(key, &search(Some("one"), None), &d),
            Reach::Inert("Environment-wide setting. Select an environment to change it.".into())
        );
        assert_eq!(
            reach(key, &search(Some("gone"), None), &d),
            Reach::Inert("This project is no longer available.".into())
        );
    }

    #[test]
    fn server_patches_use_the_contract_keys() {
        assert_eq!(
            ServerKey::SimlockHosts.patch(true),
            json!({"patch": {"simlockDeviceHostsEnabled": true}})
        );
        assert!(ServerKey::ALL.iter().all(|k| !k.project_scoped()));
        let mut flags = ServerFlags::default();
        flags.set(ServerKey::LinkedEnvironments, true);
        assert!(flags.get(ServerKey::LinkedEnvironments) && !flags.get(ServerKey::GuestSharing));
    }

    #[test]
    fn project_rows_hide_paths_for_one_environment_and_narrow_by_machine() {
        let mut d = data();
        let rows = project_choices(&search(None, None), &d);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].secondary, "/home/user/projects/one");
        assert_eq!(project_choices(&search(None, Some("a")), &d).len(), 1);
        d.environments.truncate(1);
        assert_eq!(project_choices(&search(None, None), &d)[0].secondary, "");
    }

    #[test]
    fn same_named_projects_are_told_apart_by_machine() {
        let mut d = data();
        d.projects = vec![project("x1", "Same", &["a"]), project("x2", "Same", &["b"])];
        let rows = project_choices(&search(None, None), &d);
        assert_eq!(rows[0].secondary, "Example Mac \u{b7} a.example:3773");
        d.projects[1].environments = vec!["a".into()];
        let rows = project_choices(&search(None, None), &d);
        assert!(rows[0].secondary.ends_with("/home/user/projects/x1"));
        d.projects[0].archived = true;
        assert!(
            project_choices(&search(None, None), &d)[0]
                .secondary
                .starts_with("Archived \u{b7} ")
        );
    }

    #[test]
    fn environment_rows_narrow_to_the_project_and_flag_offline() {
        let d = data();
        let rows = environment_choices(&search(Some("two"), None), &d);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].value, "b");
        let all = environment_choices(&search(None, None), &d);
        assert_eq!(all.len(), 3);
        assert!(all[2].offline && !all[0].offline);
        assert_eq!(all[0].secondary, "a.example:3773");
    }

    #[test]
    fn trigger_labels_cover_the_stale_cases() {
        let d = data();
        assert_eq!(project_label(&search(None, None), &d), "All projects");
        assert_eq!(project_label(&search(Some("one"), None), &d), "Example One");
        assert_eq!(
            project_label(&search(Some("gone"), None), &d),
            "Unavailable project"
        );
        assert_eq!(
            environment_trigger_label(&search(None, None), &d),
            "All environments"
        );
        assert_eq!(
            environment_trigger_label(&search(None, Some("gone")), &d),
            "Unavailable environment"
        );
        let mut dup = data();
        dup.environments[1].label = "Example Mac".into();
        assert_eq!(
            environment_trigger_label(&search(None, Some("a")), &dup),
            "Example Mac \u{b7} a.example:3773"
        );
    }

    #[test]
    fn picker_search_matches_label_secondary_and_value() {
        let d = data();
        let rows = project_choices(&search(None, None), &d);
        assert_eq!(filter(&rows, "  EXAMPLE two ").len(), 1);
        assert_eq!(filter(&rows, "projects/one").len(), 1);
        assert_eq!(filter(&rows, "two").len(), 1);
        assert!(filter(&rows, "nothing").is_empty());
        assert_eq!(filter(&rows, "").len(), 2);
        assert!(!searchable(6) && searchable(7));
    }

    #[test]
    fn avatars_follow_the_web_algorithm() {
        assert_eq!(crate::projects::initials("my-app"), "MA");
        assert_eq!(crate::projects::initials("Example One"), "EO");
        assert_eq!(crate::projects::initials("app2go"), "A2");
        assert_eq!(crate::projects::initials("!!!"), "PR");
        assert_eq!(
            crate::projects::color_index(""),
            crate::projects::color_index("project")
        );
        assert_eq!(
            crate::projects::color_index(" Example "),
            crate::projects::color_index("example")
        );
        assert_ne!(avatar_color(5, false), avatar_color(5, true));
        assert_eq!(avatar_color(18, false), avatar_color(0, false));
    }

    #[test]
    fn search_finds_every_labs_row_and_nothing_from_other_tabs() {
        assert_eq!(search_hits("zen").first(), Some(&"tool-call-visibility"));
        assert_eq!(search_hits("simlock").first(), Some(&"simlock-hosts"));
        assert_eq!(search_hits("guest").first(), Some(&"guest-links"));
        assert_eq!(search_hits("plan").first(), Some(&"legacy-plan-mode"));
        assert_eq!(
            search_hits("meter").first(),
            Some(&"legacy-context-window-indicator")
        );
        assert!(LABS_ITEMS.iter().all(|i| i.section == Section::Labs));
        assert_eq!(LABS_ITEMS.len(), 8);
    }

    #[test]
    fn links_point_where_the_web_does() {
        let links = |pieces: &[Piece]| -> Vec<Link> {
            pieces
                .iter()
                .filter_map(|p| match p {
                    Piece::Link(_, link) => Some(*link),
                    Piece::Text(_) => None,
                })
                .collect()
        };
        assert_eq!(links(GUEST_LINKS), [Link::Section(Section::Access)]);
        assert_eq!(
            links(LINKED_ENVIRONMENTS),
            [Link::Section(Section::Connections)]
        );
        assert_eq!(
            links(SIMLOCK_HOSTS),
            [
                Link::External("https://incubator.callstack.com/simlock/"),
                Link::Section(Section::Devices)
            ]
        );
        assert!(
            GUEST_LINKS
                .iter()
                .all(|p| !matches!(p, Piece::Text(t) if t.contains('\u{2014}')))
        );
    }
    fn entry(key: &str, scratch: bool, logical: Option<&str>) -> crate::projects::ProjectEntry {
        crate::projects::ProjectEntry {
            key: key.to_owned(),
            id: key.to_owned(),
            member_ids: vec![key.to_owned()],
            title: key.to_owned(),
            initials: "EX".to_owned(),
            color: 0,
            pinned: false,
            scratch,
            root: format!("/home/user/projects/{key}"),
            logical_key: logical.map(str::to_owned),
            remote: Vec::new(),
        }
    }

    #[test]
    fn scope_data_merges_projects_across_environments() {
        let other = crate::projects::EnvironmentProjects {
            id: "env-2".to_owned(),
            label: "Example Mac".to_owned(),
            url: "http://mac.example.test:13774/".to_owned(),
            connected: false,
            machine: MachineKind::Laptop,
            projects: vec![
                entry("b", false, Some("repo")),
                entry("only-b", false, None),
            ],
        };
        let data = scope_data(
            "Example Server",
            MachineKind::Server,
            "http://server.example.test:13773",
            &[
                entry("a", false, Some("repo")),
                entry("scratch", true, None),
            ],
            &[other],
        );
        assert_eq!(data.environments.len(), 2);
        assert!(data.environments[0].writable && !data.environments[1].writable);
        assert_eq!(data.environments[0].address, "server.example.test:13773");
        assert_eq!(data.environments[1].address, "mac.example.test:13774");
        // The flag comes from the connection, not from whether projects arrived.
        assert!(data.environments[0].connected && !data.environments[1].connected);
        assert_eq!(environment_address("", "env-9"), "env-9");
        assert_eq!(environment_address("not a url", "env-9"), "not a url");
        assert_eq!(
            environment_address("https://Example.test:443/p", "e"),
            "example.test"
        );
        let names: Vec<_> = data.projects.iter().map(|p| p.key.as_str()).collect();
        assert_eq!(names, ["repo", "only-b"]);
        assert_eq!(data.projects[0].environments, ["primary", "env-2"]);
    }
}
