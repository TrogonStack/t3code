use crate::projects::{EnvironmentProjects, ProjectEntry};
use crate::wire::generated::ThreadsEntryStatus;
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
        /// The server keeps project pins, so the window offers them.
        can_pin: bool,
        shell: Option<ShellView>,
    },
    Failed(String),
    /// Another saved environment's projects; says nothing about the primary connection.
    Environment(EnvironmentProjects),
}

/// What the window needs from the shell state, newest thread first.
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
    pub status: ThreadsEntryStatus,
    pub updated_at: String,
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
    fn connected_text_includes_counts_once_known() {
        let label = "Local".to_owned();
        let waiting = Status::Connected {
            label: label.clone(),
            can_pin: false,
            shell: None,
        };
        assert_eq!(waiting.to_string(), "Connected to Local");
        let known = Status::Connected {
            label: label.clone(),
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
