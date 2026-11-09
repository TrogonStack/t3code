//! Device settings (`ClientSettingsSchema`, contracts/src/settings.ts:353) behind the Settings
//! route's General tab, and the route's navigation and search model.
//!
//! The web persists these through `DesktopClientSettings.ts` as `client-settings.json`; this
//! store keeps that file name and its camelCase keys, so the two stay readable by each other.

use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};

const FILE: &str = "client-settings.json";

macro_rules! choice {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => ($wire:literal, $label:literal)),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            pub fn wire(self) -> &'static str {
                match self {
                    $(Self::$variant => $wire),+
                }
            }

            pub fn label(self) -> &'static str {
                match self {
                    $(Self::$variant => $label),+
                }
            }

            pub fn from_wire(value: &str) -> Option<Self> {
                match value {
                    $($wire => Some(Self::$variant),)+
                    _ => None,
                }
            }
        }
    };
}

choice! {
    /// `ThreadNotificationMode`; labels from `threadNotifications.ts:8-13`.
    NotificationMode {
        Off => ("off", "Off"),
        Notifications => ("notifications", "Notifications only"),
        Sound => ("sound", "Sound only"),
        NotificationsAndSound => ("notifications-and-sound", "Notifications with sound"),
    }
}

/// `SidebarProjectGroupingMode`; the page shows it as an on/off switch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroupingMode {
    Repository,
    RepositoryPath,
    Separate,
}

impl GroupingMode {
    pub fn wire(self) -> &'static str {
        match self {
            Self::Repository => "repository",
            Self::RepositoryPath => "repository_path",
            Self::Separate => "separate",
        }
    }

    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "repository" => Some(Self::Repository),
            "repository_path" => Some(Self::RepositoryPath),
            "separate" => Some(Self::Separate),
            _ => None,
        }
    }
}

choice! {
    /// `SidebarProjectSortOrder`; labels from `SettingsPanels.tsx` `SIDEBAR_PROJECT_SORT_LABELS`.
    SortOrder {
        UpdatedAt => ("updated_at", "Last user message"),
        CreatedAt => ("created_at", "Created at"),
        Manual => ("manual", "Manual"),
    }
}

choice! {
    /// `ConfirmQuitMode`; labels from `SettingsPanels.tsx` `CONFIRM_QUIT_LABELS`.
    QuitMode {
        Direct => ("direct", "Direct"),
        Hold => ("hold", "Hold"),
        DoubleClick => ("double-click", "Double press"),
    }
}

choice! {
    /// `TimestampFormat`; labels from `SettingsPanels.tsx` `TIMESTAMP_FORMAT_LABELS`.
    TimeFormat {
        Locale => ("locale", "System default"),
        Hour12 => ("12-hour", "12-hour"),
        Hour24 => ("24-hour", "24-hour"),
    }
}

/// The General tab's slice of the client settings, with `DEFAULT_CLIENT_SETTINGS` defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceSettings {
    pub notification_mode: NotificationMode,
    pub in_app_notifications: bool,
    pub project_grouping: GroupingMode,
    pub project_order: SortOrder,
    pub working_shelf: bool,
    pub confirm_unpin: bool,
    pub confirm_archive: bool,
    pub confirm_delete: bool,
    pub confirm_quit: QuitMode,
    pub time_format: TimeFormat,
}

impl Default for DeviceSettings {
    fn default() -> Self {
        Self {
            notification_mode: NotificationMode::Off,
            in_app_notifications: false,
            project_grouping: GroupingMode::Repository,
            project_order: SortOrder::UpdatedAt,
            working_shelf: true,
            confirm_unpin: false,
            confirm_archive: false,
            confirm_delete: true,
            confirm_quit: QuitMode::Hold,
            time_format: TimeFormat::Locale,
        }
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

impl DeviceSettings {
    /// A missing or invalid key falls back to its default; anything but an object is rejected.
    pub fn parse(text: &str) -> Option<Self> {
        let value: Value = serde_json::from_str(text).ok()?;
        let map = fields(&value)?;
        let defaults = Self::default();
        let flag =
            |key: &str, fallback: bool| map.get(key).and_then(Value::as_bool).unwrap_or(fallback);
        let choice = |key: &str| map.get(key).and_then(Value::as_str);
        Some(Self {
            notification_mode: choice("notificationMode")
                .and_then(NotificationMode::from_wire)
                .unwrap_or(defaults.notification_mode),
            in_app_notifications: flag("inAppNotificationsEnabled", defaults.in_app_notifications),
            project_grouping: choice("sidebarProjectGroupingMode")
                .and_then(GroupingMode::from_wire)
                .unwrap_or(defaults.project_grouping),
            project_order: choice("sidebarProjectSortOrder")
                .and_then(SortOrder::from_wire)
                .unwrap_or(defaults.project_order),
            working_shelf: flag("sidebarWorkingShelfEnabled", defaults.working_shelf),
            confirm_unpin: flag("confirmThreadUnpin", defaults.confirm_unpin),
            confirm_archive: flag("confirmThreadArchive", defaults.confirm_archive),
            confirm_delete: flag("confirmThreadDelete", defaults.confirm_delete),
            // A legacy boolean decodes `true` to hold and `false` to direct.
            confirm_quit: match map.get("confirmQuit") {
                Some(Value::Bool(true)) => QuitMode::Hold,
                Some(Value::Bool(false)) => QuitMode::Direct,
                Some(Value::String(mode)) => {
                    QuitMode::from_wire(mode).unwrap_or(defaults.confirm_quit)
                }
                _ => defaults.confirm_quit,
            },
            time_format: choice("timestampFormat")
                .and_then(TimeFormat::from_wire)
                .unwrap_or(defaults.time_format),
        })
    }

    pub fn entries(self) -> [(&'static str, Value); 10] {
        [
            ("notificationMode", json!(self.notification_mode.wire())),
            (
                "inAppNotificationsEnabled",
                json!(self.in_app_notifications),
            ),
            (
                "sidebarProjectGroupingMode",
                json!(self.project_grouping.wire()),
            ),
            ("sidebarProjectSortOrder", json!(self.project_order.wire())),
            ("sidebarWorkingShelfEnabled", json!(self.working_shelf)),
            ("confirmThreadUnpin", json!(self.confirm_unpin)),
            ("confirmThreadArchive", json!(self.confirm_archive)),
            ("confirmThreadDelete", json!(self.confirm_delete)),
            ("confirmQuit", json!(self.confirm_quit.wire())),
            ("timestampFormat", json!(self.time_format.wire())),
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

    pub fn path() -> Option<PathBuf> {
        let home = std::env::var(crate::config::ENV_HOME).ok()?;
        (!home.trim().is_empty()).then(|| Path::new(&home).join(FILE))
    }

    pub fn load() -> Self {
        Self::path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    pub fn save(self) {
        if let Some(path) = Self::path() {
            let existing = std::fs::read_to_string(&path).ok();
            let _ = std::fs::write(path, self.to_json(existing.as_deref()));
        }
    }

    /// `useSettingsRestore`'s labels, in its order, for the values that differ from the defaults.
    pub fn changed_labels(self) -> Vec<&'static str> {
        let d = Self::default();
        [
            (self.time_format != d.time_format, "Time format"),
            (
                self.notification_mode != d.notification_mode,
                "Thread notifications",
            ),
            (
                self.in_app_notifications != d.in_app_notifications,
                "In-app notifications",
            ),
            (
                self.project_grouping != d.project_grouping,
                "Project Grouping",
            ),
            (self.project_order != d.project_order, "Project order"),
            (self.working_shelf != d.working_shelf, "Working section"),
            (self.confirm_unpin != d.confirm_unpin, "Unpin confirmation"),
            (
                self.confirm_archive != d.confirm_archive,
                "Archive confirmation",
            ),
            (
                self.confirm_delete != d.confirm_delete,
                "Delete confirmation",
            ),
            (self.confirm_quit != d.confirm_quit, "Quit shortcut"),
        ]
        .into_iter()
        .filter_map(|(changed, label)| changed.then_some(label))
        .collect()
    }
}

/// The grouping switch is on for any mode but `separate`; turning it on again restores the last
/// enabled mode (`SettingsPanels.logic.ts:21-50`).
pub fn toggle_grouping(current: GroupingMode, last_enabled: GroupingMode) -> GroupingMode {
    if current == GroupingMode::Separate {
        match last_enabled {
            GroupingMode::RepositoryPath => GroupingMode::RepositoryPath,
            _ => GroupingMode::Repository,
        }
    } else {
        GroupingMode::Separate
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Section {
    General,
    Appearance,
    Chat,
    Diffs,
    Keybindings,
    SnapShot,
    Providers,
    Threads,
    Browser,
    Devices,
    ScheduledTasks,
    Projects,
    SourceControl,
    Worktrees,
    Connections,
    Access,
    Secrets,
    Maintenance,
    Labs,
    About,
    Archived,
}

impl Section {
    /// `SETTINGS_SECTION_LABELS` (settingsSearch.ts).
    pub fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Appearance => "Appearance",
            Self::Chat => "Chat",
            Self::Diffs => "Diffs & pull requests",
            Self::Keybindings => "Keybindings",
            Self::SnapShot => "SnapShots",
            Self::Providers => "Providers",
            Self::Threads => "Threads",
            Self::Browser => "Browser",
            Self::Devices => "Devices",
            Self::ScheduledTasks => "Scheduled tasks",
            Self::Projects => "Projects",
            Self::SourceControl => "Source control",
            Self::Worktrees => "Worktrees",
            Self::Connections => "Environments",
            Self::Access => "Sharing & access",
            Self::Secrets => "Secrets",
            Self::Maintenance => "Maintenance",
            Self::Labs => "Labs",
            Self::About => "About",
            Self::Archived => "Archive",
        }
    }
}

pub struct NavGroup {
    pub heading: Option<&'static str>,
    pub sections: &'static [Section],
}

/// `SETTINGS_NAV_GROUPS` (settingsSearch.ts:116-153), with `isSecretsFeatureEnabled` on.
pub const NAV_GROUPS: &[NavGroup] = &[
    NavGroup {
        heading: None,
        sections: &[
            Section::General,
            Section::Appearance,
            Section::Chat,
            Section::Diffs,
            Section::Keybindings,
            Section::SnapShot,
        ],
    },
    NavGroup {
        heading: Some("Agents"),
        sections: &[
            Section::Providers,
            Section::Threads,
            Section::Browser,
            Section::Devices,
            Section::ScheduledTasks,
        ],
    },
    NavGroup {
        heading: Some("Code"),
        sections: &[
            Section::Projects,
            Section::SourceControl,
            Section::Worktrees,
        ],
    },
    NavGroup {
        heading: Some("Connections"),
        sections: &[Section::Connections, Section::Access, Section::Secrets],
    },
    NavGroup {
        heading: None,
        sections: &[
            Section::Maintenance,
            Section::Labs,
            Section::About,
            Section::Archived,
        ],
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchItem {
    pub id: &'static str,
    pub title: &'static str,
    pub section: Section,
    pub terms: &'static str,
    /// Shown only in the desktop app (`desktopOnly`).
    pub desktop_only: bool,
}

const fn item(
    id: &'static str,
    title: &'static str,
    terms: &'static str,
    desktop_only: bool,
) -> SearchItem {
    SearchItem {
        id,
        title,
        section: Section::General,
        terms,
        desktop_only,
    }
}

/// The General tab's `searchableSetting` entries, in the web's order. The other tabs have no
/// panel here yet, so their settings are not searchable.
pub const GENERAL_ITEMS: &[SearchItem] = &[
    item(
        "project-grouping",
        "Project grouping",
        "combine matching repositories environments sidebar",
        false,
    ),
    item(
        "project-order",
        "Project order",
        "sort projects sidebar manual created recent",
        false,
    ),
    item(
        "working-shelf",
        "Working section",
        "hide fold running monitoring threads inbox sidebar shelf",
        false,
    ),
    item(
        "thread-notifications",
        "Thread notifications",
        "notification sound alert completion input approval desktop",
        false,
    ),
    item(
        "in-app-notifications",
        "In-app notifications",
        "notification toast popup completion input approval failure",
        false,
    ),
    item(
        "time-format",
        "Time format",
        "timestamp clock locale system browser os 12 hour 24 hour",
        false,
    ),
    item(
        "unpin-confirmation",
        "Unpin confirmation",
        "ask before thread pinned section",
        false,
    ),
    item(
        "archive-confirmation",
        "Archive confirmation",
        "ask before thread second click inline action",
        false,
    ),
    item(
        "delete-confirmation",
        "Delete confirmation",
        "ask before thread chat history",
        false,
    ),
    item(
        "quit-confirmation",
        "Quit shortcut",
        "confirmation desktop app exit direct hold double click press twice",
        true,
    ),
];

/// `normalizeSearchText`: NFKD, marks stripped, lowercased, whitespace collapsed.
pub fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars().flat_map(decompose) {
        if is_combining(ch) {
            continue;
        }
        out.extend(ch.to_lowercase());
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Latin letters with a common accent, without pulling in a Unicode normalization crate.
fn decompose(ch: char) -> impl Iterator<Item = char> {
    let base = match ch {
        'à'..='å' => 'a',
        'À'..='Å' => 'A',
        'ç' => 'c',
        'Ç' => 'C',
        'è'..='ë' => 'e',
        'È'..='Ë' => 'E',
        'ì'..='ï' => 'i',
        'Ì'..='Ï' => 'I',
        'ñ' => 'n',
        'Ñ' => 'N',
        'ò'..='ö' => 'o',
        'Ò'..='Ö' => 'O',
        'ù'..='ü' => 'u',
        'Ù'..='Ü' => 'U',
        'ý' | 'ÿ' => 'y',
        other => other,
    };
    std::iter::once(base)
}

fn is_combining(ch: char) -> bool {
    matches!(ch, '\u{0300}'..='\u{036f}')
}

/// `searchSettings`: every token must appear in the title, the section label or the terms;
/// ranked title-exact, title-prefix, title-substring, all-tokens-in-title, phrase anywhere.
pub fn search<'a>(query: &str, items: &'a [SearchItem]) -> Vec<&'a SearchItem> {
    let query = normalize(query);
    if query.is_empty() {
        return Vec::new();
    }
    let tokens: Vec<&str> = query.split(' ').collect();
    let mut hits: Vec<(u8, usize, &SearchItem)> = items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            let title = normalize(item.title);
            let fields = [
                title.clone(),
                normalize(item.section.label()),
                normalize(item.terms),
            ];
            if !tokens
                .iter()
                .all(|token| fields.iter().any(|field| field.contains(token)))
            {
                return None;
            }
            let rank = if title == query {
                5
            } else if title.starts_with(&query) {
                4
            } else if title.contains(&query) {
                3
            } else if tokens.iter().all(|token| title.contains(token)) {
                2
            } else if fields.iter().any(|field| field.contains(&query)) {
                1
            } else {
                0
            };
            Some((rank, index, item))
        })
        .collect();
    hits.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    hits.into_iter().map(|(_, _, item)| item).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_contract() {
        let d = DeviceSettings::default();
        assert_eq!(d.notification_mode, NotificationMode::Off);
        assert!(!d.in_app_notifications);
        assert_eq!(d.project_grouping, GroupingMode::Repository);
        assert_eq!(d.project_order, SortOrder::UpdatedAt);
        assert!(d.working_shelf);
        assert!(!d.confirm_unpin && !d.confirm_archive && d.confirm_delete);
        assert_eq!(d.confirm_quit, QuitMode::Hold);
        assert_eq!(d.time_format, TimeFormat::Locale);
        assert!(d.changed_labels().is_empty());
    }

    #[test]
    fn round_trips_every_field() {
        let changed = DeviceSettings {
            notification_mode: NotificationMode::NotificationsAndSound,
            in_app_notifications: true,
            project_grouping: GroupingMode::Separate,
            project_order: SortOrder::Manual,
            working_shelf: false,
            confirm_unpin: true,
            confirm_archive: true,
            confirm_delete: false,
            confirm_quit: QuitMode::DoubleClick,
            time_format: TimeFormat::Hour24,
        };
        assert_eq!(DeviceSettings::parse(&changed.to_json(None)), Some(changed));
    }

    #[test]
    fn missing_and_invalid_keys_take_defaults() {
        let parsed = DeviceSettings::parse(
            r#"{"notificationMode":"loud","confirmThreadDelete":false,"timestampFormat":"12-hour"}"#,
        )
        .unwrap();
        assert_eq!(parsed.notification_mode, NotificationMode::Off);
        assert!(!parsed.confirm_delete);
        assert_eq!(parsed.time_format, TimeFormat::Hour12);
        assert_eq!(DeviceSettings::parse("[]"), None);
        assert_eq!(DeviceSettings::parse("nope"), None);
    }

    #[test]
    fn reads_the_wrapped_document_and_the_legacy_quit_boolean() {
        let wrapped = DeviceSettings::parse(r#"{"settings":{"confirmQuit":false}}"#).unwrap();
        assert_eq!(wrapped.confirm_quit, QuitMode::Direct);
        let legacy = DeviceSettings::parse(r#"{"confirmQuit":true}"#).unwrap();
        assert_eq!(legacy.confirm_quit, QuitMode::Hold);
    }

    #[test]
    fn writing_keeps_keys_it_does_not_own() {
        let existing = r#"{"settings":{"theme":"dark","confirmThreadDelete":false}}"#;
        let written = DeviceSettings::default().to_json(Some(existing));
        let value: Value = serde_json::from_str(&written).unwrap();
        assert_eq!(value["settings"]["theme"], "dark");
        assert_eq!(value["settings"]["confirmThreadDelete"], true);
        let bare = DeviceSettings::default().to_json(Some(r#"{"theme":"dark"}"#));
        assert!(bare.contains("\"theme\":\"dark\""));
    }

    #[test]
    fn changed_labels_follow_the_restore_order_and_restore_clears_them() {
        let mut settings = DeviceSettings {
            confirm_quit: QuitMode::Direct,
            time_format: TimeFormat::Hour12,
            working_shelf: false,
            ..DeviceSettings::default()
        };
        assert_eq!(
            settings.changed_labels(),
            ["Time format", "Working section", "Quit shortcut"]
        );
        settings = DeviceSettings::default();
        assert!(settings.changed_labels().is_empty());
    }

    #[test]
    fn grouping_toggle_remembers_the_last_enabled_mode() {
        use GroupingMode::*;
        assert_eq!(toggle_grouping(Repository, Repository), Separate);
        assert_eq!(toggle_grouping(RepositoryPath, Repository), Separate);
        assert_eq!(toggle_grouping(Separate, RepositoryPath), RepositoryPath);
        assert_eq!(toggle_grouping(Separate, Repository), Repository);
        assert_eq!(toggle_grouping(Separate, Separate), Repository);
    }

    #[test]
    fn nav_lists_every_section_once_in_the_web_order() {
        let all: Vec<Section> = NAV_GROUPS
            .iter()
            .flat_map(|group| group.sections.iter().copied())
            .collect();
        assert_eq!(all.len(), 21);
        assert_eq!(all[0], Section::General);
        assert_eq!(all[all.len() - 1], Section::Archived);
        let mut unique = all.clone();
        unique.sort_by_key(|section| section.label());
        unique.dedup();
        assert_eq!(unique.len(), 21);
    }

    #[test]
    fn search_normalizes_ranks_and_requires_every_token() {
        assert!(search("", GENERAL_ITEMS).is_empty());
        assert!(search("zzz", GENERAL_ITEMS).is_empty());
        let ids = |query: &str| -> Vec<&str> {
            search(query, GENERAL_ITEMS).iter().map(|i| i.id).collect()
        };
        assert_eq!(ids("  TIME   Format ")[0], "time-format");
        assert_eq!(ids("project")[..2], ["project-grouping", "project-order"]);
        // "ask" matches only through terms, and keeps the original order.
        assert_eq!(
            ids("ask before"),
            [
                "unpin-confirmation",
                "archive-confirmation",
                "delete-confirmation"
            ]
        );
        assert_eq!(ids("sidebar manual"), ["project-order"]);
        assert_eq!(normalize("Café  Ñandú"), "cafe nandu");
    }
}
