//! Mods: small sandboxed Luau programs that add to Herdr UI without a Rust
//! change (`docs/mods-plan.md`). A mod is a folder holding `mod.toml` and
//! `main.luau`; its main chunk returns `herdr.mod { init, update, view }`.
//! `view` returns a tree of plain tables that [`Node`] checks, so a mod
//! describes its UI as data and the app draws it. Buttons carry a catalog
//! command or a message for `update`, never a Lua function, so a tree can
//! leave the thread its mod runs on.
//!
//! ```lua
//! local ui = require("@herdr/ui")
//!
//! return herdr.mod {
//!   watch = { "agents" },
//!   init = function() return { open = false } end,
//!   update = function(state, msg)
//!     if msg == "toggle" then state.open = not state.open end
//!     return state
//!   end,
//!   view = function(state, app)
//!     return ui.row { gap = "small",
//!       ui.badge { text = tostring(#app.agents.blocked), tone = "attention" },
//!       ui.button { label = "More", msg = "toggle" },
//!     }
//!   end,
//! }
//! ```
//!
//! Nothing here touches GPUI, files outside a mod's folder, or the network.

mod data;
mod effect;
mod error;
mod manifest;
mod names;
mod node;
mod pace;
mod runtime;
mod source;
mod state;

pub use data::Data;
pub use effect::Effect;
pub use error::{Budget, Error, Problem, Result};
pub use manifest::{API, Activity, CommandDecl, Manifest, Needs, Panel, Place};
pub use node::{
    Action, Align, Badge, Button, Container, Element, Gap, Icon, IconSource, Link, List, Meter,
    Node, Progress, Size, Text, Toggle, Tone, Tooltip, Weight,
};
pub use pace::Pace;
pub use runtime::{Limits, LogLine, Mod};
pub use source::{MAX_FILE_BYTES, MAX_FILES, MAX_TOTAL_BYTES, Sources, Stamp};
pub use state::{
    Agent, AgentStatus, Focus, Host, PullRequest, State, Tab, Topic, Usage, Workspace,
};

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
