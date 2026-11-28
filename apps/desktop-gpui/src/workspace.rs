//! What the composer's Workspace menu offers and does: `BranchToolbar.logic.ts` (the previous
//! worktree seed and the labels), `BranchToolbarEnvModeSelector.tsx` (the rows and the trigger)
//! and `useComposerSelection.ts` `onEnvModeChange` (what each row changes in the draft).

use crate::strip::EnvMode;

/// `PreviousWorktreeSeed`: the worktree the Previous worktree row reuses, and its branch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seed {
    pub branch: Option<String>,
    pub worktree_path: String,
}

/// The facts about one thread `resolvePreviousWorktreeSeed` reads. Archived threads are not in
/// the shell, which is how the web's `archivedAt` filter holds here.
#[derive(Clone, Copy, Debug)]
pub struct Thread<'a> {
    pub branch: Option<&'a str>,
    pub worktree_path: Option<&'a str>,
    /// `None` when the timestamp does not parse, which skips the thread.
    pub updated_at_ms: Option<i64>,
}

/// `resolvePreviousWorktreeSeed`: the most recently updated thread's worktree other than the
/// draft's own; on a tie the earlier thread wins.
pub fn previous_seed<'a>(
    threads: impl IntoIterator<Item = Thread<'a>>,
    current_worktree: Option<&str>,
) -> Option<Seed> {
    let mut latest: Option<(Seed, i64)> = None;
    for thread in threads {
        let Some(path) = thread.worktree_path.filter(|path| !path.is_empty()) else {
            continue;
        };
        if Some(path) == current_worktree {
            continue;
        }
        let Some(updated) = thread.updated_at_ms else {
            continue;
        };
        if latest.as_ref().is_none_or(|(_, at)| updated > *at) {
            latest = Some((
                Seed {
                    branch: thread.branch.map(str::to_owned),
                    worktree_path: path.to_owned(),
                },
                updated,
            ));
        }
    }
    latest.map(|(seed, _)| seed)
}

/// A row of the menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Current,
    NewWorktree,
    Previous,
}

/// The icons the menu and its trigger use (lucide `Folder`, `FolderGit`, `FolderGit2`, `History`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyph {
    Folder,
    FolderGit,
    FolderGit2,
    History,
}

/// The workspace a draft is set up with: the mode, and the worktree it already has, which only
/// the Previous worktree row sets.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Draft {
    pub mode: EnvMode,
    pub worktree: Option<Seed>,
}

impl Draft {
    /// The menu's rows, in the web's order; Previous worktree only when there is one to offer.
    pub fn rows(has_previous: bool) -> Vec<Choice> {
        let mut rows = vec![Choice::Current, Choice::NewWorktree];
        if has_previous {
            rows.push(Choice::Previous);
        }
        rows
    }

    /// The row the select's value names: the effective mode, never Previous worktree.
    pub fn selected(&self) -> Choice {
        match self.mode {
            EnvMode::Worktree => Choice::NewWorktree,
            EnvMode::Local => Choice::Current,
        }
    }

    /// `resolveCurrentWorkspaceLabel`.
    pub fn current_label(&self) -> &'static str {
        if self.worktree.is_some() {
            "Current worktree"
        } else {
            EnvMode::Local.label()
        }
    }

    /// The label of a row, other than Previous worktree's second line.
    pub fn label(&self, choice: Choice) -> &'static str {
        match choice {
            Choice::Current => self.current_label(),
            Choice::NewWorktree => EnvMode::Worktree.label(),
            Choice::Previous => "Previous worktree",
        }
    }

    pub fn glyph(&self, choice: Choice) -> Glyph {
        match choice {
            Choice::Current if self.worktree.is_some() => Glyph::FolderGit,
            Choice::Current => Glyph::Folder,
            Choice::NewWorktree => Glyph::FolderGit2,
            Choice::Previous => Glyph::History,
        }
    }

    /// What the trigger shows: the effective mode's label and icon.
    pub fn trigger(&self) -> (Glyph, &'static str) {
        let shown = self.selected();
        (self.glyph(shown), self.label(shown))
    }

    /// The draft after a row is chosen (`onEnvModeChange`, `onUsePreviousWorktree`). A new
    /// worktree drops the draft's worktree, the current checkout keeps it, and Previous worktree
    /// adopts the seed in worktree mode; without a seed nothing changes.
    pub fn choose(&self, choice: Choice, previous: Option<&Seed>) -> Self {
        match choice {
            Choice::Current => Self {
                mode: EnvMode::Local,
                worktree: self.worktree.clone(),
            },
            Choice::NewWorktree => Self {
                mode: EnvMode::Worktree,
                worktree: None,
            },
            Choice::Previous => match previous {
                Some(seed) => Self {
                    mode: EnvMode::Worktree,
                    worktree: Some(seed.clone()),
                },
                None => self.clone(),
            },
        }
    }

    /// The ref the branch control names while the draft has a worktree: its branch, as
    /// `resolveBranchTriggerLabel` returns it plain.
    pub fn branch_label(&self) -> Option<String> {
        let seed = self.worktree.as_ref()?;
        Some(
            seed.branch
                .clone()
                .unwrap_or_else(|| "Select ref".to_owned()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thread<'a>(branch: &'a str, path: Option<&'a str>, at: Option<i64>) -> Thread<'a> {
        Thread {
            branch: Some(branch),
            worktree_path: path,
            updated_at_ms: at,
        }
    }

    fn seed(branch: Option<&str>, path: &str) -> Seed {
        Seed {
            branch: branch.map(str::to_owned),
            worktree_path: path.to_owned(),
        }
    }

    #[test]
    fn the_seed_is_the_latest_other_worktree() {
        let threads = [
            thread("a", Some("/w/a"), Some(10)),
            thread("b", Some("/w/b"), Some(30)),
            thread("c", Some("/w/c"), Some(20)),
            thread("root", None, Some(99)),
            thread("blank", Some(""), Some(99)),
            thread("bad-time", Some("/w/d"), None),
        ];
        assert_eq!(previous_seed(threads, None), Some(seed(Some("b"), "/w/b")));
        assert_eq!(
            previous_seed(threads, Some("/w/b")),
            Some(seed(Some("c"), "/w/c"))
        );
        assert_eq!(previous_seed(threads[3..].iter().copied(), None), None);
        assert_eq!(previous_seed([], None), None);
    }

    #[test]
    fn a_tie_keeps_the_earlier_thread() {
        let threads = [
            thread("first", Some("/w/1"), Some(5)),
            thread("second", Some("/w/2"), Some(5)),
        ];
        assert_eq!(
            previous_seed(threads, None),
            Some(seed(Some("first"), "/w/1"))
        );
    }

    #[test]
    fn a_seed_without_a_branch_is_still_offered() {
        let threads = [Thread {
            branch: None,
            worktree_path: Some("/w/a"),
            updated_at_ms: Some(1),
        }];
        assert_eq!(previous_seed(threads, None), Some(seed(None, "/w/a")));
    }

    #[test]
    fn rows_offer_previous_worktree_last_and_only_with_a_seed() {
        assert_eq!(Draft::rows(false), [Choice::Current, Choice::NewWorktree]);
        assert_eq!(
            Draft::rows(true),
            [Choice::Current, Choice::NewWorktree, Choice::Previous]
        );
    }

    #[test]
    fn choosing_follows_on_env_mode_change() {
        let previous = seed(Some("feature"), "/w/a");
        let fresh = Draft::default();
        assert_eq!(fresh.trigger(), (Glyph::Folder, "Current checkout"));

        let reused = fresh.choose(Choice::Previous, Some(&previous));
        assert_eq!(reused.mode, EnvMode::Worktree);
        assert_eq!(reused.worktree, Some(previous.clone()));
        // The select's value is the mode, so the trigger still reads New worktree.
        assert_eq!(reused.trigger(), (Glyph::FolderGit2, "New worktree"));
        assert_eq!(reused.branch_label(), Some("feature".to_owned()));

        // The current checkout keeps the worktree and renames itself.
        let current = reused.choose(Choice::Current, None);
        assert_eq!(current.mode, EnvMode::Local);
        assert_eq!(current.worktree, Some(previous));
        assert_eq!(current.trigger(), (Glyph::FolderGit, "Current worktree"));
        assert_eq!(current.label(Choice::Current), "Current worktree");
        assert_eq!(current.glyph(Choice::Current), Glyph::FolderGit);

        // A new worktree drops it again.
        let cleared = current.choose(Choice::NewWorktree, None);
        assert_eq!(
            cleared,
            Draft {
                mode: EnvMode::Worktree,
                worktree: None
            }
        );
        assert_eq!(cleared.branch_label(), None);
    }

    #[test]
    fn previous_worktree_without_a_seed_changes_nothing() {
        let draft = Draft::default();
        assert_eq!(draft.choose(Choice::Previous, None), draft);
    }

    #[test]
    fn a_seedless_branch_reads_as_an_unselected_ref() {
        let draft = Draft {
            mode: EnvMode::Local,
            worktree: Some(seed(None, "/w/a")),
        };
        assert_eq!(draft.branch_label(), Some("Select ref".to_owned()));
    }
}
