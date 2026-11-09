//! The Appearance tab's Color scheme, Themes and Interface sections. The Interface keys are client
//! settings (`ClientSettingsSchema`, contracts/src/settings.ts:389-583) kept in
//! `client-settings.json` beside the other tabs' keys. The web keeps the color mode and the theme
//! halves in `localStorage` (`useTheme.ts`), so the desktop keeps them in `appearance-theme.json`
//! next to that file.

use crate::settings::{SearchItem, Section};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// `chatWidth` (settings.ts:393), labels from `CHAT_WIDTH_LABELS`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatWidth {
    Comfortable,
    Wide,
    Full,
}

impl ChatWidth {
    pub const ALL: &'static [Self] = &[Self::Comfortable, Self::Wide, Self::Full];

    pub fn wire(self) -> &'static str {
        match self {
            Self::Comfortable => "comfortable",
            Self::Wide => "wide",
            Self::Full => "full",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Comfortable => "Comfortable",
            Self::Wide => "Wide",
            Self::Full => "Full",
        }
    }

    pub fn menu_label(self) -> &'static str {
        match self {
            Self::Comfortable => "Comfortable (default)",
            other => other.label(),
        }
    }

    pub fn from_wire(value: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|v| v.wire() == value)
    }
}

/// `effortDisplay` (settings.ts:402).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffortDisplay {
    Label,
    Bars,
}

impl EffortDisplay {
    pub const ALL: &'static [Self] = &[Self::Label, Self::Bars];

    pub fn wire(self) -> &'static str {
        match self {
            Self::Label => "label",
            Self::Bars => "bars",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Label => "Word",
            Self::Bars => "Bars",
        }
    }

    pub fn menu_label(self) -> &'static str {
        match self {
            Self::Label => "Word (default)",
            Self::Bars => "Bars",
        }
    }

    pub fn from_wire(value: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|v| v.wire() == value)
    }
}

/// `environmentIdentificationMode` (settings.ts:446). The desktop shell always runs the dev
/// stage, so the first option is the "Dev artwork" one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvironmentIdentification {
    Artwork,
    NightlyArtwork,
    DevArtwork,
    Pill,
    None,
}

impl EnvironmentIdentification {
    /// The options a dev stage offers, in menu order.
    pub const ALL: &'static [Self] = &[Self::Artwork, Self::NightlyArtwork, Self::Pill, Self::None];

    pub fn wire(self) -> &'static str {
        match self {
            Self::Artwork => "artwork",
            Self::NightlyArtwork => "nightly-artwork",
            Self::DevArtwork => "dev-artwork",
            Self::Pill => "pill",
            Self::None => "none",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Artwork => "Dev artwork",
            Self::NightlyArtwork => "Nightly artwork",
            Self::DevArtwork => "Dev artwork",
            Self::Pill => "Version pill",
            Self::None => "None",
        }
    }

    pub fn from_wire(value: &str) -> Option<Self> {
        [
            Self::Artwork,
            Self::NightlyArtwork,
            Self::DevArtwork,
            Self::Pill,
            Self::None,
        ]
        .into_iter()
        .find(|v| v.wire() == value)
    }

    /// Where the value sits in `ALL`; the legacy `dev-artwork` reads as the default.
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|v| *v == self).unwrap_or(0)
    }
}

/// One entry of the "Thread details" menu: its sparse-map key, label, and default.
#[derive(Clone, Copy, Debug)]
pub struct ThreadItem {
    pub key: &'static str,
    pub label: &'static str,
    /// `usage` and `context` follow the retired `threadDetailsUsageEnabled` flag.
    pub follows_usage_flag: bool,
}

/// `THREAD_DETAILS_ITEMS`, in menu order.
pub const THREAD_ITEMS: [ThreadItem; 12] = [
    ThreadItem::on("environment", "Environment and worktree"),
    ThreadItem::on("openIn", "Open in editor"),
    ThreadItem::on("scripts", "Project scripts"),
    ThreadItem::on("branch", "Branch and pull request"),
    ThreadItem::on("gitActions", "Git actions"),
    ThreadItem::on("changes", "Changes"),
    ThreadItem::on("servers", "Servers"),
    ThreadItem::on("automations", "Automations"),
    ThreadItem::on("lineage", "Lineage"),
    ThreadItem::on("sideChats", "Side chats"),
    ThreadItem::usage("usage", "Usage limits"),
    ThreadItem::usage("context", "Context window"),
];

impl ThreadItem {
    const fn on(key: &'static str, label: &'static str) -> Self {
        Self {
            key,
            label,
            follows_usage_flag: false,
        }
    }

    const fn usage(key: &'static str, label: &'static str) -> Self {
        Self {
            key,
            label,
            follows_usage_flag: true,
        }
    }
}

/// The Interface section's settings, with `DEFAULT_CLIENT_SETTINGS` defaults.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppearanceSettings {
    /// `appearanceContrast`, 50..=200 step 5, default 100.
    pub contrast: u32,
    /// `glassOpacity`, 40..=100 step 5, default 80.
    pub glass_opacity: u32,
    /// `panelAnimationDurationMs`, 0..=400 step 25, default 0.
    pub panel_animation_ms: u32,
    pub chat_width: ChatWidth,
    pub effort_display: EffortDisplay,
    pub environment_identification: EnvironmentIdentification,
    /// `mathRenderingEnabled`, default off.
    pub math_rendering: bool,
    /// `persistComposerContextStrip`, default off.
    pub composer_context: bool,
    /// `threadDetailsItems`: only the choices that differ from their default.
    pub thread_items: BTreeMap<String, bool>,
    /// `threadDetailsUsageEnabled`, default off.
    pub thread_usage_flag: bool,
}

pub const CONTRAST: Slide = Slide::new(50, 200, 5, 100);
pub const GLASS: Slide = Slide::new(40, 100, 5, 80);
pub const PANEL_MS: Slide = Slide::new(0, 400, 25, 0);

/// Bounds of a stepped integer slider.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slide {
    pub min: u32,
    pub max: u32,
    pub step: u32,
    pub default: u32,
}

impl Slide {
    const fn new(min: u32, max: u32, step: u32, default: u32) -> Self {
        Self {
            min,
            max,
            step,
            default,
        }
    }

    /// Accepts only a value the schema accepts: in range and on a step.
    pub fn valid(self, value: u32) -> bool {
        (self.min..=self.max).contains(&value) && (value - self.min).is_multiple_of(self.step)
    }
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            contrast: CONTRAST.default,
            glass_opacity: GLASS.default,
            panel_animation_ms: PANEL_MS.default,
            chat_width: ChatWidth::Comfortable,
            effort_display: EffortDisplay::Label,
            environment_identification: EnvironmentIdentification::Artwork,
            math_rendering: false,
            composer_context: false,
            thread_items: BTreeMap::new(),
            thread_usage_flag: false,
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

impl AppearanceSettings {
    pub fn parse(text: &str) -> Option<Self> {
        let value: Value = serde_json::from_str(text).ok()?;
        let map = fields(&value)?;
        let d = Self::default();
        let flag =
            |key: &str, fallback: bool| map.get(key).and_then(Value::as_bool).unwrap_or(fallback);
        let choice = |key: &str| map.get(key).and_then(Value::as_str);
        let slide = |key: &str, range: Slide| {
            map.get(key)
                .and_then(Value::as_u64)
                .and_then(|v| u32::try_from(v).ok())
                .filter(|v| range.valid(*v))
                .unwrap_or(range.default)
        };
        let thread_items = map
            .get("threadDetailsItems")
            .and_then(Value::as_object)
            .map(|items| {
                THREAD_ITEMS
                    .iter()
                    .filter_map(|item| {
                        let on = items.get(item.key)?.as_bool()?;
                        Some((item.key.to_owned(), on))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some(Self {
            contrast: slide("appearanceContrast", CONTRAST),
            glass_opacity: slide("glassOpacity", GLASS),
            panel_animation_ms: slide("panelAnimationDurationMs", PANEL_MS),
            chat_width: choice("chatWidth")
                .and_then(ChatWidth::from_wire)
                .unwrap_or(d.chat_width),
            effort_display: choice("effortDisplay")
                .and_then(EffortDisplay::from_wire)
                .unwrap_or(d.effort_display),
            environment_identification: choice("environmentIdentificationMode")
                .and_then(EnvironmentIdentification::from_wire)
                .unwrap_or(d.environment_identification),
            math_rendering: flag("mathRenderingEnabled", d.math_rendering),
            composer_context: flag("persistComposerContextStrip", d.composer_context),
            thread_items,
            thread_usage_flag: flag("threadDetailsUsageEnabled", d.thread_usage_flag),
        })
    }

    pub fn entries(&self) -> [(&'static str, Value); 10] {
        let items: Map<String, Value> = self
            .thread_items
            .iter()
            .map(|(key, on)| (key.clone(), json!(on)))
            .collect();
        [
            ("appearanceContrast", json!(self.contrast)),
            ("glassOpacity", json!(self.glass_opacity)),
            ("panelAnimationDurationMs", json!(self.panel_animation_ms)),
            ("chatWidth", json!(self.chat_width.wire())),
            ("effortDisplay", json!(self.effort_display.wire())),
            (
                "environmentIdentificationMode",
                json!(self.environment_identification.wire()),
            ),
            ("mathRenderingEnabled", json!(self.math_rendering)),
            ("persistComposerContextStrip", json!(self.composer_context)),
            ("threadDetailsItems", Value::Object(items)),
            ("threadDetailsUsageEnabled", json!(self.thread_usage_flag)),
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

    pub fn load() -> Self {
        crate::settings::DeviceSettings::path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    /// Re-reads the file first, so keys the other tabs saved in the meantime survive.
    pub fn save(&self) {
        if let Some(path) = crate::settings::DeviceSettings::path() {
            let existing = std::fs::read_to_string(&path).ok();
            let _ = std::fs::write(path, self.to_json(existing.as_deref()));
        }
    }

    /// Whether a thread-details item is shown (`resolveThreadDetailsItems`).
    pub fn item_on(&self, item: &ThreadItem) -> bool {
        self.thread_items
            .get(item.key)
            .copied()
            .unwrap_or(!item.follows_usage_flag || self.thread_usage_flag)
    }

    /// `{enabled} of 12 on`.
    pub fn items_on(&self) -> usize {
        THREAD_ITEMS
            .iter()
            .filter(|item| self.item_on(item))
            .count()
    }

    /// Stores only a choice that differs from the default, as the web does.
    pub fn set_item(&mut self, item: &ThreadItem, on: bool) {
        let default = !item.follows_usage_flag;
        if on == default {
            self.thread_items.remove(item.key);
        } else {
            self.thread_items.insert(item.key.to_owned(), on);
        }
    }

    /// Whether the Thread details row differs from its default.
    pub fn items_changed(&self) -> bool {
        !self.thread_items.is_empty() || self.thread_usage_flag
    }

    pub fn reset_items(&mut self) {
        self.thread_items.clear();
        self.thread_usage_flag = false;
    }
}

/// `Math.round`-free stepping: the value under `ratio` (0 to 1) of a slider, snapped to a step.
pub fn slide_value(range: Slide, ratio: f32) -> u32 {
    let ratio = ratio.clamp(0., 1.);
    let steps = ((range.max - range.min) / range.step) as f32;
    range.min + (ratio * steps).round() as u32 * range.step
}

/// `appearanceMode` of the web theme store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorMode {
    System,
    Light,
    Dark,
}

impl ColorMode {
    pub const ALL: [Self; 3] = [Self::System, Self::Light, Self::Dark];

    pub fn wire(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }

    pub fn from_wire(value: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|v| v.wire() == value)
    }
}

/// The built-in themes, in the web's card order (T3 Code first, then `BUILT_IN_THEMES`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeId {
    Default,
    T3Chat,
    Grove,
    Ocean,
    Ember,
    Iris,
}

/// A palette color: an sRGB hex, or the OKLCH triple the web themes are written in.
#[derive(Clone, Copy, Debug)]
pub enum Paint {
    Hex(u32),
    Oklch(f32, f32, f32),
}

impl Paint {
    /// sRGB, each channel 0 to 1.
    pub fn rgb(self) -> [f32; 3] {
        match self {
            Self::Hex(hex) => [
                ((hex >> 16) & 0xff) as f32 / 255.,
                ((hex >> 8) & 0xff) as f32 / 255.,
                (hex & 0xff) as f32 / 255.,
            ],
            Self::Oklch(l, c, h) => oklch_to_srgb(l, c, h),
        }
    }

    #[cfg(test)]
    fn hex(self) -> u32 {
        let [r, g, b] = self.rgb();
        let byte = |v: f32| (v.clamp(0., 1.) * 255.).round() as u32;
        byte(r) << 16 | byte(g) << 8 | byte(b)
    }
}

/// OKLCH to gamma-encoded sRGB, clipped to the gamut (CSS Color 4 without gamut mapping).
pub fn oklch_to_srgb(l: f32, c: f32, h: f32) -> [f32; 3] {
    let (sin, cos) = h.to_radians().sin_cos();
    let (a, b) = (c * cos, c * sin);
    let lms = [
        l + 0.396_337_78 * a + 0.215_803_76 * b,
        l - 0.105_561_346 * a - 0.063_854_17 * b,
        l - 0.089_484_18 * a - 1.291_485_5 * b,
    ]
    .map(|v| v * v * v);
    let linear = [
        4.076_741_7 * lms[0] - 3.307_711_6 * lms[1] + 0.230_969_93 * lms[2],
        -1.268_438 * lms[0] + 2.609_757_4 * lms[1] - 0.341_319_4 * lms[2],
        -0.004_196_086_3 * lms[0] - 0.703_418_6 * lms[1] + 1.707_614_7 * lms[2],
    ];
    linear.map(|v| {
        let v = v.clamp(0., 1.);
        if v <= 0.003_130_8 {
            12.92 * v
        } else {
            1.055 * v.powf(1. / 2.4) - 0.055
        }
    })
}

/// The three roles a theme orb paints from (`THEME_PREVIEW_RENDER_SPECS`): the canvas it tints,
/// the accent that fills it, and the message action that glows opposite.
#[derive(Clone, Copy, Debug)]
pub struct OrbPalette {
    pub canvas: Paint,
    pub accent: Paint,
    pub action: Paint,
}

const fn hex_orb(canvas: u32, accent: u32, action: u32) -> OrbPalette {
    OrbPalette {
        canvas: Paint::Hex(canvas),
        accent: Paint::Hex(accent),
        action: Paint::Hex(action),
    }
}

const fn orb(canvas: [f32; 3], accent: [f32; 3], action: [f32; 3]) -> OrbPalette {
    OrbPalette {
        canvas: Paint::Oklch(canvas[0], canvas[1], canvas[2]),
        accent: Paint::Oklch(accent[0], accent[1], accent[2]),
        action: Paint::Oklch(action[0], action[1], action[2]),
    }
}

impl ThemeId {
    pub const ALL: [Self; 6] = [
        Self::Default,
        Self::T3Chat,
        Self::Grove,
        Self::Ocean,
        Self::Ember,
        Self::Iris,
    ];

    pub fn wire(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::T3Chat => "t3-chat",
            Self::Grove => "grove",
            Self::Ocean => "ocean",
            Self::Ember => "ember",
            Self::Iris => "iris",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "T3 Code",
            Self::T3Chat => "T3 Chat",
            Self::Grove => "Grove",
            Self::Ocean => "Ocean",
            Self::Ember => "Ember",
            Self::Iris => "Iris",
        }
    }

    pub fn from_wire(value: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|v| v.wire() == value)
    }

    /// The orb colors for one half (themePalettes.ts).
    pub fn orb(self, dark: bool) -> OrbPalette {
        match (self, dark) {
            (Self::Default, false) => hex_orb(0xfcfcfc, 0xf4f4f5, 0x4f46e5),
            (Self::Default, true) => hex_orb(0x0a0a0a, 0x1c1c1f, 0x8b9cff),
            (Self::T3Chat, false) => orb(
                [0.982446, 0.010114, 325.653],
                [0.591646, 0.217985, 0.584],
                [0.591646, 0.217985, 0.584],
            ),
            (Self::T3Chat, true) => orb(
                [0.22813, 0.020366, 307.469],
                [0.460685, 0.185347, 4.099],
                [0.460685, 0.185347, 4.099],
            ),
            (Self::Grove, false) => orb(
                [0.972369, 0.005497, 157.15],
                [0.523295, 0.112292, 158.089],
                [0.535028, 0.106403, 77.549],
            ),
            (Self::Grove, true) => orb(
                [0.260865, 0.02152, 162.75],
                [0.796228, 0.133058, 157.319],
                [0.791603, 0.129713, 83.299],
            ),
            (Self::Ocean, false) => orb(
                [0.974199, 0.002856, 241.597],
                [0.536684, 0.120219, 247.01],
                [0.493961, 0.08175, 201.584],
            ),
            (Self::Ocean, true) => orb(
                [0.242641, 0.024125, 250.573],
                [0.758933, 0.105833, 241.548],
                [0.793363, 0.105022, 199.893],
            ),
            (Self::Ember, false) => orb(
                [0.976527, 0.002685, 60.725],
                [0.552831, 0.129438, 44.656],
                [0.516323, 0.161628, 24.82],
            ),
            (Self::Ember, true) => orb(
                [0.245899, 0.019144, 42.044],
                [0.762174, 0.124117, 52.082],
                [0.747955, 0.135578, 29.432],
            ),
            (Self::Iris, false) => orb(
                [0.976531, 0.003855, 303.226],
                [0.525348, 0.15373, 294.176],
                [0.516084, 0.185229, 340.776],
            ),
            (Self::Iris, true) => orb(
                [0.225975, 0.031062, 293.741],
                [0.671712, 0.169136, 293.929],
                [0.789904, 0.130063, 337.621],
            ),
        }
    }
}

/// The color mode and the two theme halves; light and dark each name their own theme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThemeChoice {
    pub mode: ColorMode,
    pub light: ThemeId,
    pub dark: ThemeId,
}

impl Default for ThemeChoice {
    fn default() -> Self {
        Self {
            mode: ColorMode::System,
            light: ThemeId::Default,
            dark: ThemeId::Default,
        }
    }
}

impl ThemeChoice {
    const FILE: &'static str = "appearance-theme.json";

    /// The halves a theme owns, light first (`pickedModesFor`).
    pub fn picked(self, id: ThemeId) -> (bool, bool) {
        (self.light == id, self.dark == id)
    }

    /// A card sets both halves (`persistTheme`).
    pub fn pick_both(&mut self, id: ThemeId) {
        self.light = id;
        self.dark = id;
    }

    pub fn pick_half(&mut self, id: ThemeId, dark: bool) {
        if dark {
            self.dark = id;
        } else {
            self.light = id;
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        let value: Value = serde_json::from_str(text).ok()?;
        let object = value.as_object()?;
        let d = Self::default();
        let half = |key: &str, fallback: ThemeId| {
            object
                .get("themeHalves")
                .and_then(|halves| halves.get(key))
                .and_then(Value::as_str)
                .and_then(ThemeId::from_wire)
                .unwrap_or(fallback)
        };
        Some(Self {
            mode: object
                .get("appearanceMode")
                .and_then(Value::as_str)
                .and_then(ColorMode::from_wire)
                .unwrap_or(d.mode),
            light: half("light", d.light),
            dark: half("dark", d.dark),
        })
    }

    pub fn to_json(self) -> String {
        json!({
            "appearanceMode": self.mode.wire(),
            "themeHalves": { "light": self.light.wire(), "dark": self.dark.wire() },
        })
        .to_string()
    }

    fn path() -> Option<PathBuf> {
        crate::settings::DeviceSettings::path().map(|path| path.with_file_name(Self::FILE))
    }

    pub fn load() -> Self {
        Self::path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    pub fn save(self) {
        if let Some(path) = Self::path() {
            let _ = std::fs::write(path, self.to_json());
        }
    }
}

/// `color-mix(in srgb, from, to weight)`: `weight` of `from` over the rest of `to`.
pub fn mix(from: [f32; 3], to: [f32; 3], weight: f32) -> [f32; 3] {
    std::array::from_fn(|i| from[i] * weight + to[i] * (1. - weight))
}

const fn item(id: &'static str, title: &'static str, terms: &'static str) -> SearchItem {
    SearchItem {
        id,
        title,
        section: Section::Appearance,
        terms,
        desktop_only: false,
    }
}

/// The Appearance `searchableSetting` entries for these sections (settingsSearch.ts), in the
/// web's order. Typography's come from its own module.
pub const APPEARANCE_ITEMS: &[SearchItem] = &[
    item(
        "color-scheme",
        "Color scheme",
        "appearance light dark system mode",
    ),
    item("theme", "Themes", "appearance colors palette custom import"),
    item(
        "setting-appearance-contrast",
        "Contrast",
        "colors borders interface",
    ),
    item(
        "setting-glass-opacity",
        "Glass opacity",
        "transparent transparency solid menus dialogs composer",
    ),
    item(
        "chat-width",
        "Chat width",
        "wide full width column layout messages composer monitor",
    ),
    item(
        "thread-details-items",
        "Thread details",
        "customize show hide panel card environment editor scripts branch pull request git changes servers automations lineage usage limits 5 hour weekly quota tokens context window",
    ),
    item(
        "effort-display",
        "Effort display",
        "reasoning effort thinking bars graph word label level composer",
    ),
    item(
        "math-rendering",
        "Render math",
        "latex tex katex equations formulas markdown",
    ),
    item("panel-animations", "Panel animations", ""),
    item(
        "environment-identification",
        "Environment identification",
        "dev nightly latest channel artwork pill label hide none",
    ),
    item("composer-context", "Composer context", ""),
];

/// The search ids that live under the Advanced disclosure.
pub fn is_advanced(id: &str) -> bool {
    matches!(
        id,
        "panel-animations" | "environment-identification" | "composer-context"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::search;

    #[test]
    fn defaults_match_the_contract() {
        let d = AppearanceSettings::default();
        assert_eq!(
            (d.contrast, d.glass_opacity, d.panel_animation_ms),
            (100, 80, 0)
        );
        assert_eq!(d.items_on(), 10);
        assert!(!d.items_changed());
    }

    #[test]
    fn parse_falls_back_per_key_and_rejects_off_step_numbers() {
        let text = r#"{"settings":{"appearanceContrast":120,"glassOpacity":42,"chatWidth":"full","effortDisplay":"nope","panelAnimationDurationMs":400,"environmentIdentificationMode":"pill","mathRenderingEnabled":true}}"#;
        let s = AppearanceSettings::parse(text).unwrap();
        assert_eq!(s.contrast, 120);
        assert_eq!(s.glass_opacity, 80);
        assert_eq!(s.chat_width, ChatWidth::Full);
        assert_eq!(s.effort_display, EffortDisplay::Label);
        assert_eq!(s.panel_animation_ms, 400);
        assert_eq!(
            s.environment_identification,
            EnvironmentIdentification::Pill
        );
        assert!(s.math_rendering);
        assert!(AppearanceSettings::parse("[]").is_none());
    }

    #[test]
    fn thread_items_store_only_non_defaults_and_follow_the_usage_flag() {
        let mut s = AppearanceSettings::default();
        let usage = &THREAD_ITEMS[10];
        let changes = &THREAD_ITEMS[5];
        s.set_item(usage, true);
        s.set_item(changes, false);
        assert_eq!(s.thread_items.len(), 2);
        assert_eq!(s.items_on(), 10);
        s.set_item(usage, false);
        s.set_item(changes, true);
        assert!(s.thread_items.is_empty());

        s.thread_usage_flag = true;
        assert!(s.item_on(usage) && s.item_on(&THREAD_ITEMS[11]));
        assert_eq!(s.items_on(), 12);
        s.reset_items();
        assert_eq!(s.items_on(), 10);
    }

    #[test]
    fn save_keeps_foreign_keys_and_round_trips() {
        let mut s = AppearanceSettings {
            chat_width: ChatWidth::Wide,
            ..AppearanceSettings::default()
        };
        s.set_item(&THREAD_ITEMS[0], false);
        let out = s.to_json(Some(r#"{"settings":{"timestampFormat":"24-hour"}}"#));
        let value: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(value["settings"]["timestampFormat"], "24-hour");
        assert_eq!(
            value["settings"]["threadDetailsItems"]["environment"],
            false
        );
        assert_eq!(AppearanceSettings::parse(&out), Some(s));
    }

    #[test]
    fn slider_snaps_to_steps() {
        assert_eq!(slide_value(CONTRAST, 0.), 50);
        assert_eq!(slide_value(CONTRAST, 1.), 200);
        assert_eq!(slide_value(CONTRAST, 1. / 3.), 100);
        assert_eq!(slide_value(PANEL_MS, 0.51), 200);
        assert_eq!(slide_value(GLASS, 2.), 100);
    }

    #[test]
    fn oklch_matches_known_colors() {
        assert_eq!(Paint::Oklch(1., 0., 0.).hex(), 0xffffff);
        assert_eq!(Paint::Oklch(0., 0., 0.).hex(), 0x000000);
        // oklch(0.628 0.2577 29.23) is sRGB red.
        assert_eq!(
            Paint::Oklch(0.627_955, 0.257_683, 29.233_89).hex(),
            0xff0000
        );
        assert_eq!(Paint::Hex(0x4f46e5).hex(), 0x4f46e5);
    }

    #[test]
    fn theme_choice_follows_the_web_semantics() {
        let mut t = ThemeChoice::default();
        assert_eq!(t.picked(ThemeId::Default), (true, true));
        t.pick_both(ThemeId::Grove);
        t.pick_half(ThemeId::Iris, true);
        assert_eq!(t.picked(ThemeId::Grove), (true, false));
        assert_eq!(t.picked(ThemeId::Iris), (false, true));
        t.mode = ColorMode::Dark;
        assert_eq!(ThemeChoice::parse(&t.to_json()), Some(t));
        assert_eq!(ThemeChoice::parse("{}"), Some(ThemeChoice::default()));
    }

    #[test]
    fn items_are_searchable() {
        assert!(
            APPEARANCE_ITEMS
                .iter()
                .all(|i| i.section == Section::Appearance)
        );
        assert_eq!(
            search("latex", APPEARANCE_ITEMS).first().map(|i| i.id),
            Some("math-rendering")
        );
        assert!(is_advanced("composer-context") && !is_advanced("chat-width"));
    }
}
