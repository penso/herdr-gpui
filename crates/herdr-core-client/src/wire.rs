//! The records herdr-core sends. Its API only grows, so unknown fields are
//! ignored and new words land in an `Other` variant rather than failing.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Terminal {
    /// `t7` here, `box/t7` on a machine this core added.
    pub address: String,
    #[serde(default)]
    pub argv: Vec<String>,
    #[serde(default)]
    pub cols: u16,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub foreground: Option<Foreground>,
    #[serde(default)]
    pub keep: bool,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub pid: Option<i32>,
    /// Why it ended: `exited`, `killed`, `server_stop`, ...
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub rows: u16,
    pub state: TerminalState,
    #[serde(default)]
    pub status: Option<Status>,
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalState {
    Finished,
    Live,
    #[serde(other)]
    Other,
}

/// What runs in a terminal now: its foreground process group's leader.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Foreground {
    #[serde(default)]
    pub argv: Vec<String>,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub name: String,
    pub pid: i32,
}

/// One OSC 7501 report, in the program's own words.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Status {
    #[serde(default)]
    pub app: String,
    /// `permission`, `question`, or `auth`, when blocked.
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub msg: String,
    #[serde(default)]
    pub progress: Option<u8>,
    pub state: StatusState,
    #[serde(default)]
    pub title: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusState {
    Blocked,
    Done,
    Error,
    Idle,
    Working,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct List {
    #[serde(default)]
    pub hidden_finished: u64,
    pub terminals: Vec<Terminal>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Subscribed {
    #[serde(default)]
    pub channel: Option<u32>,
    pub seq: u64,
    pub terminals: Vec<Terminal>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Attached {
    pub channel: u32,
    pub terminal: Terminal,
}

/// A stored document and the version to write over it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Doc {
    #[serde(default)]
    pub value: Value,
    /// Zero for a name never written.
    pub version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct DocSet {
    pub version: u64,
}

/// What the viewer looks like, so the core answers display queries for it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Display {
    /// `#rrggbb`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bg: Option<String>,
    /// Cell size in pixels: `[width, height]`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cell: Option<[u32; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fg: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Attach<'a> {
    pub cols: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<&'a Display>,
    /// Scrollback lines to paint first (main screen only).
    #[serde(skip_serializing_if = "is_zero")]
    pub history: u32,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub observe: bool,
    pub rows: u16,
    pub terminal: &'a str,
}

fn is_zero(value: &u32) -> bool {
    *value == 0
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Run<'a> {
    #[serde(skip_serializing_if = "<[String]>::is_empty")]
    pub argv: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<&'a str>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<&'a str, &'a str>,
    /// Replace a lost program with a shell in place, never rerun it.
    pub keep: bool,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<&'a str, &'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<&'a str>,
    /// Another machine this core added.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on: Option<&'a str>,
}
