//! The thread details card's logic: what it shows (`ThreadDetailsPanel.tsx`), how it fits
//! (`threadDetailsCardLayout.ts`), the editors it opens (`editor.ts`, `editorPreferences.ts`), and
//! the version control status behind the branch and changes rows (`GitActionsControl.logic.ts`).
//! Only the default item visibility applies: client settings are not on the wire.

use serde_json::Value;

/// The web's default binding for `threadPanel.toggle` and `editor.openFavorite` on macOS.
pub const TOGGLE_SHORTCUT: &str = "\u{21e7}\u{2318}B";
pub const OPEN_FAVORITE_SHORTCUT: &str = "\u{2318}O";

/// `THREAD_DETAILS_CARD_WIDTH` and `THREAD_DETAILS_CARD_GAP`.
pub const CARD_WIDTH: f32 = 280.;
/// A row inside the card: its width less the 1px border and `px-2` padding on each side, which
/// the web's panel menus match with `w-(--anchor-width)`.
pub const CARD_ROW_WIDTH: f32 = CARD_WIDTH - 2. - 16.;
pub const CARD_GAP: f32 = 12.;
/// `DETAILS_CARD_CLEARANCE`, and the chat lane's padding and minimum width (`canvasStore.ts`).
const CARD_CLEARANCE: f32 = 32.;
const LANE_PADDING: f32 = 48.;
const MIN_CHAT_WIDTH: f32 = 640.;
const MIN_CARD_HEIGHT: f32 = 160.;
/// A `h-8` row, and a section's `pt-2 pb-2.5` with its 1px divider when separated.
const ROW_HEIGHT: f32 = 32.;
const SECTION_PADDING: f32 = 18.;
/// A usage row is `h-6`.
const USAGE_ROW_HEIGHT: f32 = 24.;

/// What room there is for the card in the main area, or `None` when it does not fit beside the
/// chat lane (the web falls back to a popover there).
pub fn card_height(container_width: f32, container_height: f32) -> Option<f32> {
    let x = container_width - CARD_WIDTH - CARD_GAP;
    if x - CARD_CLEARANCE - LANE_PADDING < MIN_CHAT_WIDTH {
        return None;
    }
    let height = container_height - CARD_GAP * 2.;
    (height >= MIN_CARD_HEIGHT).then_some(height)
}

/// `MAX_CHAT_WIDTH`: the widest the chat lane grows (`canvasStore.ts`).
const MAX_CHAT_WIDTH: f32 = 768.;

/// The chat lane's left and right insets in the main area (`resolveChatCanvasLayout`). The lane
/// stays centered in the full width, and only slides left, as little as it must, to keep
/// `CARD_CLEARANCE` between it and the floating card.
pub fn lane_insets(container_width: f32, card_shown: bool) -> (f32, f32) {
    if !card_shown {
        return (0., 0.);
    }
    let centered = (container_width - LANE_PADDING * 2.).clamp(0., MAX_CHAT_WIDTH);
    let lane_right = container_width - CARD_WIDTH - CARD_GAP - CARD_CLEARANCE;
    let width = centered.min(lane_right - LANE_PADDING).max(0.);
    let center_left = (container_width - width) / 2.;
    let left = center_left.min(lane_right - width).max(LANE_PADDING);
    (left, (container_width - left - width).max(0.))
}

/// How much of the card shows: rows drop out as the available height shrinks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Density {
    Full,
    Compact,
    Essential,
}

/// `resolveThreadDetailsCardDensity`; a zero content height means not measured, so it fits.
pub fn resolve_density(height: f32, full: f32, compact: f32) -> Density {
    if full == 0. || full <= height {
        Density::Full
    } else if compact == 0. || compact <= height {
        Density::Compact
    } else {
        Density::Essential
    }
}

/// What the card knows about its target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Inputs {
    /// The thread's worktree or the project's root.
    pub has_cwd: bool,
    pub is_repo: bool,
    pub has_editors: bool,
    /// A fresh draft or a thread with no run, with nothing to commit or push.
    pub quiet_empty_draft: bool,
    /// A git action in flight, or the notice its result left.
    pub git_activity: bool,
    /// The rows the usage limits would take: one per window, or one for the notice; 0 for none.
    pub usage_rows: usize,
}

/// The rows each section renders, as `ThreadDetailsPanel` flags them. Environment and scripts
/// are not in this client yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Show {
    pub open_in: bool,
    pub branch: bool,
    pub git_actions: bool,
    pub changes: bool,
    pub usage: bool,
}

impl Show {
    pub fn resolve(density: Density, inputs: Inputs) -> Self {
        Self {
            open_in: inputs.has_cwd && inputs.has_editors && density != Density::Essential,
            branch: inputs.has_cwd && inputs.is_repo,
            git_actions: inputs.has_cwd
                && inputs.is_repo
                && !inputs.quiet_empty_draft
                && (density == Density::Full || inputs.git_activity),
            changes: inputs.has_cwd && inputs.is_repo && !inputs.quiet_empty_draft,
            usage: inputs.usage_rows > 0 && density != Density::Essential,
        }
    }

    /// Nothing to render, so the card is left out rather than drawn empty.
    pub fn is_empty(self) -> bool {
        !(self.open_in || self.branch || self.git_actions || self.changes || self.usage)
    }

    /// The card's content height at `density`, standing in for the web's measured heights.
    pub fn content_height(self, density: Density, usage_rows: usize) -> f32 {
        let workspace = self.open_in as u8 as f32 * ROW_HEIGHT;
        let version =
            (self.branch as u8 + self.git_actions as u8 + self.changes as u8) as f32 * ROW_HEIGHT;
        let usage = if self.usage {
            usage_rows as f32 * USAGE_ROW_HEIGHT
        } else {
            0.
        };
        let sections = [workspace, version, usage]
            .into_iter()
            .filter(|rows| *rows > 0.);
        let count = sections.clone().count() as f32;
        let divider = if density == Density::Full {
            (count - 1.).max(0.)
        } else {
            0.
        };
        sections.sum::<f32>() + count * SECTION_PADDING + divider
    }
}

/// The density for `height`, from the content each density would render.
pub fn density_for(height: f32, inputs: Inputs) -> Density {
    let heights =
        |density| Show::resolve(density, inputs).content_height(density, inputs.usage_rows);
    resolve_density(height, heights(Density::Full), heights(Density::Compact))
}

/// `EDITORS` in `editor.ts`, in the order that picks the default.
pub const EDITORS: &[(&str, &str)] = &[
    ("cursor", "Cursor"),
    ("trae", "Trae"),
    ("kiro", "Kiro"),
    ("vscode", "VS Code"),
    ("vscode-insiders", "VS Code Insiders"),
    ("vscodium", "VSCodium"),
    ("zed", "Zed"),
    ("antigravity", "Antigravity"),
    ("idea", "IntelliJ IDEA"),
    ("aqua", "Aqua"),
    ("clion", "CLion"),
    ("datagrip", "DataGrip"),
    ("dataspell", "DataSpell"),
    ("goland", "GoLand"),
    ("phpstorm", "PhpStorm"),
    ("pycharm", "PyCharm"),
    ("rider", "Rider"),
    ("rubymine", "RubyMine"),
    ("rustrover", "RustRover"),
    ("webstorm", "WebStorm"),
    ("file-manager", "File Manager"),
];

pub const FILE_MANAGER: &str = "file-manager";

/// `editorLabelForPlatform` on macOS, where the file manager is Finder.
pub fn editor_label(id: &str) -> &'static str {
    if id == FILE_MANAGER {
        return "Finder";
    }
    EDITORS
        .iter()
        .find(|(known, _)| *known == id)
        .map_or("Editor", |(_, label)| label)
}

/// The installed editors the picker lists, in `EDITORS` order; ids this client does not know are dropped.
pub fn installed_editors(available: &[String]) -> Vec<&'static str> {
    EDITORS
        .iter()
        .filter(|(id, _)| available.iter().any(|a| a == id))
        .map(|(id, _)| *id)
        .collect()
}

/// `usePreferredEditor`: the last pick while it is installed, else the first installed editor.
pub fn preferred_editor(last: Option<&str>, installed: &[&'static str]) -> Option<&'static str> {
    last.and_then(|last| installed.iter().copied().find(|id| *id == last))
        .or_else(|| installed.first().copied())
}

/// `formatDiffCount`: `Intl.NumberFormat` in en-US groups thousands with commas.
pub fn format_diff_count(count: u64) -> String {
    let digits = count.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Lines {
    pub insertions: u64,
    pub deletions: u64,
}

impl Lines {
    fn decode(value: &Value) -> Self {
        Self {
            insertions: value["insertions"].as_u64().unwrap_or(0),
            deletions: value["deletions"].as_u64().unwrap_or(0),
        }
    }

    fn is_zero(self) -> bool {
        self.insertions == 0 && self.deletions == 0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangedFile {
    pub path: String,
    pub lines: Lines,
}

/// The local half of the `subscribeVcsStatus` stream (`VcsStatusLocalResult`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Local {
    pub is_repo: bool,
    pub has_primary_remote: bool,
    pub is_default_ref: bool,
    pub ref_name: Option<String>,
    pub has_working_tree_changes: bool,
    pub working_tree: Lines,
    pub branch_changes: Option<Lines>,
    /// `workingTree.files`, which the commit dialog lists.
    pub files: Vec<ChangedFile>,
    /// `sourceControlProvider.kind`, which picks PR or MR wording.
    pub provider_kind: Option<String>,
}

/// The remote half (`VcsStatusRemoteResult`); absent until the server has looked, which reads as
/// no upstream, as in `mergeGitStatusParts`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Remote {
    pub has_upstream: bool,
    pub ahead: u64,
    pub behind: u64,
    pub ahead_of_default: Option<u64>,
    pub pr: Option<PullRequest>,
}

/// The change request `pr` carries; only its state decides the actions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PullRequest {
    pub number: u64,
    pub url: String,
    pub open: bool,
}

impl Remote {
    pub fn pr_open(&self) -> bool {
        self.pr.as_ref().is_some_and(|pr| pr.open)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VcsStatus {
    pub local: Local,
    pub remote: Remote,
}

impl Local {
    fn decode(value: &Value) -> Option<Self> {
        let flag = |name: &str| value[name].as_bool();
        Some(Self {
            is_repo: flag("isRepo")?,
            has_primary_remote: flag("hasPrimaryRemote").unwrap_or(false),
            is_default_ref: flag("isDefaultRef").unwrap_or(false),
            ref_name: value["refName"].as_str().map(str::to_owned),
            has_working_tree_changes: flag("hasWorkingTreeChanges").unwrap_or(false),
            working_tree: Lines::decode(&value["workingTree"]),
            branch_changes: value
                .get("branchChanges")
                .filter(|changes| changes.is_object())
                .map(Lines::decode),
            files: value["workingTree"]["files"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|file| {
                    Some(ChangedFile {
                        path: file["path"].as_str()?.to_owned(),
                        lines: Lines::decode(file),
                    })
                })
                .collect(),
            provider_kind: value["sourceControlProvider"]["kind"]
                .as_str()
                .map(str::to_owned),
        })
    }
}

impl Remote {
    fn decode(value: &Value) -> Self {
        if !value.is_object() {
            return Self::default();
        }
        Self {
            has_upstream: value["hasUpstream"].as_bool().unwrap_or(false),
            ahead: value["aheadCount"].as_u64().unwrap_or(0),
            behind: value["behindCount"].as_u64().unwrap_or(0),
            ahead_of_default: value["aheadOfDefaultCount"].as_u64(),
            pr: value["pr"].is_object().then(|| PullRequest {
                number: value["pr"]["number"].as_u64().unwrap_or(0),
                url: value["pr"]["url"].as_str().unwrap_or_default().to_owned(),
                open: value["pr"]["state"] == "open",
            }),
        }
    }
}

impl VcsStatus {
    /// `mergeVcsStatusStream`: a snapshot replaces both halves, an update only its own. `None`
    /// when the event is not one this client reads.
    pub fn apply(current: Option<&Self>, event: &Value) -> Option<Self> {
        let kept = |current: Option<&Self>| current.cloned().unwrap_or_default();
        match event["_tag"].as_str()? {
            "snapshot" => Some(Self {
                local: Local::decode(&event["local"])?,
                remote: Remote::decode(&event["remote"]),
            }),
            "localUpdated" => Some(Self {
                local: Local::decode(&event["local"])?,
                remote: kept(current).remote,
            }),
            "remoteUpdated" => {
                let base = match current {
                    Some(current) => current.clone(),
                    None => Self {
                        local: Local {
                            is_repo: true,
                            ..Local::default()
                        },
                        remote: Remote::default(),
                    },
                };
                Some(Self {
                    local: base.local,
                    remote: Remote::decode(&event["remote"]),
                })
            }
            _ => None,
        }
    }

    /// `resolveChangesTotals`: a zero branch total must not hide a dirty working tree.
    pub fn changes_totals(&self) -> Lines {
        let tree = self.local.working_tree;
        match self.local.branch_changes {
            Some(branch) if !branch.is_zero() => branch,
            _ if !tree.is_zero() => tree,
            Some(branch) => branch,
            None => tree,
        }
    }

    /// Whether `resolveQuickAction` would offer nothing to run, which with no work quiets a fresh draft.
    pub fn quick_action_disabled(&self) -> bool {
        let (local, remote) = (&self.local, &self.remote);
        if local.ref_name.is_none() {
            return true;
        }
        if local.has_working_tree_changes {
            return false;
        }
        if !remote.has_upstream {
            return local.has_primary_remote && remote.ahead == 0;
        }
        let diverged = remote.ahead > 0 && remote.behind > 0;
        if diverged {
            return true;
        }
        if remote.behind > 0 || remote.ahead > 0 {
            return false;
        }
        if remote.pr_open() {
            return true;
        }
        let delta = remote.ahead_of_default.unwrap_or(remote.ahead) > 0;
        !(delta && !local.is_default_ref)
    }
}

/// `quietEmptyDraft` without a git action in flight or a status error: a draft, or a thread that
/// has not run, hides the changes row until there is work.
pub fn quiet_empty_draft(has_run: bool, status: Option<&VcsStatus>) -> bool {
    if has_run {
        return false;
    }
    match status {
        // Until the status loads there is nothing to show either way.
        None => true,
        Some(status) => {
            let totals = status.changes_totals();
            status.quick_action_disabled() && totals.is_zero()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn lane_stays_centered_until_the_card_forces_it_left() {
        assert_eq!(lane_insets(1200., false), (0., 0.));
        assert_eq!(lane_insets(1600., true), (416., 416.));
        assert_eq!(lane_insets(1416., true), (324., 324.));
        assert_eq!(lane_insets(1208., true), (116., 324.));
        assert_eq!(lane_insets(1012., true), (48., 324.));
    }

    fn inputs() -> Inputs {
        Inputs {
            has_cwd: true,
            is_repo: true,
            has_editors: true,
            quiet_empty_draft: false,
            git_activity: false,
            usage_rows: 0,
        }
    }

    fn status(local: Value, remote: Value) -> Value {
        json!({"_tag": "snapshot", "local": local, "remote": remote})
    }

    fn clean() -> Value {
        json!({
            "isRepo": true, "hasPrimaryRemote": true, "isDefaultRef": false,
            "refName": "feature", "hasWorkingTreeChanges": false,
            "workingTree": {"files": [], "insertions": 0, "deletions": 0},
        })
    }

    fn remote(ahead: u64, behind: u64, upstream: bool) -> Value {
        json!({"hasUpstream": upstream, "aheadCount": ahead, "behindCount": behind, "pr": null})
    }

    #[test]
    fn the_card_fits_beside_the_chat_lane_or_not_at_all() {
        // 48 + 640 + 32 + 280 + 12 is the narrowest main area.
        assert_eq!(card_height(1012., 600.), Some(576.));
        assert_eq!(card_height(1011., 600.), None);
        assert_eq!(card_height(1400., 183.), None);
        assert_eq!(card_height(1400., 184.), Some(160.));
    }

    #[test]
    fn density_drops_to_the_smaller_content_that_fits() {
        assert_eq!(resolve_density(500., 400., 300.), Density::Full);
        assert_eq!(resolve_density(350., 400., 300.), Density::Compact);
        assert_eq!(resolve_density(200., 400., 300.), Density::Essential);
        assert_eq!(resolve_density(10., 0., 0.), Density::Full);
        // Four rows in two sections: 4 * 32 + 2 * 18 + a divider when full.
        assert_eq!(density_for(165., inputs()), Density::Full);
        assert_eq!(density_for(164., inputs()), Density::Compact);
        assert_eq!(density_for(100., inputs()), Density::Essential);
        // Two usage windows add a third section of two 24px rows.
        let limited = Inputs {
            usage_rows: 2,
            ..inputs()
        };
        assert_eq!(density_for(165. + 48. + 18. + 1., limited), Density::Full);
        assert_eq!(density_for(165. + 48. + 18., limited), Density::Compact);
    }

    #[test]
    fn sections_show_only_the_rows_that_work() {
        let show = |density, change: fn(&mut Inputs)| {
            let mut i = inputs();
            change(&mut i);
            Show::resolve(density, i)
        };
        assert_eq!(
            show(Density::Full, |_| {}),
            Show {
                open_in: true,
                branch: true,
                git_actions: true,
                changes: true,
                usage: false
            }
        );
        assert!(show(Density::Full, |i| i.usage_rows = 1).usage);
        assert!(!show(Density::Essential, |i| i.usage_rows = 1).usage);
        assert!(!show(Density::Essential, |_| {}).open_in);
        assert!(!show(Density::Full, |i| i.has_editors = false).open_in);
        let not_repo = show(Density::Full, |i| i.is_repo = false);
        assert!(not_repo.open_in && !not_repo.branch && !not_repo.git_actions && !not_repo.changes);
        let quiet = show(Density::Full, |i| i.quiet_empty_draft = true);
        assert!(quiet.branch && !quiet.git_actions && !quiet.changes);
        assert!(!show(Density::Compact, |_| {}).git_actions);
        assert!(show(Density::Compact, |i| i.git_activity = true).git_actions);
        assert!(show(Density::Full, |i| i.has_cwd = false).is_empty());
    }

    #[test]
    fn editors_follow_the_web_order_and_preference() {
        let available: Vec<String> = ["zed", "cursor", "file-manager", "unknown"]
            .map(str::to_owned)
            .into();
        let installed = installed_editors(&available);
        assert_eq!(installed, ["cursor", "zed", "file-manager"]);
        assert_eq!(preferred_editor(Some("zed"), &installed), Some("zed"));
        assert_eq!(preferred_editor(Some("vscode"), &installed), Some("cursor"));
        assert_eq!(preferred_editor(None, &[]), None);
        assert_eq!(editor_label("file-manager"), "Finder");
        assert_eq!(editor_label("vscode-insiders"), "VS Code Insiders");
    }

    #[test]
    fn diff_counts_group_thousands() {
        let shown: Vec<_> = [0, 7, 999, 1000, 12_345, 1_234_567]
            .map(format_diff_count)
            .into();
        assert_eq!(shown, ["0", "7", "999", "1,000", "12,345", "1,234,567"]);
    }

    #[test]
    fn the_stream_merges_like_the_web() {
        let mut dirty = clean();
        dirty["hasWorkingTreeChanges"] = json!(true);
        dirty["workingTree"] = json!({"files": [], "insertions": 4, "deletions": 2});
        let snapshot =
            VcsStatus::apply(None, &status(clean(), remote(2, 0, true))).expect("snapshot");
        assert_eq!(snapshot.remote.ahead, 2);
        let local = json!({"_tag": "localUpdated", "local": dirty});
        let updated = VcsStatus::apply(Some(&snapshot), &local).expect("local");
        assert_eq!(updated.remote.ahead, 2);
        assert_eq!(
            updated.changes_totals(),
            Lines {
                insertions: 4,
                deletions: 2
            }
        );
        let remote_event = json!({"_tag": "remoteUpdated", "remote": null});
        let reset = VcsStatus::apply(Some(&updated), &remote_event).expect("remote");
        assert_eq!(reset.remote, Remote::default());
        assert_eq!(reset.local, updated.local);
        // A remote update before any snapshot starts from an empty repository.
        assert!(
            VcsStatus::apply(None, &remote_event)
                .expect("remote")
                .local
                .is_repo
        );
        assert!(VcsStatus::apply(None, &json!({"_tag": "other"})).is_none());
    }

    #[test]
    fn a_zero_branch_total_does_not_hide_a_dirty_tree() {
        let mut local = clean();
        local["branchChanges"] = json!({"baseRef": "main", "insertions": 0, "deletions": 0});
        local["workingTree"] = json!({"files": [], "insertions": 3, "deletions": 1});
        let dirty = VcsStatus::apply(None, &status(local.clone(), Value::Null)).expect("status");
        assert_eq!(
            dirty.changes_totals(),
            Lines {
                insertions: 3,
                deletions: 1
            }
        );
        local["branchChanges"] = json!({"baseRef": "main", "insertions": 10, "deletions": 5});
        let branch = VcsStatus::apply(None, &status(local, Value::Null)).expect("status");
        assert_eq!(
            branch.changes_totals(),
            Lines {
                insertions: 10,
                deletions: 5
            }
        );
        let none = VcsStatus::apply(None, &status(clean(), Value::Null)).expect("status");
        assert_eq!(none.changes_totals(), Lines::default());
    }

    #[test]
    fn a_fresh_draft_is_quiet_until_there_is_work() {
        let at = |local: Value, remote: Value| {
            VcsStatus::apply(None, &status(local, remote)).expect("status")
        };
        let synced = at(clean(), remote(0, 0, true));
        assert!(synced.quick_action_disabled());
        assert!(quiet_empty_draft(false, Some(&synced)));
        assert!(quiet_empty_draft(false, None));
        // A thread that has run keeps its rows.
        assert!(!quiet_empty_draft(true, Some(&synced)));
        let mut dirty = clean();
        dirty["hasWorkingTreeChanges"] = json!(true);
        assert!(!quiet_empty_draft(
            false,
            Some(&at(dirty, remote(0, 0, true)))
        ));
        assert!(!quiet_empty_draft(
            false,
            Some(&at(clean(), remote(1, 0, true)))
        ));
        // No upstream and no commits to push is still nothing to do.
        assert!(at(clean(), remote(0, 0, false)).quick_action_disabled());
        assert!(at(clean(), remote(1, 1, true)).quick_action_disabled());
        let mut detached = clean();
        detached["refName"] = Value::Null;
        assert!(at(detached, remote(0, 0, true)).quick_action_disabled());
    }
}
