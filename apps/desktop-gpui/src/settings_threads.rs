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
    pub description: &'static str,
}

/// `runtimeModeConfig`, in the order the select lists them.
pub const RUNTIME_MODES: [RuntimeMode; 4] = [
    RuntimeMode {
        value: "approval-required",
        label: "Supervised",
        description: "Ask before commands and file changes.",
    },
    RuntimeMode {
        value: "auto-accept-edits",
        label: "Auto-accept edits",
        description: "Auto-approve edits, ask before other actions.",
    },
    RuntimeMode {
        value: "auto",
        label: "Auto",
        description: "Supported providers approve routine actions; others still ask.",
    },
    RuntimeMode {
        value: DEFAULT_RUNTIME_MODE,
        label: "Full access",
        description: "Allow commands and edits without prompts.",
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
#[cfg(test)]
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

/// `SnoozePreset`: one resolved menu option.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub id: String,
    pub label: String,
    pub when: String,
    pub wake: i64,
}

fn local_tm(secs: i64) -> libc::tm {
    let secs: libc::time_t = secs;
    // SAFETY: `tm` is plain data and `localtime_r` writes only through the pointers given.
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&secs, &mut tm);
        tm
    }
}

/// `12:01 PM`: the hour without a leading zero, as the web's default time formatter.
pub fn clock(secs: i64) -> String {
    let tm = local_tm(secs);
    let hour = if tm.tm_hour % 12 == 0 {
        12
    } else {
        tm.tm_hour % 12
    };
    format!(
        "{hour}:{:02} {}",
        tm.tm_min,
        if tm.tm_hour < 12 { "AM" } else { "PM" }
    )
}

/// A time input's text for `HH:MM`: `06:00 PM`.
pub fn time_text(time: &str) -> String {
    let (hour, minute) = parse_clock(time).unwrap_or((0, 0));
    let shown = if hour % 12 == 0 { 12 } else { hour % 12 };
    format!(
        "{shown:02}:{minute:02} {}",
        if hour < 12 { "AM" } else { "PM" }
    )
}

fn parse_clock(text: &str) -> Option<(u32, u32)> {
    let (hour, minute) = (text.get(..2)?.parse().ok()?, text.get(3..5)?.parse().ok()?);
    (text.as_bytes().get(2) == Some(&b':') && hour < 24 && minute < 60).then_some((hour, minute))
}

/// What a time input commits: `18:00`, `6:00 pm` and `06:00 PM` all read as `18:00`.
pub fn parse_time(text: &str) -> Option<String> {
    let text = text.trim().to_lowercase();
    let (body, meridiem) = match text.strip_suffix("pm") {
        Some(body) => (body.trim(), Some(true)),
        None => match text.strip_suffix("am") {
            Some(body) => (body.trim(), Some(false)),
            None => (text.as_str(), None),
        },
    };
    let (hour, minute) = body.split_once(':')?;
    let (hour, minute): (u32, u32) = (hour.trim().parse().ok()?, minute.trim().parse().ok()?);
    let hour = match meridiem {
        Some(pm) if (1..=12).contains(&hour) => hour % 12 + if pm { 12 } else { 0 },
        Some(_) => return None,
        None => hour,
    };
    (hour < 24 && minute < 60).then(|| format!("{hour:02}:{minute:02}"))
}

const WEEKDAYS: [&str; 7] = [
    "sunday",
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
];

/// `snoozePresetDaysAhead`.
fn days_ahead(day: &str, weekday: i32) -> i32 {
    match day {
        "today" => 0,
        "tomorrow" => 1,
        _ => {
            let target = WEEKDAYS.iter().position(|d| *d == day).unwrap_or(0) as i32;
            match (target - weekday).rem_euclid(7) {
                0 => 7,
                ahead => ahead,
            }
        }
    }
}

/// `snoozePresetLabel`: labels follow the actual day gap.
fn day_label(day: &str, time: &str, ahead: i32) -> String {
    let hour: u32 = time.get(..2).and_then(|h| h.parse().ok()).unwrap_or(0);
    match ahead {
        0 if hour < 12 => "This morning".into(),
        0 if hour < 17 => "This afternoon".into(),
        0 => "This evening".into(),
        1 => "Tomorrow".into(),
        _ if day == "monday" => "Next week".into(),
        7 => format!("Next {}", capitalize(day)),
        _ => capitalize(day),
    }
}

/// `resolveSnoozePreset`.
pub fn resolve_preset(preset: &Value, now: i64) -> Option<Resolved> {
    let id = preset_id(preset);
    let (wake, label, names_day) = if preset.get("kind").and_then(Value::as_str) == Some("after") {
        let minutes = preset["minutes"].as_u64().unwrap_or(0);
        let label = format!("In {}", duration(minutes));
        (now + minutes as i64 * 60, label, false)
    } else {
        let (day, time) = (
            preset["day"].as_str().unwrap_or_default(),
            preset["time"].as_str().unwrap_or_default(),
        );
        let (hour, minute) = parse_clock(time)?;
        let mut tm = local_tm(now);
        let ahead = days_ahead(day, tm.tm_wday);
        tm.tm_mday += ahead;
        (tm.tm_hour, tm.tm_min, tm.tm_sec) = (hour as i32, minute as i32, 0);
        tm.tm_isdst = -1;
        // SAFETY: `tm` is a valid, initialised `tm`; `mktime` normalises it in place.
        let wake = unsafe { libc::mktime(&mut tm) } as i64;
        // A today option whose time has passed is left out until tomorrow.
        if wake <= now {
            return None;
        }
        let label = day_label(day, time, ahead);
        let names = label != "Next week";
        (wake, label, names)
    };
    let (then, today) = (local_tm(wake), local_tm(now));
    let same_day = (then.tm_year, then.tm_yday) == (today.tm_year, today.tm_yday);
    let time = clock(wake);
    let when = if names_day || same_day {
        time
    } else {
        let name = capitalize(WEEKDAYS[then.tm_wday as usize]);
        format!("{} {time}", &name[..3])
    };
    Some(Resolved {
        id,
        label,
        when,
        wake,
    })
}

/// `resolveSnoozePresets`: soonest first, one row per wake minute.
pub fn menu_now(presets: &[Value], now: i64) -> Vec<Resolved> {
    let mut resolved: Vec<Resolved> = presets
        .iter()
        .filter_map(|preset| resolve_preset(preset, now))
        .collect();
    resolved.sort_by_key(|preset| preset.wake.div_euclid(60));
    let mut seen = Vec::new();
    resolved.retain(|preset| {
        let minute = preset.wake.div_euclid(60);
        let fresh = !seen.contains(&minute);
        seen.push(minute);
        fresh
    });
    resolved
}

/// The "Menu now" line's entries: a repeated label names its time too.
pub fn menu_order(menu: &[Resolved]) -> Vec<String> {
    menu.iter()
        .map(|preset| {
            if menu
                .iter()
                .filter(|other| other.label == preset.label)
                .count()
                > 1
            {
                format!("{} {}", preset.label, preset.when)
            } else {
                preset.label.clone()
            }
        })
        .collect()
}

/// What sits beside a row: the menu entry it makes, or why it makes none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Preview {
    Menu { label: String, when: String },
    Note(&'static str),
}

pub fn previews(presets: &[Value], now: i64) -> Vec<Preview> {
    let (mut ids, mut wakes) = (Vec::new(), Vec::new());
    presets
        .iter()
        .map(|preset| {
            let Some(resolved) = resolve_preset(preset, now) else {
                return Preview::Note("Passed for today");
            };
            if ids.contains(&resolved.id) {
                return Preview::Note("Duplicate of an option above");
            }
            if wakes.contains(&resolved.wake) {
                return Preview::Note("Same time as an option above");
            }
            ids.push(resolved.id);
            wakes.push(resolved.wake);
            Preview::Menu {
                label: resolved.label,
                when: resolved.when,
            }
        })
        .collect()
}

/// `splitMinutes`: the largest unit that divides evenly, as an amount and a unit index
/// (0 minutes, 1 hours, 2 days).
pub fn split_minutes(minutes: u64) -> (u64, usize) {
    if minutes > 0 && minutes.is_multiple_of(24 * 60) {
        (minutes / (24 * 60), 2)
    } else if minutes > 0 && minutes.is_multiple_of(60) {
        (minutes / 60, 1)
    } else {
        (minutes, 0)
    }
}

pub const UNIT_MINUTES: [u64; 3] = [1, 60, 24 * 60];

/// `WHEN_VALUES`: "after", then the `SnoozePresetDay` literals.
pub const WHEN_VALUES: [&str; 10] = [
    "after",
    "today",
    "tomorrow",
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
];

pub const WHEN_LABELS: [&str; 10] = [
    "In",
    "Today",
    "Tomorrow",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

/// `withWhen`: a duration keeps its minutes, a set time its time; switching kind takes the
/// remembered value of the other.
pub fn with_when(preset: &Value, when: &str, minutes: u64, time: &str) -> Value {
    let after = preset.get("kind").and_then(Value::as_str) == Some("after");
    if when == "after" {
        return if after {
            preset.clone()
        } else {
            json!({"kind": "after", "minutes": minutes})
        };
    }
    let time = if after {
        time.to_owned()
    } else {
        preset["time"].as_str().unwrap_or(time).to_owned()
    };
    json!({"kind": "at", "day": when, "time": time})
}

/// `MAX_SNOOZE_PRESET_MINUTES`.
pub const MAX_PRESET_MINUTES: u64 = 30 * 24 * 60;

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

    #[test]
    fn day_gaps_name_the_option() {
        assert_eq!(days_ahead("monday", 0), 1);
        assert_eq!(days_ahead("monday", 1), 7);
        assert_eq!(days_ahead("friday", 2), 3);
        assert_eq!(days_ahead("today", 4), 0);
        assert_eq!(day_label("monday", "09:00", 1), "Tomorrow");
        assert_eq!(day_label("monday", "09:00", 7), "Next week");
        assert_eq!(day_label("friday", "09:00", 7), "Next Friday");
        assert_eq!(day_label("friday", "09:00", 3), "Friday");
        assert_eq!(day_label("today", "18:00", 0), "This evening");
        assert_eq!(day_label("today", "08:00", 0), "This morning");
        assert_eq!(day_label("today", "13:00", 0), "This afternoon");
    }

    #[test]
    fn durations_resolve_from_now_and_dedupe_by_minute() {
        let now = 1_700_000_000;
        let hour = resolve_preset(&json!({"kind": "after", "minutes": 60}), now).unwrap();
        assert_eq!((hour.label.as_str(), hour.wake), ("In 1 hour", now + 3600));
        let twin = [
            json!({"kind": "after", "minutes": 60}),
            json!({"kind": "after", "minutes": 60}),
        ];
        assert_eq!(menu_now(&twin, now).len(), 1);
        assert_eq!(
            previews(&twin, now)[1],
            Preview::Note("Duplicate of an option above")
        );
    }

    #[test]
    fn time_inputs_read_both_clocks() {
        assert_eq!(time_text("18:00"), "06:00 PM");
        assert_eq!(time_text("00:30"), "12:30 AM");
        assert_eq!(parse_time("06:00 PM").as_deref(), Some("18:00"));
        assert_eq!(parse_time("6:05 am").as_deref(), Some("06:05"));
        assert_eq!(parse_time("12:00 AM").as_deref(), Some("00:00"));
        assert_eq!(parse_time("17:45").as_deref(), Some("17:45"));
        assert_eq!(parse_time("25:00"), None);
        assert_eq!(parse_time("soon"), None);
    }

    #[test]
    fn switching_kind_keeps_the_other_kinds_value() {
        let after = json!({"kind": "after", "minutes": 180});
        assert_eq!(
            with_when(&after, "friday", 60, "07:30"),
            json!({"kind": "at", "day": "friday", "time": "07:30"})
        );
        let at = json!({"kind": "at", "day": "friday", "time": "07:30"});
        assert_eq!(
            with_when(&at, "after", 45, "09:00"),
            json!({"kind": "after", "minutes": 45})
        );
        assert_eq!(
            with_when(&at, "monday", 45, "09:00"),
            json!({"kind": "at", "day": "monday", "time": "07:30"})
        );
        assert_eq!(split_minutes(2880), (2, 2));
        assert_eq!(split_minutes(90), (90, 0));
    }
}
