//! The SnapShots tab (`SnapShotSettings.tsx`, `SnapShotSetupDialog.tsx`): its device settings, the
//! capture state the web reads from the Electron bridge, the ports of the two `.logic.ts` files
//! (direct and unavailable modes only; the portal, Linux and Windows branches have no GPUI
//! counterpart), the shortcut helpers of `lib/snapShotShortcut.ts` and the search entries.
//!
//! Every key lives in `client-settings.json` (`ClientSettingsSchema`).

use crate::keybinding_rules::conflict_key;
use crate::keybindings::{Rule, Shortcut as Chord};
use crate::mac_permissions::MacPermissions;
use crate::settings::{SearchItem, Section};
use serde_json::{Map, Value, json};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modifier {
    Shift,
    Meta,
    Control,
    Alt,
}

impl Modifier {
    fn wire(self) -> &'static str {
        match self {
            Self::Shift => "shift",
            Self::Meta => "meta",
            Self::Control => "control",
            Self::Alt => "alt",
        }
    }

    fn from_wire(value: &str) -> Option<Self> {
        Some(match value {
            "shift" => Self::Shift,
            "meta" => Self::Meta,
            "control" => Self::Control,
            "alt" => Self::Alt,
            _ => return None,
        })
    }
}

/// `SnapShotShortcut`: both Shift keys, a pair of another modifier, or a key chord.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Shortcut {
    BothShift,
    Pair(Modifier),
    Chord(Chord),
}

impl Shortcut {
    fn pair(&self) -> Option<Modifier> {
        match self {
            Self::BothShift => Some(Modifier::Shift),
            Self::Pair(modifier) => Some(*modifier),
            Self::Chord(_) => None,
        }
    }

    fn to_json(&self) -> Value {
        match self {
            Self::BothShift => json!({"kind": "both-shift-keys"}),
            Self::Pair(modifier) => json!({"kind": "modifier-pair", "modifier": modifier.wire()}),
            Self::Chord(chord) => json!({
                "key": chord.key,
                "metaKey": chord.meta,
                "ctrlKey": chord.ctrl,
                "shiftKey": chord.shift,
                "altKey": chord.alt,
                "modKey": chord.modifier,
            }),
        }
    }

    fn from_json(value: &Value) -> Option<Self> {
        let object = value.as_object()?;
        if let Some(kind) = object.get("kind").and_then(Value::as_str) {
            return match kind {
                "both-shift-keys" => Some(Self::BothShift),
                "modifier-pair" => {
                    Modifier::from_wire(object.get("modifier")?.as_str()?).map(Self::Pair)
                }
                _ => None,
            };
        }
        let flag = |key: &str| object.get(key).and_then(Value::as_bool).unwrap_or(false);
        let chord = Chord {
            key: object.get("key")?.as_str()?.to_owned(),
            meta: flag("metaKey"),
            ctrl: flag("ctrlKey"),
            shift: flag("shiftKey"),
            alt: flag("altKey"),
            modifier: flag("modKey"),
        };
        (chord.meta || chord.ctrl || chord.shift || chord.alt || chord.modifier)
            .then_some(Self::Chord(chord))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sound {
    SoftPop,
    CameraShutter,
}

impl Sound {
    fn wire(self) -> &'static str {
        match self {
            Self::SoftPop => "soft-pop",
            Self::CameraShutter => "camera-shutter",
        }
    }
}

/// `SnapShotSoundSelection`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundSelection {
    Off,
    Sound(Sound),
}

impl SoundSelection {
    pub const ALL: [Self; 3] = [
        Self::Off,
        Self::Sound(Sound::SoftPop),
        Self::Sound(Sound::CameraShutter),
    ];
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapShotSettings {
    pub enabled: bool,
    pub include_accessibility: bool,
    pub shortcut: Shortcut,
    pub play_sound: bool,
    pub sound: Sound,
    pub flash: bool,
    pub animations: bool,
}

impl Default for SnapShotSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            include_accessibility: true,
            shortcut: Shortcut::BothShift,
            play_sound: true,
            sound: Sound::SoftPop,
            flash: true,
            animations: true,
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

impl SnapShotSettings {
    pub fn sound_selection(&self) -> SoundSelection {
        if self.play_sound {
            SoundSelection::Sound(self.sound)
        } else {
            SoundSelection::Off
        }
    }

    /// `snapShotSoundPatch`: off only stops the sound; a choice also turns it on.
    pub fn select_sound(&mut self, selection: SoundSelection) {
        match selection {
            SoundSelection::Off => self.play_sound = false,
            SoundSelection::Sound(sound) => {
                self.play_sound = true;
                self.sound = sound;
            }
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        let value: Value = serde_json::from_str(text).ok()?;
        let map = fields(&value)?;
        let defaults = Self::default();
        let flag =
            |key: &str, default: bool| map.get(key).and_then(Value::as_bool).unwrap_or(default);
        Some(Self {
            enabled: flag("snapShotEnabled", defaults.enabled),
            include_accessibility: flag(
                "snapShotIncludeAccessibility",
                defaults.include_accessibility,
            ),
            shortcut: map
                .get("snapShotShortcut")
                .and_then(Shortcut::from_json)
                .unwrap_or(defaults.shortcut),
            play_sound: flag("snapShotPlaySound", defaults.play_sound),
            sound: match map.get("snapShotSound").and_then(Value::as_str) {
                Some("camera-shutter") => Sound::CameraShutter,
                _ => Sound::SoftPop,
            },
            flash: flag("snapShotFlash", defaults.flash),
            animations: flag("snapShotAnimations", defaults.animations),
        })
    }

    fn entries(&self) -> [(&'static str, Value); 7] {
        [
            ("snapShotEnabled", json!(self.enabled)),
            (
                "snapShotIncludeAccessibility",
                json!(self.include_accessibility),
            ),
            ("snapShotShortcut", self.shortcut.to_json()),
            ("snapShotPlaySound", json!(self.play_sound)),
            ("snapShotSound", json!(self.sound.wire())),
            ("snapShotFlash", json!(self.flash)),
            ("snapShotAnimations", json!(self.animations)),
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Direct,
    Unavailable,
}

/// `DesktopSnapShotState` for the modes GPUI has.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    pub mode: Mode,
    pub mac_permissions: Option<MacPermissions>,
    pub shortcut: Shortcut,
    pub shortcut_registered: bool,
    pub shortcut_message: Option<String>,
    pub message: Option<String>,
}

const MAC_SCREEN_CAPTURE_PERMISSION_MESSAGE: &str =
    "Allow Screen Recording in System Settings, then restart T3 Code.";
const MAC_ACCESSIBILITY_PERMISSION_MESSAGE: &str =
    "Allow Accessibility in System Settings, then restart T3 Code.";
const MAC_BOTH_PERMISSIONS_MESSAGE: &str =
    "Allow Accessibility and Screen Recording in System Settings, then restart T3 Code.";

/// `macPermissionMessage`.
pub fn mac_permission_message(
    permissions: MacPermissions,
    include_accessibility: bool,
) -> Option<&'static str> {
    let accessibility_granted = !include_accessibility || permissions.accessibility;
    if !accessibility_granted && !permissions.screen_recording {
        Some(MAC_BOTH_PERMISSIONS_MESSAGE)
    } else if !accessibility_granted {
        Some(MAC_ACCESSIBILITY_PERMISSION_MESSAGE)
    } else if permissions.screen_recording {
        None
    } else {
        Some(MAC_SCREEN_CAPTURE_PERMISSION_MESSAGE)
    }
}

/// `observedPairMessage` for a pair on macOS.
fn observed_pair_message(modifier: Modifier) -> String {
    let label = match modifier {
        Modifier::Shift => "Shift",
        Modifier::Meta => "Command",
        Modifier::Control => "Control",
        Modifier::Alt => "Option",
    };
    format!("{label} + {label} is observed and cannot be reserved exclusively.")
}

impl State {
    /// The state the bridge reports. Without a global shortcut backend, a shortcut counts as
    /// registered once capture is enabled and every permission is allowed.
    pub fn compute(settings: &SnapShotSettings, permissions: Option<MacPermissions>) -> Self {
        let Some(permissions) = permissions else {
            return Self {
                mode: Mode::Unavailable,
                mac_permissions: None,
                shortcut: settings.shortcut.clone(),
                shortcut_registered: false,
                shortcut_message: None,
                message: None,
            };
        };
        let message = settings
            .enabled
            .then(|| mac_permission_message(permissions, settings.include_accessibility))
            .flatten()
            .map(str::to_owned);
        let registered = settings.enabled && message.is_none();
        Self {
            mode: Mode::Direct,
            mac_permissions: Some(permissions),
            shortcut: settings.shortcut.clone(),
            shortcut_registered: registered,
            shortcut_message: registered
                .then(|| settings.shortcut.pair().map(observed_pair_message))
                .flatten(),
            message,
        }
    }
}

/// `snapShotStatus`.
pub fn status(state: Option<&State>, enabled: bool) -> String {
    let Some(state) = state else {
        return "Checking snapshots\u{2026}".to_owned();
    };
    if state.mode == Mode::Unavailable {
        return state
            .message
            .clone()
            .unwrap_or_else(|| "Not supported on this platform.".to_owned());
    }
    if !enabled {
        return "Turn this on to set up snapshots.".to_owned();
    }
    setup_summary(state, enabled).to_owned()
}

/// `snapShotSetupSummary`, direct mode.
pub fn setup_summary(state: &State, enabled: bool) -> &'static str {
    if state.message.is_some() {
        "Capture needs attention"
    } else if !enabled {
        "Enable capture to continue"
    } else if state.shortcut_registered {
        "Ready to capture"
    } else {
        "Finish shortcut setup"
    }
}

/// `snapShotShortcutStatus`.
pub fn shortcut_status(state: Option<&State>) -> Option<String> {
    let state = state?;
    if state.shortcut_registered {
        Some("Shortcut saved.".to_owned())
    } else {
        state.shortcut_message.clone()
    }
}

/// `snapShotSetupButtonLabel`.
pub fn setup_button_label(state: Option<&State>) -> &'static str {
    match state {
        Some(state) if access_ready(state) => "Manage capture",
        _ => "Continue setup",
    }
}

/// `snapShotSetupComplete`.
pub fn setup_complete(state: Option<&State>, include_accessibility: bool) -> bool {
    state.is_some_and(|state| {
        state.mac_permissions.is_some()
            && access_ready(state)
            && mac_permissions_ready(state, include_accessibility)
            && state.shortcut_registered
    })
}

pub const DESCRIPTION: &str = "Capture a window and attach it to your current draft.";

/// `captureSetupAccessReady`.
pub fn access_ready(state: &State) -> bool {
    state.mode != Mode::Unavailable && state.message.is_none()
}

/// `captureSetupMacPermissionsReady`.
pub fn mac_permissions_ready(state: &State, include_accessibility: bool) -> bool {
    match state.mac_permissions {
        None => true,
        Some(permissions) => {
            permissions.screen_recording && (!include_accessibility || permissions.accessibility)
        }
    }
}

/// `captureSetupShortcutReady`.
pub fn shortcut_ready(state: &State, unsaved: bool) -> bool {
    !unsaved && access_ready(state) && state.shortcut_registered
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Access,
    Shortcut,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Requested {
    Resume,
    Step(Step),
}

/// `captureSetupInitialStep`.
pub fn initial_step(state: &State, requested: Requested) -> Step {
    if !access_ready(state) {
        return Step::Access;
    }
    match requested {
        Requested::Resume if state.mode == Mode::Direct && !state.shortcut_registered => {
            Step::Access
        }
        Requested::Resume => Step::Shortcut,
        Requested::Step(step) => step,
    }
}

/// `captureSetupShouldDisableOnClose`.
pub fn should_disable_on_close(was_enabled: bool, completed: bool) -> bool {
    !was_enabled && !completed
}

fn modifier_key_label(modifier: Modifier, mac: bool) -> &'static str {
    match (modifier, mac) {
        (Modifier::Shift, _) => "\u{21e7}",
        (Modifier::Meta, true) => "\u{2318}",
        (Modifier::Control, true) => "\u{2303}",
        (Modifier::Alt, true) => "\u{2325}",
        (Modifier::Meta, false) => "Super",
        (Modifier::Control, false) => "Ctrl",
        (Modifier::Alt, false) => "Alt",
    }
}

/// `formatShortcutKeyLabel`.
fn key_label(key: &str) -> String {
    match key {
        " " => "Space".to_owned(),
        "escape" => "Esc".to_owned(),
        "arrowup" => "Up".to_owned(),
        "arrowdown" => "Down".to_owned(),
        "arrowleft" => "Left".to_owned(),
        "arrowright" => "Right".to_owned(),
        _ if key.chars().count() == 1 => key.to_uppercase(),
        _ => {
            let mut chars = key.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
                .unwrap_or_default()
        }
    }
}

/// `snapShotShortcutKeyLabels`.
pub fn key_labels(shortcut: &Shortcut, mac: bool) -> Vec<String> {
    if let Some(modifier) = shortcut.pair() {
        let label = modifier_key_label(modifier, mac).to_owned();
        return vec![label.clone(), label];
    }
    let Shortcut::Chord(chord) = shortcut else {
        return Vec::new();
    };
    [
        (chord.ctrl || (chord.modifier && !mac), Modifier::Control),
        (chord.alt, Modifier::Alt),
        (chord.shift, Modifier::Shift),
        (chord.meta || (chord.modifier && mac), Modifier::Meta),
    ]
    .into_iter()
    .filter(|(enabled, _)| *enabled)
    .map(|(_, modifier)| modifier_key_label(modifier, mac).to_owned())
    .chain(std::iter::once(key_label(&chord.key)))
    .collect()
}

/// `sameSnapShotShortcut`.
pub fn same_shortcut(left: &Shortcut, right: &Shortcut, mac: bool) -> bool {
    match (left, right) {
        (Shortcut::Chord(a), Shortcut::Chord(b)) => conflict_key(a, mac) == conflict_key(b, mac),
        (Shortcut::Chord(_), _) | (_, Shortcut::Chord(_)) => false,
        _ => left.pair() == right.pair(),
    }
}

/// `snapShotKeybindingConflict`: the command of the first binding on the same keys.
pub fn keybinding_conflict<'a>(
    shortcut: &Shortcut,
    rules: &'a [Rule],
    mac: bool,
) -> Option<&'a str> {
    let Shortcut::Chord(chord) = shortcut else {
        return None;
    };
    let key = conflict_key(chord, mac);
    rules
        .iter()
        .find(|rule| conflict_key(&rule.shortcut, mac) == key)
        .map(|rule| rule.command.as_str())
}

const fn item(id: &'static str, title: &'static str, terms: &'static str) -> SearchItem {
    SearchItem {
        id,
        title,
        section: Section::SnapShot,
        terms,
        desktop_only: false,
    }
}

/// The tab's `searchableSetting` entries (settingsSearch.ts:942-978).
pub const SNAPSHOT_ITEMS: &[SearchItem] = &[
    item(
        "snap-shot-enabled",
        "SnapShots",
        "window capture screenshot",
    ),
    item(
        "snap-shot-accessibility",
        "Include app text",
        "capture accessibility data text UI structure elements privacy omit agent context",
    ),
    item("snap-shot-shortcut", "Capture shortcut", ""),
    item("snap-shot-sound", "Capture sound", ""),
    item("snap-shot-flash", "Capture flash", ""),
    item("snap-shot-animations", "Capture animations", ""),
];

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: MacPermissions = MacPermissions {
        screen_recording: true,
        accessibility: true,
    };

    fn enabled() -> SnapShotSettings {
        SnapShotSettings {
            enabled: true,
            ..SnapShotSettings::default()
        }
    }

    fn chord(key: &str, modifier: bool, shift: bool) -> Shortcut {
        Shortcut::Chord(Chord {
            key: key.to_owned(),
            meta: false,
            ctrl: false,
            shift,
            alt: false,
            modifier,
        })
    }

    #[test]
    fn defaults_match_the_schema_and_survive_a_round_trip() {
        let defaults = SnapShotSettings::default();
        assert_eq!(SnapShotSettings::parse("{}"), Some(defaults.clone()));
        let mut changed = defaults;
        changed.enabled = true;
        changed.shortcut = chord("k", true, true);
        changed.sound = Sound::CameraShutter;
        changed.flash = false;
        let text = changed.to_json(Some(r#"{"settings":{"other":1}}"#));
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["settings"]["other"], 1);
        assert_eq!(value["settings"]["snapShotEnabled"], true);
        assert_eq!(value["settings"]["snapShotSound"], "camera-shutter");
        assert_eq!(value["settings"]["snapShotShortcut"]["modKey"], true);
        assert_eq!(SnapShotSettings::parse(&text), Some(changed));
    }

    #[test]
    fn shortcuts_round_trip_and_a_chord_needs_a_modifier() {
        for shortcut in [
            Shortcut::BothShift,
            Shortcut::Pair(Modifier::Alt),
            chord("space", true, false),
        ] {
            assert_eq!(Shortcut::from_json(&shortcut.to_json()), Some(shortcut));
        }
        assert_eq!(
            Shortcut::from_json(&json!({"key": "k", "metaKey": false})),
            None
        );
    }

    #[test]
    fn sound_patch_matches_the_web() {
        let mut settings = SnapShotSettings::default();
        settings.select_sound(SoundSelection::Off);
        assert!(!settings.play_sound);
        assert_eq!(settings.sound, Sound::SoftPop);
        assert_eq!(settings.sound_selection(), SoundSelection::Off);
        settings.select_sound(SoundSelection::Sound(Sound::CameraShutter));
        assert!(settings.play_sound);
        assert_eq!(
            settings.sound_selection(),
            SoundSelection::Sound(Sound::CameraShutter)
        );
    }

    #[test]
    fn permission_messages_follow_what_is_missing() {
        let none = MacPermissions::default();
        assert_eq!(
            mac_permission_message(none, true),
            Some(MAC_BOTH_PERMISSIONS_MESSAGE)
        );
        assert_eq!(
            mac_permission_message(none, false),
            Some(MAC_SCREEN_CAPTURE_PERMISSION_MESSAGE)
        );
        let screen_only = MacPermissions {
            screen_recording: true,
            accessibility: false,
        };
        assert_eq!(
            mac_permission_message(screen_only, true),
            Some(MAC_ACCESSIBILITY_PERMISSION_MESSAGE)
        );
        assert_eq!(mac_permission_message(screen_only, false), None);
        assert_eq!(mac_permission_message(ALL, true), None);
    }

    #[test]
    fn state_is_quiet_while_off_and_flags_missing_permissions_when_on() {
        let off = State::compute(
            &SnapShotSettings::default(),
            Some(MacPermissions::default()),
        );
        assert_eq!(off.message, None);
        assert_eq!(
            status(Some(&off), false),
            "Turn this on to set up snapshots."
        );
        let needs = State::compute(&enabled(), Some(MacPermissions::default()));
        assert!(needs.message.is_some());
        assert!(!needs.shortcut_registered);
        assert_eq!(status(Some(&needs), true), "Capture needs attention");
        assert_eq!(setup_button_label(Some(&needs)), "Continue setup");
        let ready = State::compute(&enabled(), Some(ALL));
        assert_eq!(status(Some(&ready), true), "Ready to capture");
        assert_eq!(setup_button_label(Some(&ready)), "Manage capture");
        assert!(setup_complete(Some(&ready), true));
        assert!(!setup_complete(Some(&needs), true));
        assert_eq!(
            shortcut_status(Some(&ready)).as_deref(),
            Some("Shortcut saved.")
        );
    }

    #[test]
    fn unavailable_platforms_report_it() {
        let state = State::compute(&enabled(), None);
        assert_eq!(
            status(Some(&state), true),
            "Not supported on this platform."
        );
        assert!(!access_ready(&state));
        assert_eq!(status(None, false), "Checking snapshots\u{2026}");
        assert_eq!(shortcut_status(None), None);
    }

    #[test]
    fn setup_steps_resume_where_the_web_does() {
        let needs = State::compute(&enabled(), Some(MacPermissions::default()));
        assert_eq!(
            initial_step(&needs, Requested::Step(Step::Shortcut)),
            Step::Access
        );
        let off = State::compute(&SnapShotSettings::default(), Some(ALL));
        assert_eq!(initial_step(&off, Requested::Resume), Step::Access);
        let ready = State::compute(&enabled(), Some(ALL));
        assert_eq!(initial_step(&ready, Requested::Resume), Step::Shortcut);
        assert_eq!(
            initial_step(&ready, Requested::Step(Step::Access)),
            Step::Access
        );
        assert!(shortcut_ready(&ready, false));
        assert!(!shortcut_ready(&ready, true));
        assert!(mac_permissions_ready(&ready, true));
        assert!(!mac_permissions_ready(&needs, false));
    }

    #[test]
    fn closing_disables_only_an_unfinished_first_setup() {
        assert!(should_disable_on_close(false, false));
        assert!(!should_disable_on_close(false, true));
        assert!(!should_disable_on_close(true, false));
    }

    #[test]
    fn key_labels_use_the_platform_symbols() {
        assert_eq!(
            key_labels(&Shortcut::BothShift, true),
            ["\u{21e7}", "\u{21e7}"]
        );
        assert_eq!(
            key_labels(&Shortcut::Pair(Modifier::Meta), true),
            ["\u{2318}", "\u{2318}"]
        );
        assert_eq!(
            key_labels(&Shortcut::Pair(Modifier::Meta), false),
            ["Super", "Super"]
        );
        assert_eq!(
            key_labels(&chord("k", true, true), true),
            ["\u{21e7}", "\u{2318}", "K"]
        );
        assert_eq!(key_labels(&chord("k", true, false), false), ["Ctrl", "K"]);
        assert_eq!(
            key_labels(&chord(" ", true, false), true),
            ["\u{2318}", "Space"]
        );
        assert_eq!(
            key_labels(&chord("arrowup", true, false), true),
            ["\u{2318}", "Up"]
        );
    }

    #[test]
    fn same_shortcut_ignores_how_the_modifier_is_spelled() {
        let by_mod = chord("k", true, false);
        let by_meta = Shortcut::Chord(Chord {
            key: "k".to_owned(),
            meta: true,
            ctrl: false,
            shift: false,
            alt: false,
            modifier: false,
        });
        assert!(same_shortcut(&by_mod, &by_meta, true));
        assert!(!same_shortcut(&by_mod, &by_meta, false));
        assert!(same_shortcut(
            &Shortcut::BothShift,
            &Shortcut::Pair(Modifier::Shift),
            true
        ));
        assert!(!same_shortcut(&Shortcut::BothShift, &by_mod, true));
        assert!(!same_shortcut(
            &Shortcut::Pair(Modifier::Alt),
            &Shortcut::BothShift,
            true
        ));
    }

    #[test]
    fn conflicts_come_from_the_keybindings_and_skip_pairs() {
        let rule = Rule {
            command: "commandPalette.toggle".to_owned(),
            shortcut: Chord {
                key: "k".to_owned(),
                meta: false,
                ctrl: false,
                shift: false,
                alt: false,
                modifier: true,
            },
            when: None,
        };
        let rules = [rule];
        assert_eq!(
            keybinding_conflict(&chord("k", true, false), &rules, true),
            Some("commandPalette.toggle")
        );
        assert_eq!(
            keybinding_conflict(&chord("j", true, false), &rules, true),
            None
        );
        assert_eq!(
            keybinding_conflict(&Shortcut::BothShift, &rules, true),
            None
        );
    }

    #[test]
    fn search_entries_are_the_tabs_six_in_order() {
        assert!(
            SNAPSHOT_ITEMS
                .iter()
                .all(|i| i.section == Section::SnapShot)
        );
        let titles: Vec<_> = SNAPSHOT_ITEMS.iter().map(|i| i.title).collect();
        assert_eq!(
            titles,
            [
                "SnapShots",
                "Include app text",
                "Capture shortcut",
                "Capture sound",
                "Capture flash",
                "Capture animations"
            ]
        );
    }
}
