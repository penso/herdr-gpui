//! The sidebar: spaces and agents, the rows that show them, and the hover
//! menu a resting pointer opens.

mod agents;
mod hover;
mod layout;
mod metrics;
mod rail;
mod render;
mod reorder;
mod row;
mod view;
mod workspaces;

#[cfg(test)]
mod tests;

#[cfg(any(test, feature = "integration-test"))]
pub(crate) mod layout_tests;

#[cfg(all(feature = "integration-test", target_os = "macos"))]
pub(crate) mod native_tests;

pub(crate) use {
    agents::agent_name,
    hover::{HoverMenu, HoverRest},
    metrics::{ARROW_RESERVE, HOST_ARROW_WIDTH, HOST_GAP, ICON_RESERVE, LABEL_GAP},
    reorder::WorkspaceDrag,
    row::{compact, github_mark, label_text},
    view::SidebarView,
    workspaces::workspace_label,
};

#[cfg(any(test, feature = "integration-test"))]
pub(crate) use metrics::LABEL_WIDTH;

pub(crate) use view::cached as cached_view;

use agents::{agents_sort, sorted_agents, status_indicator};
use metrics::*;
use row::{RowBadge, first_text};
use workspaces::visible_workspace_entries;

pub(crate) const DEVICE_FOOTER_HEIGHT: f32 = 40.;

#[derive(Clone, Copy)]
pub(crate) enum SidebarDrag {
    /// `preferred` is the stored width when the drag began, restored if the
    /// drag collapses the sidebar.
    Width {
        start: f32,
        width: f32,
        preferred: Option<f32>,
    },
    Split,
}
