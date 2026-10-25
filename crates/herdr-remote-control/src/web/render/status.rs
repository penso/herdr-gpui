//! An agent's status from the daemon's snapshot, drawn in the set the user
//! picked in Herdr UI: Herdr's dots, or Herdr UI's solid, outline, or tile
//! marks from the same files (`assets/icons/status-*.svg`), in the shared
//! status colours (`.status-mark` in input.css).

use crate::web::Indicators;
use herdr_client::protocol::AgentStatus;
use maud::{Markup, PreEscaped, html};

/// Declares each set's files once, by status, in `AgentStatus` order.
macro_rules! marks {
    ($($set:literal),+) => {
        [$([
            include_str!(concat!("../../../../../assets/icons/status-", $set, "-idle.svg")),
            include_str!(concat!("../../../../../assets/icons/status-", $set, "-working.svg")),
            include_str!(concat!("../../../../../assets/icons/status-", $set, "-blocked.svg")),
            include_str!(concat!("../../../../../assets/icons/status-", $set, "-done.svg")),
            include_str!(concat!("../../../../../assets/icons/status-", $set, "-unknown.svg")),
        ]),+]
    };
}

/// Solid, outline, then the tiles' inner marks.
const MARKS: [[&str; 5]; 3] = marks!("solid", "outline", "mark");

fn index(status: AgentStatus) -> usize {
    match status {
        AgentStatus::Idle => 0,
        AgentStatus::Working => 1,
        AgentStatus::Blocked => 2,
        AgentStatus::Done => 3,
        AgentStatus::Unknown => 4,
    }
}

fn words(status: AgentStatus) -> (&'static str, &'static str) {
    match status {
        AgentStatus::Idle => ("st-idle", "Idle"),
        AgentStatus::Working => ("st-working", "Working"),
        AgentStatus::Blocked => ("st-blocked", "Blocked"),
        AgentStatus::Done => ("st-done", "Done"),
        AgentStatus::Unknown => ("st-unknown", "Unknown"),
    }
}

/// `status` as a mark of `indicators`, labelled for screen readers.
pub(crate) fn status_mark(status: AgentStatus, indicators: Indicators) -> Markup {
    let (class, label) = words(status);
    let svg = |set: usize| PreEscaped(MARKS[set][index(status)].trim());
    html! {
        @match indicators {
            Indicators::Dots => span class={ "status-dot " (class) } role="img" aria-label=(label) {},
            Indicators::Solid => span class={ "status-mark " (class) } role="img" aria-label=(label) { (svg(0)) },
            Indicators::Outline => span class={ "status-mark " (class) } role="img" aria-label=(label) { (svg(1)) },
            Indicators::Tiles => span class={ "status-mark status-tile " (class) } role="img" aria-label=(label) { (svg(2)) },
        }
    }
}
