//! What did not carry over in a move that otherwise finished. `Display` is
//! English, for logs; the app shows each warning by its variant.

use super::{Error, launch::AgentKind};

#[derive(Debug, thiserror::Error)]
pub enum Warning {
    // keep-sorted start
    /// The agent's session did not move, so it started without it.
    #[error("{} started afresh: {source}", agent.binary())]
    AgentStartedAfresh {
        agent: AgentKind,
        #[source]
        source: Error,
    },
    /// The repository tracks `.envrc`, so the token was not added to it.
    #[error(
        "The repository tracks .envrc, so GH_TOKEN was not added; git pull and push still work"
    )]
    EnvrcTracked,
    #[error("GitHub access was not set up: {source}")]
    GitHubAccess {
        #[source]
        source: Error,
    },
    /// An agent wrote no handoff note. `pane` names it when a move carries
    /// several; a pane moved on its own leaves it out.
    #[error("No handoff note{}: {source}", from(pane.as_deref()))]
    NoHandoffNote {
        pane: Option<String>,
        #[source]
        source: Error,
    },
    /// A pane's program is not installed on the destination, which skipped it.
    #[error("{program} is not installed on {host}")]
    ProgramMissing { program: String, host: String },
    #[error("The source pane stayed open: {source}")]
    SourcePaneOpen {
        #[source]
        source: Error,
    },
    // keep-sorted end
}

/// " from <pane>" for a warning's log line, when it names a pane.
fn from(pane: Option<&str>) -> String {
    pane.map(|pane| format!(" from {pane}")).unwrap_or_default()
}
