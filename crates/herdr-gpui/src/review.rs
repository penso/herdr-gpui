//! Reviewing the changes an agent made: the focused checkout's uncommitted
//! diff, notes on its lines, and sending them back to the agent the way page
//! annotations go (see `agent_notes`).
mod diff;
pub(crate) mod highlight;
mod notes;
mod view;

pub(crate) use diff::push_clean;
pub(crate) use view::{Review, styled_code};
