use std::fmt;

/// What the window shows about the managed server and its socket.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    NotConnected,
    Starting,
    Connecting,
    Connected(String),
    Failed(String),
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotConnected => f.write_str("Not connected"),
            Self::Starting => f.write_str("Starting server..."),
            Self::Connecting => f.write_str("Connecting..."),
            Self::Connected(label) => write!(f, "Connected to {label}"),
            Self::Failed(message) => f.write_str(message),
        }
    }
}

pub type StatusSender = async_channel::Sender<Status>;

pub fn publish(sender: &StatusSender, status: Status) {
    // Unbounded, so this only fails once the window is gone.
    let _ = sender.try_send(status);
}
