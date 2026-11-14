//! The Worktrees tab (`WorktreesSettings.tsx`, `BranchNamingSettings.tsx`, the worktree rows of
//! `StorageSettings.tsx` and `ProjectDefaultsSettings.tsx`): the text, defaults and option lists
//! of its rows. The keys live in `settings_source_control::Field`, which already resolves a
//! project override over the environment's value. The panel only draws this.

use crate::settings::{SearchItem, Section};
use serde_json::{Map, Value, json};

/// Call tokens of the Worktrees tab's requests start here.
pub const TOKEN_BASE: u64 = 1 << 56;

/// What the retention switch writes when turned on.
pub const RETENTION_DEFAULT_DAYS: u32 = 8;
pub const RETENTION_MAX_DAYS: u32 = 3650;

const fn item(id: &'static str, title: &'static str, terms: &'static str) -> SearchItem {
    SearchItem {
        id,
        title,
        section: Section::Worktrees,
        terms,
        desktop_only: false,
    }
}

/// The tab's `searchableSetting` entries (settingsSearch.ts).
pub const WORKTREES_ITEMS: &[SearchItem] = &[
    item(
        "storage-worktrees-location",
        "Worktree location",
        "worktree location folder directory path drive external disk",
    ),
    item(
        "storage-worktrees",
        "Worktree cleanup",
        "disk storage automatic delete deleted archived threads old inactive merged settled unchanged worktrees retention days project inherit off custom",
    ),
    item(
        "storage-worktree-keep-when",
        "Keep worktrees with local changes",
        "any local files uncommitted changes edited tracked files ignored env build output cleanup",
    ),
    item(
        "worktree-submodules",
        "Submodules",
        "git submodule init recursive top-level none worktree t3.json",
    ),
    item(
        "start-from-origin",
        "Start from remote",
        "new worktrees start remote origin latest fetched branch local",
    ),
    item(
        "worktree-branch-naming",
        "Worktree branch naming",
        "static semantic prefix custom prompt instructions feat fix refactor chore",
    ),
];

/// `WORKTREE_SUBMODULES_OPTIONS` and `WORKTREE_SUBMODULES_LABELS`.
pub const SUBMODULES: [(&str, &str); 3] = [
    ("recursive", "Recursive"),
    ("top-level", "Top level only"),
    ("none", "Skip"),
];

/// `BranchNamingMode`'s options with the labels of `BranchNamingSettings`.
pub const BRANCH_MODES: [(&str, &str); 3] = [
    ("static", "Static prefix"),
    ("semantic", "Semantic prefix"),
    ("custom", "Custom instructions"),
];

/// `KEEP_WHEN_LABELS`.
pub const KEEP_WHEN: [(&str, &str); 3] = [
    ("any-local-files", "Any local files"),
    ("uncommitted-changes", "Uncommitted changes"),
    ("tracked-changes", "Edited tracked files"),
];

/// The project's cleanup policy options: `Inherit` clears the key.
pub const CLEANUP_MODES: [(&str, &str); 3] =
    [("inherit", "Inherit"), ("off", "Off"), ("custom", "Custom")];

pub const SEMANTIC_STATUS: &str = "The model chooses a prefix that describes the work, such as feat/add-search, fix/login-timeout, or refactor/auth.";

/// `StorageCleanupSettings` defaults (contracts/src/settings.ts).
pub fn default_storage_cleanup() -> Value {
    json!({
        "worktreeKeepWhen": "uncommitted-changes",
        "worktreeAfterDays": null,
        "worktreeSettledAfterDays": null,
        "worktreeOnMerge": false,
        "worktreeOnDelete": false,
        "worktreeUnchanged": false,
        "browserArtifactsAfterDays": null,
        "logsAfterDays": null,
    })
}

fn index_of(options: &[(&str, &str)], wire: &str) -> usize {
    options
        .iter()
        .position(|(key, _)| *key == wire)
        .unwrap_or(0)
}

pub fn submodules_index(value: &Value) -> usize {
    index_of(&SUBMODULES, value.as_str().unwrap_or_default())
}

/// The layer label of a value; unset inherits.
pub fn submodules_label(value: &Value) -> &'static str {
    if value.is_null() {
        return "Inherit";
    }
    SUBMODULES[submodules_index(value)].1
}

pub fn branch_mode_index(value: &Value) -> usize {
    index_of(&BRANCH_MODES, value.as_str().unwrap_or("static"))
}

pub fn branch_mode_label(value: &Value) -> &'static str {
    BRANCH_MODES[branch_mode_index(value)].1
}

pub fn keep_when_index(value: &str) -> usize {
    index_of(
        &KEEP_WHEN,
        if value.is_empty() {
            "uncommitted-changes"
        } else {
            value
        },
    )
}

/// The description under "Keep worktrees with local changes".
pub fn keep_when_description(value: &str) -> &'static str {
    match value {
        "any-local-files" => "Includes ignored files, except node_modules.",
        "tracked-changes" => "Deletes untracked and ignored files, including .env.",
        _ => "Keeps edits and untracked files. Deletes ignored files, including .env.",
    }
}

/// The mode of a `worktreeCleanup` policy: no policy inherits the machine's rules.
pub fn cleanup_mode(policy: &Value) -> &'static str {
    match policy.get("mode").and_then(Value::as_str) {
        Some("off") => "off",
        Some("custom") => "custom",
        _ => "inherit",
    }
}

pub fn cleanup_mode_index(policy: &Value) -> usize {
    index_of(&CLEANUP_MODES, cleanup_mode(policy))
}

pub fn cleanup_mode_label(policy: &Value) -> &'static str {
    CLEANUP_MODES[cleanup_mode_index(policy)].1
}

/// The description of the project's "Automatic worktree cleanup" row.
pub fn cleanup_mode_description(mode: &str) -> &'static str {
    match mode {
        "off" => "Keep this project's worktrees until you delete them manually.",
        "custom" => "Use these rules for this project.",
        _ => "Use each machine's worktree cleanup settings.",
    }
}

/// The description under a project's or machine's submodules row.
pub fn submodules_description(project: bool) -> &'static str {
    if project {
        "How new worktrees in this project populate git submodules."
    } else {
        "How new worktrees populate git submodules. Projects and their t3.json can override it."
    }
}

const RULE_KEYS: [&str; 6] = [
    "worktreeKeepWhen",
    "worktreeAfterDays",
    "worktreeSettledAfterDays",
    "worktreeOnMerge",
    "worktreeOnDelete",
    "worktreeUnchanged",
];

/// The machine's rules over the defaults, so a server that sends fewer keys still reads whole.
pub fn machine_rules(storage: &Value) -> Map<String, Value> {
    let defaults = default_storage_cleanup();
    let mut rules = Map::new();
    for key in RULE_KEYS {
        let value = storage.get(key).or_else(|| defaults.get(key));
        rules.insert(key.to_owned(), value.cloned().unwrap_or(Value::Null));
    }
    rules
}

/// `resolveWorktreeCleanup` (shared/projectSettings.ts) for the rules a scope shows. A custom
/// policy keeps its own rules; a key it leaves out reads as the machine's.
pub fn resolve_rules(storage: &Value, policy: &Value) -> Map<String, Value> {
    let machine = machine_rules(storage);
    match cleanup_mode(policy) {
        "custom" => {
            let own = policy.get("rules");
            machine
                .into_iter()
                .map(|(key, value)| {
                    let value = own
                        .and_then(|rules| rules.get(&key))
                        .cloned()
                        .unwrap_or(value);
                    (key, value)
                })
                .collect()
        }
        "off" => {
            let mut rules = Map::new();
            rules.insert(
                "worktreeKeepWhen".to_owned(),
                machine["worktreeKeepWhen"].clone(),
            );
            rules.insert("worktreeAfterDays".to_owned(), Value::Null);
            rules.insert("worktreeSettledAfterDays".to_owned(), Value::Null);
            rules.insert("worktreeOnMerge".to_owned(), json!(false));
            rules.insert("worktreeOnDelete".to_owned(), json!(false));
            rules.insert("worktreeUnchanged".to_owned(), json!(false));
            rules
        }
        _ => machine,
    }
}

/// What editing a retention field writes: a whole number of days within the field's bounds, or
/// `None` when the text is not a number, so the field shows the setting again.
pub fn clamp_days(text: &str, minimum: u32) -> Option<u32> {
    let days = text.trim().parse::<f64>().ok()?;
    if !days.is_finite() {
        return None;
    }
    Some(
        days.round()
            .clamp(f64::from(minimum), f64::from(RETENTION_MAX_DAYS)) as u32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_contract() {
        let defaults = default_storage_cleanup();
        assert_eq!(defaults["worktreeKeepWhen"], "uncommitted-changes");
        assert_eq!(defaults["worktreeAfterDays"], Value::Null);
        assert_eq!(defaults["worktreeOnMerge"], false);
    }

    #[test]
    fn policy_modes() {
        assert_eq!(cleanup_mode(&Value::Null), "inherit");
        assert_eq!(cleanup_mode(&json!({"mode": "off"})), "off");
        assert_eq!(
            cleanup_mode(&json!({"mode": "custom", "rules": {}})),
            "custom"
        );
        assert_eq!(cleanup_mode_label(&json!({"mode": "off"})), "Off");
    }

    #[test]
    fn unset_options_read_as_the_first_or_default() {
        assert_eq!(submodules_label(&Value::Null), "Inherit");
        assert_eq!(submodules_label(&json!("none")), "Skip");
        assert_eq!(branch_mode_label(&Value::Null), "Static prefix");
        assert_eq!(keep_when_index(""), 1);
        assert_eq!(keep_when_index("tracked-changes"), 2);
    }

    #[test]
    fn rules_resolve_like_the_web() {
        let storage = json!({
            "worktreeKeepWhen": "tracked-changes",
            "worktreeAfterDays": 30,
            "worktreeOnMerge": true,
        });
        assert_eq!(
            resolve_rules(&storage, &Value::Null)["worktreeAfterDays"],
            30
        );

        let off = resolve_rules(&storage, &json!({"mode": "off"}));
        assert_eq!(off["worktreeKeepWhen"], "tracked-changes");
        assert_eq!(off["worktreeAfterDays"], Value::Null);
        assert_eq!(off["worktreeOnMerge"], false);

        let custom = resolve_rules(
            &storage,
            &json!({"mode": "custom", "rules": {"worktreeOnDelete": true}}),
        );
        assert_eq!(custom["worktreeOnDelete"], true);
        assert_eq!(custom["worktreeOnMerge"], true);
        assert_eq!(custom["worktreeKeepWhen"], "tracked-changes");
    }

    #[test]
    fn retention_days_clamp_to_their_bounds() {
        assert_eq!(clamp_days("12", 1), Some(12));
        assert_eq!(clamp_days("0", 1), Some(1));
        assert_eq!(clamp_days("0", 0), Some(0));
        assert_eq!(clamp_days("99999", 1), Some(3650));
        assert_eq!(clamp_days("2.6", 1), Some(3));
        assert_eq!(clamp_days("", 1), None);
        assert_eq!(clamp_days("soon", 1), None);
    }
}
