//! The shell read model built from `orchestration.subscribeShell`, mirroring
//! `packages/client-runtime/src/state/shellReducer.ts` for what this client shows.

use crate::projects;
use crate::status::{ShellView, ThreadRow};
use crate::wire::generated::{
    OrchestrationSubscribeShellSuccess as Item,
    OrchestrationSubscribeShellSuccessVariant4Location as Location,
    OrchestrationSubscribeShellSuccessVariant5Location as RemovedLocation,
};
use crate::wire::{Project, Thread};
use std::collections::HashMap;

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

    /// Threads newest first (ties by id); the first project is the oldest by creation.
    pub fn view(&self) -> ShellView {
        let mut rows: Vec<ThreadRow> = self
            .threads
            .values()
            .map(|thread| ThreadRow {
                id: thread.id.clone(),
                project_id: thread.project_id.clone(),
                project: self
                    .projects
                    .get(&thread.project_id)
                    .map(|project| project.title.clone())
                    .unwrap_or_default(),
                title: thread.title.clone(),
                branch: thread.branch.clone(),
                status: thread.status,
                updated_at: thread.updated_at.clone(),
            })
            .collect();
        rows.sort_by(|a, b| {
            b.updated_at
                .cmp(&a.updated_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        ShellView {
            projects: self.project_count(),
            threads: self.thread_count(),
            project_entries: projects::build_entries(self.projects.values(), self.threads.values()),
            rows,
        }
    }

    /// Applies one stream item. Returns whether the counts or sync marker may have changed.
    pub fn apply(&mut self, item: Item) -> bool {
        match item {
            // The completion marker carries nothing the counts need.
            Item::Variant0 { .. } => false,
            Item::Variant1 { snapshot, .. } => {
                self.projects = snapshot
                    .projects
                    .0
                    .into_iter()
                    .map(|p| (p.id.clone(), p))
                    .collect();
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

    #[test]
    fn view_lists_newest_threads_first_with_project_names() {
        let mut state = seeded();
        state.apply(with(THREAD_UPDATED, |value| {
            value["thread"]["id"] = "thread-new".into();
            value["thread"]["updatedAt"] = "2026-07-01T00:00:00.000Z".into();
        }));
        let view = state.view();
        assert_eq!((view.projects, view.threads), (1, 2));
        let titles: Vec<_> = view
            .project_entries
            .iter()
            .map(|e| e.title.as_str())
            .collect();
        assert_eq!(titles, ["Project"]);
        let ids: Vec<_> = view.rows.iter().map(|row| row.id.as_str()).collect();
        assert_eq!(ids, ["thread-new", "thread-v2"]);
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
