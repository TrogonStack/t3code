use crate::details::VcsStatus;
use crate::diff;
use crate::projects::{EnvironmentProjects, MachineKind, ProjectEntry};
use crate::providers::Catalog;
use crate::strip::Branch;
use crate::terminal;
use crate::thread::Entry;
use crate::thread_card::PrLine;
use crate::wire::generated::{ThreadsEntryLastErrorClass, ThreadsEntryStatus};
use std::fmt;

/// What the window shows about the managed server and its socket.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    NotConnected,
    Starting,
    Connecting,
    /// Socket is up. `shell` stays `None` until the first shell snapshot lands.
    Connected {
        label: String,
        machine: MachineKind,
        /// The server keeps project pins, so the window offers them.
        can_pin: bool,
        shell: Option<ShellView>,
    },
    Failed(String),
    /// Another saved environment's projects; says nothing about the primary connection.
    Environment(EnvironmentProjects),
    /// The primary connection's provider instances, for the composer's model picker.
    Providers(Catalog),
    /// The selected thread's transcript, or why it could not be loaded.
    Thread {
        id: String,
        transcript: Result<Vec<Entry>, String>,
    },
    /// The branch checked out in a workspace root; `None` when it is not a repository.
    Branch {
        root: String,
        branch: Option<Branch>,
    },
    /// The version control status of a directory the details card watches; `None` when the
    /// subscription failed or ended.
    Vcs {
        cwd: String,
        status: Option<VcsStatus>,
    },
    /// One element of the terminal drawer's attach stream, for the thread it belongs to.
    Terminal {
        thread_id: String,
        event: terminal::Event,
    },
    /// The working-tree diff of a directory, or why it could not be loaded.
    Diff {
        cwd: String,
        diff: Result<diff::Preview, String>,
    },
}

/// Where the web sidebar lists a thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Pinned,
    Active,
    Working,
    Snoozed,
    Settled,
}

/// What the window needs from the shell state, `rows` in sidebar order: sections in
/// `Section` order, each sorted as the web sorts it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShellView {
    pub projects: usize,
    pub threads: usize,
    /// Unarchived projects in picker order, scratch included.
    pub project_entries: Vec<ProjectEntry>,
    pub rows: Vec<ThreadRow>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadRow {
    pub id: String,
    pub project_id: String,
    pub project: String,
    pub title: String,
    pub branch: Option<String>,
    pub worktree: bool,
    pub provider_instance: String,
    pub top: Option<TopStatus>,
    pub pr: Option<PrBadge>,
    pub section: Section,
    /// What the hover card adds: the stored model slug, past instances, pull requests, the error.
    pub model: String,
    pub instance_history: Vec<String>,
    pub pr_lines: Vec<PrLine>,
    pub last_error: Option<LastError>,
    /// The thread's own worktree, where its changes live; `None` works in the project root.
    pub worktree_path: Option<String>,
    /// Whether the thread has ever run, which un-quiets the details card's changes row.
    pub has_run: bool,
}

/// The card's error row: `usage_limit` is a warning, any other class an error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LastError {
    UsageLimit,
    Other,
}

/// A pull request's state as the badge tones it; `Pending` has no snapshot yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrState {
    Pending,
    Open,
    Draft,
    Closed,
    Merged,
}

/// The web's `ThreadPullRequestBadgePresentation`: the number, `+N` for several, or the layer
/// count of a stack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrBadge {
    pub state: PrState,
    pub stack: bool,
    pub text: String,
}

/// The status the web's thread card shows at the top right (`topStatus` in `SidebarThreadRow`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TopStatus {
    Working,
    Waiting,
    Limited,
    Failed,
}

impl TopStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Working => "Working",
            Self::Waiting => "Waiting",
            Self::Limited => "Limited",
            Self::Failed => "Failed",
        }
    }

    /// `resolveSidebarThreadStatus` without approvals, input and guest requests, which the row
    /// does not carry yet.
    pub fn of(
        status: ThreadsEntryStatus,
        blocked_on_thread_wait: bool,
        last_error_class: Option<ThreadsEntryLastErrorClass>,
    ) -> Option<Self> {
        use ThreadsEntryStatus as S;
        match status {
            S::Running if blocked_on_thread_wait => Some(Self::Waiting),
            S::Preparing | S::Queued | S::Starting | S::Running | S::Waiting => Some(Self::Working),
            S::Idle => Some(Self::Waiting),
            S::Failed if last_error_class == Some(ThreadsEntryLastErrorClass::UsageLimit) => {
                Some(Self::Limited)
            }
            S::Failed => Some(Self::Failed),
            _ => None,
        }
    }
}

fn plural(count: usize, noun: &str) -> String {
    if count == 1 {
        noun.to_owned()
    } else {
        format!("{noun}s")
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotConnected => f.write_str("Not connected"),
            Self::Starting => f.write_str("Starting server..."),
            Self::Connecting => f.write_str("Connecting..."),
            Self::Connected {
                label, shell: None, ..
            } => write!(f, "Connected to {label}"),
            Self::Connected {
                label,
                shell: Some(ShellView {
                    projects, threads, ..
                }),
                ..
            } => write!(
                f,
                "Connected to {label} \u{b7} {projects} {} \u{b7} {threads} {}",
                plural(*projects, "project"),
                plural(*threads, "thread"),
            ),
            Self::Failed(message) => f.write_str(message),
            Self::Environment(other) => write!(f, "Environment {}", other.label),
            Self::Providers(_) => f.write_str("Providers"),
            Self::Thread { id, .. } => write!(f, "Thread {id}"),
            Self::Branch { root, .. } => write!(f, "Branch of {root}"),
            Self::Vcs { cwd, .. } => write!(f, "Status of {cwd}"),
            Self::Terminal { thread_id, .. } => write!(f, "Terminal of {thread_id}"),
            Self::Diff { cwd, .. } => write!(f, "Diff of {cwd}"),
        }
    }
}

pub type StatusSender = async_channel::Sender<Status>;

pub fn publish(sender: &StatusSender, status: Status) {
    // Unbounded, so this only fails once the window is gone.
    let _ = sender.try_send(status);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_top_status_follows_the_web() {
        let of = |status, blocked, class| TopStatus::of(status, blocked, class);
        assert_eq!(
            of(ThreadsEntryStatus::Running, false, None),
            Some(TopStatus::Working)
        );
        assert_eq!(
            of(ThreadsEntryStatus::Running, true, None),
            Some(TopStatus::Waiting)
        );
        assert_eq!(
            of(ThreadsEntryStatus::Idle, false, None),
            Some(TopStatus::Waiting)
        );
        assert_eq!(
            of(
                ThreadsEntryStatus::Failed,
                false,
                Some(ThreadsEntryLastErrorClass::UsageLimit)
            ),
            Some(TopStatus::Limited)
        );
        assert_eq!(
            of(ThreadsEntryStatus::Failed, false, None),
            Some(TopStatus::Failed)
        );
        assert_eq!(of(ThreadsEntryStatus::Completed, false, None), None);
    }

    #[test]
    fn connected_text_includes_counts_once_known() {
        let label = "Local".to_owned();
        let waiting = Status::Connected {
            label: label.clone(),
            machine: MachineKind::Server,
            can_pin: false,
            shell: None,
        };
        assert_eq!(waiting.to_string(), "Connected to Local");
        let known = Status::Connected {
            label: label.clone(),
            machine: MachineKind::Server,
            can_pin: false,
            shell: Some(ShellView {
                projects: 2,
                threads: 14,
                ..Default::default()
            }),
        };
        assert_eq!(
            known.to_string(),
            "Connected to Local \u{b7} 2 projects \u{b7} 14 threads"
        );
        let single = Status::Connected {
            label,
            machine: MachineKind::Server,
            can_pin: false,
            shell: Some(ShellView {
                projects: 1,
                threads: 1,
                ..Default::default()
            }),
        };
        assert_eq!(
            single.to_string(),
            "Connected to Local \u{b7} 1 project \u{b7} 1 thread"
        );
    }
}
