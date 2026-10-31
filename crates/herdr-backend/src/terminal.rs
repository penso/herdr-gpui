use crate::{AgentStatus, CursorState, Error, FrameData, PaneSurfacePatchRow, Result};
use std::{fmt, path::PathBuf};

/// The longest side a terminal may have, matching the app's viewport clamp.
pub const MAX_SIDE: u16 = 4096;
/// The most cells a screen may hold, matching the app's viewport clamp.
pub const MAX_CELLS: u32 = 1_000_000;

/// A terminal's size in cells: never empty, at most [`MAX_SIDE`] on a side
/// and [`MAX_CELLS`] in all.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Size {
    cols: u16,
    rows: u16,
}

impl Size {
    pub fn new(cols: u16, rows: u16) -> Result<Self> {
        if cols == 0 || rows == 0 {
            return Err(Error::EmptySize);
        }
        if cols > MAX_SIDE || rows > MAX_SIDE || u32::from(cols) * u32::from(rows) > MAX_CELLS {
            return Err(Error::LargeSize { cols, rows });
        }
        Ok(Self { cols, rows })
    }

    pub fn cols(self) -> u16 {
        self.cols
    }

    pub fn rows(self) -> u16 {
        self.rows
    }
}

/// The longest terminal ID accepted, in bytes.
pub const MAX_TERMINAL_ID: usize = 256;

/// How the backend names a terminal: a Herdr 1 pane ID, or a herdr-core
/// address such as `t7` or `box/t7`. Saved IDs are checked again on load.
#[derive(
    Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(try_from = "String", into = "String")]
pub struct TerminalId(String);

impl TerminalId {
    pub fn new(id: impl Into<String>) -> Result<Self> {
        let id = id.into();
        if id.is_empty() {
            return Err(Error::EmptyTerminalId);
        }
        if id.len() > MAX_TERMINAL_ID || id.chars().any(char::is_control) {
            return Err(Error::InvalidTerminalId);
        }
        Ok(Self(id))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for TerminalId {
    type Error = Error;

    fn try_from(id: String) -> Result<Self> {
        Self::new(id)
    }
}

impl From<TerminalId> for String {
    fn from(id: TerminalId) -> Self {
        id.0
    }
}

impl AsRef<str> for TerminalId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TerminalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// What the app shows about a terminal without attaching to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalInfo {
    pub agent: AgentStatus,
    pub cwd: Option<PathBuf>,
    pub id: TerminalId,
    pub name: Option<String>,
    pub state: RunState,
    pub title: Option<String>,
}

/// Whether a terminal's program still runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunState {
    Running,
    /// The program is gone but the terminal is kept for a fresh shell.
    Lost,
    Exited(Exit),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Exit {
    /// None when a signal or a kill ended it.
    pub code: Option<i32>,
}

/// One change after a watch snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// A terminal appeared or its facts changed; replace any earlier record.
    Upsert(TerminalInfo),
    Removed(TerminalId),
    /// Changes were missed: this is every terminal now.
    Resync(Vec<TerminalInfo>),
}

/// Whether an attached view drives the terminal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Role {
    /// Types, resizes, and reports focus.
    #[default]
    Active,
    /// Watches only, such as a preview beside the terminal's own pane.
    Observer,
}

/// A terminal to start.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Spawn {
    /// The program and its arguments; empty for the host's shell.
    pub argv: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub env: Vec<(String, String)>,
    pub name: Option<String>,
}

/// Key events a program asked for, as the kitty keyboard protocol lets it:
/// the default asks for neither, and typing arrives as text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyReporting {
    /// Every key, printable ones too, as a key event rather than text.
    pub every_key: bool,
    /// Releases as well as presses.
    pub releases: bool,
}

/// What an attached terminal shows now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SurfaceUpdate {
    Bell,
    /// The program ended; no further updates follow.
    Ended(Exit),
    /// The whole visible screen.
    Frame(FrameData),
    /// Which key events the program asked for beyond typed text.
    KeyReporting(KeyReporting),
    /// Whether the program reads the mouse: presses and drags go to it,
    /// rather than selecting text, until it stops.
    MouseReporting(bool),
    /// Rows changed since the last update, and where the cursor is now.
    Rows {
        cursor: Option<CursorState>,
        rows: Vec<PaneSurfacePatchRow>,
    },
    Title(String),
}
