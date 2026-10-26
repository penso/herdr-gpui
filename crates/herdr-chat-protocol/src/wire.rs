//! Messages between the chat program and an attached client.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{Attachment, Change, MAX_ATTACHMENTS, OptionValue, PermissionId, Transcript};

/// The attach protocol version; a client and program must agree exactly.
pub const VERSION: u32 = 1;

/// The longest prompt a client may send, in bytes.
pub const MAX_PROMPT_SIZE: usize = 256 * 1024;

/// A position in one run's change stream.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct Cursor {
    /// Changes whenever the program starts or reloads its session.
    pub epoch: Uuid,
    /// The number of changes applied since the epoch began.
    pub seq: u64,
}

impl Cursor {
    /// The start of a new epoch: a history no earlier cursor belongs to, so
    /// every client replaces what it holds rather than merging into it.
    pub fn new_epoch() -> Self {
        Self {
            epoch: Uuid::new_v4(),
            seq: 0,
        }
    }
}

/// A client's identity for one command, kept across reconnects so a resend is
/// recognised.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CommandId(pub Uuid);

impl CommandId {
    pub fn random() -> Self {
        Self(Uuid::new_v4())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientKind {
    /// Herdr UI.
    Gui,
    /// A command-line client, such as a script.
    Cli,
    /// The remote control daemon, on behalf of paired phones.
    Remote,
    #[serde(other)]
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ClientMessage {
    /// The first message on a connection.
    Hello {
        client: ClientKind,
        /// The last cursor this client applied, to resume without a snapshot.
        #[serde(default)]
        since: Option<Cursor>,
        version: u32,
    },
    Command {
        command: Command,
        id: CommandId,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "command")]
pub enum Command {
    /// Sends a prompt now, or queues it behind the running turn.
    Prompt {
        text: String,
        /// Files sent with it, at most [`MAX_ATTACHMENTS`].
        #[serde(default)]
        attachments: Vec<Attachment>,
    },
    /// Ends the running turn, declines its pending permission requests, and
    /// drops queued prompts.
    Cancel,
    Answer {
        option: String,
        permission: PermissionId,
    },
    SetMode {
        id: String,
    },
    /// Changes a setting, such as the model or the thinking level.
    SetOption {
        id: String,
        value: OptionValue,
    },
    /// Withdraws a queued prompt before it is sent.
    Unqueue {
        id: CommandId,
    },
    /// Answers permission requests with their allow option from now on,
    /// including those waiting, or goes back to asking.
    SetAutoAccept {
        on: bool,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ServerMessage {
    /// The first message on a connection.
    Welcome {
        agent: String,
        /// The Herdr pane the program runs in, when it runs in one.
        #[serde(default)]
        pane: Option<String>,
        version: u32,
    },
    /// The whole transcript as of `cursor`; replaces whatever the client had.
    Snapshot {
        cursor: Cursor,
        transcript: Box<Transcript>,
    },
    /// One change, which brings the client to `cursor`.
    Change { change: Change, cursor: Cursor },
    /// The outcome of one command.
    Ack {
        id: CommandId,
        #[serde(default)]
        rejected: Option<Rejection>,
    },
    /// The program closes the connection after this message.
    Bye { reason: ByeReason },
}

/// Why a command did nothing.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum Rejection {
    #[error("the prompt is empty")]
    EmptyPrompt,
    #[error("the prompt is longer than the program accepts")]
    PromptTooLong,
    #[error("the agent cannot take prompts")]
    NotReady,
    #[error("no turn is running")]
    NotWorking,
    #[error("that permission request was already answered")]
    UnknownPermission,
    #[error("that is not one of the request's options")]
    UnknownOption,
    #[error("the agent has no such mode")]
    UnknownMode,
    #[error("the agent has no such setting, or it does not take that value")]
    UnknownSetting,
    #[error("that prompt is no longer queued")]
    UnknownQueued,
    #[error("the program could not deliver the command to the agent")]
    AgentUnavailable,
    #[error("a prompt takes at most {MAX_ATTACHMENTS} files")]
    TooManyAttachments,
    #[error("too many prompts are waiting; wait for the turn to end")]
    QueueFull,
    #[error("an attached file does not exist or cannot be read")]
    UnknownFile,
    /// A command this program's version does not know, from a newer client.
    #[error("this chat program is older than Herdr UI; start a new chat to use that")]
    Unsupported,
    #[serde(other)]
    #[error("the program refused the command")]
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "reason")]
pub enum ByeReason {
    /// The client speaks another protocol version.
    Version { supported: u32 },
    /// The program is exiting.
    Exiting,
    /// The first message was not `Hello`.
    Handshake,
}
