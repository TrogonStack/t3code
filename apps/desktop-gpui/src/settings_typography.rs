//! The Appearance tab's Typography section: the font keys of the device settings
//! (`UnifiedSettings`, contracts/src/settings.ts:467-484, 576) kept in `client-settings.json`
//! beside the other tabs' keys. The web keeps these in the server settings and the "Separate
//! fonts" flag in `localStorage` (`t3code:typography-advanced`); the desktop keeps all of them in
//! the one device file.

use crate::settings::{SearchItem, Section};
use serde_json::{Map, Value, json};
use std::path::PathBuf;

/// `DEFAULT_*_FONT_SIZE` and their bounds (settings.ts:160-196).
pub const INTERFACE_SIZE: SizeRange = SizeRange::new(12, 20, 16);
pub const PROMPT_SIZE: SizeRange = SizeRange::new(12, 20, 14);
pub const CODE_SIZE: SizeRange = SizeRange::new(10, 18, 13);
pub const TERMINAL_SIZE: SizeRange = SizeRange::new(8, 20, 12);

/// `FontFamilyPreference` is a string of at most 200 characters (settings.ts:294).
const MAX_FAMILY: usize = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SizeRange {
    pub min: u32,
    pub max: u32,
    pub default: u32,
}

impl SizeRange {
    const fn new(min: u32, max: u32, default: u32) -> Self {
        Self { min, max, default }
    }

    pub fn contains(self, px: u32) -> bool {
        (self.min..=self.max).contains(&px)
    }

    /// The px list of the size select, `min` through `max`.
    pub fn options(self) -> impl Iterator<Item = u32> {
        self.min..=self.max
    }
}

/// The four font rows of the section.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Interface,
    Prompt,
    Code,
    Terminal,
}

impl Slot {
    pub const ALL: [Self; 4] = [Self::Interface, Self::Prompt, Self::Code, Self::Terminal];

    pub fn range(self) -> SizeRange {
        match self {
            Self::Interface => INTERFACE_SIZE,
            Self::Prompt => PROMPT_SIZE,
            Self::Code => CODE_SIZE,
            Self::Terminal => TERMINAL_SIZE,
        }
    }

    /// The monospace rows take only fixed-width families (`requireMonospace`).
    pub fn requires_monospace(self) -> bool {
        matches!(self, Self::Code | Self::Terminal)
    }

    fn family_key(self) -> &'static str {
        match self {
            Self::Interface => "fontFamilySans",
            Self::Prompt => "fontFamilyComposer",
            Self::Code => "fontFamilyCode",
            Self::Terminal => "fontFamilyTerminal",
        }
    }

    fn size_key(self) -> &'static str {
        match self {
            Self::Interface => "fontSizeInterface",
            Self::Prompt => "fontSizePrompt",
            Self::Code => "fontSizeCode",
            Self::Terminal => "fontSizeTerminal",
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }
}

/// The Typography section's settings, with the contract's defaults.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypographySettings {
    /// `fontFamily{Sans,Composer,Code,Terminal}`, empty for the default.
    pub families: [String; 4],
    /// `fontSize{Interface,Prompt,Code,Terminal}`.
    pub sizes: [u32; 4],
    /// `fontSmoothing`, default on (settings.ts:484).
    pub smoothing: bool,
    /// `wordWrap`, default on (settings.ts:576).
    pub word_wrap: bool,
    /// "Separate fonts": the web's `TYPOGRAPHY_ADVANCED_STORAGE_KEY`, default off.
    pub separate: bool,
}

impl Default for TypographySettings {
    fn default() -> Self {
        Self {
            families: Default::default(),
            sizes: Slot::ALL.map(|slot| slot.range().default),
            smoothing: true,
            word_wrap: true,
            separate: false,
        }
    }
}

fn fields(value: &Value) -> Option<&Map<String, Value>> {
    let object = value.as_object()?;
    match object.get("settings") {
        Some(Value::Object(inner)) => Some(inner),
        _ => Some(object),
    }
}

impl TypographySettings {
    pub fn family(&self, slot: Slot) -> &str {
        &self.families[slot.index()]
    }

    pub fn size(&self, slot: Slot) -> u32 {
        self.sizes[slot.index()]
    }

    pub fn set_family(&mut self, slot: Slot, family: &str) {
        self.families[slot.index()] = family.chars().take(MAX_FAMILY).collect();
    }

    pub fn set_size(&mut self, slot: Slot, px: u32) {
        if slot.range().contains(px) {
            self.sizes[slot.index()] = px;
        }
    }

    /// A row's reset puts back its family and its size (TypographySettings.tsx:62-68).
    pub fn reset(&mut self, slot: Slot) {
        let d = Self::default();
        self.families[slot.index()] = d.families[slot.index()].clone();
        self.sizes[slot.index()] = d.sizes[slot.index()];
    }

    pub fn is_default(&self, slot: Slot) -> bool {
        let d = Self::default();
        self.family(slot) == d.family(slot) && self.size(slot) == d.size(slot)
    }

    /// The family and size the terminal preview uses: the code row's unless fonts are separate
    /// (`resolveTerminalFontPreference`, `resolveTerminalFontSizePreference`).
    pub fn terminal_slot(&self) -> Slot {
        if self.separate {
            Slot::Terminal
        } else {
            Slot::Code
        }
    }

    /// The prompt preview's family slot and size: the prompt row's when fonts are separate, else
    /// the interface font at `text-sm` (`--font-size-prompt`, appearanceFonts.ts:92-95).
    pub fn prompt_font(&self) -> (Slot, f32) {
        if self.separate {
            (Slot::Prompt, self.size(Slot::Prompt) as f32)
        } else {
            (Slot::Interface, self.size(Slot::Interface) as f32 * 0.875)
        }
    }

    /// A missing or invalid key falls back to its default; anything but an object is rejected.
    pub fn parse(text: &str) -> Option<Self> {
        let value: Value = serde_json::from_str(text).ok()?;
        let map = fields(&value)?;
        let d = Self::default();
        let mut out = d.clone();
        for slot in Slot::ALL {
            if let Some(family) = map.get(slot.family_key()).and_then(Value::as_str)
                && family.chars().count() <= MAX_FAMILY
            {
                out.families[slot.index()] = family.to_owned();
            }
            if let Some(px) = map
                .get(slot.size_key())
                .and_then(Value::as_u64)
                .and_then(|px| u32::try_from(px).ok())
                .filter(|px| slot.range().contains(*px))
            {
                out.sizes[slot.index()] = px;
            }
        }
        let flag =
            |key: &str, fallback: bool| map.get(key).and_then(Value::as_bool).unwrap_or(fallback);
        out.smoothing = flag("fontSmoothing", d.smoothing);
        out.word_wrap = flag("wordWrap", d.word_wrap);
        out.separate = flag("typographyAdvanced", d.separate);
        Some(out)
    }

    pub fn entries(&self) -> Vec<(&'static str, Value)> {
        let mut entries = Vec::with_capacity(11);
        for slot in Slot::ALL {
            entries.push((slot.family_key(), json!(self.family(slot))));
            entries.push((slot.size_key(), json!(self.size(slot))));
        }
        entries.push(("fontSmoothing", json!(self.smoothing)));
        entries.push(("wordWrap", json!(self.word_wrap)));
        entries.push(("typographyAdvanced", json!(self.separate)));
        entries
    }

    /// Writes these keys into `existing`, keeping every key this section does not own.
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
    pub fn save(&self) {
        if let Some(path) = Self::path() {
            let existing = std::fs::read_to_string(&path).ok();
            let _ = std::fs::write(path, self.to_json(existing.as_deref()));
        }
    }
}

/// What the picker shows for the default family: `resolveDefaultFamilyLabel` over the web's
/// default stacks, which resolves to the Apple faces on macOS and to the web's fallback labels
/// elsewhere (TypographySettings.tsx:48-49, appearanceFonts.ts:243-250).
pub fn default_family_label(monospace: bool) -> &'static str {
    match (cfg!(target_os = "macos"), monospace) {
        (true, false) => "SF Pro",
        (true, true) => "SF Mono",
        (false, false) => "System default",
        (false, true) => "System monospace",
    }
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

/// The Typography `searchableSetting` entries (settingsSearch.ts:371-412), in the web's order.
/// Font smoothing is macOS-only on the web (`macOnly`).
pub const TYPOGRAPHY_ITEMS: &[SearchItem] = &[
    item(
        "interface-font",
        "Interface font",
        "typography family size system sans",
    ),
    item(
        "prompt-font",
        "Prompt font",
        "typography family size composer input",
    ),
    item(
        "code-font",
        "Code font",
        "typography family size monospace code blocks diffs file previews",
    ),
    item(
        "terminal-font",
        "Terminal font",
        "typography family size monospace output",
    ),
    item(
        "separate-fonts",
        "Separate fonts",
        "typography advanced prompt code terminal font family size individually",
    ),
    item(
        "font-smoothing",
        "Font smoothing",
        "typography text grayscale anti aliasing macos thin",
    ),
    item(
        "word-wrap",
        "Word wrap",
        "long lines code blocks diffs file previews",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_contract() {
        let d = TypographySettings::default();
        assert_eq!(d.sizes, [16, 14, 13, 12]);
        assert_eq!(d.families, ["", "", "", ""].map(String::from));
        assert!(d.smoothing && d.word_wrap && !d.separate);
    }

    #[test]
    fn round_trips_and_keeps_foreign_keys() {
        let mut s = TypographySettings::default();
        s.set_family(Slot::Code, "Menlo");
        s.set_size(Slot::Terminal, 9);
        s.word_wrap = false;
        s.separate = true;
        let existing = r#"{"settings":{"diffLayout":"split"}}"#;
        let text = s.to_json(Some(existing));
        assert_eq!(TypographySettings::parse(&text), Some(s));
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["settings"]["diffLayout"], "split");
        assert_eq!(value["settings"]["fontFamilyCode"], "Menlo");
    }

    #[test]
    fn invalid_keys_take_defaults() {
        let parsed = TypographySettings::parse(
            r#"{"fontSizeCode":99,"fontSizeInterface":"big","fontFamilySans":7,"wordWrap":"no"}"#,
        )
        .unwrap();
        assert_eq!(parsed, TypographySettings::default());
        assert_eq!(TypographySettings::parse("[]"), None);
    }

    #[test]
    fn size_range_bounds_follow_the_contract() {
        assert!(CODE_SIZE.contains(10) && CODE_SIZE.contains(18) && !CODE_SIZE.contains(19));
        assert!(!TERMINAL_SIZE.contains(7) && TERMINAL_SIZE.contains(20));
        let mut s = TypographySettings::default();
        s.set_size(Slot::Code, 40);
        assert_eq!(s.size(Slot::Code), 13);
        assert_eq!(CODE_SIZE.options().count(), 9);
    }

    #[test]
    fn reset_restores_family_and_size_of_one_row() {
        let mut s = TypographySettings::default();
        s.set_family(Slot::Interface, "Georgia");
        s.set_size(Slot::Interface, 18);
        s.set_size(Slot::Prompt, 18);
        assert!(!s.is_default(Slot::Interface));
        s.reset(Slot::Interface);
        assert!(s.is_default(Slot::Interface));
        assert_eq!(s.size(Slot::Prompt), 18);
    }

    #[test]
    fn previews_follow_the_separate_flag() {
        let mut s = TypographySettings::default();
        assert_eq!(s.terminal_slot(), Slot::Code);
        assert_eq!(s.prompt_font(), (Slot::Interface, 14.));
        s.separate = true;
        assert_eq!(s.terminal_slot(), Slot::Terminal);
        assert_eq!(s.prompt_font(), (Slot::Prompt, 14.));
    }

    #[test]
    fn items_are_searchable_by_the_web_terms() {
        let hits = crate::settings::search("grayscale", crate::settings::searchable());
        assert!(hits.iter().any(|item| item.id == "font-smoothing"));
    }
}
