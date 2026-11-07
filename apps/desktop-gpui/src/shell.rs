//! The shell read model built from `orchestration.subscribeShell`, mirroring
//! `packages/client-runtime/src/state/shellReducer.ts` for what this client shows.

use crate::projects;
use crate::status::{LastError, PrBadge, PrState, Section, ShellView, ThreadRow, TopStatus};
use crate::thread_card;
use crate::wire::generated::{
    LineageRelationshipToParent as LineageRelationship, PullRequestsItem,
    PullRequestsItemSource as LinkSource, SnapshotState as PrSnapshotState,
    ThreadsEntryActivityRunStatus as Activity, ThreadsEntryInteractionMode as InteractionMode,
    ThreadsEntryLastErrorClass, ThreadsEntryPendingRuntimeRequestKind as RequestKind,
    ThreadsEntrySettledOverride as Override, ThreadsEntryStatus as RunStatus,
};
use crate::wire::generated::{
    OrchestrationSubscribeShellSuccess as Item,
    OrchestrationSubscribeShellSuccessVariant4Location as Location,
    OrchestrationSubscribeShellSuccessVariant5Location as RemovedLocation,
};
use crate::wire::{Project, Thread};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Epoch milliseconds of an RFC 3339 timestamp, as `Date.parse` reads the server's.
pub(crate) fn timestamp_ms(text: &str) -> Option<i64> {
    let bytes = text.as_bytes();
    let number = |from: usize, len: usize| -> Option<i64> {
        let digits = text.get(from..from + len)?;
        digits
            .bytes()
            .all(|b| b.is_ascii_digit())
            .then(|| digits.parse().ok())?
    };
    let (year, month, day) = (number(0, 4)?, number(5, 2)?, number(8, 2)?);
    let (hour, minute, second) = (number(11, 2)?, number(14, 2)?, number(17, 2)?);
    if bytes.get(4) != Some(&b'-')
        || bytes.get(7) != Some(&b'-')
        || !matches!(bytes.get(10), Some(b'T' | b't' | b' '))
        || bytes.get(13) != Some(&b':')
        || bytes.get(16) != Some(&b':')
        || !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }
    let mut at = 19;
    let mut millis = 0;
    if bytes.get(at) == Some(&b'.') {
        let digits = text[at + 1..]
            .bytes()
            .take_while(u8::is_ascii_digit)
            .count();
        if digits == 0 {
            return None;
        }
        let fraction = &text[at + 1..at + 1 + digits.min(3)];
        millis = fraction.parse::<i64>().ok()? * 10_i64.pow(3 - fraction.len() as u32);
        at += 1 + digits;
    }
    let offset_minutes = match text.get(at..)? {
        "Z" | "z" => 0,
        zone if zone.len() == 6 && (zone.starts_with('+') || zone.starts_with('-')) => {
            let hours = number(at + 1, 2)?;
            let minutes = number(at + 4, 2)?;
            let total = hours * 60 + minutes;
            if zone.starts_with('-') { -total } else { total }
        }
        _ => return None,
    };
    let (y, m) = if month <= 2 {
        (year - 1, month + 9)
    } else {
        (year, month - 3)
    };
    let era = y.div_euclid(400);
    let year_of_era = y - era * 400;
    let day_of_year = (153 * m + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    let seconds = days * 86_400 + hour * 3_600 + minute * 60 + second - offset_minutes * 60;
    Some(seconds * 1_000 + millis)
}

const SECTION_ORDER: [Section; 5] = [
    Section::Pinned,
    Section::Active,
    Section::Working,
    Section::Snoozed,
    Section::Settled,
];

fn ms(text: Option<&str>) -> Option<i64> {
    text.and_then(timestamp_ms)
}

fn is_active(status: RunStatus) -> bool {
    matches!(
        status,
        RunStatus::Preparing
            | RunStatus::Queued
            | RunStatus::Starting
            | RunStatus::Running
            | RunStatus::Waiting
    )
}

/// The runtime status the web derives: the activity run's while one is open, else the thread's.
fn runtime_status(thread: &Thread) -> Option<RunStatus> {
    if thread.latest_run_id.is_none() && thread.active_provider_thread_id.is_none() {
        return None;
    }
    Some(match thread.activity_run_status {
        Some(activity) => match activity {
            Activity::Preparing => RunStatus::Preparing,
            Activity::Starting => RunStatus::Starting,
            Activity::Running => RunStatus::Running,
            Activity::Waiting => RunStatus::Waiting,
        },
        None => thread.status,
    })
}

fn pending_request(thread: &Thread) -> Option<RequestKind> {
    thread.pending_runtime_request.as_ref().map(|r| r.kind)
}

/// `isThreadWorking` in `packages/client-runtime/src/state/threadInbox.ts`.
fn is_working(thread: &Thread) -> bool {
    if pending_request(thread).is_some_and(|kind| kind != RequestKind::AuthRefresh) {
        return false;
    }
    let status = runtime_status(thread);
    if status.is_none_or(|s| !is_active(s) && s != RunStatus::Idle) {
        return false;
    }
    let run_settled = thread.latest_run_id.is_some()
        && !is_active(run_status(thread))
        && thread.active_run_id != thread.latest_run_id;
    !(thread.interaction_mode == InteractionMode::Plan
        && thread.has_actionable_proposed_plan
        && run_settled)
}

fn run_status(thread: &Thread) -> RunStatus {
    match thread.status {
        RunStatus::Idle => RunStatus::Completed,
        other => other,
    }
}

fn run_completed_ms(thread: &Thread) -> Option<i64> {
    thread.latest_run_id.as_ref()?;
    match thread.latest_run_completed_at.as_deref() {
        Some(at) => timestamp_ms(at),
        None if thread.status == RunStatus::Idle || !is_active(thread.status) => {
            timestamp_ms(&thread.updated_at)
        }
        None => None,
    }
}

/// `threadRaisedHandWhileSnoozed` without the subagent-request case, which needs other threads.
fn raised_hand(thread: &Thread) -> bool {
    if pending_request(thread).is_some() || thread.pending_guest_message_count.unwrap_or(0) > 0 {
        return true;
    }
    if runtime_status(thread) != Some(RunStatus::Failed) {
        return false;
    }
    let failed_at = if thread.status == RunStatus::Failed {
        run_completed_ms(thread)
    } else {
        timestamp_ms(&thread.updated_at)
    };
    match ms(thread.snoozed_at.as_deref()) {
        None => true,
        Some(snoozed) => failed_at.is_some_and(|failed| failed > snoozed),
    }
}

/// `effectiveSnoozed` in `packages/client-runtime/src/state/threadSettled.ts`.
fn is_snoozed(thread: &Thread, now_ms: i64) -> bool {
    ms(thread.snoozed_until.as_deref()).is_some_and(|wake| wake > now_ms) && !raised_hand(thread)
}

/// Same precedence as `resolveSidebarThreadSection`, then the Working shelf split.
fn section_of(thread: &Thread, now_ms: i64) -> Section {
    if is_snoozed(thread, now_ms) {
        Section::Snoozed
    } else if thread.settled_override == Some(Override::Settled) {
        Section::Settled
    } else if thread.pinned_at.is_some() {
        Section::Pinned
    } else if is_working(thread) {
        Section::Working
    } else {
        Section::Active
    }
}

/// `isListedThreadLineage` in `packages/contracts/src/orchestrationV2.ts`: subagents and side
/// chats live inside their parent's view, never in the sidebar. The web also hides tasks linked
/// environments run for a listed thread (`delegatedUnderParent`); this client does not track them.
fn is_listed(thread: &Thread) -> bool {
    thread.lineage.relationship_to_parent != Some(LineageRelationship::Subagent)
        && thread.lineage.side_chat != Some(true)
}

fn newest_first(a: (i64, &Thread), b: (i64, &Thread)) -> Ordering {
    b.0.cmp(&a.0).then_with(|| a.1.id.cmp(&b.1.id))
}

fn created_ms(thread: &Thread) -> i64 {
    timestamp_ms(&thread.created_at).unwrap_or(0)
}

/// `sortPinnedThreadsByOrderKey`: keyed threads by key, then keyless newest created first.
fn sort_pinned(threads: &mut [&Thread]) {
    threads.sort_by(|a, b| match (&a.pin_order_key, &b.pin_order_key) {
        (Some(left), Some(right)) => left.cmp(right).then_with(|| a.id.cmp(&b.id)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => newest_first((created_ms(a), a), (created_ms(b), b)),
    });
}

/// `sortInboxThreadsByReturn` with no observed returns.
fn sort_inbox(threads: &mut [&Thread]) {
    let anchor = |t: &Thread| {
        [
            Some(created_ms(t)),
            ms(t.unsettled_at.as_deref()),
            t.latest_run_id
                .as_ref()
                .and(ms(t.latest_run_requested_at.as_deref())),
            run_completed_ms(t),
        ]
        .into_iter()
        .flatten()
        .max()
        .unwrap_or(0)
    };
    threads.sort_by(|a, b| newest_first((anchor(a), a), (anchor(b), b)));
}

/// `sortWorkingThreadsBySend`.
fn sort_working(threads: &mut [&Thread]) {
    let anchor = |t: &Thread| {
        let sent = t.latest_user_authored_message_at.as_deref().or_else(|| {
            t.latest_run_id
                .as_ref()
                .and(t.latest_run_requested_at.as_deref())
        });
        created_ms(t).max(ms(sent).unwrap_or(0))
    };
    threads.sort_by(|a, b| newest_first((anchor(a), a), (anchor(b), b)));
}

/// Oldest wake first.
fn sort_snoozed(threads: &mut [&Thread]) {
    let wake = |t: &Thread| ms(t.snoozed_until.as_deref()).unwrap_or(0);
    threads.sort_by(|a, b| wake(a).cmp(&wake(b)).then_with(|| a.id.cmp(&b.id)));
}

/// `sortSettledThreads` with `resolveSettledThreadTimestamp`.
fn sort_settled(threads: &mut [&Thread]) {
    let anchor = |t: &Thread| {
        if let Some(settled) = ms(t.settled_at.as_deref()) {
            return settled;
        }
        let latest = [
            ms(t.latest_user_message_at.as_deref()),
            t.latest_run_id
                .as_ref()
                .and(ms(t.latest_run_requested_at.as_deref())),
            t.latest_run_id
                .as_ref()
                .and(ms(t.latest_run_started_at.as_deref())),
            run_completed_ms(t),
        ]
        .into_iter()
        .flatten()
        .max();
        latest.or_else(|| timestamp_ms(&t.updated_at)).unwrap_or(0)
    };
    threads.sort_by(|a, b| newest_first((anchor(a), a), (anchor(b), b)));
}

/// `resolveThreadPullRequestBadge` with the presentation of `ThreadStatusIndicators`. A stack is
/// the links of one native stack; stacks the web derives from head and base branches show as `+N`.
fn pr_badge(thread: &Thread) -> Option<PrBadge> {
    let visible: Vec<_> = thread
        .pull_requests
        .iter()
        .flat_map(|links| &links.0)
        .filter(|link| link.source != LinkSource::StackDismissed)
        .collect();
    if visible.is_empty() {
        let linked = thread.linked_pull_request.as_ref()?;
        return Some(PrBadge {
            state: PrState::Pending,
            stack: false,
            text: linked.number.to_string(),
        });
    }
    let state_of = |link: &&PullRequestsItem| match link.snapshot.as_ref() {
        None => PrState::Open,
        Some(snapshot) => match snapshot.state {
            PrSnapshotState::Open if snapshot.is_draft => PrState::Draft,
            PrSnapshotState::Open => PrState::Open,
            PrSnapshotState::Closed => PrState::Closed,
            PrSnapshotState::Merged => PrState::Merged,
        },
    };
    if let [only] = visible.as_slice() {
        return Some(PrBadge {
            state: only
                .snapshot
                .as_ref()
                .map_or(PrState::Pending, |_| state_of(only)),
            stack: false,
            text: only.number.to_string(),
        });
    }
    let states: Vec<PrState> = visible.iter().map(state_of).collect();
    let state = if states.iter().all(|state| *state == PrState::Draft) {
        PrState::Draft
    } else if states.contains(&PrState::Open) {
        PrState::Open
    } else if states.iter().all(|state| *state == PrState::Merged) {
        PrState::Merged
    } else {
        PrState::Closed
    };
    let stack_id = visible[0].stack.as_ref().map(|stack| &stack.id);
    let stack = stack_id.is_some()
        && visible
            .iter()
            .all(|link| link.stack.as_ref().map(|stack| &stack.id) == stack_id);
    Some(PrBadge {
        state,
        stack,
        text: if stack {
            visible.len().to_string()
        } else {
            format!("+{}", visible.len())
        },
    })
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

#[derive(Default)]
pub struct ShellState {
    projects: HashMap<String, Project>,
    threads: HashMap<String, Thread>,
    sequence: i64,
    seeded: bool,
}

impl ShellState {
    /// False until the first snapshot arrives; counts before that would read as "0".
    pub fn is_seeded(&self) -> bool {
        self.seeded
    }

    pub fn project_count(&self) -> usize {
        self.projects.len()
    }

    /// Active threads only: the archive has its own query.
    pub fn thread_count(&self) -> usize {
        self.threads.len()
    }

    /// Rows in the web sidebar's default order (see `Section`); the first project is the
    /// oldest by creation.
    pub fn view(&self) -> ShellView {
        self.view_at(now_ms())
    }

    /// Mirrors `partitionSidebarThreads` in `apps/web/src/components/Sidebar.tsx` under the
    /// default settings: Working shelf on, no project folders, nothing dragged.
    pub fn view_at(&self, now_ms: i64) -> ShellView {
        let mut sections: [Vec<&Thread>; 5] = Default::default();
        for thread in self.threads.values().filter(|thread| is_listed(thread)) {
            sections[section_of(thread, now_ms) as usize].push(thread);
        }
        let [pinned, active, working, snoozed, settled] = &mut sections;
        sort_pinned(pinned);
        sort_inbox(active);
        sort_working(working);
        sort_snoozed(snoozed);
        sort_settled(settled);
        let row = |thread: &Thread, section| ThreadRow {
            id: thread.id.clone(),
            project_id: thread.project_id.clone(),
            project: self
                .projects
                .get(&thread.project_id)
                .map(|project| project.title.clone())
                .unwrap_or_default(),
            title: thread.title.clone(),
            branch: thread.branch.clone(),
            worktree: thread
                .worktree_path
                .as_deref()
                .is_some_and(|path| !path.trim().is_empty()),
            provider_instance: thread.provider_instance_id.clone(),
            top: TopStatus::of(
                thread.status,
                thread.blocked_on_thread_wait == Some(true),
                thread.last_error_class,
            ),
            pr: pr_badge(thread),
            section,
            model: thread
                .model_selection
                .model
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            instance_history: thread.provider_instance_history.clone().unwrap_or_default(),
            pr_lines: thread
                .pull_requests
                .as_ref()
                .map_or_else(Vec::new, |links| thread_card::pr_lines(&links.0)),
            last_error: thread
                .last_error
                .as_deref()
                .filter(|error| !error.is_empty())
                .map(|_| match thread.last_error_class {
                    Some(ThreadsEntryLastErrorClass::UsageLimit) => LastError::UsageLimit,
                    _ => LastError::Other,
                }),
            worktree_path: thread
                .worktree_path
                .clone()
                .filter(|path| !path.trim().is_empty()),
            has_run: thread.latest_run_id.is_some(),
        };
        let rows = SECTION_ORDER
            .into_iter()
            .zip(&sections)
            .flat_map(|(section, threads)| threads.iter().map(move |thread| (section, *thread)))
            .map(|(section, thread)| row(thread, section))
            .collect();
        ShellView {
            projects: self.project_count(),
            threads: self.thread_count(),
            project_entries: projects::build_entries(
                self.projects.values(),
                self.threads.values().filter(|thread| is_listed(thread)),
            ),
            rows,
        }
    }

    /// Applies one stream item. Returns whether the counts or sync marker may have changed.
    pub fn apply(&mut self, item: Item) -> bool {
        match item {
            // The completion marker carries nothing the counts need.
            Item::Variant0 { .. } => false,
            Item::Variant1 {
                snapshot,
                resolved_repository_identity_roots: resolved,
                ..
            } => {
                // `authoritative = item.resolvedRepositoryIdentityRoots === undefined` in
                // `shell.ts`; the server only sends non-empty root groups, so empty is absent.
                if !resolved.is_empty() {
                    self.merge_enrichment(snapshot.projects.0, &resolved);
                    return true;
                }
                // `retainRepositoryIdentity`: a refresh that lost the identity keeps the known one.
                let mut projects: HashMap<String, Project> = HashMap::new();
                for mut project in snapshot.projects.0 {
                    if project.repository_identity.is_none()
                        && let Some(prior) = self.projects.get(&project.id)
                        && prior.workspace_root == project.workspace_root
                    {
                        project.repository_identity = prior.repository_identity.clone();
                    }
                    projects.insert(project.id.clone(), project);
                }
                self.projects = projects;
                self.threads = snapshot
                    .threads
                    .0
                    .into_iter()
                    .map(|t| (t.id.clone(), t))
                    .collect();
                self.sequence = snapshot.snapshot_sequence;
                self.seeded = true;
                true
            }
            Item::Variant2 {
                sequence, project, ..
            } => {
                if !self.advance(sequence) {
                    return false;
                }
                self.projects.insert(project.id.clone(), project);
                true
            }
            Item::Variant3 {
                sequence,
                project_id,
                ..
            } => {
                if !self.advance(sequence) {
                    return false;
                }
                self.projects.remove(&project_id);
                true
            }
            Item::Variant4 {
                sequence,
                location,
                thread,
                ..
            } => {
                if !self.advance(sequence) {
                    return false;
                }
                match location {
                    Location::Active => {
                        self.threads.insert(thread.id.clone(), thread);
                    }
                    // Archived threads leave the normal shell; older servers still send these.
                    Location::Archive => {
                        self.threads.remove(&thread.id);
                    }
                }
                true
            }
            Item::Variant5 {
                sequence,
                thread_id,
                location,
                ..
            } => {
                if !self.advance(sequence) {
                    return false;
                }
                let _: RemovedLocation = location;
                self.threads.remove(&thread_id);
                true
            }
        }
    }

    /// `mergeShellSnapshotProjects` for an enrichment refresh: only repository identities change,
    /// on known projects with the same workspace root. Resolved roots take the identity exactly,
    /// others only fill a missing one. Threads, sequence and the project set are untouched.
    fn merge_enrichment(&mut self, refreshed: Vec<Project>, resolved: &[String]) {
        for candidate in refreshed {
            let Some(project) = self.projects.get_mut(&candidate.id) else {
                continue;
            };
            if project.workspace_root != candidate.workspace_root {
                continue;
            }
            if resolved.contains(&project.workspace_root)
                || (project.repository_identity.is_none()
                    && candidate.repository_identity.is_some())
            {
                project.repository_identity = candidate.repository_identity;
            }
        }
    }

    /// Events at or below the last applied sequence are stale.
    fn advance(&mut self, sequence: i64) -> bool {
        if sequence <= self.sequence {
            return false;
        }
        self.sequence = sequence;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    // Encoded with the contracts' own schema from packages/client-runtime's shell test fixtures.
    const SNAPSHOT: &str = include_str!("../tests/fixtures/shell-snapshot.json");
    const PROJECT_UPDATED: &str = include_str!("../tests/fixtures/shell-project-updated.json");
    const THREAD_UPDATED: &str = include_str!("../tests/fixtures/shell-thread-updated.json");

    fn item(text: &str) -> Item {
        serde_json::from_str(text).expect("fixture decodes")
    }

    fn with(text: &str, edit: impl FnOnce(&mut Value)) -> Item {
        let mut value: Value = serde_json::from_str(text).expect("json");
        edit(&mut value);
        serde_json::from_value(value).expect("edited fixture decodes")
    }

    fn seeded() -> ShellState {
        let mut state = ShellState::default();
        assert!(!state.is_seeded());
        state.apply(item(SNAPSHOT));
        state
    }

    #[test]
    fn snapshot_seeds_counts() {
        let state = seeded();
        assert!(state.is_seeded());
        assert_eq!((state.project_count(), state.thread_count()), (1, 1));
    }

    #[test]
    fn enrichment_refreshes_only_update_repository_identity() {
        let mut state = seeded();
        let key = |state: &ShellState| {
            state.projects["project-v2"]
                .repository_identity
                .as_ref()
                .map(|identity| identity.canonical_key.clone())
        };
        let refresh = |roots: &[&str], root: &str| {
            with(SNAPSHOT, |value| {
                value["resolvedRepositoryIdentityRoots"] = roots.into();
                let snapshot = &mut value["snapshot"];
                snapshot["threads"] = serde_json::json!([]);
                snapshot["snapshotSequence"] = 99.into();
                let project = &mut snapshot["projects"][0];
                project["workspaceRoot"] = root.into();
                project["repositoryIdentity"] = serde_json::json!({
                    "canonicalKey": "github.com/o/r", "locator": {"remoteName": "origin", "remoteUrl": "u", "source": "git-remote"}
                });
            })
        };
        // A different workspace root is not the same project.
        state.apply(refresh(&["/elsewhere"], "/elsewhere"));
        assert_eq!(key(&state), None);
        // An unresolved root only fills a missing identity.
        state.apply(refresh(&["/other"], "/workspace/project"));
        assert_eq!(key(&state).as_deref(), Some("github.com/o/r"));
        // The threads, project set and sequence survive every refresh.
        assert_eq!((state.thread_count(), state.project_count()), (1, 1));
        assert_eq!(state.sequence, 5);
        // A resolved root takes the identity exactly, including none.
        state.apply(with(SNAPSHOT, |value| {
            value["resolvedRepositoryIdentityRoots"] = serde_json::json!(["/workspace/project"]);
            value["snapshot"]["threads"] = serde_json::json!([]);
        }));
        assert_eq!(key(&state), None);
        assert_eq!(state.thread_count(), 1);
    }

    #[test]
    fn an_authoritative_snapshot_keeps_a_known_identity() {
        let mut state = seeded();
        state.apply(with(SNAPSHOT, |value| {
            value["resolvedRepositoryIdentityRoots"] = serde_json::json!(["/workspace/project"]);
            value["snapshot"]["projects"][0]["repositoryIdentity"] = serde_json::json!({
                "canonicalKey": "k", "locator": {"remoteName": "origin", "remoteUrl": "u", "source": "git-remote"}
            });
        }));
        state.apply(item(SNAPSHOT));
        assert!(state.projects["project-v2"].repository_identity.is_some());
    }

    #[test]
    fn updates_upsert_and_removes_delete() {
        let mut state = seeded();
        // A renamed existing project does not add a row.
        assert!(state.apply(item(PROJECT_UPDATED)));
        assert_eq!(state.project_count(), 1);
        assert!(state.apply(item(THREAD_UPDATED)));
        assert_eq!(state.thread_count(), 2);

        let removed_thread = serde_json::json!({
            "kind": "thread.removed", "sequence": 8, "location": "active", "threadId": "thread-two"
        });
        assert!(state.apply(serde_json::from_value(removed_thread).unwrap()));
        assert_eq!(state.thread_count(), 1);

        let removed_project = serde_json::json!({
            "kind": "project.removed", "sequence": 9, "projectId": "project-v2"
        });
        assert!(state.apply(serde_json::from_value(removed_project).unwrap()));
        assert_eq!(state.project_count(), 0);
    }

    const NOW: i64 = 1_800_000_000_000;

    fn thread(id: &str, edit: impl FnOnce(&mut Value)) -> Thread {
        let mut value: Value = serde_json::from_str(THREAD_UPDATED).expect("json");
        let mut thread = value["thread"].take();
        thread["id"] = id.into();
        edit(&mut thread);
        serde_json::from_value(thread).expect("thread decodes")
    }

    fn view_of(threads: Vec<Thread>) -> ShellView {
        let mut state = seeded();
        state.threads = threads.into_iter().map(|t| (t.id.clone(), t)).collect();
        state.view_at(NOW)
    }

    fn ids(view: &ShellView) -> Vec<&str> {
        view.rows.iter().map(|row| row.id.as_str()).collect()
    }

    fn set(thread: &mut Value, fields: &[(&str, Value)]) {
        for (key, value) in fields {
            thread[*key] = value.clone();
        }
    }

    #[test]
    fn subagents_and_side_chats_have_no_sidebar_row() {
        let view = view_of(vec![
            thread("top", |_| {}),
            thread("child", |t| {
                t["lineage"] = serde_json::json!({
                    "parentThreadId": "top", "relationshipToParent": "subagent", "rootThreadId": "top"
                });
                set(t, &[("settledOverride", "settled".into())]);
            }),
            thread("chat", |t| {
                t["lineage"] = serde_json::json!({
                    "parentThreadId": "top", "relationshipToParent": null,
                    "rootThreadId": "top", "sideChat": true
                });
                set(t, &[("pinnedAt", "2026-01-01T00:00:00.000Z".into())]);
            }),
            thread("forked", |t| {
                t["lineage"] = serde_json::json!({
                    "parentThreadId": "top", "relationshipToParent": "fork", "rootThreadId": "top"
                });
            }),
        ]);
        let mut shown = ids(&view);
        shown.sort_unstable();
        assert_eq!(shown, ["forked", "top"]);
    }

    #[test]
    fn the_pull_request_badge_follows_the_web() {
        let link = |number: u64, source: &str, state: Option<&str>| {
            let mut link = serde_json::json!({
                "host": "github.com", "linkedAt": "2026-01-01T00:00:00.000Z",
                "number": number, "repository": "o/r", "source": source,
                "url": format!("https://github.com/o/r/pull/{number}"),
                "snapshot": null, "stack": null,
            });
            if let Some(state) = state {
                link["snapshot"] = serde_json::json!({
                    "baseBranch": "main", "headBranch": "h", "isDraft": false, "state": state,
                    "syncedAt": "2026-01-01T00:00:00.000Z", "title": "t", "updatedAt": null,
                });
            }
            link
        };
        let badge = |links: Vec<Value>| {
            pr_badge(&thread("t", |t| set(t, &[("pullRequests", links.into())])))
                .map(|badge| (badge.state, badge.text))
        };
        assert_eq!(badge(vec![]), None);
        assert_eq!(
            badge(vec![link(7, "manual", None)]),
            Some((PrState::Pending, "7".to_owned()))
        );
        assert_eq!(
            badge(vec![link(7, "manual", Some("merged"))]),
            Some((PrState::Merged, "7".to_owned()))
        );
        assert_eq!(
            badge(vec![
                link(1, "manual", Some("merged")),
                link(2, "agent", Some("open")),
                link(3, "stack-dismissed", Some("closed")),
            ]),
            Some((PrState::Open, "+2".to_owned()))
        );
    }

    #[test]
    fn timestamps_parse_like_date_parse() {
        for (text, expected) in [
            ("1970-01-01T00:00:00.000Z", Some(0)),
            ("2000-03-01T00:00:00Z", Some(951_868_800_000)),
            ("2026-07-01T00:00:00.123Z", Some(1_782_864_000_123)),
            ("2026-07-01T02:00:00.5+02:00", Some(1_782_864_000_500)),
            ("2026-07-01", None),
            ("nope", None),
        ] {
            assert_eq!(timestamp_ms(text), expected, "{text}");
        }
    }

    #[test]
    fn an_updated_at_bump_does_not_reorder_active_threads() {
        let old = |bump: &str| {
            thread("old", |t| {
                set(
                    t,
                    &[
                        ("createdAt", "2026-01-01T00:00:00.000Z".into()),
                        ("updatedAt", bump.into()),
                    ],
                )
            })
        };
        let new = thread("new", |t| {
            set(t, &[("createdAt", "2026-02-01T00:00:00.000Z".into())])
        });
        for bump in ["2026-01-01T00:00:00.000Z", "2030-12-01T00:00:00.000Z"] {
            let view = view_of(vec![old(bump), new.clone()]);
            assert_eq!(ids(&view), ["new", "old"], "updatedAt {bump}");
        }
    }

    #[test]
    fn active_threads_sort_by_latest_anchor_and_ignore_the_order_key() {
        let at = |id: &str, fields: Vec<(&str, Value)>| thread(id, |t| set(t, &fields));
        let created = |m: &str| Value::from(format!("2026-{m}-01T00:00:00.000Z"));
        let view = view_of(vec![
            at("a-created", vec![("createdAt", created("01"))]),
            at(
                "b-unsettled",
                vec![("createdAt", created("01")), ("unsettledAt", created("05"))],
            ),
            at(
                "c-keyed",
                vec![("createdAt", created("02")), ("activeOrderKey", "a".into())],
            ),
            at(
                "d-run",
                vec![
                    ("createdAt", created("01")),
                    ("latestRunId", "run".into()),
                    ("status", "completed".into()),
                    ("latestRunRequestedAt", created("03")),
                    ("latestRunCompletedAt", created("04")),
                ],
            ),
            at("e-tie", vec![("createdAt", created("01"))]),
        ]);
        assert_eq!(
            ids(&view),
            ["b-unsettled", "d-run", "c-keyed", "a-created", "e-tie"]
        );
    }

    #[test]
    fn pinned_threads_sort_keyed_first_then_newest_created() {
        let pinned = |id: &str, key: Option<&str>, created: &str| {
            thread(id, |t| {
                set(
                    t,
                    &[
                        ("pinnedAt", "2026-03-01T00:00:00.000Z".into()),
                        ("createdAt", created.into()),
                    ],
                );
                if let Some(key) = key {
                    t["pinOrderKey"] = key.into();
                }
            })
        };
        let view = view_of(vec![
            pinned("keyless-old", None, "2026-01-01T00:00:00.000Z"),
            pinned("keyed-b", Some("b"), "2026-01-01T00:00:00.000Z"),
            pinned("keyless-new", None, "2026-02-01T00:00:00.000Z"),
            pinned("keyed-a", Some("a"), "2026-02-01T00:00:00.000Z"),
        ]);
        assert_eq!(
            ids(&view),
            ["keyed-a", "keyed-b", "keyless-new", "keyless-old"]
        );
        assert!(view.rows.iter().all(|r| r.section == Section::Pinned));
    }

    #[test]
    fn settled_threads_sort_by_settled_at_then_the_fallback_chain() {
        let settled = |id: &str, fields: Vec<(&str, Value)>| {
            thread(id, |t| {
                set(t, &[("settledOverride", "settled".into())]);
                set(t, &fields);
            })
        };
        let day = |d: u32| Value::from(format!("2026-05-{d:02}T00:00:00.000Z"));
        let view = view_of(vec![
            settled("settled-at", vec![("settledAt", day(20))]),
            settled("user-message", vec![("latestUserMessageAt", day(15))]),
            settled(
                "run-started",
                vec![
                    ("latestRunId", "r".into()),
                    ("status", "completed".into()),
                    ("latestRunStartedAt", day(18)),
                    ("latestRunCompletedAt", day(10)),
                ],
            ),
            settled("updated", vec![("updatedAt", day(12))]),
            settled("unparsable", vec![("updatedAt", "garbage".into())]),
        ]);
        assert_eq!(
            ids(&view),
            [
                "settled-at",
                "run-started",
                "user-message",
                "updated",
                "unparsable"
            ]
        );
        assert!(view.rows.iter().all(|r| r.section == Section::Settled));
    }

    #[test]
    fn sections_follow_snooze_then_settled_then_pinned_then_working() {
        let future = "2030-12-01T00:00:00.000Z";
        let running = |t: &mut Value| {
            set(
                t,
                &[
                    ("latestRunId", "run".into()),
                    ("activeRunId", "run".into()),
                    ("status", "running".into()),
                ],
            )
        };
        let view = view_of(vec![
            thread("idle", |_| {}),
            thread("working", running),
            thread("pinned-working", |t| {
                running(t);
                t["pinnedAt"] = "2026-03-01T00:00:00.000Z".into();
            }),
            thread("settled-pinned", |t| {
                set(
                    t,
                    &[
                        ("settledOverride", "settled".into()),
                        ("pinnedAt", "2026-03-01T00:00:00.000Z".into()),
                    ],
                )
            }),
            thread("snoozed", |t| {
                set(
                    t,
                    &[
                        ("snoozedUntil", future.into()),
                        ("snoozedAt", "2026-03-01T00:00:00.000Z".into()),
                    ],
                )
            }),
            thread("woken", |t| {
                t["snoozedUntil"] = "2026-01-01T00:00:00.000Z".into()
            }),
            thread("snoozed-with-approval", |t| {
                set(
                    t,
                    &[
                        ("snoozedUntil", future.into()),
                        (
                            "pendingRuntimeRequest",
                            serde_json::json!({"id": "q", "kind": "command", "createdAt": "2026-03-01T00:00:00.000Z"}),
                        ),
                    ],
                )
            }),
            thread("plan-ready", |t| {
                set(
                    t,
                    &[
                        ("interactionMode", "plan".into()),
                        ("hasActionableProposedPlan", true.into()),
                        ("latestRunId", "run".into()),
                        ("activeRunId", Value::Null),
                        ("status", "completed".into()),
                    ],
                )
            }),
        ]);
        let sections: HashMap<_, _> = view
            .rows
            .iter()
            .map(|row| (row.id.as_str(), row.section))
            .collect();
        assert_eq!(
            sections,
            HashMap::from([
                ("idle", Section::Active),
                ("working", Section::Working),
                ("pinned-working", Section::Pinned),
                ("settled-pinned", Section::Settled),
                ("snoozed", Section::Snoozed),
                ("woken", Section::Active),
                ("snoozed-with-approval", Section::Active),
                ("plan-ready", Section::Active),
            ])
        );
        let order: Vec<_> = view.rows.iter().map(|r| r.section).collect();
        assert!(order.is_sorted_by_key(|s| *s as u8));
    }

    #[test]
    fn working_threads_sort_by_latest_send_and_snoozed_by_wake() {
        let running = |id: &str, sent: &str| {
            thread(id, |t| {
                set(
                    t,
                    &[
                        ("latestRunId", "run".into()),
                        ("activeRunId", "run".into()),
                        ("status", "running".into()),
                        ("latestUserAuthoredMessageAt", sent.into()),
                    ],
                )
            })
        };
        let snoozed = |id: &str, until: &str| thread(id, |t| t["snoozedUntil"] = until.into());
        let view = view_of(vec![
            running("sent-early", "2026-08-01T00:00:00.000Z"),
            running("sent-late", "2026-09-01T00:00:00.000Z"),
            snoozed("wakes-late", "2030-02-01T00:00:00.000Z"),
            snoozed("wakes-soon", "2030-01-01T00:00:00.000Z"),
        ]);
        assert_eq!(
            ids(&view),
            ["sent-late", "sent-early", "wakes-soon", "wakes-late"]
        );
    }

    #[test]
    fn view_names_projects_and_counts_threads() {
        let mut state = seeded();
        state.apply(with(THREAD_UPDATED, |value| {
            value["thread"]["id"] = "thread-new".into();
            value["thread"]["createdAt"] = "2026-07-01T00:00:00.000Z".into();
        }));
        let view = state.view();
        assert_eq!((view.projects, view.threads), (1, 2));
        let titles: Vec<_> = view
            .project_entries
            .iter()
            .map(|e| e.title.as_str())
            .collect();
        assert_eq!(titles, ["Project"]);
        assert!(view.rows.iter().all(|row| row.project == "Project"));
    }

    #[test]
    fn new_project_id_adds_a_row() {
        let mut state = seeded();
        state.apply(with(PROJECT_UPDATED, |value| {
            value["project"]["id"] = "project-two".into()
        }));
        assert_eq!(state.project_count(), 2);
    }

    #[test]
    fn events_at_or_below_the_last_sequence_are_ignored() {
        let mut state = seeded();
        for sequence in [4, 5] {
            let stale = with(THREAD_UPDATED, |value| value["sequence"] = sequence.into());
            assert!(!state.apply(stale));
        }
        assert_eq!(state.thread_count(), 1);
        assert!(state.apply(item(THREAD_UPDATED)));
        // Replaying the same sequence is stale now.
        assert!(!state.apply(item(THREAD_UPDATED)));
        assert_eq!(state.thread_count(), 2);
    }

    #[test]
    fn archive_located_thread_updates_leave_the_shell() {
        let mut state = seeded();
        state.apply(item(THREAD_UPDATED));
        let archived = with(THREAD_UPDATED, |value| {
            value["sequence"] = 8.into();
            value["location"] = "archive".into();
        });
        assert!(state.apply(archived));
        assert_eq!(state.thread_count(), 1);
    }

    #[test]
    fn a_later_snapshot_replaces_everything() {
        let mut state = seeded();
        state.apply(item(THREAD_UPDATED));
        state.apply(item(SNAPSHOT));
        assert_eq!(state.thread_count(), 1);
    }
}
