//! The Devices tab (`DeviceIntegrationSettings`, IntegrationsSettings.tsx:804-1030, and
//! `DeviceSourcesSettings.tsx`): its server settings, the device service state, the inheritance
//! chain of a row and the host lists. The panel only draws this.
//!
//! Where each key lives (packages/contracts/src/settings.ts, device.ts):
//! - `enableDeviceSupport` is environment-wide: it is not in
//!   `PROJECT_SCOPED_SERVER_SETTING_KEYS`, so a project scope cannot write it.
//! - `enableAgentDeviceAccess` is project-scoped. A project override lives in
//!   `projectSettingsOverrides[projectId].enableAgentDeviceAccess`, and the effective value is the
//!   override, else the environment's, else `false` (`DEFAULT_SERVER_SETTINGS`). No `t3.json`
//!   layer exists for it (`PROJECT_FILE_BACKED_SETTINGS`).
//! - `deviceHosts`, `easDeviceHosts` and `simlockDeviceHosts` are environment-wide lists, written
//!   with `server.updateSettings`. The hub and agent switches write through `device.configure`
//!   outside a project scope, because the server starts or stops the hub with them.
//! - The switches above and the tools' versions describe the local host: `hosts[kind=local].tools`
//!   of the `DeviceServiceState` that `device.list` returns.

use crate::settings::{SearchItem, Section};
use serde::{Deserialize, Deserializer, Serialize, de::DeserializeOwned};
use serde_json::{Map, Value, json};
use std::collections::HashMap;

/// Call tokens of the Devices tab's requests start here.
pub const TOKEN_BASE: u64 = 1 << 45;

const fn item(id: &'static str, title: &'static str, terms: &'static str) -> SearchItem {
    SearchItem {
        id,
        title,
        section: Section::Devices,
        terms,
        desktop_only: false,
    }
}

/// The Devices tab's `searchableSetting` entries (settingsSearch.ts:845-872).
pub const DEVICE_ITEMS: &[SearchItem] = &[
    item(
        "device-sources",
        "Device sources",
        "device hosts ssh remote mac mini identity key connection eas expo cloud simlock pool gateway local simulators",
    ),
    item(
        "agent-device-access",
        "Agents can use devices",
        "allow simulator emulator ios android drive tools sessions",
    ),
    item(
        "device-hub",
        "Use devices",
        "device hub simulator emulator ios android install start version",
    ),
    item(
        "device-platform-support",
        "Local simulators",
        "simulator support xcode android studio sdk avd runtime",
    ),
];

/// The two switches of the Devices card.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Hub,
    Agent,
}

impl Key {
    pub fn wire(self) -> &'static str {
        match self {
            Self::Hub => "enableDeviceSupport",
            Self::Agent => "enableAgentDeviceAccess",
        }
    }

    /// `isProjectScopedSettingKey`.
    pub fn project_scoped(self) -> bool {
        matches!(self, Self::Agent)
    }
}

/// `lenient`: a host that does not decode is dropped, as the web drops an undecodable entry.
fn lenient<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let items = Value::deserialize(deserializer)?;
    Ok(match items {
        Value::Array(items) => items
            .into_iter()
            .filter_map(|item| serde_json::from_value(item).ok())
            .collect(),
        _ => Vec::new(),
    })
}

fn ssh_kind() -> String {
    "ssh".to_owned()
}

/// `SshDeviceHostConfig` (device.ts:21-30).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshHost {
    #[serde(default = "ssh_kind")]
    pub kind: String,
    pub id: String,
    pub label: String,
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
}

/// `EasDeviceHostConfig` (device.ts:32-67). The token is kept as the server sent it: a saved
/// token arrives redacted and sending it back keeps it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EasHost {
    pub kind: String,
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
    #[serde(default)]
    pub access_token: Value,
    pub max_session_minutes: u32,
    pub idle_minutes: u32,
    pub max_concurrent_sessions: u32,
}

/// `SimlockDeviceHostConfig` (device.ts:69-90).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimlockHost {
    pub kind: String,
    pub id: String,
    pub label: String,
    pub gateway_url: String,
    #[serde(default)]
    pub agent_token: Value,
    pub max_leases: u32,
    pub max_lease_minutes: u32,
}

/// The device keys of `server.getConfig`'s `settings`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceConfig {
    #[serde(default)]
    pub enable_device_support: bool,
    #[serde(default)]
    pub enable_agent_device_access: bool,
    #[serde(default)]
    pub simlock_device_hosts_enabled: bool,
    #[serde(default, deserialize_with = "lenient")]
    pub device_hosts: Vec<SshHost>,
    #[serde(default, deserialize_with = "lenient")]
    pub eas_device_hosts: Vec<EasHost>,
    #[serde(default, deserialize_with = "lenient")]
    pub simlock_device_hosts: Vec<SimlockHost>,
    /// Each project's whole override entry: a write replaces it, so every key is kept.
    #[serde(default, deserialize_with = "overrides")]
    pub project_settings_overrides: HashMap<String, Map<String, Value>>,
}

fn overrides<'de, D>(deserializer: D) -> Result<HashMap<String, Map<String, Value>>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(match value {
        Value::Object(entries) => entries
            .into_iter()
            .filter_map(|(id, entry)| match entry {
                Value::Object(entry) => Some((id, entry)),
                _ => None,
            })
            .collect(),
        _ => HashMap::new(),
    })
}

impl DeviceConfig {
    /// Reads the device keys out of a `settings` object; anything else gives the defaults.
    pub fn from_settings(settings: &Value) -> Self {
        serde_json::from_value(settings.clone()).unwrap_or_default()
    }

    pub fn environment_value(&self, key: Key) -> bool {
        match key {
            Key::Hub => self.enable_device_support,
            Key::Agent => self.enable_agent_device_access,
        }
    }

    /// The project's override of the key, when it has one.
    pub fn project_override(&self, key: Key, project: &str) -> Option<bool> {
        if !key.project_scoped() {
            return None;
        }
        self.project_settings_overrides
            .get(project)?
            .get(key.wire())?
            .as_bool()
    }

    /// `resolveProjectSettings` for the one key: the override, else the environment's value.
    #[cfg(test)]
    pub fn effective(&self, key: Key, project: Option<&str>) -> bool {
        project
            .and_then(|project| self.project_override(key, project))
            .unwrap_or_else(|| self.environment_value(key))
    }
}

/// Which layer an inheritance row is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerKind {
    Project,
    Environment,
    Default,
}

/// `InheritanceLayer` (SettingInheritance.tsx:29-35).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layer {
    pub kind: LayerKind,
    pub label: String,
    pub value: String,
    pub effective: bool,
    pub set: bool,
}

fn on_off(value: bool) -> String {
    if value { "On" } else { "Off" }.to_owned()
}

/// `settingInheritanceLayers` (SettingInheritance.tsx:105-147) for a boolean key whose default
/// is `false`. `project` is the project scope's override, `None` outside a project scope.
pub fn layers(
    key: Key,
    environment_label: &str,
    in_project: bool,
    project_override: Option<bool>,
    environment: bool,
) -> Vec<Layer> {
    let from_project = key.project_scoped() && project_override.is_some();
    let environment_set = environment;
    let mut out = Vec::new();
    if in_project && key.project_scoped() {
        out.push(Layer {
            kind: LayerKind::Project,
            label: "Project".to_owned(),
            value: match project_override {
                Some(value) => on_off(value),
                None => "Inherits".to_owned(),
            },
            effective: from_project,
            set: from_project,
        });
    }
    out.push(Layer {
        kind: LayerKind::Environment,
        label: environment_label.to_owned(),
        value: if environment_set {
            on_off(environment)
        } else {
            "Inherits".to_owned()
        },
        effective: !from_project && environment_set,
        set: environment_set,
    });
    out.push(Layer {
        kind: LayerKind::Default,
        label: "Default".to_owned(),
        value: on_off(false),
        effective: !from_project && !environment_set,
        set: true,
    });
    out
}

/// `SettingInheritanceState` (SettingInheritance.tsx:149-154).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Default,
    Environment,
    Inherited,
    Overridden,
}

/// The tooltip and marker state of a row (settingsLayout.tsx:381-395). `environment_name` is the
/// selected environment's label, or "environment" when several are in scope.
pub fn inheritance(
    key: Key,
    in_project: bool,
    project_override: Option<bool>,
    environment: bool,
    environment_name: &str,
) -> (State, String) {
    if in_project && key.project_scoped() {
        return if project_override.is_some() {
            (State::Overridden, "Overridden for this project".to_owned())
        } else {
            (
                State::Inherited,
                format!("Inherited from {environment_name}"),
            )
        };
    }
    if environment {
        (State::Environment, "Set on the environment".to_owned())
    } else {
        (State::Default, "Built-in default".to_owned())
    }
}

/// The marker waits for hover or focus unless it explains something (settingsLayout.tsx:455-459).
pub fn marker_always_visible(state: State) -> bool {
    !matches!(state, State::Default | State::Environment)
}

/// `server.updateSettings` payload that sets a project override of the agent switch: the whole
/// entry is replaced, so the other keys of it ride along.
pub fn override_patch(
    config: &DeviceConfig,
    project: &str,
    key: Key,
    value: Option<bool>,
) -> Value {
    let mut entry = config
        .project_settings_overrides
        .get(project)
        .cloned()
        .unwrap_or_default();
    match value {
        Some(value) => {
            entry.insert(key.wire().to_owned(), json!(value));
        }
        None => {
            entry.remove(key.wire());
        }
    }
    let entry = if entry.is_empty() {
        Value::Null
    } else {
        Value::Object(entry)
    };
    json!({"patch": {"projectSettingsOverrides": {project: entry}}})
}

/// `device.configure` for a switch: turning the hub on completes onboarding, and turning it off
/// also turns off agent access (IntegrationsSettings.tsx:850-852, 932-939).
pub fn configure_payload(key: Key, checked: bool) -> Value {
    match key {
        Key::Hub => {
            let mut payload = json!({"enabled": checked});
            if checked {
                payload["onboardingCompleted"] = json!(true);
            } else {
                payload["agentAccessEnabled"] = json!(false);
            }
            payload
        }
        Key::Agent => json!({"agentAccessEnabled": checked}),
    }
}

/// `DeviceHostStatus` (device.ts:223-231).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostStatus {
    Disabled,
    Idle,
    Installing,
    Starting,
    Ready,
    Failed,
}

impl HostStatus {
    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "disabled" => Self::Disabled,
            "idle" => Self::Idle,
            "installing" => Self::Installing,
            "starting" => Self::Starting,
            "ready" => Self::Ready,
            "failed" => Self::Failed,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostState {
    pub status: HostStatus,
    pub detail: Option<String>,
    pub android_incomplete: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Ios,
    Android,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Availability {
    pub platform: Platform,
    pub available: bool,
    pub reason: Option<String>,
}

/// `DeviceToolVersion` (device.ts:206-210).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolVersion {
    pub required: String,
    pub installed: Vec<String>,
    pub running: Option<String>,
}

impl ToolVersion {
    /// The version shown: the running one, else the required one when installed, else the
    /// highest installed (DeviceToolVersions.tsx:21-30).
    pub fn version(&self) -> Option<String> {
        if let Some(running) = &self.running {
            return Some(running.clone());
        }
        if self.installed.contains(&self.required) {
            return Some(self.required.clone());
        }
        let mut sorted = self.installed.clone();
        sorted.sort_by(|a, b| numeric_order(a, b));
        sorted.pop()
    }

    pub fn needs_update(&self) -> bool {
        !self.installed.contains(&self.required)
    }
}

/// `localeCompare(b, undefined, {numeric: true})` for dotted versions.
fn numeric_order(a: &str, b: &str) -> std::cmp::Ordering {
    let parts = |text: &str| -> Vec<String> {
        let mut out = Vec::new();
        let mut current = String::new();
        let mut digits = None;
        for ch in text.chars() {
            let is_digit = ch.is_ascii_digit();
            if digits.is_some_and(|d| d != is_digit) {
                out.push(std::mem::take(&mut current));
            }
            digits = Some(is_digit);
            current.push(ch);
        }
        out.push(current);
        out
    };
    let (left, right) = (parts(a), parts(b));
    for (x, y) in left.iter().zip(right.iter()) {
        let order = match (x.parse::<u64>(), y.parse::<u64>()) {
            (Ok(x), Ok(y)) => x.cmp(&y),
            _ => x.cmp(y),
        };
        if order != std::cmp::Ordering::Equal {
            return order;
        }
    }
    left.len().cmp(&right.len())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tools {
    pub hub: ToolVersion,
    pub agent: ToolVersion,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Hub,
    Agent,
}

impl Tool {
    /// `updateTool` of `device.list`.
    pub fn wire(self) -> &'static str {
        match self {
            Self::Hub => "hub",
            Self::Agent => "agent",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Hub => "Device hub",
            Self::Agent => "Agent device",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostSummary {
    pub id: String,
    pub kind: String,
    pub platforms: Vec<Availability>,
    pub tools: Option<Tools>,
}

/// `DeviceServiceState` (device.ts:253-296), reduced to what the tab reads.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeviceState {
    pub supports_host_retry: bool,
    pub supports_tool_update: bool,
    pub supports_tool_inspection: bool,
    pub supports_eas_hosts: bool,
    pub supports_simlock_hosts: bool,
    pub host_status: Option<HostStatus>,
    pub host_statuses: HashMap<String, HostState>,
    pub hosts: Vec<HostSummary>,
}

fn string(value: &Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(str::to_owned)
}

fn flag(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn tool_version(value: &Value) -> Option<ToolVersion> {
    Some(ToolVersion {
        required: string(value, "requiredVersion")?,
        installed: value
            .get("installedVersions")?
            .as_array()?
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect(),
        running: string(value, "runningVersion"),
    })
}

pub fn host_summary(value: &Value) -> Option<HostSummary> {
    let platforms = value
        .get("platforms")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let platform = match item.get("platform")?.as_str()? {
                        "ios" => Platform::Ios,
                        "android" => Platform::Android,
                        _ => return None,
                    };
                    Some(Availability {
                        platform,
                        available: flag(item, "available"),
                        reason: string(item, "reason"),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let tools = value.get("tools").and_then(|tools| {
        Some(Tools {
            hub: tool_version(tools.get("hub")?)?,
            agent: tool_version(tools.get("agent")?)?,
        })
    });
    Some(HostSummary {
        id: string(value, "id")?,
        kind: string(value, "kind")?,
        platforms,
        tools,
    })
}

impl DeviceState {
    /// Decodes `device.list` and `device.configure` results. The wire form keeps EAS and Simlock
    /// summaries apart; decoded, every host is in `hosts` (device.ts:304-306).
    pub fn parse(value: &Value) -> Self {
        let hosts = ["hosts", "easHosts", "simlockHosts"]
            .iter()
            .filter_map(|key| value.get(*key)?.as_array())
            .flatten()
            .filter_map(host_summary)
            .collect();
        let host_statuses = value
            .get("hostStatuses")
            .and_then(Value::as_object)
            .map(|statuses| {
                statuses
                    .iter()
                    .filter_map(|(id, status)| {
                        Some((
                            id.clone(),
                            HostState {
                                status: HostStatus::parse(status.get("status")?.as_str()?)?,
                                detail: string(status, "detail"),
                                android_incomplete: flag(status, "androidDiscoveryIncomplete"),
                            },
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self {
            supports_host_retry: flag(value, "supportsHostRetry"),
            supports_tool_update: flag(value, "supportsToolUpdate"),
            supports_tool_inspection: flag(value, "supportsToolInspection"),
            supports_eas_hosts: flag(value, "supportsEasHosts"),
            supports_simlock_hosts: flag(value, "supportsSimlockHosts"),
            host_status: value
                .get("hostStatus")
                .and_then(Value::as_str)
                .and_then(HostStatus::parse),
            host_statuses,
            hosts,
        }
    }

    pub fn local(&self) -> Option<&HostSummary> {
        self.hosts.iter().find(|host| host.kind == "local")
    }

    /// `deviceHubReadiness` (DeviceSetup.tsx:36-45): whether a hub is installing or starting
    /// with none ready.
    pub fn busy(&self) -> bool {
        let statuses = self.host_statuses.values().map(|host| host.status);
        let all: Vec<HostStatus> = self.host_status.into_iter().chain(statuses).collect();
        let ready = all.contains(&HostStatus::Ready);
        !ready
            && all
                .iter()
                .any(|s| matches!(s, HostStatus::Installing | HostStatus::Starting))
    }

    /// `DeviceToolVersions`'s trigger text for a tool of the local host.
    pub fn tool_label(&self, tool: Tool) -> String {
        if self.local().and_then(|host| host.tools.as_ref()).is_none() {
            return "Version unknown".to_owned();
        }
        match self.tool(tool).and_then(ToolVersion::version) {
            Some(version) => format!("v{version}"),
            None => "Not installed".to_owned(),
        }
    }

    pub fn tool(&self, tool: Tool) -> Option<&ToolVersion> {
        let tools = self.local()?.tools.as_ref()?;
        Some(match tool {
            Tool::Hub => &tools.hub,
            Tool::Agent => &tools.agent,
        })
    }
}

/// `SourceStatus` tones (DeviceSourcesSettings.tsx:58-63).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Pending,
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceStatus {
    pub tone: Tone,
    pub label: String,
    pub detail: Option<String>,
    pub action: Option<StatusAction>,
}

/// The action offered next to a failed source (DeviceSourcesSettings.tsx:269-330).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StatusAction {
    /// `device.list {}`.
    Refresh,
    /// `device.list {retryHostId}`.
    Retry(String),
}

/// `hostStatus` (DeviceSourcesSettings.tsx:109-126): what a configured host is doing, when it is
/// not simply working.
pub fn host_status(state: Option<&DeviceState>, host_id: &str) -> Option<SourceStatus> {
    let Some(state) = state else {
        return Some(SourceStatus {
            tone: Tone::Pending,
            label: "Loading".to_owned(),
            detail: None,
            action: None,
        });
    };
    let status = state.host_statuses.get(host_id)?;
    let detail = status.detail.clone();
    match status.status {
        HostStatus::Ready if status.android_incomplete => Some(SourceStatus {
            tone: Tone::Warning,
            label: "Android list may be incomplete".to_owned(),
            detail,
            action: None,
        }),
        HostStatus::Installing | HostStatus::Starting => Some(SourceStatus {
            tone: Tone::Pending,
            label: "Starting".to_owned(),
            detail,
            action: None,
        }),
        HostStatus::Failed => Some(SourceStatus {
            tone: Tone::Error,
            label: "Failed".to_owned(),
            detail,
            action: None,
        }),
        _ => None,
    }
}

/// The kind of a source row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    Local,
    Ssh(SshHost),
    Eas(EasHost),
    Simlock(SimlockHost),
}

/// One row of the Device sources card, as `DeviceSourcesSettings` builds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub key: String,
    pub source: Source,
    pub title: String,
    pub kind: Option<&'static str>,
    pub description: String,
    pub status: Option<SourceStatus>,
    pub platforms: Vec<Availability>,
}

/// `easSessionLimits` (easDeviceHostsSettings.logic.ts:38-42).
pub fn eas_session_limits(host: &EasHost) -> String {
    let sessions = if host.max_concurrent_sessions == 1 {
        "1 session".to_owned()
    } else {
        format!("{} sessions", host.max_concurrent_sessions)
    };
    format!(
        "Up to {sessions} at once, each stopped after {} min or {} idle min",
        host.max_session_minutes, host.idle_minutes
    )
}

/// `simlockLeaseLimits` (simlockDeviceHostsSettings.logic.ts:46-49).
pub fn simlock_lease_limits(host: &SimlockHost) -> String {
    let devices = if host.max_leases == 1 {
        "1 device".to_owned()
    } else {
        format!("{} devices", host.max_leases)
    };
    format!(
        "Up to {devices} at once, each released after {} min",
        host.max_lease_minutes
    )
}

/// The rows of the Device sources card for the attached environment, in the web's order: the
/// local source, the SSH hosts, EAS hosts when the server supports them, and Simlock hosts when
/// it supports them and the Labs switch is on (DeviceSourcesSettings.tsx:269-455).
pub fn rows(
    config: &DeviceConfig,
    state: Option<&DeviceState>,
    environments: usize,
    active: bool,
) -> Vec<Row> {
    let platforms = |id: &str| -> Vec<Availability> {
        if !active {
            return Vec::new();
        }
        state
            .and_then(|state| state.hosts.iter().find(|host| host.id == id))
            .map(|host| host.platforms.clone())
            .unwrap_or_default()
    };
    let mut rows = Vec::new();
    let local = state.and_then(DeviceState::local);
    rows.push(Row {
        key: "local".to_owned(),
        source: Source::Local,
        title: "Local simulators".to_owned(),
        kind: None,
        description: if environments > 1 {
            "Simulators and emulators installed on each environment's own machine."
        } else {
            "Simulators and emulators installed on this environment's machine."
        }
        .to_owned(),
        status: active
            .then(|| host_status(state, local.map_or("", |host| host.id.as_str())))
            .flatten()
            .map(|mut status| {
                if status.tone == Tone::Error {
                    status.action = Some(StatusAction::Refresh);
                }
                status
            }),
        platforms: if active {
            local.map(|host| host.platforms.clone()).unwrap_or_default()
        } else {
            Vec::new()
        },
    });
    for host in &config.device_hosts {
        let status = active
            .then(|| host_status(state, &host.id))
            .flatten()
            .map(|mut status| {
                if status.tone == Tone::Error && state.is_some_and(|s| s.supports_host_retry) {
                    status.label = "Can't connect".to_owned();
                    status.action = Some(StatusAction::Retry(host.id.clone()));
                }
                status
            });
        rows.push(Row {
            key: format!("ssh:{}", host.id),
            source: Source::Ssh(host.clone()),
            title: host.label.clone(),
            kind: Some("SSH"),
            description: host.target.clone(),
            status,
            platforms: platforms(&host.id),
        });
    }
    if state.is_some_and(|s| s.supports_eas_hosts) {
        for host in &config.eas_device_hosts {
            rows.push(Row {
                key: format!("eas:{}", host.id),
                source: Source::Eas(host.clone()),
                title: host.label.clone(),
                kind: Some("EAS"),
                description: eas_session_limits(host),
                status: active.then(|| host_status(state, &host.id)).flatten(),
                platforms: platforms(&host.id),
            });
        }
    }
    if state.is_some_and(|s| s.supports_simlock_hosts) && config.simlock_device_hosts_enabled {
        for host in &config.simlock_device_hosts {
            let url = host
                .gateway_url
                .trim_start_matches("https://")
                .trim_start_matches("http://");
            rows.push(Row {
                key: format!("simlock:{}", host.id),
                source: Source::Simlock(host.clone()),
                title: host.label.clone(),
                kind: Some("Simlock"),
                description: format!("{url} \u{b7} {}", simlock_lease_limits(host)),
                status: active.then(|| host_status(state, &host.id)).flatten(),
                platforms: platforms(&host.id),
            });
        }
    }
    rows
}

/// The items of the Add source menu (DeviceSourcesSettings.tsx:521-557).
pub fn add_menu(
    state: Option<&DeviceState>,
    simlock_enabled: bool,
) -> Vec<(AddKind, &'static str)> {
    let mut items = vec![(AddKind::Ssh, "Machine over SSH")];
    if state.is_some_and(|s| s.supports_eas_hosts) {
        items.push((AddKind::Eas, "EAS Simulator"));
    }
    if state.is_some_and(|s| s.supports_simlock_hosts) && simlock_enabled {
        items.push((AddKind::Simlock, "Simlock pool (alpha)"));
    }
    items
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddKind {
    Ssh,
    Eas,
    Simlock,
}

/// A host id the server accepts (`ConfiguredDeviceHostId`): an alphanumeric first character,
/// then letters, digits, `_` and `-`, never `local`.
pub fn new_host_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!("h{:x}-{:x}", nanos, COUNTER.fetch_add(1, Ordering::Relaxed))
}

pub fn valid_host_id(id: &str) -> bool {
    let mut chars = id.chars();
    id != "local"
        && id.len() <= 128
        && chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

pub fn new_eas_host(id: String) -> EasHost {
    EasHost {
        kind: "eas".to_owned(),
        id,
        label: "EAS Simulator".to_owned(),
        app_id: None,
        access_token: json!(""),
        max_session_minutes: 30,
        idle_minutes: 10,
        max_concurrent_sessions: 2,
    }
}

pub fn new_simlock_host(id: String) -> SimlockHost {
    SimlockHost {
        kind: "simlock".to_owned(),
        id,
        label: "Simlock".to_owned(),
        gateway_url: String::new(),
        agent_token: json!(""),
        max_leases: 2,
        max_lease_minutes: 60,
    }
}

/// `parseDeviceHostDraft` (deviceHostConnectionChecks.ts:23-29): the host the form describes,
/// trimmed, or `None` when the server would reject it.
pub fn parse_ssh_draft(host: &SshHost) -> Option<SshHost> {
    let label = host.label.trim();
    let target = host.target.trim();
    let identity = host
        .identity_file
        .as_deref()
        .map(str::trim)
        .filter(|file| !file.is_empty());
    let target_ok =
        !target.is_empty() && !target.starts_with('-') && !target.chars().any(char::is_whitespace);
    let port_ok = host.port.is_none_or(|port| port >= 1);
    (valid_host_id(&host.id) && !label.is_empty() && target_ok && port_ok).then(|| SshHost {
        kind: "ssh".to_owned(),
        id: host.id.clone(),
        label: label.to_owned(),
        target: target.to_owned(),
        identity_file: identity.map(str::to_owned),
        port: host.port,
    })
}

/// The host a connection test or save checks: the label falls back to the target while it is
/// blank, so the check can run before the name is typed (DeviceHostEditor.tsx).
pub fn ssh_test_host(draft: &SshHost) -> Option<SshHost> {
    let mut host = draft.clone();
    if host.label.trim().is_empty() {
        host.label = host.target.clone();
    }
    parse_ssh_draft(&host)
}

/// `updateDeviceHosts` (deviceHostsSettings.logic.ts:56-85) for a single environment: replaces
/// the host with the same id, or the one at the same SSH destination, or appends.
pub fn update_ssh_hosts(
    hosts: &[SshHost],
    host: &SshHost,
    remove: bool,
    original: &SshHost,
) -> Result<Vec<SshHost>, String> {
    let same_destination = |a: &SshHost, b: &SshHost| {
        a.target == b.target && a.port == b.port && a.identity_file == b.identity_file
    };
    let find_destination = |destination: &SshHost| -> Result<Option<&SshHost>, String> {
        let matches: Vec<&SshHost> = hosts
            .iter()
            .filter(|candidate| same_destination(candidate, destination))
            .collect();
        if matches.len() > 1 {
            return Err(
                "Multiple hosts match this SSH destination. Select the environment to edit its hosts."
                    .to_owned(),
            );
        }
        Ok(matches.first().copied())
    };
    let existing = match hosts.iter().find(|candidate| candidate.id == original.id) {
        Some(found) => Some(found),
        None => match find_destination(original)? {
            Some(found) => Some(found),
            None if remove => None,
            None => find_destination(host)?,
        },
    };
    if remove {
        return Ok(hosts
            .iter()
            .filter(|candidate| Some(candidate.id.as_str()) != existing.map(|e| e.id.as_str()))
            .cloned()
            .collect());
    }
    Ok(match existing {
        Some(existing) => hosts
            .iter()
            .map(|candidate| {
                if candidate.id == existing.id {
                    SshHost {
                        id: existing.id.clone(),
                        ..host.clone()
                    }
                } else {
                    candidate.clone()
                }
            })
            .collect(),
        None => hosts.iter().cloned().chain([host.clone()]).collect(),
    })
}

/// `updateEasDeviceHosts` (easDeviceHostsSettings.logic.ts:20-31).
pub fn update_eas_hosts(
    hosts: &[EasHost],
    host: &EasHost,
    remove: bool,
    keep_token: bool,
) -> Vec<EasHost> {
    if remove {
        return hosts.iter().filter(|c| c.id != host.id).cloned().collect();
    }
    let existing = hosts.iter().find(|c| c.id == host.id);
    let mut next = host.clone();
    if keep_token {
        next.access_token = existing.map_or_else(|| json!(""), |e| e.access_token.clone());
    }
    match existing {
        Some(_) => hosts
            .iter()
            .map(|c| {
                if c.id == host.id {
                    next.clone()
                } else {
                    c.clone()
                }
            })
            .collect(),
        None => hosts.iter().cloned().chain([next]).collect(),
    }
}

/// `updateSimlockDeviceHosts` (simlockDeviceHostsSettings.logic.ts:20-28).
pub fn update_simlock_hosts(
    hosts: &[SimlockHost],
    host: &SimlockHost,
    remove: bool,
) -> Vec<SimlockHost> {
    if remove {
        return hosts.iter().filter(|c| c.id != host.id).cloned().collect();
    }
    if hosts.iter().any(|c| c.id == host.id) {
        hosts
            .iter()
            .map(|c| {
                if c.id == host.id {
                    host.clone()
                } else {
                    c.clone()
                }
            })
            .collect()
    } else {
        hosts.iter().cloned().chain([host.clone()]).collect()
    }
}

/// EAS limits the form's ranges allow (device.ts:48-67): the label is set, sessions 1..=20,
/// session minutes 1..=115, idle minutes 1..=60, and the idle limit is below the session limit.
/// Returns why Save is off.
pub fn eas_problem(host: &EasHost) -> Option<&'static str> {
    if host.max_session_minutes > 0 && host.idle_minutes >= host.max_session_minutes {
        return Some("Idle minutes must be smaller than session minutes.");
    }
    let ok = !host.label.trim().is_empty()
        && (1..=115).contains(&host.max_session_minutes)
        && (1..=60).contains(&host.idle_minutes)
        && (1..=20).contains(&host.max_concurrent_sessions);
    (!ok).then_some("Give the host a label and limits within the ranges shown.")
}

/// `simlockDeviceHostProblem` (simlockDeviceHostsSettings.logic.ts:30-44).
pub fn simlock_problem(host: &SimlockHost) -> Option<&'static str> {
    if host.label.trim().is_empty() {
        return Some("Give the host a label.");
    }
    if !valid_gateway(host.gateway_url.trim()) {
        return Some("Enter the gateway's origin, such as https://gateway.example.ts.net:4700.");
    }
    let limits = (1..=20).contains(&host.max_leases) && (5..=240).contains(&host.max_lease_minutes);
    (!limits).then_some("Keep the limits within the ranges shown.")
}

/// `^https?:\/\/[^\s/]+\/?$`.
fn valid_gateway(url: &str) -> bool {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"));
    let Some(rest) = rest else {
        return false;
    };
    let host = rest.strip_suffix('/').unwrap_or(rest);
    !host.is_empty() && !host.chars().any(|c| c.is_whitespace() || c == '/')
}

/// `server.updateSettings` payload for one host list.
pub fn hosts_patch<T: Serialize>(key: &str, hosts: &[T]) -> Value {
    json!({"patch": {key: hosts}})
}

/// `device.testHost` payload (an `SshDeviceHostConfig`).
pub fn test_payload(host: &SshHost) -> Value {
    serde_json::to_value(host).unwrap_or(Value::Null)
}

/// Why the Add source and Edit controls are off (DeviceSourcesSettings.tsx:152-156): a project
/// scope, a write in flight, or no connected environment.
pub fn sources_disabled(in_project: bool, busy: bool, connected: bool) -> bool {
    in_project || busy || !connected
}

/// The scope's project keys to the environment's project ids that hold the overrides.
pub type ProjectIds = HashMap<String, Vec<String>>;

/// Project scope key (`logical_key`, else `key`) to every project of the group.
pub fn project_ids(entries: &[crate::projects::ProjectEntry]) -> ProjectIds {
    let mut map = ProjectIds::new();
    for entry in entries.iter().filter(|e| !e.scratch) {
        let key = entry
            .logical_key
            .clone()
            .unwrap_or_else(|| entry.key.clone());
        map.entry(key)
            .or_default()
            .extend(entry.member_ids.iter().cloned());
    }
    map
}

/// Merges the `projectSettingsOverrides` entries of several patches into one.
pub fn merge_override_patches(patches: Vec<Value>) -> Value {
    let mut merged = Map::new();
    for patch in patches {
        if let Some(entries) = patch["patch"]["projectSettingsOverrides"].as_object() {
            merged.extend(entries.clone());
        }
    }
    json!({"patch": {"projectSettingsOverrides": merged}})
}

/// Why a switch of the tab is inert, or that its write reaches the attached environment
/// (settingsLayout.tsx:405-448). The hub is environment-wide; the agent switch is project-scoped.
pub fn reach(
    key: Key,
    search: &crate::settings_labs::ScopeSearch,
    data: &crate::settings_labs::ScopeData,
) -> Result<(), String> {
    use crate::settings_labs::{Scope, resolve};
    let writable = |id: &str| {
        data.environments
            .iter()
            .any(|e| e.id == id && e.connected && e.writable)
    };
    let reconnect = || "Reconnect the selected environment to change this setting.".to_owned();
    match resolve(search, data) {
        Scope::Unavailable(message) => Err(message.to_owned()),
        Scope::Project(_) if !key.project_scoped() => {
            Err("Environment-wide setting. Select an environment to change it.".to_owned())
        }
        Scope::Project(ids) => ids
            .iter()
            .any(|id| writable(id))
            .then_some(())
            .ok_or_else(reconnect),
        Scope::Environment(id) => match data.environments.iter().find(|e| e.id == id) {
            Some(e) if !e.connected => Err(reconnect()),
            Some(e) if !e.writable => Err(format!(
                "Settings of {} cannot be changed from this app yet.",
                e.label
            )),
            _ => Ok(()),
        },
        Scope::All => data
            .environments
            .iter()
            .any(|e| e.connected && e.writable)
            .then_some(())
            .ok_or_else(reconnect),
    }
}

impl DeviceState {
    /// The pending status beside a switch (DeviceSetup.tsx:271-361, compact).
    pub fn setup_label(&self) -> &'static str {
        match self.host_status {
            Some(HostStatus::Installing) => "Installing\u{2026}",
            Some(HostStatus::Starting) => "Starting\u{2026}",
            _ => "Updating\u{2026}",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ssh(id: &str, target: &str) -> SshHost {
        SshHost {
            kind: "ssh".to_owned(),
            id: id.to_owned(),
            label: format!("Host {id}"),
            target: target.to_owned(),
            identity_file: None,
            port: None,
        }
    }

    #[test]
    fn config_reads_the_device_keys_and_drops_bad_hosts() {
        let config = DeviceConfig::from_settings(&json!({
            "enableDeviceSupport": true,
            "deviceHosts": [
                {"id": "a", "label": "A", "target": "user@example"},
                {"id": 3},
            ],
            "easDeviceHosts": [{
                "kind": "eas", "id": "e", "label": "E", "accessToken": "redacted",
                "maxSessionMinutes": 30, "idleMinutes": 10, "maxConcurrentSessions": 2
            }],
            "projectSettingsOverrides": {
                "p1": {"enableAgentDeviceAccess": true, "defaultRuntimeMode": "full-access"},
                "p2": null
            }
        }));
        assert!(config.enable_device_support);
        assert_eq!(
            config.device_hosts,
            vec![ssh("a", "user@example").tap(|h| h.label = "A".into())]
        );
        assert_eq!(config.eas_device_hosts.len(), 1);
        assert_eq!(config.project_override(Key::Agent, "p1"), Some(true));
        assert_eq!(config.project_override(Key::Agent, "p2"), None);
        assert_eq!(config.project_override(Key::Hub, "p1"), None);
    }

    trait Tap: Sized {
        fn tap(mut self, change: impl FnOnce(&mut Self)) -> Self {
            change(&mut self);
            self
        }
    }
    impl Tap for SshHost {}

    #[test]
    fn effective_value_prefers_the_project_override() {
        let config = DeviceConfig::from_settings(&json!({
            "enableAgentDeviceAccess": true,
            "projectSettingsOverrides": {"p": {"enableAgentDeviceAccess": false}}
        }));
        assert!(!config.effective(Key::Agent, Some("p")));
        assert!(config.effective(Key::Agent, Some("other")));
        assert!(config.effective(Key::Agent, None));
    }

    #[test]
    fn layers_mark_the_effective_one() {
        let inherited = layers(Key::Agent, "Example Mac", true, None, true);
        assert_eq!(
            inherited
                .iter()
                .map(|l| (l.kind, l.value.as_str(), l.effective))
                .collect::<Vec<_>>(),
            vec![
                (LayerKind::Project, "Inherits", false),
                (LayerKind::Environment, "On", true),
                (LayerKind::Default, "Off", false),
            ]
        );
        let overridden = layers(Key::Agent, "Example Mac", true, Some(false), true);
        assert!(overridden[0].effective && overridden[0].value == "Off");
        assert!(!overridden[1].effective);
        let default = layers(Key::Agent, "Example Mac", false, None, false);
        assert_eq!(default.len(), 2);
        assert_eq!(default[0].value, "Inherits");
        assert!(default[1].effective);
        let hub = layers(Key::Hub, "Example Mac", true, None, true);
        assert!(hub.iter().all(|l| l.kind != LayerKind::Project));
    }

    #[test]
    fn inheritance_summaries_follow_the_row() {
        assert_eq!(
            inheritance(Key::Agent, true, None, true, "Example Mac"),
            (State::Inherited, "Inherited from Example Mac".to_owned())
        );
        assert_eq!(
            inheritance(Key::Agent, true, Some(true), false, "x").0,
            State::Overridden
        );
        assert_eq!(
            inheritance(Key::Hub, true, None, true, "x").0,
            State::Environment
        );
        assert_eq!(
            inheritance(Key::Hub, false, None, false, "x").0,
            State::Default
        );
        assert!(marker_always_visible(State::Inherited));
        assert!(!marker_always_visible(State::Default));
    }

    #[test]
    fn override_patch_replaces_the_whole_entry() {
        let config = DeviceConfig::from_settings(&json!({
            "projectSettingsOverrides": {"p": {"defaultRuntimeMode": "full-access"}}
        }));
        assert_eq!(
            override_patch(&config, "p", Key::Agent, Some(true)),
            json!({"patch": {"projectSettingsOverrides": {"p": {
                "defaultRuntimeMode": "full-access", "enableAgentDeviceAccess": true
            }}}})
        );
        let only = DeviceConfig::from_settings(&json!({
            "projectSettingsOverrides": {"p": {"enableAgentDeviceAccess": true}}
        }));
        assert_eq!(
            override_patch(&only, "p", Key::Agent, None),
            json!({"patch": {"projectSettingsOverrides": {"p": null}}})
        );
    }

    #[test]
    fn configure_payloads_match_the_switches() {
        assert_eq!(
            configure_payload(Key::Hub, true),
            json!({"enabled": true, "onboardingCompleted": true})
        );
        assert_eq!(
            configure_payload(Key::Hub, false),
            json!({"enabled": false, "agentAccessEnabled": false})
        );
        assert_eq!(
            configure_payload(Key::Agent, true),
            json!({"agentAccessEnabled": true})
        );
    }

    fn state() -> DeviceState {
        DeviceState::parse(&json!({
            "supportsEasHosts": true,
            "supportsToolUpdate": true,
            "hostStatus": "idle",
            "hostStatuses": {
                "a": {"status": "failed", "detail": "no route"},
                "b": {"status": "ready", "androidDiscoveryIncomplete": true},
                "c": {"status": "starting"}
            },
            "hosts": [{
                "id": "local", "kind": "local", "label": "This machine",
                "platforms": [{"platform": "ios", "available": true},
                              {"platform": "android", "available": false, "reason": "No SDK"}],
                "tools": {
                    "hub": {"requiredVersion": "1.2.0", "installedVersions": ["1.1.0", "1.10.0"], "runningVersion": null},
                    "agent": {"requiredVersion": "2.0.0", "installedVersions": [], "runningVersion": null}
                },
                "hubInstalled": true, "agentDeviceInstalled": false
            }],
            "easHosts": [{"id": "e", "kind": "eas", "label": "E", "platforms": []}]
        }))
    }

    #[test]
    fn state_reports_tool_versions_like_the_web() {
        let state = state();
        assert_eq!(state.hosts.len(), 2);
        assert_eq!(state.tool_label(Tool::Hub), "v1.10.0");
        assert_eq!(state.tool_label(Tool::Agent), "Not installed");
        assert!(state.tool(Tool::Hub).unwrap().needs_update());
        assert_eq!(
            DeviceState::default().tool_label(Tool::Hub),
            "Version unknown"
        );
        let running = ToolVersion {
            required: "1.0.0".into(),
            installed: vec!["1.0.0".into()],
            running: Some("0.9.0".into()),
        };
        assert_eq!(running.version().as_deref(), Some("0.9.0"));
    }

    #[test]
    fn busy_means_starting_with_nothing_ready() {
        let mut state = state();
        assert!(!state.busy());
        state.host_statuses.remove("b");
        assert!(state.busy());
        assert!(!DeviceState::default().busy());
    }

    #[test]
    fn host_status_maps_the_tones() {
        let state = state();
        assert_eq!(host_status(Some(&state), "a").unwrap().tone, Tone::Error);
        assert_eq!(
            host_status(Some(&state), "b").unwrap().label,
            "Android list may be incomplete"
        );
        assert_eq!(host_status(Some(&state), "c").unwrap().label, "Starting");
        assert_eq!(host_status(Some(&state), "missing"), None);
        assert_eq!(host_status(None, "a").unwrap().label, "Loading");
    }

    #[test]
    fn rows_list_the_local_source_first_and_gate_by_support() {
        let config = DeviceConfig::from_settings(&json!({
            "deviceHosts": [{"id": "a", "label": "Example Mini", "target": "user@example"}],
            "easDeviceHosts": [{
                "kind": "eas", "id": "e", "label": "EAS Simulator", "accessToken": "",
                "maxSessionMinutes": 30, "idleMinutes": 10, "maxConcurrentSessions": 1
            }],
            "simlockDeviceHosts": [{
                "kind": "simlock", "id": "s", "label": "Pool", "gatewayUrl": "https://gateway.example.test:4700",
                "maxLeases": 2, "maxLeaseMinutes": 60
            }],
        }));
        let state = state();
        let listed = rows(&config, Some(&state), 1, true);
        assert_eq!(
            listed.iter().map(|r| r.title.as_str()).collect::<Vec<_>>(),
            vec!["Local simulators", "Example Mini", "EAS Simulator"]
        );
        assert_eq!(
            listed[0].description,
            "Simulators and emulators installed on this environment's machine."
        );
        assert_eq!(listed[1].kind, Some("SSH"));
        assert_eq!(
            listed[2].description,
            "Up to 1 session at once, each stopped after 30 min or 10 idle min"
        );
        let mut with_simlock = config.clone();
        with_simlock.simlock_device_hosts_enabled = true;
        let mut supported = state.clone();
        supported.supports_simlock_hosts = true;
        let listed = rows(&with_simlock, Some(&supported), 2, true);
        assert_eq!(listed.len(), 4);
        assert_eq!(
            listed[3].description,
            "gateway.example.test:4700 \u{b7} Up to 2 devices at once, each released after 60 min"
        );
        assert!(
            listed[0]
                .description
                .contains("each environment's own machine")
        );
        let off = rows(&config, Some(&state), 1, false);
        assert!(
            off.iter()
                .all(|r| r.platforms.is_empty() && r.status.is_none())
        );
    }

    #[test]
    fn add_menu_offers_what_the_server_supports() {
        let none = add_menu(None, true);
        assert_eq!(none, vec![(AddKind::Ssh, "Machine over SSH")]);
        let mut state = state();
        assert_eq!(add_menu(Some(&state), true).len(), 2);
        state.supports_simlock_hosts = true;
        assert_eq!(add_menu(Some(&state), false).len(), 2);
        assert_eq!(
            add_menu(Some(&state), true).last(),
            Some(&(AddKind::Simlock, "Simlock pool (alpha)"))
        );
    }

    #[test]
    fn ssh_hosts_add_replace_and_remove() {
        let a = ssh("a", "user@one");
        let b = ssh("b", "user@two");
        let added = update_ssh_hosts(std::slice::from_ref(&a), &b, false, &b).unwrap();
        assert_eq!(added, vec![a.clone(), b.clone()]);
        let renamed = SshHost {
            label: "Renamed".into(),
            ..b.clone()
        };
        let replaced = update_ssh_hosts(&added, &renamed, false, &b).unwrap();
        assert_eq!(replaced[1].label, "Renamed");
        let removed = update_ssh_hosts(&replaced, &renamed, true, &renamed).unwrap();
        assert_eq!(removed, vec![a.clone()]);
        let twins = vec![
            a.clone(),
            SshHost {
                id: "c".into(),
                ..a.clone()
            },
        ];
        assert!(update_ssh_hosts(&twins, &a, false, &ssh("x", "user@one")).is_err());
        let same_destination = update_ssh_hosts(
            std::slice::from_ref(&a),
            &SshHost {
                id: "n".into(),
                ..a.clone()
            },
            false,
            &SshHost {
                id: "n".into(),
                ..a.clone()
            },
        )
        .unwrap();
        assert_eq!(same_destination.len(), 1);
        assert_eq!(same_destination[0].id, "a");
    }

    #[test]
    fn eas_and_simlock_hosts_keep_tokens_and_update_by_id() {
        let mut saved = new_eas_host("e".into());
        saved.access_token = json!("redacted");
        let mut edited = saved.clone();
        edited.access_token = json!("");
        edited.label = "Renamed".into();
        let kept = update_eas_hosts(&[saved.clone()], &edited, false, true);
        assert_eq!(kept[0].access_token, json!("redacted"));
        assert_eq!(kept[0].label, "Renamed");
        let dropped = update_eas_hosts(&[saved.clone()], &edited, false, false);
        assert_eq!(dropped[0].access_token, json!(""));
        assert!(update_eas_hosts(&[saved.clone()], &saved, true, false).is_empty());
        let simlock = new_simlock_host("s".into());
        let list = update_simlock_hosts(&[], &simlock, false);
        assert_eq!(list.len(), 1);
        assert!(update_simlock_hosts(&list, &simlock, true).is_empty());
    }

    #[test]
    fn drafts_validate_like_the_contract() {
        let mut host = ssh("a", " user@example ");
        host.label = "  Mini ".into();
        host.identity_file = Some("  ".into());
        let parsed = parse_ssh_draft(&host).unwrap();
        assert_eq!(parsed.target, "user@example");
        assert_eq!(parsed.label, "Mini");
        assert_eq!(parsed.identity_file, None);
        assert!(parse_ssh_draft(&ssh("a", "-oProxy")).is_none());
        assert!(parse_ssh_draft(&ssh("a", "two words")).is_none());
        assert!(parse_ssh_draft(&ssh("local", "user@example")).is_none());
        assert!(
            parse_ssh_draft(&SshHost {
                port: Some(0),
                ..ssh("a", "x")
            })
            .is_none()
        );
        let blank = SshHost {
            label: " ".into(),
            ..ssh("a", "user@example")
        };
        assert!(parse_ssh_draft(&blank).is_none());
        assert_eq!(ssh_test_host(&blank).unwrap().label, "user@example");
        assert!(valid_host_id(&new_host_id()));
        assert_ne!(new_host_id(), new_host_id());
    }

    #[test]
    fn limits_problems_name_the_cause() {
        let mut eas = new_eas_host("e".into());
        assert_eq!(eas_problem(&eas), None);
        eas.idle_minutes = 30;
        assert_eq!(
            eas_problem(&eas),
            Some("Idle minutes must be smaller than session minutes.")
        );
        eas.idle_minutes = 5;
        eas.label = " ".into();
        assert_eq!(
            eas_problem(&eas),
            Some("Give the host a label and limits within the ranges shown.")
        );
        let mut simlock = new_simlock_host("s".into());
        assert_eq!(
            simlock_problem(&simlock),
            Some("Enter the gateway's origin, such as https://gateway.example.ts.net:4700.")
        );
        simlock.gateway_url = "https://gateway.example.test:4700/".into();
        assert_eq!(simlock_problem(&simlock), None);
        simlock.max_leases = 40;
        assert_eq!(
            simlock_problem(&simlock),
            Some("Keep the limits within the ranges shown.")
        );
        simlock.label = "".into();
        assert_eq!(simlock_problem(&simlock), Some("Give the host a label."));
        assert!(!valid_gateway("https://host/path"));
        assert!(!valid_gateway("ftp://host"));
    }

    #[test]
    fn sources_are_off_in_a_project_scope() {
        assert!(sources_disabled(true, false, true));
        assert!(sources_disabled(false, true, true));
        assert!(sources_disabled(false, false, false));
        assert!(!sources_disabled(false, false, true));
    }

    #[test]
    fn searchable_titles_match_the_web() {
        assert_eq!(
            DEVICE_ITEMS.iter().map(|i| i.title).collect::<Vec<_>>(),
            vec![
                "Device sources",
                "Agents can use devices",
                "Use devices",
                "Local simulators"
            ]
        );
        assert!(DEVICE_ITEMS.iter().all(|i| i.section == Section::Devices));
    }
    #[test]
    fn setup_label_follows_the_host_status() {
        let mut state = DeviceState::default();
        assert_eq!(state.setup_label(), "Updating\u{2026}");
        state.host_status = Some(HostStatus::Installing);
        assert_eq!(state.setup_label(), "Installing\u{2026}");
        state.host_status = Some(HostStatus::Starting);
        assert_eq!(state.setup_label(), "Starting\u{2026}");
    }

    #[test]
    fn override_patches_merge_per_project() {
        let config = DeviceConfig::default();
        let merged = merge_override_patches(vec![
            override_patch(&config, "p1", Key::Agent, Some(true)),
            override_patch(&config, "p2", Key::Agent, None),
        ]);
        let entries = &merged["patch"]["projectSettingsOverrides"];
        assert_eq!(entries["p1"]["enableAgentDeviceAccess"], json!(true));
        assert!(entries["p2"].is_null());
    }
}
