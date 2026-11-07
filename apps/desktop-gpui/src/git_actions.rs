//! The git action controls' logic: `GitActionsControl.logic.ts`. Which actions the menu offers,
//! what the quick action button says and does, and the default ref confirmation wording.

use crate::details::VcsStatus;

/// `GitStackedAction`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Commit,
    CommitPush,
    CommitPushPr,
    Push,
    CreatePr,
}

impl Action {
    pub fn wire(self) -> &'static str {
        match self {
            Self::Commit => "commit",
            Self::CommitPush => "commit_push",
            Self::CommitPushPr => "commit_push_pr",
            Self::Push => "push",
            Self::CreatePr => "create_pr",
        }
    }
}

/// `ChangeRequestTerminology`: GitLab says merge request, every other provider pull request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Terminology {
    pub short: &'static str,
    pub singular: &'static str,
}

impl Terminology {
    pub fn of(status: Option<&VcsStatus>) -> Self {
        let kind = status.and_then(|status| status.local.provider_kind.as_deref());
        if kind == Some("gitlab") {
            Self {
                short: "MR",
                singular: "merge request",
            }
        } else {
            Self {
                short: "PR",
                singular: "pull request",
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuId {
    Commit,
    Push,
    Pr,
}

/// `GitActionMenuItem`; each opens a dialog or runs after the default ref confirmation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuItem {
    pub id: MenuId,
    pub label: String,
    pub disabled: bool,
}

pub fn build_menu_items(
    status: Option<&VcsStatus>,
    is_busy: bool,
    has_primary_remote: bool,
) -> Vec<MenuItem> {
    let Some(status) = status else {
        return Vec::new();
    };
    let (local, remote) = (&status.local, &status.remote);
    let terminology = Terminology::of(Some(status));
    let has_branch = local.ref_name.is_some();
    let has_open_pr = remote.pr_open();
    let is_behind = remote.behind > 0;
    let has_default_delta = remote.ahead_of_default.unwrap_or(remote.ahead) > 0;
    let can_push_without_upstream = has_primary_remote && !remote.has_upstream;
    let can_publish = remote.has_upstream || can_push_without_upstream;
    let can_commit = !is_busy && local.has_working_tree_changes;
    let can_push = !is_busy && has_branch && !is_behind && remote.ahead > 0 && can_publish;
    let can_create_pr =
        !is_busy && has_branch && !has_open_pr && has_default_delta && !is_behind && can_publish;

    let commit = MenuItem {
        id: MenuId::Commit,
        label: "Commit".to_owned(),
        disabled: !can_commit,
    };
    if !has_primary_remote {
        return vec![commit];
    }
    let push = MenuItem {
        id: MenuId::Push,
        label: "Push".to_owned(),
        disabled: !can_push,
    };
    if has_open_pr {
        return vec![commit, push];
    }
    vec![
        commit,
        push,
        MenuItem {
            id: MenuId::Pr,
            label: format!("Create {}", terminology.short),
            disabled: !can_create_pr,
        },
    ]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuickKind {
    Run(Action),
    Pull,
    Publish,
    Hint,
}

/// `GitQuickAction`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Quick {
    pub label: String,
    pub disabled: bool,
    pub kind: QuickKind,
    pub hint: Option<String>,
}

impl Quick {
    fn run(label: impl Into<String>, action: Action) -> Self {
        Self {
            label: label.into(),
            disabled: false,
            kind: QuickKind::Run(action),
            hint: None,
        }
    }

    fn hint(label: &str, hint: impl Into<String>) -> Self {
        Self {
            label: label.to_owned(),
            disabled: true,
            kind: QuickKind::Hint,
            hint: Some(hint.into()),
        }
    }
}

pub fn resolve_quick_action(
    status: Option<&VcsStatus>,
    is_busy: bool,
    is_default_ref: bool,
    has_primary_remote: bool,
) -> Quick {
    if is_busy {
        return Quick::hint("Commit", "Git action in progress.");
    }
    let Some(status) = status else {
        return Quick::hint("Commit", "Git status is unavailable.");
    };
    let (local, remote) = (&status.local, &status.remote);
    let terminology = Terminology::of(Some(status));
    let short = terminology.short;
    let is_ahead = remote.ahead > 0;
    let is_behind = remote.behind > 0;
    let has_open_pr = remote.pr_open();
    let has_default_delta = remote.ahead_of_default.unwrap_or(remote.ahead) > 0;
    let push_action = if is_default_ref {
        Action::CommitPush
    } else {
        Action::Push
    };

    if local.ref_name.is_none() {
        return Quick::hint(
            "Commit",
            format!(
                "Create and checkout a ref before pushing or opening a {}.",
                terminology.singular
            ),
        );
    }
    if local.has_working_tree_changes {
        if is_behind || (!remote.has_upstream && !has_primary_remote) {
            return Quick::run("Commit", Action::Commit);
        }
        if has_open_pr || is_default_ref {
            return Quick::run("Commit & push", Action::CommitPush);
        }
        return Quick::run(format!("Commit, push & {short}"), Action::CommitPushPr);
    }
    if !remote.has_upstream {
        if !has_primary_remote {
            return Quick {
                label: "Publish repository".to_owned(),
                disabled: false,
                kind: QuickKind::Publish,
                hint: None,
            };
        }
        if !is_ahead {
            return if has_open_pr {
                Quick::hint("Commit", "Branch is up to date. No action needed.")
            } else {
                Quick::hint("Push", "No local commits to push.")
            };
        }
        if has_open_pr || is_default_ref {
            return Quick::run("Push", push_action);
        }
        return Quick::run(format!("Push & create {short}"), Action::CreatePr);
    }
    if is_ahead && is_behind {
        return Quick::hint(
            "Sync ref",
            "Branch has diverged from upstream. Rebase/merge first.",
        );
    }
    if is_behind {
        return Quick {
            label: "Pull".to_owned(),
            disabled: false,
            kind: QuickKind::Pull,
            hint: None,
        };
    }
    if is_ahead {
        if has_open_pr || is_default_ref {
            return Quick::run("Push", push_action);
        }
        return Quick::run(format!("Push & create {short}"), Action::CreatePr);
    }
    if has_open_pr {
        return Quick::hint("Commit", "Branch is up to date. No action needed.");
    }
    if has_default_delta && !is_default_ref {
        return Quick::run(format!("Create {short}"), Action::CreatePr);
    }
    Quick::hint("Commit", "Branch is up to date. No action needed.")
}

/// `requiresDefaultBranchConfirmation`.
pub fn requires_default_branch_confirmation(action: Action, is_default_ref: bool) -> bool {
    is_default_ref && action != Action::Commit
}

/// `DefaultBranchActionDialogCopy`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DialogCopy {
    pub title: String,
    pub description: String,
    pub continue_label: String,
}

/// `resolveDefaultBranchActionDialogCopy`; `action` is one `requires_default_branch_confirmation` allows.
pub fn default_branch_dialog_copy(
    action: Action,
    branch: &str,
    includes_commit: bool,
    terminology: Terminology,
) -> DialogCopy {
    let suffix = format!(
        " on \"{branch}\". You can continue on this ref or create a feature ref and run the same action there."
    );
    if matches!(action, Action::Push | Action::CommitPush) {
        return if includes_commit {
            DialogCopy {
                title: "Commit & push to default ref?".to_owned(),
                description: format!("This action will commit and push changes{suffix}"),
                continue_label: format!("Commit & push to {branch}"),
            }
        } else {
            DialogCopy {
                title: "Push to default ref?".to_owned(),
                description: format!("This action will push local commits{suffix}"),
                continue_label: format!("Push to {branch}"),
            }
        };
    }
    let (short, singular) = (terminology.short, terminology.singular);
    if includes_commit {
        DialogCopy {
            title: format!("Commit, push & create {short} from default ref?"),
            description: format!("This action will commit, push, and create a {singular}{suffix}"),
            continue_label: format!("Commit, push & create {short}"),
        }
    } else {
        DialogCopy {
            title: format!("Push & create {short} from default ref?"),
            description: format!(
                "This action will push local commits and create a {singular}{suffix}"
            ),
            continue_label: format!("Push & create {short}"),
        }
    }
}

/// `resolveFeatureBranchUnavailableReason`: clean trees cannot name a feature branch.
pub fn feature_branch_unavailable_reason(branch: &str, has_changes: bool) -> Option<String> {
    (!has_changes).then(|| {
        format!(
            "There are no changes to commit, so a feature branch can't be named. Create a branch yourself or continue on {branch}."
        )
    })
}

/// What the primary button reports while a stacked action runs (`GitActionsControl.tsx`'s
/// progress state): the phase, and the last line of hook output.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    pub phase: Option<String>,
    pub hook: Option<String>,
    pub output: Option<String>,
}

/// How a stacked action ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Success {
        title: String,
        description: Option<String>,
    },
    Failed(String),
}

impl Progress {
    pub const STARTING: &'static str = "Starting source control action...";

    /// The status line: a running hook, else the phase.
    pub fn label(&self) -> String {
        match (&self.hook, &self.phase) {
            (Some(hook), _) => format!("Running {hook}..."),
            (None, Some(phase)) => phase.clone(),
            (None, None) => Self::STARTING.to_owned(),
        }
    }

    /// Folds one `GitActionProgressEvent`; the finishing events give the outcome.
    pub fn apply(&mut self, event: &serde_json::Value) -> Option<Outcome> {
        match event["kind"].as_str()? {
            "phase_started" => {
                self.phase = event["label"].as_str().map(str::to_owned);
                self.hook = None;
            }
            "hook_started" => self.hook = event["hookName"].as_str().map(str::to_owned),
            "hook_output" => {
                let line = event["text"]
                    .as_str()
                    .and_then(|text| text.lines().rev().find(|line| !line.trim().is_empty()));
                if let Some(line) = line {
                    self.output = Some(line.trim().to_owned());
                }
            }
            "hook_finished" => self.hook = None,
            "action_finished" => return Some(Self::finished(&event["result"])),
            "action_failed" => {
                let message = event["message"].as_str();
                return Some(Outcome::Failed(
                    message.unwrap_or("Action failed.").to_owned(),
                ));
            }
            _ => {}
        }
        None
    }

    /// The toast of a result, or the plain wording when the server sent none.
    pub fn finished(result: &serde_json::Value) -> Outcome {
        let toast = &result["toast"];
        Outcome::Success {
            title: toast["title"].as_str().unwrap_or("Done").to_owned(),
            description: toast["description"].as_str().map(str::to_owned),
        }
    }
}

/// `formatElapsedDescription`: seconds, then minutes and seconds.
pub fn format_elapsed(seconds: u64) -> String {
    if seconds < 60 {
        format!("{seconds}s")
    } else {
        format!("{}m {}s", seconds / 60, seconds % 60)
    }
}

/// The toast a pull result gives (`vcs.pull`): updated, or already in sync.
pub fn pull_outcome(result: &serde_json::Value) -> Outcome {
    let name = result["refName"].as_str().unwrap_or("The branch");
    if result["status"] == "pulled" {
        let from = result["upstreamRef"].as_str().unwrap_or("upstream");
        Outcome::Success {
            title: "Pulled".to_owned(),
            description: Some(format!("Updated {name} from {from}")),
        }
    } else {
        Outcome::Success {
            title: "Already up to date".to_owned(),
            description: Some(format!("{name} is already synchronized.")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    #[test]
    fn progress_follows_the_stream() {
        let mut progress = Progress::default();
        assert_eq!(progress.label(), Progress::STARTING);
        assert!(
            progress
                .apply(&json!({"kind": "phase_started", "label": "Pushing..."}))
                .is_none()
        );
        assert_eq!(progress.label(), "Pushing...");
        progress.apply(&json!({"kind": "hook_started", "hookName": "pre-push"}));
        assert_eq!(progress.label(), "Running pre-push...");
        progress.apply(&json!({"kind": "hook_output", "text": "lint\nok\n\n"}));
        assert_eq!(progress.output.as_deref(), Some("ok"));
        progress.apply(&json!({"kind": "hook_finished"}));
        assert_eq!(progress.label(), "Pushing...");
        let done = progress.apply(&json!({
            "kind": "action_finished",
            "result": {"toast": {"title": "Pushed", "description": "feature"}}
        }));
        assert_eq!(
            done,
            Some(Outcome::Success {
                title: "Pushed".to_owned(),
                description: Some("feature".to_owned())
            })
        );
        assert_eq!(
            progress.apply(&json!({"kind": "action_failed", "message": "no remote"})),
            Some(Outcome::Failed("no remote".to_owned()))
        );
        assert!(progress.apply(&json!({"kind": "action_started"})).is_none());
    }

    #[test]
    fn elapsed_and_pull_wording() {
        assert_eq!(format_elapsed(9), "9s");
        assert_eq!(format_elapsed(75), "1m 15s");
        let pulled = pull_outcome(
            &json!({"status": "pulled", "refName": "main", "upstreamRef": "origin/main"}),
        );
        assert_eq!(
            pulled,
            Outcome::Success {
                title: "Pulled".to_owned(),
                description: Some("Updated main from origin/main".to_owned())
            }
        );
        let synced = pull_outcome(&json!({"status": "skipped_up_to_date", "refName": "main"}));
        assert!(matches!(synced, Outcome::Success { title, .. } if title == "Already up to date"));
    }

    fn status(local: Value, remote: Value) -> VcsStatus {
        let mut base = json!({
            "isRepo": true, "hasPrimaryRemote": true, "isDefaultRef": false,
            "refName": "feature/test", "hasWorkingTreeChanges": false,
            "workingTree": {"files": [], "insertions": 0, "deletions": 0},
        });
        let mut tail = json!({"hasUpstream": true, "aheadCount": 0, "behindCount": 0, "pr": null});
        for (target, patch) in [(&mut base, local), (&mut tail, remote)] {
            if let Some(patch) = patch.as_object() {
                for (key, value) in patch {
                    target[key] = value.clone();
                }
            }
        }
        VcsStatus::apply(
            None,
            &json!({"_tag": "snapshot", "local": base, "remote": tail}),
        )
        .expect("status")
    }

    fn open_pr() -> Value {
        json!({"number": 13, "title": "Open PR", "url": "https://example.com/pr/13",
               "baseRef": "main", "headRef": "feature/test", "state": "open"})
    }

    fn quick(status: &VcsStatus) -> Quick {
        resolve_quick_action(Some(status), false, false, true)
    }

    fn pr_item(items: &[MenuItem]) -> &MenuItem {
        items
            .iter()
            .find(|item| item.id == MenuId::Pr)
            .expect("pr item")
    }

    #[test]
    fn a_clean_ahead_ref_with_an_open_pr_prefers_push() {
        let ahead = status(json!({}), json!({"aheadCount": 3, "pr": open_pr()}));
        assert_eq!(quick(&ahead), Quick::run("Push", Action::Push));
    }

    #[test]
    fn a_synced_ref_ahead_of_default_creates_a_pr() {
        let synced = status(json!({}), json!({"aheadOfDefaultCount": 1}));
        assert_eq!(quick(&synced), Quick::run("Create PR", Action::CreatePr));
        assert!(!pr_item(&build_menu_items(Some(&synced), false, true)).disabled);
        let dirty = status(
            json!({"hasWorkingTreeChanges": true}),
            json!({"aheadOfDefaultCount": 1}),
        );
        assert!(!pr_item(&build_menu_items(Some(&dirty), false, true)).disabled);
    }

    #[test]
    fn the_default_ref_commits_and_pushes() {
        let dirty = status(
            json!({"refName": "main", "hasWorkingTreeChanges": true}),
            json!({}),
        );
        let quick = resolve_quick_action(Some(&dirty), false, true, true);
        assert_eq!(quick, Quick::run("Commit & push", Action::CommitPush));
    }

    #[test]
    fn dirty_changes_behind_upstream_only_commit() {
        let behind = status(
            json!({"hasWorkingTreeChanges": true}),
            json!({"behindCount": 2}),
        );
        let commit = Quick::run("Commit", Action::Commit);
        assert_eq!(quick(&behind), commit);
        let with_pr = status(
            json!({"hasWorkingTreeChanges": true}),
            json!({"behindCount": 2, "pr": open_pr()}),
        );
        assert_eq!(quick(&with_pr), commit);
    }

    #[test]
    fn no_upstream_pushes_and_creates_or_publishes() {
        let ahead = status(json!({}), json!({"hasUpstream": false, "aheadCount": 2}));
        assert_eq!(
            quick(&ahead),
            Quick::run("Push & create PR", Action::CreatePr)
        );
        let publish = resolve_quick_action(Some(&ahead), false, false, false);
        assert_eq!(publish.kind, QuickKind::Publish);
        assert_eq!(publish.label, "Publish repository");
        assert!(!publish.disabled);
    }

    #[test]
    fn hints_explain_each_disabled_state() {
        let synced = status(json!({}), json!({}));
        assert_eq!(
            quick(&synced).hint.as_deref(),
            Some("Branch is up to date. No action needed.")
        );
        let diverged = status(json!({}), json!({"aheadCount": 1, "behindCount": 1}));
        assert_eq!(quick(&diverged).label, "Sync ref");
        let behind = status(json!({}), json!({"behindCount": 1}));
        assert_eq!(quick(&behind).kind, QuickKind::Pull);
        let detached = status(json!({"refName": null}), json!({}));
        let hint = quick(&detached).hint.expect("hint");
        assert!(hint.starts_with("Create and checkout a ref"));
        let busy = resolve_quick_action(Some(&synced), true, false, true);
        assert_eq!(busy.hint.as_deref(), Some("Git action in progress."));
        let none = resolve_quick_action(None, false, false, true);
        assert_eq!(none.hint.as_deref(), Some("Git status is unavailable."));
        let fresh = status(json!({}), json!({"hasUpstream": false}));
        assert_eq!(
            quick(&fresh).hint.as_deref(),
            Some("No local commits to push.")
        );
    }

    #[test]
    fn the_menu_follows_remote_and_pr_state() {
        let ahead = status(json!({}), json!({"aheadCount": 1}));
        let ids = |items: &[MenuItem]| items.iter().map(|item| item.id).collect::<Vec<_>>();
        let full = build_menu_items(Some(&ahead), false, true);
        assert_eq!(ids(&full), [MenuId::Commit, MenuId::Push, MenuId::Pr]);
        assert!(full[0].disabled && !full[1].disabled);
        let no_remote = build_menu_items(Some(&ahead), false, false);
        assert_eq!(ids(&no_remote), [MenuId::Commit]);
        let with_pr = status(json!({}), json!({"aheadCount": 1, "pr": open_pr()}));
        assert_eq!(
            ids(&build_menu_items(Some(&with_pr), false, true)),
            [MenuId::Commit, MenuId::Push]
        );
        let busy = build_menu_items(Some(&ahead), true, true);
        assert!(busy.iter().all(|item| item.disabled));
        assert!(build_menu_items(None, false, true).is_empty());
    }

    #[test]
    fn gitlab_says_merge_request() {
        let provider = json!({"kind": "gitlab", "name": "GitLab"});
        let ahead = status(
            json!({"sourceControlProvider": provider}),
            json!({"hasUpstream": false, "aheadCount": 2}),
        );
        assert_eq!(quick(&ahead).label, "Push & create MR");
        let dirty = status(
            json!({"sourceControlProvider": provider, "hasWorkingTreeChanges": true}),
            json!({}),
        );
        assert_eq!(quick(&dirty).label, "Commit, push & MR");
        assert_eq!(
            pr_item(&build_menu_items(Some(&ahead), false, true)).label,
            "Create MR"
        );
        let copy = default_branch_dialog_copy(
            Action::CreatePr,
            "main",
            false,
            Terminology::of(Some(&ahead)),
        );
        assert_eq!(copy.title, "Push & create MR from default ref?");
        assert!(copy.description.contains("create a merge request on"));
    }

    #[test]
    fn the_default_ref_confirms_every_push_action() {
        assert!(!requires_default_branch_confirmation(Action::Commit, true));
        for action in [
            Action::Push,
            Action::CreatePr,
            Action::CommitPush,
            Action::CommitPushPr,
        ] {
            assert!(requires_default_branch_confirmation(action, true));
            assert!(!requires_default_branch_confirmation(action, false));
        }
    }

    #[test]
    fn the_confirmation_names_the_branch() {
        let pr = Terminology::of(None);
        let copy = default_branch_dialog_copy(Action::CommitPush, "main", true, pr);
        assert_eq!(copy.title, "Commit & push to default ref?");
        assert_eq!(copy.continue_label, "Commit & push to main");
        let copy = default_branch_dialog_copy(Action::Push, "main", false, pr);
        assert_eq!(copy.title, "Push to default ref?");
        let copy = default_branch_dialog_copy(Action::CommitPushPr, "main", true, pr);
        assert_eq!(copy.continue_label, "Commit, push & create PR");
        assert!(feature_branch_unavailable_reason("main", true).is_none());
        let reason = feature_branch_unavailable_reason("main", false).expect("reason");
        assert!(reason.ends_with("continue on main."));
    }
}
