//! The "Diffs & pull requests" tab's slice of the device settings (`ClientSettingsSchema`,
//! contracts/src/settings.ts:353). Every key here is a client setting, not a server setting, so
//! it lives in `client-settings.json` beside the General tab's keys, which this store keeps.

use crate::settings::{SearchItem, Section};
use serde_json::{Map, Value, json};
use std::path::PathBuf;

/// `DiffLayout` (settings.ts:59); labels from `DIFF_LAYOUT_LABELS` (SettingsPanels.tsx:136).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffLayout {
    Stacked,
    Split,
}

impl DiffLayout {
    pub const ALL: &'static [Self] = &[Self::Stacked, Self::Split];

    pub fn wire(self) -> &'static str {
        match self {
            Self::Stacked => "stacked",
            Self::Split => "split",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Stacked => "Stacked",
            Self::Split => "Split",
        }
    }

    pub fn from_wire(value: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|v| v.wire() == value)
    }
}

/// `DiffColorScheme` (settings.ts:327); labels from `DiffColorsSettingsRow` (SettingsPanels.tsx:1275).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffColors {
    RedGreen,
    BlueOrange,
}

impl DiffColors {
    pub const ALL: &'static [Self] = &[Self::RedGreen, Self::BlueOrange];

    pub fn wire(self) -> &'static str {
        match self {
            Self::RedGreen => "red-green",
            Self::BlueOrange => "blue-orange",
        }
    }

    /// The trigger shows the plain label.
    pub fn label(self) -> &'static str {
        match self {
            Self::RedGreen => "Red & green",
            Self::BlueOrange => "Blue & orange",
        }
    }

    /// The menu marks the default option (SettingsPanels.tsx:1281).
    pub fn menu_label(self) -> &'static str {
        match self {
            Self::RedGreen => "Red & green (default)",
            Self::BlueOrange => "Blue & orange",
        }
    }

    pub fn from_wire(value: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|v| v.wire() == value)
    }

    /// The two dots before the label, left to right, as sRGB (SettingsPanels.tsx:1264-1273).
    /// The row paints deletion then addition; blue and orange reverse it (`flex-row-reverse`).
    /// Red and green are `--diff-deletion` (`--error`: red-500, in dark mixed 90% into white) and
    /// `--diff-addition` (`--success`: emerald-500); blue and orange are `blue-600/400` and
    /// `orange-600/400` (index.css:2274, 2291-2292).
    pub fn dots(self, dark: bool) -> [u32; 2] {
        match (self, dark) {
            (Self::RedGreen, false) => [0xef4444, 0x10b981],
            (Self::RedGreen, true) => [0xf15757, 0x10b981],
            (Self::BlueOrange, false) => [0x2563eb, 0xea580c],
            (Self::BlueOrange, true) => [0x60a5fa, 0xfb923c],
        }
    }
}

/// `diffFilesCollapsed` is a boolean on the wire; the select shows it as two states
/// (SettingsPanels.tsx:1372).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileState {
    Expanded,
    Collapsed,
}

impl FileState {
    pub const ALL: &'static [Self] = &[Self::Expanded, Self::Collapsed];

    pub fn label(self) -> &'static str {
        match self {
            Self::Expanded => "Expanded",
            Self::Collapsed => "Collapsed",
        }
    }

    pub fn collapsed(self) -> bool {
        self == Self::Collapsed
    }

    pub fn of(collapsed: bool) -> Self {
        if collapsed {
            Self::Collapsed
        } else {
            Self::Expanded
        }
    }
}

/// The Diffs & pull requests tab's settings, with `DEFAULT_CLIENT_SETTINGS` defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiffSettings {
    /// `diffLayout`, default `stacked` (settings.ts:424).
    pub layout: DiffLayout,
    /// `diffIgnoreWhitespace`, default on (settings.ts:423).
    pub ignore_whitespace: bool,
    /// `diffFilesCollapsed`, default on (settings.ts:422).
    pub files_collapsed: bool,
    /// `diffColorScheme`, default `red-green` (settings.ts:358).
    pub colors: DiffColors,
    /// `proactivePanelsEnabled`, default off (settings.ts:477).
    pub proactive_panels: bool,
    /// `pullRequestLinksOpenInApp`, default on (settings.ts:466).
    pub pr_links_in_app: bool,
}

impl Default for DiffSettings {
    fn default() -> Self {
        Self {
            layout: DiffLayout::Stacked,
            ignore_whitespace: true,
            files_collapsed: true,
            colors: DiffColors::RedGreen,
            proactive_panels: false,
            pr_links_in_app: true,
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

impl DiffSettings {
    /// A missing or invalid key falls back to its default; anything but an object is rejected.
    pub fn parse(text: &str) -> Option<Self> {
        let value: Value = serde_json::from_str(text).ok()?;
        let map = fields(&value)?;
        let d = Self::default();
        let flag =
            |key: &str, fallback: bool| map.get(key).and_then(Value::as_bool).unwrap_or(fallback);
        let choice = |key: &str| map.get(key).and_then(Value::as_str);
        Some(Self {
            layout: choice("diffLayout")
                .and_then(DiffLayout::from_wire)
                .unwrap_or(d.layout),
            ignore_whitespace: flag("diffIgnoreWhitespace", d.ignore_whitespace),
            files_collapsed: flag("diffFilesCollapsed", d.files_collapsed),
            colors: choice("diffColorScheme")
                .and_then(DiffColors::from_wire)
                .unwrap_or(d.colors),
            proactive_panels: flag("proactivePanelsEnabled", d.proactive_panels),
            pr_links_in_app: flag("pullRequestLinksOpenInApp", d.pr_links_in_app),
        })
    }

    pub fn entries(self) -> [(&'static str, Value); 6] {
        [
            ("diffLayout", json!(self.layout.wire())),
            ("diffIgnoreWhitespace", json!(self.ignore_whitespace)),
            ("diffFilesCollapsed", json!(self.files_collapsed)),
            ("diffColorScheme", json!(self.colors.wire())),
            ("proactivePanelsEnabled", json!(self.proactive_panels)),
            ("pullRequestLinksOpenInApp", json!(self.pr_links_in_app)),
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

    /// Re-reads the file first, so keys the General tab saved in the meantime survive.
    pub fn save(self) {
        if let Some(path) = Self::path() {
            let existing = std::fs::read_to_string(&path).ok();
            let _ = std::fs::write(path, self.to_json(existing.as_deref()));
        }
    }

    /// The labels `useSettingsRestore` lists (SettingsPanels.tsx:451, 511-520), keyed by where
    /// they sit among the other tabs' labels: `colors` comes first, the other three after the
    /// General tab's sidebar labels. `pr_links_in_app` is restored but never listed there.
    fn changed(self) -> (Option<&'static str>, Vec<&'static str>) {
        let d = Self::default();
        let colors = (self.colors != d.colors).then_some("Diff colors");
        let rest = [
            (
                self.files_collapsed != d.files_collapsed,
                "Default diff file state",
            ),
            (
                self.ignore_whitespace != d.ignore_whitespace,
                "Diff whitespace changes",
            ),
            (self.layout != d.layout, "Diff layout"),
            (
                self.proactive_panels != d.proactive_panels,
                "Open pull requests and changes panels proactively",
            ),
        ]
        .into_iter()
        .filter_map(|(changed, label)| changed.then_some(label))
        .collect();
        (colors, rest)
    }

    /// Interleaves these labels into the General tab's list in the web's order: `Diff colors`
    /// first, the rest before the first Chat or confirmation label.
    pub fn merge_labels(self, general: Vec<&'static str>) -> Vec<&'static str> {
        const CONFIRMATIONS: [&str; 8] = [
            "Show skills in slash menu",
            "Rich text composer",
            "Send shortcut",
            "Follow-up behavior",
            "Unpin confirmation",
            "Archive confirmation",
            "Delete confirmation",
            "Quit shortcut",
        ];
        let (colors, rest) = self.changed();
        let split = general
            .iter()
            .position(|label| CONFIRMATIONS.contains(label))
            .unwrap_or(general.len());
        let mut merged = Vec::with_capacity(general.len() + rest.len() + 1);
        merged.extend(colors);
        merged.extend_from_slice(&general[..split]);
        merged.extend(rest);
        merged.extend_from_slice(&general[split..]);
        merged
    }
}

const fn item(id: &'static str, title: &'static str, terms: &'static str) -> SearchItem {
    SearchItem {
        id,
        title,
        section: Section::Diffs,
        terms,
        desktop_only: false,
    }
}

/// The Diffs & pull requests tab's `searchableSetting` entries (settingsSearch.ts:327, 556-577,
/// 934), in the web's order. The host names in the last entry are the built-in source control
/// definitions' labels, in registry order.
pub const DIFFS_ITEMS: &[SearchItem] = &[
    item(
        "diff-color-scheme",
        "Diff colors",
        "red green blue orange additions deletions changes counts palette colorblind",
    ),
    item(
        "hide-whitespace-changes",
        "Hide whitespace changes",
        "diff ignore spaces edits default",
    ),
    item(
        "default-diff-file-state",
        "Default diff file state",
        "collapsed expanded collapse expand files pull request pr code tab",
    ),
    item(
        "diff-layout",
        "Diff layout",
        "stacked split side by side unified inline view",
    ),
    item(
        "proactive-panels",
        "Open pull requests and changes panels proactively",
        "proactive automatically open diff pull request pr right panel agent completion",
    ),
    item(
        "pull-request-links-open-in-app",
        "Open pull requests in T3 Code",
        "pr commit links GitHub GitLab GitCafe Forgejo Bitbucket Azure DevOps browser external click default pull request view panel",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{search, searchable};

    #[test]
    fn defaults_match_the_contract() {
        let d = DiffSettings::default();
        assert_eq!(d.layout, DiffLayout::Stacked);
        assert!(d.ignore_whitespace && d.files_collapsed && d.pr_links_in_app);
        assert!(!d.proactive_panels);
        assert_eq!(d.colors, DiffColors::RedGreen);
    }

    #[test]
    fn parse_reads_bare_and_wrapped_documents() {
        let text = r#"{"settings":{"diffLayout":"split","diffIgnoreWhitespace":false,
            "diffFilesCollapsed":false,"diffColorScheme":"blue-orange",
            "proactivePanelsEnabled":true,"pullRequestLinksOpenInApp":false}}"#;
        let parsed = DiffSettings::parse(text).unwrap();
        assert_eq!(parsed.layout, DiffLayout::Split);
        assert!(!parsed.ignore_whitespace && !parsed.files_collapsed && !parsed.pr_links_in_app);
        assert_eq!(parsed.colors, DiffColors::BlueOrange);
        assert!(parsed.proactive_panels);
        assert_eq!(
            DiffSettings::parse(r#"{"diffLayout":"split"}"#)
                .unwrap()
                .layout,
            DiffLayout::Split
        );
    }

    #[test]
    fn parse_falls_back_per_key_and_rejects_non_objects() {
        let parsed = DiffSettings::parse(r#"{"diffLayout":"bogus","diffColorScheme":3}"#).unwrap();
        assert_eq!(parsed, DiffSettings::default());
        assert!(DiffSettings::parse("[]").is_none());
        assert!(DiffSettings::parse("not json").is_none());
    }

    #[test]
    fn to_json_keeps_foreign_keys_in_either_shape() {
        let changed = DiffSettings {
            layout: DiffLayout::Split,
            ..DiffSettings::default()
        };
        let wrapped: Value = serde_json::from_str(
            &changed.to_json(Some(r#"{"settings":{"timestampFormat":"24-hour"}}"#)),
        )
        .unwrap();
        assert_eq!(wrapped["settings"]["timestampFormat"], "24-hour");
        assert_eq!(wrapped["settings"]["diffLayout"], "split");
        let bare: Value =
            serde_json::from_str(&changed.to_json(Some(r#"{"chatWidth":"wide"}"#))).unwrap();
        assert_eq!(bare["chatWidth"], "wide");
        assert_eq!(bare["diffColorScheme"], "red-green");
        assert_eq!(DiffSettings::parse(&changed.to_json(None)), Some(changed));
    }

    #[test]
    fn menu_marks_only_the_default_colors() {
        assert_eq!(DiffColors::RedGreen.menu_label(), "Red & green (default)");
        assert_eq!(DiffColors::RedGreen.label(), "Red & green");
        assert_eq!(DiffColors::BlueOrange.menu_label(), "Blue & orange");
    }

    #[test]
    fn dots_follow_the_row_order() {
        // Red then green; blue (addition) then orange (deletion), reversed by `flex-row-reverse`.
        assert_eq!(DiffColors::RedGreen.dots(false), [0xef4444, 0x10b981]);
        assert_eq!(DiffColors::BlueOrange.dots(true), [0x60a5fa, 0xfb923c]);
    }

    #[test]
    fn file_state_round_trips_the_boolean() {
        assert_eq!(FileState::of(true), FileState::Collapsed);
        assert!(!FileState::Expanded.collapsed());
    }

    #[test]
    fn labels_interleave_in_the_web_order() {
        let all = DiffSettings {
            layout: DiffLayout::Split,
            ignore_whitespace: false,
            files_collapsed: false,
            colors: DiffColors::BlueOrange,
            proactive_panels: true,
            pr_links_in_app: false,
        };
        let general = vec![
            "Time format",
            "Working section",
            "Unpin confirmation",
            "Quit shortcut",
        ];
        assert_eq!(
            all.merge_labels(general),
            vec![
                "Diff colors",
                "Time format",
                "Working section",
                "Default diff file state",
                "Diff whitespace changes",
                "Diff layout",
                "Open pull requests and changes panels proactively",
                "Unpin confirmation",
                "Quit shortcut",
            ]
        );
        assert_eq!(
            DiffSettings::default().merge_labels(vec!["Time format"]),
            vec!["Time format"]
        );
    }

    #[test]
    fn the_links_setting_is_restored_but_never_listed() {
        let only_links = DiffSettings {
            pr_links_in_app: false,
            ..DiffSettings::default()
        };
        assert!(only_links.merge_labels(Vec::new()).is_empty());
        assert!(DiffSettings::default().pr_links_in_app);
    }

    #[test]
    fn search_finds_each_setting_in_the_diffs_section() {
        let hit = |query: &str| search(query, DIFFS_ITEMS).first().map(|i| i.id);
        assert_eq!(hit("colorblind"), Some("diff-color-scheme"));
        assert_eq!(hit("side by side"), Some("diff-layout"));
        assert_eq!(hit("whitespace"), Some("hide-whitespace-changes"));
        assert_eq!(hit("gitlab"), Some("pull-request-links-open-in-app"));
        assert!(DIFFS_ITEMS.iter().all(|i| i.section == Section::Diffs));
        assert_eq!(
            search("diff layout", searchable())
                .first()
                .map(|i| i.section),
            Some(Section::Diffs)
        );
    }
}
