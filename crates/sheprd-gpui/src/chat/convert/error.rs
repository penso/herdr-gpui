use sheprd_chat_protocol::Agent;

/// Why an agent did not move into a chat. Each one before the agent is told
/// to stop leaves it running; [`Self::Stale`], [`Self::Stopped`], and
/// [`Self::Type`] can come after, and leave its session to continue by hand.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("the pane already runs a chat")]
    AlreadyChat,
    #[error("another agent is still moving into a chat")]
    AlreadyConverting,
    #[error("could not read the pane's processes")]
    Answer(#[source] serde_json::Error),
    #[error("{} is busy; wait until it is idle", .0.display_name())]
    Busy(Agent),
    #[error("ACP chats are off in Settings")]
    Disabled,
    #[error(transparent)]
    Existing(#[from] super::existing::Error),
    #[error("sheprd-acp was not found on this host")]
    HostProgram(#[source] super::existing::Error),
    #[error("the session's folder or flags cannot be typed safely")]
    Line,
    #[error("the pane changed before its agent could stop")]
    Moved,
    #[error("the pane runs no agent")]
    NoAgent,
    #[error(
        "Herdr reports no session for {} in this pane; run `herdr integration install {}`, then restart the agent",
        .0.display_name(),
        .0.name()
    )]
    NoSession(Agent),
    #[error("Sheprd is not connected")]
    NotConnected,
    #[error("agents move into chats on this machine's Herdr or an SSH host's, on macOS and Linux")]
    NotReachable,
    #[error("{} is not the program running in this pane", .0.display_name())]
    NotRunning(Agent),
    #[error(transparent)]
    Program(#[from] crate::chat::StartError),
    #[error("the window moved to another connection before the chat could start")]
    Stale,
    #[error("{} did not stop in time; it is still in its terminal", .0.display_name())]
    StillRunning(Agent),
    #[error("the worker stopped before the agent moved")]
    Stopped,
    #[error("{0}")]
    Type(#[source] std::sync::Arc<crate::Error>),
    #[error("`{0}` is not an agent Sheprd can chat with")]
    Unknown(String),
    #[error("{} cannot move into a chat yet", .0.display_name())]
    Unsupported(Agent),
    #[error("could not start the worker")]
    Worker(#[source] std::io::Error),
    // keep-sorted end
}
