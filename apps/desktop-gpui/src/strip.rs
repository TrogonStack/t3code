//! What the strip under the composer shows: the environment, workspace, branch and permissions
//! controls (`BranchToolbar.logic.ts`, `BranchToolbarBranchSelector.tsx`, `runtimeModeConfig.ts`).
//! The branch comes from `vcs.listRefs`, decoded by hand like `server.getConfig`.

use serde_json::Value;

/// The shortcut half of a tooltip, the web's `shortcutLabelForCommand` for the default bindings on macOS.
pub const HOST_SHORTCUT: &str = "\u{21e7}\u{2318}H";
pub const WORKSPACE_SHORTCUT: &str = "\u{21e7}\u{2318}X";
pub const PERMISSIONS_SHORTCUT: &str = "\u{21e7}\u{2318}A";

/// The refs listed to find the checked-out branch.
pub const REFS_LIMIT: u32 = 100;

const GENERIC_LOCAL_LABELS: [&str; 2] = ["local", "local environment"];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EnvMode {
    #[default]
    Local,
    Worktree,
}

impl EnvMode {
    fn parse(text: &str) -> Option<Self> {
        match text {
            "local" => Some(Self::Local),
            "worktree" => Some(Self::Worktree),
            _ => None,
        }
    }

    /// `resolveEnvModeLabel`.
    pub fn label(self) -> &'static str {
        match self {
            Self::Local => "Current checkout",
            Self::Worktree => "New worktree",
        }
    }
}

/// `RuntimeMode` (packages/contracts/src/providerPolicy.ts).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeMode {
    ApprovalRequired,
    AutoAcceptEdits,
    Auto,
    FullAccess,
}

impl RuntimeMode {
    fn parse(text: &str) -> Option<Self> {
        match text {
            "approval-required" => Some(Self::ApprovalRequired),
            "auto-accept-edits" => Some(Self::AutoAcceptEdits),
            "auto" => Some(Self::Auto),
            "full-access" => Some(Self::FullAccess),
            _ => None,
        }
    }

    /// `runtimeModeOptions`, the order of the permissions toggle.
    pub const ALL: [Self; 4] = [
        Self::ApprovalRequired,
        Self::AutoAcceptEdits,
        Self::Auto,
        Self::FullAccess,
    ];

    /// `runtimeModeConfig`.
    pub fn description(self) -> &'static str {
        match self {
            Self::ApprovalRequired => "Ask before commands and file changes.",
            Self::AutoAcceptEdits => "Auto-approve edits, ask before other actions.",
            Self::Auto => "Supported providers approve routine actions; others still ask.",
            Self::FullAccess => "Allow commands and edits without prompts.",
        }
    }

    /// `runtimeModeConfig`.
    pub fn label(self) -> &'static str {
        match self {
            Self::ApprovalRequired => "Supervised",
            Self::AutoAcceptEdits => "Auto-accept edits",
            Self::Auto => "Auto",
            Self::FullAccess => "Full access",
        }
    }
}

/// The mode the permissions popover names, and whether it is the selected one, as hover previews.
pub fn previewed(selected: RuntimeMode, preview: Option<RuntimeMode>) -> (RuntimeMode, bool) {
    let shown = preview.unwrap_or(selected);
    (shown, shown == selected)
}

/// What a fresh draft starts with, from the settings of the environment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Defaults {
    pub env_mode: EnvMode,
    pub runtime_mode: RuntimeMode,
    pub start_from_origin: bool,
}

impl Defaults {
    /// Unset or unknown values take the schema defaults (settings.ts): a local checkout, full
    /// access, and new worktrees starting from origin. The project's `t3.json` is not read.
    pub fn resolve(
        env_mode: Option<&str>,
        runtime_mode: Option<&str>,
        start_from_origin: Option<bool>,
    ) -> Self {
        Self {
            env_mode: env_mode.and_then(EnvMode::parse).unwrap_or_default(),
            runtime_mode: runtime_mode
                .and_then(RuntimeMode::parse)
                .unwrap_or(RuntimeMode::FullAccess),
            start_from_origin: start_from_origin.unwrap_or(true),
        }
    }
}

/// The branch a project's checkout is on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Branch {
    pub name: Option<String>,
    /// Whether the listed ref is a remote one; `None` when it was not listed.
    pub is_remote: Option<bool>,
}

impl Branch {
    /// The `vcs.listRefs` result; `None` when no ref is listed, as in a folder that is not a repository.
    pub fn decode(result: &Value) -> Option<Self> {
        let refs = result["refs"].as_array().filter(|refs| !refs.is_empty())?;
        let current = refs.iter().find(|r| r["current"] == true);
        Some(Self {
            name: current.and_then(|r| r["name"].as_str()).map(str::to_owned),
            is_remote: current.map(|r| r["isRemote"] == true),
        })
    }

    /// `resolveBranchTriggerLabel` for a draft that has no worktree yet.
    pub fn label(&self, env_mode: EnvMode, start_from_origin: bool) -> String {
        let Some(name) = &self.name else {
            return "Select ref".to_owned();
        };
        if env_mode != EnvMode::Worktree {
            return name.clone();
        }
        // Only the server can determine a local branch's configured remote.
        if start_from_origin && self.is_remote == Some(false) {
            format!("From {name} (remote)")
        } else {
            format!("From {name}")
        }
    }
}

/// `resolveEnvironmentOptionLabel` for the primary environment: a generic name reads as "This device".
pub fn environment_label(label: &str) -> String {
    let label = label.trim();
    if label.is_empty() || GENERIC_LOCAL_LABELS.contains(&label.to_lowercase().as_str()) {
        "This device".to_owned()
    } else {
        label.to_owned()
    }
}

/// A tooltip as the web writes it: the label, then the shortcut after a middle dot.
pub fn tooltip_text(label: &str, shortcut: Option<&str>) -> String {
    match shortcut {
        Some(shortcut) => format!("{label} \u{b7} {shortcut}"),
        None => label.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn branch(name: Option<&str>, is_remote: Option<bool>) -> Branch {
        Branch {
            name: name.map(str::to_owned),
            is_remote,
        }
    }

    #[test]
    fn branch_label_names_the_base_of_a_new_worktree() {
        let main = branch(Some("main"), Some(false));
        assert_eq!(main.label(EnvMode::Worktree, true), "From main (remote)");
        assert_eq!(main.label(EnvMode::Worktree, false), "From main");
        assert_eq!(main.label(EnvMode::Local, true), "main");
        let remote = branch(Some("origin/main"), Some(true));
        assert_eq!(remote.label(EnvMode::Worktree, true), "From origin/main");
        let unlisted = branch(Some("main"), None);
        assert_eq!(unlisted.label(EnvMode::Worktree, true), "From main");
        assert_eq!(
            branch(None, None).label(EnvMode::Worktree, true),
            "Select ref"
        );
    }

    #[test]
    fn decode_reads_the_checked_out_ref() {
        let result = json!({"refs": [
            {"name": "dev", "current": false, "isDefault": false},
            {"name": "main", "current": true, "isDefault": true, "isRemote": false},
        ]});
        assert_eq!(
            Branch::decode(&result),
            Some(branch(Some("main"), Some(false)))
        );
        let detached = json!({"refs": [{"name": "dev", "current": false}]});
        assert_eq!(Branch::decode(&detached), Some(branch(None, None)));
        assert_eq!(Branch::decode(&json!({"refs": []})), None);
        assert_eq!(Branch::decode(&json!({})), None);
    }

    #[test]
    fn defaults_fall_back_to_the_schema_values() {
        let unset = Defaults::resolve(None, None, None);
        assert_eq!(unset.env_mode, EnvMode::Local);
        assert_eq!(unset.runtime_mode, RuntimeMode::FullAccess);
        assert!(unset.start_from_origin);
        let set = Defaults::resolve(Some("worktree"), Some("approval-required"), Some(false));
        assert_eq!(set.env_mode, EnvMode::Worktree);
        assert_eq!(set.runtime_mode, RuntimeMode::ApprovalRequired);
        assert!(!set.start_from_origin);
        let unknown = Defaults::resolve(Some("cloud"), Some("yolo"), None);
        assert_eq!(unknown.env_mode, EnvMode::Local);
        assert_eq!(unknown.runtime_mode, RuntimeMode::FullAccess);
    }

    #[test]
    fn previews_name_the_hovered_mode_until_the_pointer_leaves() {
        let selected = RuntimeMode::FullAccess;
        assert_eq!(previewed(selected, None), (selected, true));
        assert_eq!(previewed(selected, Some(selected)), (selected, true));
        assert_eq!(
            previewed(selected, Some(RuntimeMode::Auto)),
            (RuntimeMode::Auto, false)
        );
        assert_eq!(RuntimeMode::ALL[0], RuntimeMode::ApprovalRequired);
        assert_eq!(RuntimeMode::ALL[3], selected);
        assert_eq!(
            RuntimeMode::Auto.description(),
            "Supported providers approve routine actions; others still ask."
        );
    }

    #[test]
    fn labels_follow_the_web() {
        assert_eq!(EnvMode::Worktree.label(), "New worktree");
        assert_eq!(EnvMode::Local.label(), "Current checkout");
        assert_eq!(RuntimeMode::FullAccess.label(), "Full access");
        assert_eq!(RuntimeMode::ApprovalRequired.label(), "Supervised");
        assert_eq!(RuntimeMode::AutoAcceptEdits.label(), "Auto-accept edits");
        assert_eq!(RuntimeMode::Auto.label(), "Auto");
    }

    #[test]
    fn generic_environment_names_read_as_this_device() {
        assert_eq!(environment_label("Local"), "This device");
        assert_eq!(environment_label(" local environment "), "This device");
        assert_eq!(environment_label(""), "This device");
        assert_eq!(environment_label("Ada's Mac Studio"), "Ada's Mac Studio");
    }

    #[test]
    fn tooltips_put_the_shortcut_after_a_middle_dot() {
        assert_eq!(
            tooltip_text("New worktree", Some(WORKSPACE_SHORTCUT)),
            "New worktree \u{b7} \u{21e7}\u{2318}X"
        );
        assert_eq!(tooltip_text("From main", None), "From main");
    }
}
