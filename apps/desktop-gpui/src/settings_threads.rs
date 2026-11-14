//! The Threads tab (`ThreadsSettingsPanel`, SettingsPanels.tsx:1496-1830; `SnoozePresetsSettings.tsx`;
//! `chat/runtimeModeConfig.ts`): the text, defaults and option lists of its rows. The keys live in
//! `settings_source_control::Field`, which already resolves a project override over the
//! environment's value. The panel only draws this.

use crate::settings::{SearchItem, Section};
use serde_json::{Value, json};

/// Call tokens of the Threads tab's requests start here.
pub const TOKEN_BASE: u64 = 1 << 55;

/// `defaultRuntimeMode`'s default (contracts/src/settings.ts).
pub const DEFAULT_RUNTIME_MODE: &str = "full-access";
/// `sidebarAutoSettleAfterDays`'s default, and what the switch writes when turned on.
pub const AUTO_SETTLE_DEFAULT_DAYS: u32 = 3;
pub const AUTO_SETTLE_MIN_DAYS: u32 = 1;
pub const AUTO_SETTLE_MAX_DAYS: u32 = 90;
/// `MAX_SNOOZE_PRESETS`.
pub const MAX_PRESETS: usize = 8;

const fn item(id: &'static str, title: &'static str, terms: &'static str) -> SearchItem {
    SearchItem {
        id,
        title,
        section: Section::Threads,
        terms,
        desktop_only: false,
    }
}

/// The tab's `searchableSetting` entries (settingsSearch.ts).
pub const THREADS_ITEMS: &[SearchItem] = &[
    item(
        "add-project-starts-in",
        "Add project starts in",
        "base directory folder browser path home",
    ),
    item(
        "default-model",
        "Default model",
        "new thread project provider reasoning effort",
    ),
    item(
        "fallback-model",
        "Fallback model",
        "fallback agent provider out of usage new threads",
    ),
    item(
        "default-permissions",
        "Permissions",
        "new thread default runtime mode supervised approvals auto accept edits full access",
    ),
    item(
        "new-threads",
        "New threads",
        "default workspace mode draft local worktree",
    ),
    item(
        "snooze-options",
        "Snooze options",
        "snooze presets times later tonight tomorrow next week remind",
    ),
    item(
        "snooze-limited-threads",
        "Snooze until limits reset",
        "usage quota rate limit reset wake recover continue",
    ),
    item(
        "auto-resume-limited-threads",
        "Resume after usage limits",
        "usage quota rate limit reset recover continue",
    ),
    item(
        "auto-settle-inactive-threads",
        "Auto-settle inactive threads",
        "sidebar inactivity days no activity automatically",
    ),
    item(
        "auto-settle-merged-threads",
        "Auto-settle merged threads",
        "pull request merge closed automatically sidebar",
    ),
    item(
        "keep-threads-with-pull-requests",
        "Keep threads with a PR",
        "pull request inactivity auto-settle merge close sidebar",
    ),
    item(
        "days-before-auto-settle",
        "Days of inactivity before auto-settle",
        "thread timeout activity sidebar",
    ),
    item(
        "text-generation-model",
        "Text generation model",
        "generated thread titles source control content default provider",
    ),
    item(
        "thread-title-instructions",
        "Thread title instructions",
        "generated thread titles prompt custom style naming rename regenerate",
    ),
    item(
        "response-streaming",
        "Response streaming",
        "output token paragraph buffered wait turn legacy",
    ),
];

/// One `runtimeModeConfig` entry: wire value and label (the web's select items also carry a description, which the menu here has no row for).
pub struct RuntimeMode {
    pub value: &'static str,
    pub label: &'static str,
}

/// `runtimeModeConfig`, in the order the select lists them.
pub const RUNTIME_MODES: [RuntimeMode; 4] = [
    RuntimeMode {
        value: "approval-required",
        label: "Supervised",
    },
    RuntimeMode {
        value: "auto-accept-edits",
        label: "Auto-accept edits",
    },
    RuntimeMode {
        value: "auto",
        label: "Auto",
    },
    RuntimeMode {
        value: DEFAULT_RUNTIME_MODE,
        label: "Full access",
    },
];

pub fn runtime_mode(value: &str) -> Option<&'static RuntimeMode> {
    RUNTIME_MODES.iter().find(|mode| mode.value == value)
}

/// The select's row of a runtime mode (the default for a value the list does not hold).
pub fn runtime_mode_index(value: &str) -> usize {
    RUNTIME_MODES
        .iter()
        .position(|mode| mode.value == value)
        .unwrap_or(RUNTIME_MODES.len() - 1)
}

/// `new-threads`: `defaultThreadEnvMode` values and labels, in the select's order.
pub const WORKSPACES: [&str; 2] = ["local", "worktree"];

pub fn workspace_index(value: &str) -> usize {
    WORKSPACES
        .iter()
        .position(|mode| *mode == value)
        .unwrap_or(0)
}

/// `RESPONSE_STREAMING_MODE_LABELS` and `_DESCRIPTIONS` (SettingsPanels.tsx:113-121).
pub const STREAMING_MODES: [(&str, &str, &str); 2] = [
    (
        "turn",
        "Wait for the full response",
        "Text appears once the agent finishes its turn.",
    ),
    (
        "paragraph",
        "Show finished paragraphs",
        "Each paragraph or code block appears as soon as it is complete.",
    ),
];

pub fn streaming_index(value: &str) -> usize {
    STREAMING_MODES
        .iter()
        .position(|(wire, _, _)| *wire == value)
        .unwrap_or(1)
}

pub fn streaming_label(value: &str) -> &'static str {
    STREAMING_MODES[streaming_index(value)].1
}

/// `DEFAULT_SNOOZE_PRESETS`.
pub fn default_presets() -> Value {
    json!([
        {"kind": "after", "minutes": 60},
        {"kind": "after", "minutes": 180},
        {"kind": "at", "day": "today", "time": "18:00"},
        {"kind": "at", "day": "tomorrow", "time": "09:00"},
        {"kind": "at", "day": "monday", "time": "09:00"},
    ])
}

/// `textGenerationModelSelection`'s default.
pub fn default_text_model() -> Value {
    json!({
        "instanceId": "codex",
        "model": "gpt-6-luna",
        "options": [{"id": "reasoningEffort", "value": "low"}],
    })
}

/// `snoozePresetId`.
pub fn preset_id(preset: &Value) -> String {
    if preset.get("kind").and_then(Value::as_str) == Some("after") {
        format!("after:{}", preset["minutes"].as_u64().unwrap_or(0))
    } else {
        format!(
            "at:{}:{}",
            preset["day"].as_str().unwrap_or_default(),
            preset["time"].as_str().unwrap_or_default()
        )
    }
}

fn plural(count: u64, unit: &str) -> String {
    format!("{count} {unit}{}", if count == 1 { "" } else { "s" })
}

/// `formatSnoozeDuration`: the largest unit that divides evenly.
fn duration(minutes: u64) -> String {
    if minutes > 0 && minutes.is_multiple_of(24 * 60) {
        plural(minutes / (24 * 60), "day")
    } else if minutes > 0 && minutes.is_multiple_of(60) {
        plural(minutes / 60, "hour")
    } else {
        plural(minutes, "minute")
    }
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// What a row of the options list reads, for example "In 3 hours" or "Monday at 09:00".
pub fn preset_label(preset: &Value) -> String {
    if preset.get("kind").and_then(Value::as_str) == Some("after") {
        format!("In {}", duration(preset["minutes"].as_u64().unwrap_or(0)))
    } else {
        format!(
            "{} at {}",
            capitalize(preset["day"].as_str().unwrap_or_default()),
            preset["time"].as_str().unwrap_or_default()
        )
    }
}

/// `sameSnoozePresets`: row order is display order, so only the multiset of ids counts.
pub fn same_presets(left: &[Value], right: &[Value]) -> bool {
    let ids = |presets: &[Value]| {
        let mut ids: Vec<String> = presets.iter().map(preset_id).collect();
        ids.sort();
        ids
    };
    ids(left) == ids(right)
}

/// `nextCandidate`: the defaults, then 30 minutes, Saturday 09:00 and a day, skipping any in use.
pub fn next_candidate(presets: &[Value]) -> Value {
    let used: Vec<String> = presets.iter().map(preset_id).collect();
    let mut candidates = default_presets().as_array().cloned().unwrap_or_default();
    candidates.extend([
        json!({"kind": "after", "minutes": 30}),
        json!({"kind": "at", "day": "saturday", "time": "09:00"}),
        json!({"kind": "after", "minutes": 24 * 60}),
    ]);
    candidates
        .into_iter()
        .find(|candidate| !used.contains(&preset_id(candidate)))
        .unwrap_or_else(|| json!({"kind": "after", "minutes": 60}))
}

/// The amount of `sidebarAutoSettleAfterDays` as the number field commits it: a whole number
/// from 1 to 90, else nothing is written.
pub fn parse_settle_days(text: &str) -> Option<u32> {
    let days: u32 = text.trim().parse().ok()?;
    (AUTO_SETTLE_MIN_DAYS..=AUTO_SETTLE_MAX_DAYS)
        .contains(&days)
        .then_some(days)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_follow_the_largest_even_unit() {
        let labels: Vec<String> = default_presets()
            .as_array()
            .unwrap()
            .iter()
            .map(preset_label)
            .collect();
        assert_eq!(
            labels,
            [
                "In 1 hour",
                "In 3 hours",
                "Today at 18:00",
                "Tomorrow at 09:00",
                "Monday at 09:00"
            ]
        );
        assert_eq!(
            preset_label(&json!({"kind": "after", "minutes": 90})),
            "In 90 minutes"
        );
        assert_eq!(
            preset_label(&json!({"kind": "after", "minutes": 2880})),
            "In 2 days"
        );
    }

    #[test]
    fn order_does_not_make_a_list_differ_from_the_defaults() {
        let defaults = default_presets().as_array().unwrap().clone();
        let mut shuffled = defaults.clone();
        shuffled.reverse();
        assert!(same_presets(&defaults, &shuffled));
        shuffled.pop();
        assert!(!same_presets(&defaults, &shuffled));
    }

    #[test]
    fn added_options_skip_those_in_use() {
        let mut presets = default_presets().as_array().unwrap().clone();
        assert_eq!(preset_id(&next_candidate(&presets)), "after:30");
        presets.push(next_candidate(&presets));
        assert_eq!(preset_id(&next_candidate(&presets)), "at:saturday:09:00");
        presets.push(next_candidate(&presets));
        presets.push(next_candidate(&presets));
        assert_eq!(preset_id(&next_candidate(&presets)), "after:60");
        assert_eq!(next_candidate(&[]), json!({"kind": "after", "minutes": 60}));
    }

    #[test]
    fn settle_days_commit_only_whole_numbers_in_range() {
        assert_eq!(parse_settle_days(" 7 "), Some(7));
        assert_eq!(parse_settle_days("90"), Some(90));
        assert_eq!(parse_settle_days("0"), None);
        assert_eq!(parse_settle_days("91"), None);
        assert_eq!(parse_settle_days("2.5"), None);
        assert_eq!(parse_settle_days(""), None);
    }

    #[test]
    fn unknown_values_read_as_the_defaults() {
        assert_eq!(runtime_mode_index("nope"), 3);
        assert_eq!(runtime_mode("auto").unwrap().label, "Auto");
        assert_eq!(streaming_label("paragraph"), "Show finished paragraphs");
        assert_eq!(streaming_label("x"), "Show finished paragraphs");
        assert_eq!(workspace_index("worktree"), 1);
    }
}
