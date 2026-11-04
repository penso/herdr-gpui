//! The native views of `herdr-acp` chats over their panes.
use super::app::Switch;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum ChatOp {
    Status,
    /// Opens a tab running a `herdr-acp` chat with a built-in agent, by name.
    New {
        agent: String,
        /// The pane whose workspace and folder it starts in, else the
        /// caller's own, else the focused one.
        #[serde(default)]
        pane: Option<String>,
    },
    /// Shows the chat view over a pane (`on`), or its terminal (`off`).
    View {
        switch: Switch,
        /// The pane, else the caller's own, else the focused one.
        #[serde(default)]
        pane: Option<String>,
    },
}

/// Who owns input in the conversation pane.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputMode {
    #[default]
    Chat,
    /// The running CLI's original editor is visible in the same pane.
    Terminal,
    ReadOnly,
}

/// One chat pane of the window's selected host.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatInfo {
    pub pane_id: String,
    pub agent: String,
    /// "starting", "idle", "working", "blocked", "failed", or "exited".
    pub status: String,
    /// Connected to the program now, rather than reconnecting.
    pub attached: bool,
    /// The chat view stands over the pane; otherwise its terminal shows.
    pub shown: bool,
    /// Permission requests waiting for an answer.
    pub permissions: usize,
    /// Prompts waiting for the running turn.
    pub queued: usize,
    /// The model the agent says it uses, by name.
    #[serde(default)]
    pub model: Option<String>,
    /// How full the context window is, in percent, once the agent says.
    #[serde(default)]
    pub context: Option<u8>,
    #[serde(default)]
    pub input_mode: InputMode,
    #[serde(default)]
    pub session_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Reply {
    Chats {
        chats: Vec<ChatInfo>,
    },
    View {
        pane_id: String,
        shown: bool,
    },
    /// The chat's tab was asked for; it attaches once its program reports.
    Started {
        agent: String,
    },
}

#[cfg(test)]
mod tests;
