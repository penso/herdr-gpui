//! Mods: the sandboxed Luau folders the app runs (`docs/mods-plan.md`).
use super::app::Switch;
use serde::{Deserialize, Serialize};

/// The most log lines one request returns.
pub const MAX_LOG_LINES: usize = 200;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum ModOp {
    List,
    /// Reads a mod's files again now.
    Reload {
        name: String,
    },
    /// The last lines a mod wrote with `herdr.log`, oldest first.
    Logs {
        name: String,
        lines: usize,
    },
    /// Shows a mod's activity bar app in the window's body.
    Open {
        name: String,
    },
    /// Shows, hides, or toggles the mod panel; `None` only reports it.
    Panel {
        #[serde(default)]
        switch: Option<Switch>,
    },
}

/// One mod the window runs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModInfo {
    pub name: String,
    /// Where it draws: `status_bar`, `sidebar`, `tab`, `activity_bar`.
    pub places: Vec<String>,
    /// The palette commands it declares, by ID.
    pub commands: Vec<String>,
    /// It loaded and drew at least once.
    pub drawn: bool,
    /// Why its last load, `update`, or `view` failed, while that holds.
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Reply {
    Mods {
        /// Where mods are read from.
        dir: Option<String>,
        mods: Vec<ModInfo>,
    },
    Reloaded {
        name: String,
    },
    Logs {
        name: String,
        lines: Vec<String>,
    },
    Opened {
        name: String,
    },
    Panel {
        /// The user has not hidden it.
        shown: bool,
        /// The mods with a section, in the panel's order. The panel shows
        /// only while there is one.
        sections: Vec<String>,
    },
}
