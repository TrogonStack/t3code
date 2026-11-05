use std::fmt;

/// What the window shows about the managed server and its socket.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    NotConnected,
    Starting,
    Connecting,
    /// Socket is up. Counts stay `None` until the first shell snapshot lands.
    Connected {
        label: String,
        counts: Option<Counts>,
    },
    Failed(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Counts {
    pub projects: usize,
    pub threads: usize,
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
                label,
                counts: None,
            } => write!(f, "Connected to {label}"),
            Self::Connected {
                label,
                counts: Some(Counts { projects, threads }),
            } => write!(
                f,
                "Connected to {label} \u{b7} {projects} {} \u{b7} {threads} {}",
                plural(*projects, "project"),
                plural(*threads, "thread"),
            ),
            Self::Failed(message) => f.write_str(message),
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
            counts: None,
        };
        assert_eq!(waiting.to_string(), "Connected to Local");
        let known = Status::Connected {
            label: label.clone(),
            counts: Some(Counts {
                projects: 2,
                threads: 14,
            }),
        };
        assert_eq!(
            known.to_string(),
            "Connected to Local \u{b7} 2 projects \u{b7} 14 threads"
        );
        let single = Status::Connected {
            label,
            counts: Some(Counts {
                projects: 1,
                threads: 1,
            }),
        };
        assert_eq!(
            single.to_string(),
            "Connected to Local \u{b7} 1 project \u{b7} 1 thread"
        );
    }
}
