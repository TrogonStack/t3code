//! The Browser tab (`IntegrationsSettings.tsx` `BrowserSettingsPanel`): the device keys it
//! persists in `client-settings.json`, the option lists the web takes from `contracts` and
//! `shared`, the profile rules, and the search entries. The one server key,
//! `enableAgentBrowserAccess`, lives in `settings_source_control::Field`. The panel only draws.

use crate::settings::{SearchItem, Section, fields};
use serde_json::{Value, json};
use std::path::PathBuf;

/// Call tokens of the Browser tab's requests start here.
pub const TOKEN_BASE: u64 = 1 << 58;

const FILE: &str = "client-settings.json";

pub const PROFILE_NAME_MAX: usize = 48;
pub const PROFILE_MAX_COUNT: usize = 24;
pub const DEFAULT_PROFILE_ID: &str = "default";
pub const CUSTOM_URL_MAX: usize = 2048;
pub const VIEWPORT_MIN: u32 = 240;
pub const VIEWPORT_MAX: u32 = 3840;
pub const VIEWPORT_MAX_AREA: u64 = 3840 * 2160;
/// `RESPONSIVE_SEED_SIZE`.
pub const RESPONSIVE_SEED: (u32, u32) = (1280, 800);

/// `PREVIEW_ZOOM_LEVELS`.
pub const ZOOM_LEVELS: [f64; 17] = [
    0.25, 0.33, 0.5, 0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0, 4.0, 5.0,
];
pub const DEFAULT_ZOOM: f64 = 1.0;
/// `BROWSER_RECORDING_FRAME_RATES`.
pub const FRAME_RATES: [u32; 2] = [30, 60];
pub const DEFAULT_FRAME_RATE: u32 = 30;
/// `PreviewAppearancePreference`, with the labels of `APPEARANCE_LABELS`.
pub const APPEARANCES: [(&str, &str); 3] =
    [("system", "System"), ("light", "Light"), ("dark", "Dark")];
/// `browserLinkTarget`: wire value and the select's label.
pub const LINK_TARGETS: [(&str, &str); 2] =
    [("system", "Your default browser"), ("app", "T3 Code")];
/// `browserSearchEngine` with its labels; `custom` last.
pub const SEARCH_ENGINES: [(&str, &str); 7] = [
    ("duckduckgo", "DuckDuckGo"),
    ("google", "Google"),
    ("bing", "Bing"),
    ("brave", "Brave Search"),
    ("kagi", "Kagi"),
    ("ecosia", "Ecosia"),
    ("custom", "Custom"),
];
pub const DEFAULT_SEARCH_ENGINE: &str = "duckduckgo";

/// `PREVIEW_VIEWPORT_PRESETS`, in the order of the select's "Standard" group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Preset {
    pub id: &'static str,
    pub label: &'static str,
    pub detail: &'static str,
    pub width: u32,
    pub height: u32,
}

pub const PRESETS: [Preset; 17] = [
    Preset {
        id: "iphone-se",
        label: "iPhone SE",
        detail: "375 × 667",
        width: 375,
        height: 667,
    },
    Preset {
        id: "iphone-xr",
        label: "iPhone XR",
        detail: "414 × 896",
        width: 414,
        height: 896,
    },
    Preset {
        id: "iphone-12-pro",
        label: "iPhone 12 Pro",
        detail: "390 × 844",
        width: 390,
        height: 844,
    },
    Preset {
        id: "iphone-14-pro-max",
        label: "iPhone 14 Pro Max",
        detail: "430 × 932",
        width: 430,
        height: 932,
    },
    Preset {
        id: "pixel-7",
        label: "Pixel 7",
        detail: "412 × 915",
        width: 412,
        height: 915,
    },
    Preset {
        id: "samsung-galaxy-s8-plus",
        label: "Samsung Galaxy S8+",
        detail: "360 × 740",
        width: 360,
        height: 740,
    },
    Preset {
        id: "samsung-galaxy-s20-ultra",
        label: "Samsung Galaxy S20 Ultra",
        detail: "412 × 915",
        width: 412,
        height: 915,
    },
    Preset {
        id: "ipad-mini",
        label: "iPad Mini",
        detail: "768 × 1024",
        width: 768,
        height: 1024,
    },
    Preset {
        id: "ipad-air",
        label: "iPad Air",
        detail: "820 × 1180",
        width: 820,
        height: 1180,
    },
    Preset {
        id: "ipad-pro",
        label: "iPad Pro",
        detail: "1024 × 1366",
        width: 1024,
        height: 1366,
    },
    Preset {
        id: "surface-pro-7",
        label: "Surface Pro 7",
        detail: "912 × 1368",
        width: 912,
        height: 1368,
    },
    Preset {
        id: "surface-duo",
        label: "Surface Duo",
        detail: "540 × 720",
        width: 540,
        height: 720,
    },
    Preset {
        id: "galaxy-z-fold-5",
        label: "Galaxy Z Fold 5",
        detail: "344 × 882",
        width: 344,
        height: 882,
    },
    Preset {
        id: "asus-zenbook-fold",
        label: "Asus Zenbook Fold",
        detail: "853 × 1280",
        width: 853,
        height: 1280,
    },
    Preset {
        id: "samsung-galaxy-a51-71",
        label: "Samsung Galaxy A51/71",
        detail: "412 × 914",
        width: 412,
        height: 914,
    },
    Preset {
        id: "nest-hub",
        label: "Nest Hub",
        detail: "1024 × 600",
        width: 1024,
        height: 600,
    },
    Preset {
        id: "nest-hub-max",
        label: "Nest Hub Max",
        detail: "1280 × 800",
        width: 1280,
        height: 800,
    },
];

const fn item(id: &'static str, title: &'static str, terms: &'static str) -> SearchItem {
    SearchItem {
        id,
        title,
        section: Section::Browser,
        terms,
        desktop_only: false,
    }
}

/// The tab's `searchableSetting` entries (settingsSearch.ts).
pub const BROWSER_ITEMS: &[SearchItem] = &[
    item(
        "agent-browser-access",
        "Agent browser access",
        "allow disable enable open drive preview tools sessions project override",
    ),
    item("browser-profiles", "Browser profiles", ""),
    item("browser-default-profile", "Default browser profile", ""),
    item(
        "browser-default-viewport",
        "Default browser viewport",
        "preview size width height device desktop mobile rotate",
    ),
    item(
        "browser-default-zoom",
        "Default browser zoom",
        "preview page scale tabs percent",
    ),
    item(
        "browser-default-appearance",
        "Default browser appearance",
        "preview color scheme light dark system os",
    ),
    item(
        "browser-recording-frame-rate",
        "Browser recording frame rate",
        "",
    ),
    item(
        "browser-recording-key-presses",
        "Show key presses in recordings",
        "browser preview keyboard shortcuts keystrokes overlay capture",
    ),
    item(
        "browser-recording-mouse-presses",
        "Show mouse presses in recordings",
        "browser preview clicks buttons drag overlay capture",
    ),
    item(
        "browser-link-target",
        "Open links in",
        "links default browser in-app browser external open",
    ),
    item(
        "browser-search-engine",
        "Search engine",
        "address bar duckduckgo google bing brave kagi ecosia custom url",
    ),
    item(
        "browser-auto-show-floating-preview",
        "Auto-show floating preview",
        "agent opens browser device simulator pop into view hide",
    ),
];

/// `PreviewViewportSetting`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Viewport {
    Fill,
    Freeform { width: u32, height: u32 },
    Preset { id: String, width: u32, height: u32 },
}

impl Viewport {
    /// The size the number fields show; `None` while the viewport fills the panel.
    pub fn size(&self) -> Option<(u32, u32)> {
        match self {
            Self::Fill => None,
            Self::Freeform { width, height } | Self::Preset { width, height, .. } => {
                Some((*width, *height))
            }
        }
    }

    /// The select's items: Fill panel, Responsive, then the presets.
    pub fn index(&self) -> usize {
        match self {
            Self::Fill => 0,
            Self::Freeform { .. } => 1,
            Self::Preset { id, .. } => PRESETS
                .iter()
                .position(|preset| preset.id == id)
                .map_or(1, |at| at + 2),
        }
    }

    /// `viewportSelectLabel`: a preset the list no longer holds reads as its size.
    pub fn label(&self) -> String {
        match self {
            Self::Fill => "Fill panel".to_owned(),
            Self::Freeform { .. } => "Responsive".to_owned(),
            Self::Preset { id, width, height } => {
                PRESETS.iter().find(|preset| preset.id == id).map_or_else(
                    || format!("{width} \u{d7} {height}"),
                    |p| p.label.to_owned(),
                )
            }
        }
    }

    /// What picking item `index` of the select stores.
    pub fn pick(&self, index: usize) -> Self {
        match index {
            0 => Self::Fill,
            1 => {
                let (width, height) = self.size().unwrap_or(RESPONSIVE_SEED);
                Self::Freeform { width, height }
            }
            _ => PRESETS.get(index - 2).map_or_else(
                || self.clone(),
                |preset| Self::Preset {
                    id: preset.id.to_owned(),
                    width: preset.width,
                    height: preset.height,
                },
            ),
        }
    }

    /// `rotateViewport`: width and height swap, a preset stays a preset.
    pub fn rotated(&self) -> Self {
        match self {
            Self::Fill => Self::Fill,
            Self::Freeform { width, height } => Self::Freeform {
                width: *height,
                height: *width,
            },
            Self::Preset { id, width, height } => Self::Preset {
                id: id.clone(),
                width: *height,
                height: *width,
            },
        }
    }

    /// `commitDimension`: `None` when the text is not a valid size, or changes nothing.
    pub fn resized(&self, axis: Axis, text: &str) -> Option<Self> {
        let value: u32 = text.trim().parse().ok()?;
        if !(VIEWPORT_MIN..=VIEWPORT_MAX).contains(&value) {
            return None;
        }
        let (width, height) = self.size().unwrap_or(RESPONSIVE_SEED);
        let (width, height) = match axis {
            Axis::Width => (value, height),
            Axis::Height => (width, value),
        };
        if u64::from(width) * u64::from(height) > VIEWPORT_MAX_AREA {
            return None;
        }
        if self.size() == Some((width, height)) {
            return None;
        }
        Some(Self::Freeform { width, height })
    }

    fn decode(value: &Value) -> Option<Self> {
        let size = || {
            let side = |key: &str| {
                let n = u32::try_from(value.get(key)?.as_u64()?).ok()?;
                (VIEWPORT_MIN..=VIEWPORT_MAX).contains(&n).then_some(n)
            };
            let (width, height) = (side("width")?, side("height")?);
            (u64::from(width) * u64::from(height) <= VIEWPORT_MAX_AREA).then_some((width, height))
        };
        match value.get("_tag")?.as_str()? {
            "fill" => Some(Self::Fill),
            "freeform" => size().map(|(width, height)| Self::Freeform { width, height }),
            "preset" => size().and_then(|(width, height)| {
                Some(Self::Preset {
                    id: value.get("presetId")?.as_str()?.to_owned(),
                    width,
                    height,
                })
            }),
            _ => None,
        }
    }

    fn encode(&self) -> Value {
        match self {
            Self::Fill => json!({"_tag": "fill"}),
            Self::Freeform { width, height } => {
                json!({"_tag": "freeform", "width": width, "height": height})
            }
            Self::Preset { id, width, height } => {
                json!({"_tag": "preset", "width": width, "height": height, "presetId": id})
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Width,
    Height,
}

/// `BrowserProfile`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub incognito: bool,
}

impl Profile {
    pub fn built_in(&self) -> bool {
        self.id == DEFAULT_PROFILE_ID || self.id == "incognito"
    }

    fn decode(value: &Value) -> Option<Self> {
        let id = value.get("id")?.as_str()?.trim();
        let name = value.get("name")?.as_str()?.trim();
        if id.is_empty()
            || name.is_empty()
            || id.chars().count() > 64
            || name.chars().count() > PROFILE_NAME_MAX
            || id.chars().any(char::is_control)
        {
            return None;
        }
        Some(Self {
            id: id.to_owned(),
            name: name.to_owned(),
            incognito: value.get("kind").and_then(Value::as_str) == Some("incognito"),
        })
    }

    fn encode(&self) -> Value {
        let kind = if self.incognito {
            "incognito"
        } else {
            "persistent"
        };
        json!({"id": self.id, "name": self.name, "kind": kind})
    }
}

/// The device settings of the tab (`ClientSettingsSchema`'s browser keys).
#[derive(Clone, Debug, PartialEq)]
pub struct BrowserSettings {
    pub viewport: Viewport,
    pub zoom: f64,
    pub appearance: String,
    pub frame_rate: u32,
    pub key_presses: bool,
    pub mouse_presses: bool,
    pub link_target: String,
    pub search_engine: String,
    pub custom_url: String,
    pub auto_show: bool,
    /// `browserProfiles`: the profiles the user made, without the built-ins.
    pub profiles: Vec<Profile>,
    pub default_profile: String,
}

impl Default for BrowserSettings {
    fn default() -> Self {
        Self {
            viewport: Viewport::Fill,
            zoom: DEFAULT_ZOOM,
            appearance: "system".to_owned(),
            frame_rate: DEFAULT_FRAME_RATE,
            key_presses: false,
            mouse_presses: false,
            link_target: "system".to_owned(),
            search_engine: DEFAULT_SEARCH_ENGINE.to_owned(),
            custom_url: String::new(),
            auto_show: true,
            profiles: Vec::new(),
            default_profile: DEFAULT_PROFILE_ID.to_owned(),
        }
    }
}

/// `resolveBrowserProfiles`: the built-ins first, then the user's, one entry per id.
pub fn resolve_profiles(user: &[Profile]) -> Vec<Profile> {
    let mut resolved = vec![
        Profile {
            id: DEFAULT_PROFILE_ID.to_owned(),
            name: "Default".to_owned(),
            incognito: false,
        },
        Profile {
            id: "incognito".to_owned(),
            name: "Incognito".to_owned(),
            incognito: true,
        },
    ];
    for profile in user {
        if resolved.iter().any(|seen| seen.id == profile.id) {
            continue;
        }
        resolved.push(Profile {
            incognito: false,
            ..profile.clone()
        });
    }
    resolved
}

/// `isValidSearchUrlTemplate`: an http(s) URL with `%s` after the host and credentials.
pub fn valid_search_url(template: &str) -> bool {
    if !template.contains("%s") {
        return false;
    }
    let authority = |text: &str| {
        let rest = text
            .strip_prefix("https://")
            .or_else(|| text.strip_prefix("http://"))?;
        let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        let authority = &rest[..end];
        (!authority.is_empty()).then(|| authority.to_owned())
    };
    match (
        authority(&template.replace("%s", "a")),
        authority(&template.replace("%s", "b")),
    ) {
        (Some(one), Some(other)) => one == other,
        _ => false,
    }
}

impl BrowserSettings {
    /// A missing or invalid key falls back to its default; anything but an object is rejected.
    pub fn parse(text: &str) -> Option<Self> {
        let value: Value = serde_json::from_str(text).ok()?;
        let map = fields(&value)?;
        let defaults = Self::default();
        let flag =
            |key: &str, fallback: bool| map.get(key).and_then(Value::as_bool).unwrap_or(fallback);
        let choice = |key: &str, options: &[(&str, &str)], fallback: &str| {
            map.get(key)
                .and_then(Value::as_str)
                .filter(|text| options.iter().any(|(wire, _)| wire == text))
                .unwrap_or(fallback)
                .to_owned()
        };
        let mut profiles: Vec<Profile> = map
            .get("browserProfiles")
            .and_then(Value::as_array)
            .map(|list| list.iter().filter_map(Profile::decode).collect())
            .unwrap_or_default();
        profiles.truncate(PROFILE_MAX_COUNT);
        Some(Self {
            viewport: map
                .get("browserDefaultViewport")
                .and_then(Viewport::decode)
                .unwrap_or(defaults.viewport),
            zoom: map
                .get("browserDefaultZoomFactor")
                .and_then(Value::as_f64)
                .filter(|zoom| ZOOM_LEVELS.iter().any(|level| (level - zoom).abs() < 1e-9))
                .unwrap_or(defaults.zoom),
            appearance: choice("browserDefaultAppearance", &APPEARANCES, "system"),
            frame_rate: map
                .get("browserRecordingFrameRate")
                .and_then(Value::as_u64)
                .and_then(|rate| u32::try_from(rate).ok())
                .filter(|rate| FRAME_RATES.contains(rate))
                .unwrap_or(defaults.frame_rate),
            key_presses: flag("browserRecordingShowKeyPresses", false),
            mouse_presses: flag("browserRecordingShowMousePresses", false),
            link_target: choice("browserLinkTarget", &LINK_TARGETS, "system"),
            search_engine: choice(
                "browserSearchEngine",
                &SEARCH_ENGINES,
                DEFAULT_SEARCH_ENGINE,
            ),
            custom_url: map
                .get("browserCustomSearchUrl")
                .and_then(Value::as_str)
                .filter(|text| text.chars().count() <= CUSTOM_URL_MAX)
                .unwrap_or_default()
                .to_owned(),
            auto_show: flag("browserAutoShowFloatingPreview", true),
            profiles,
            default_profile: map
                .get("browserDefaultProfileId")
                .and_then(Value::as_str)
                .filter(|id| !id.trim().is_empty())
                .unwrap_or(DEFAULT_PROFILE_ID)
                .to_owned(),
        })
    }

    pub fn entries(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("browserDefaultViewport", self.viewport.encode()),
            ("browserDefaultZoomFactor", json!(self.zoom)),
            ("browserDefaultAppearance", json!(self.appearance)),
            ("browserRecordingFrameRate", json!(self.frame_rate)),
            ("browserRecordingShowKeyPresses", json!(self.key_presses)),
            (
                "browserRecordingShowMousePresses",
                json!(self.mouse_presses),
            ),
            ("browserLinkTarget", json!(self.link_target)),
            ("browserSearchEngine", json!(self.search_engine)),
            ("browserCustomSearchUrl", json!(self.custom_url)),
            ("browserAutoShowFloatingPreview", json!(self.auto_show)),
            (
                "browserProfiles",
                Value::Array(self.profiles.iter().map(Profile::encode).collect()),
            ),
            ("browserDefaultProfileId", json!(self.default_profile)),
        ]
    }

    /// Writes these keys into `existing`, keeping every key this tab does not own.
    pub fn to_json(&self, existing: Option<&str>) -> String {
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
        crate::settings_voice::home().map(|home| home.join(FILE))
    }

    pub fn load() -> Self {
        Self::path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Some(path) = Self::path() {
            let existing = std::fs::read_to_string(&path).ok();
            let _ = std::fs::write(path, self.to_json(existing.as_deref()));
        }
    }

    /// Every profile, built-ins first.
    pub fn resolved(&self) -> Vec<Profile> {
        resolve_profiles(&self.profiles)
    }

    /// The profiles the list shows: everything but Incognito.
    pub fn listed(&self) -> Vec<Profile> {
        self.resolved()
            .into_iter()
            .filter(|profile| profile.id != "incognito")
            .collect()
    }

    /// `findBrowserProfile(listed, defaultId)?.id ?? "default"`.
    pub fn default_id(&self) -> String {
        self.listed()
            .into_iter()
            .find(|profile| profile.id == self.default_profile)
            .map_or_else(|| DEFAULT_PROFILE_ID.to_owned(), |profile| profile.id)
    }

    pub fn at_profile_limit(&self) -> bool {
        self.profiles.len() >= PROFILE_MAX_COUNT
    }

    /// `createProfile`: "New profile", then "New profile 2" and on while a name is taken.
    pub fn add_profile(&mut self, base: &str) -> Option<Profile> {
        if self.at_profile_limit() {
            return None;
        }
        let taken: Vec<String> = self.resolved().into_iter().map(|p| p.name).collect();
        let mut name = base.to_owned();
        let mut index = 2;
        while taken.contains(&name) {
            name = format!("{base} {index}");
            index += 1;
        }
        let profile = Profile {
            id: format!("profile-{}", crate::settings_voice::uuid()),
            name,
            incognito: false,
        };
        self.profiles.push(profile.clone());
        Some(profile)
    }

    /// `renameProfile`: trimmed and cut to the limit; an empty name changes nothing.
    pub fn rename_profile(&mut self, id: &str, text: &str) -> bool {
        let name: String = text.trim().chars().take(PROFILE_NAME_MAX).collect();
        if name.is_empty() {
            return false;
        }
        let Some(profile) = self.profiles.iter_mut().find(|p| p.id == id) else {
            return false;
        };
        let changed = profile.name != name;
        profile.name = name;
        changed
    }

    /// Drops a profile, and the default with it when it was the default.
    pub fn remove_profile(&mut self, id: &str) {
        self.profiles.retain(|profile| profile.id != id);
        if self.default_profile == id {
            self.default_profile = DEFAULT_PROFILE_ID.to_owned();
        }
    }

    /// The reset button of the search engine row: the engine or a custom URL is set.
    pub fn search_dirty(&self) -> bool {
        self.search_engine != DEFAULT_SEARCH_ENGINE || !self.custom_url.is_empty()
    }
}

/// `zoomLabel`.
pub fn zoom_label(zoom: f64) -> String {
    format!("{}%", (zoom * 100.).round())
}

pub fn zoom_index(zoom: f64) -> usize {
    ZOOM_LEVELS
        .iter()
        .position(|level| (level - zoom).abs() < 1e-9)
        .unwrap_or(7)
}

pub fn frame_rate_index(rate: u32) -> usize {
    FRAME_RATES.iter().position(|r| *r == rate).unwrap_or(0)
}

pub fn index_of(options: &[(&str, &str)], wire: &str) -> usize {
    options
        .iter()
        .position(|(value, _)| *value == wire)
        .unwrap_or(0)
}

/// A browser `BrowserImportSource` the machine can import cookies from: the app's name in the
/// menu and the bundle that tells it is installed.
pub struct ImportSource {
    pub id: &'static str,
    pub name: &'static str,
    app: &'static str,
}

/// The definitions of `BrowserImport/sources` in the order the menu lists them.
pub const IMPORT_SOURCES: [ImportSource; 10] = [
    ImportSource {
        id: "chrome",
        name: "Chrome",
        app: "Google Chrome",
    },
    ImportSource {
        id: "edge",
        name: "Edge",
        app: "Microsoft Edge",
    },
    ImportSource {
        id: "brave",
        name: "Brave",
        app: "Brave Browser",
    },
    ImportSource {
        id: "vivaldi",
        name: "Vivaldi",
        app: "Vivaldi",
    },
    ImportSource {
        id: "opera",
        name: "Opera",
        app: "Opera",
    },
    ImportSource {
        id: "arc",
        name: "Arc",
        app: "Arc",
    },
    ImportSource {
        id: "helium",
        name: "Helium",
        app: "Helium",
    },
    ImportSource {
        id: "firefox",
        name: "Firefox",
        app: "Firefox",
    },
    ImportSource {
        id: "zen",
        name: "Zen",
        app: "Zen",
    },
    ImportSource {
        id: "safari",
        name: "Safari",
        app: "Safari",
    },
];

/// The sources whose app is installed (the web hides `notInstalled` and `unsupportedPlatform`).
pub fn installed_sources() -> Vec<&'static ImportSource> {
    if !cfg!(target_os = "macos") {
        return Vec::new();
    }
    let user = std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Applications"));
    IMPORT_SOURCES
        .iter()
        .filter(|source| {
            let bundle = format!("{}.app", source.app);
            PathBuf::from("/Applications").join(&bundle).exists()
                || user.as_ref().is_some_and(|dir| dir.join(&bundle).exists())
                || (source.id == "safari"
                    && std::path::Path::new("/System/Cryptexes/App/System/Applications/Safari.app")
                        .exists())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_contract() {
        let settings = BrowserSettings::parse("{}").unwrap();
        assert_eq!(settings, BrowserSettings::default());
        assert_eq!(settings.viewport, Viewport::Fill);
        assert!(settings.auto_show && !settings.key_presses && !settings.mouse_presses);
        assert_eq!(settings.default_id(), "default");
    }

    #[test]
    fn invalid_values_fall_back() {
        let settings = BrowserSettings::parse(
            r#"{"browserDefaultZoomFactor": 1.3, "browserRecordingFrameRate": 45,
                "browserSearchEngine": "yahoo", "browserLinkTarget": "tab",
                "browserDefaultViewport": {"_tag": "freeform", "width": 10, "height": 800}}"#,
        )
        .unwrap();
        assert_eq!(settings, BrowserSettings::default());
    }

    #[test]
    fn viewport_round_trips_and_rotates() {
        let text = r#"{"settings": {"browserDefaultViewport":
            {"_tag":"preset","width":375,"height":667,"presetId":"iphone-se"}, "other": 1}}"#;
        let settings = BrowserSettings::parse(text).unwrap();
        assert_eq!(settings.viewport.label(), "iPhone SE");
        assert_eq!(settings.viewport.index(), 2);
        assert_eq!(settings.viewport.rotated().size(), Some((667, 375)));
        let saved: Value = serde_json::from_str(&settings.to_json(Some(text))).unwrap();
        assert_eq!(saved["settings"]["other"], 1);
        assert_eq!(
            saved["settings"]["browserDefaultViewport"]["presetId"],
            "iphone-se"
        );
    }

    #[test]
    fn picking_responsive_keeps_the_size() {
        let preset = Viewport::Preset {
            id: "pixel-7".to_owned(),
            width: 412,
            height: 915,
        };
        assert_eq!(
            preset.pick(1),
            Viewport::Freeform {
                width: 412,
                height: 915
            }
        );
        assert_eq!(
            Viewport::Fill.pick(1),
            Viewport::Freeform {
                width: 1280,
                height: 800
            }
        );
        assert_eq!(Viewport::Fill.pick(0), Viewport::Fill);
    }

    #[test]
    fn resizing_checks_range_area_and_change() {
        let wide = Viewport::Freeform {
            width: 1280,
            height: 800,
        };
        assert_eq!(wide.resized(Axis::Width, "1280"), None);
        assert_eq!(wide.resized(Axis::Width, "200"), None);
        assert_eq!(wide.resized(Axis::Width, "abc"), None);
        let broad = Viewport::Freeform {
            width: 3000,
            height: 800,
        };
        assert_eq!(broad.resized(Axis::Height, "2800"), None);
        assert_eq!(
            wide.resized(Axis::Width, " 1000 "),
            Some(Viewport::Freeform {
                width: 1000,
                height: 800
            })
        );
        assert_eq!(
            Viewport::Fill.resized(Axis::Height, "900"),
            Some(Viewport::Freeform {
                width: 1280,
                height: 900
            })
        );
    }

    #[test]
    fn profiles_are_named_uniquely_and_capped() {
        let mut settings = BrowserSettings::default();
        let first = settings.add_profile("New profile").unwrap();
        let second = settings.add_profile("New profile").unwrap();
        assert_eq!(first.name, "New profile");
        assert_eq!(second.name, "New profile 2");
        assert_ne!(first.id, second.id);
        assert!(first.id.starts_with("profile-"));
        while settings.add_profile("More").is_some() {}
        assert_eq!(settings.profiles.len(), PROFILE_MAX_COUNT);
        assert!(settings.at_profile_limit());
    }

    #[test]
    fn built_ins_come_first_and_incognito_is_not_listed() {
        let mut settings = BrowserSettings::default();
        settings.profiles.push(Profile {
            id: "default".to_owned(),
            name: "Clash".to_owned(),
            incognito: false,
        });
        let work = settings.add_profile("Work").unwrap();
        let names: Vec<_> = settings.resolved().into_iter().map(|p| p.name).collect();
        assert_eq!(names, ["Default", "Incognito", "Work"]);
        let listed: Vec<_> = settings.listed().into_iter().map(|p| p.name).collect();
        assert_eq!(listed, ["Default", "Work"]);
        settings.default_profile = work.id.clone();
        assert_eq!(settings.default_id(), work.id);
        settings.remove_profile(&work.id);
        assert_eq!(settings.default_profile, "default");
    }

    #[test]
    fn renaming_trims_and_ignores_empty_names() {
        let mut settings = BrowserSettings::default();
        let work = settings.add_profile("Work").unwrap();
        assert!(!settings.rename_profile(&work.id, "   "));
        assert!(settings.rename_profile(&work.id, &format!("  {}  ", "x".repeat(60))));
        assert_eq!(settings.profiles[0].name.chars().count(), PROFILE_NAME_MAX);
        assert!(!settings.rename_profile("default", "Mine"));
    }

    #[test]
    fn custom_search_url_needs_the_query_after_the_host() {
        assert!(valid_search_url("https://example.com/search?q=%s"));
        assert!(valid_search_url("http://example.com/%s"));
        assert!(!valid_search_url("https://example.com/search"));
        assert!(!valid_search_url("ftp://example.com/?q=%s"));
        assert!(!valid_search_url("https://%s.example.com/"));
        assert!(!valid_search_url("https://user:%s@example.com/"));
        assert!(!valid_search_url("example.com/?q=%s"));
    }

    #[test]
    fn labels_follow_the_web() {
        assert_eq!(zoom_label(0.33), "33%");
        assert_eq!(zoom_label(1.0), "100%");
        assert_eq!(zoom_index(1.0), 7);
        assert_eq!(index_of(&SEARCH_ENGINES, "custom"), 6);
        assert_eq!(PRESETS.len(), 17);
    }

    #[test]
    fn search_reset_shows_for_a_custom_url_alone() {
        let mut settings = BrowserSettings::default();
        assert!(!settings.search_dirty());
        settings.custom_url = "https://a.b/?q=%s".to_owned();
        assert!(settings.search_dirty());
    }
}
