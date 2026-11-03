//! Worktree notes, and the work flows that end in a dialog of the app's own:
//! restoring a checkpoint, running a worktree script, teleporting, and
//! fanning a prompt out. A command opens the flow already pointed at the
//! caller's checkout; the user still confirms it in the app.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkOp {
    /// Every worktree note, most recently edited first.
    Notes,
    /// The note on the caller's checkout.
    Note,
    /// Replace the caller's checkout's note; empty text clears it.
    SetNote { text: String },
    /// Open the checkpoints of the caller's checkout, with `id` chosen to
    /// restore once the user confirms.
    Restore {
        #[serde(default)]
        id: Option<String>,
    },
    /// Run one of the caller's repository's worktree scripts in a new tab.
    /// An untrusted script still asks the user first.
    Script { kind: Script },
    /// Open the teleport dialog for the caller's worktree, reviewing `host`.
    Teleport { host: String },
    /// Open the fan-out dialog with this prompt and these agents.
    FanOut(FanOutRequest),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Script {
    Setup,
    Run,
    Archive,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FanOutRequest {
    pub prompt: String,
    /// Agent kinds by name (`claude`, `codex`, ...), each with how many.
    pub agents: Vec<(String, u8)>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteInfo {
    pub host: String,
    pub repository: String,
    pub branch: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Reply {
    /// The most recent notes that fit one answer; `omitted` older ones did not.
    Notes {
        notes: Vec<NoteInfo>,
        #[serde(default)]
        omitted: usize,
    },
    Note {
        note: Option<NoteInfo>,
    },
}

#[cfg(test)]
mod tests;
