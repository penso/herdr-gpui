//! The sidebar: spaces and agents built from the kit's sidebar parts, the
//! resizable panels that size it, and the hover menu a resting pointer opens.

mod agents;
mod hover;
mod metrics;
mod panels;
mod render;
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
    panels::Panels,
    row::compact,
    view::SidebarView,
    workspaces::workspace_label,
};

pub(crate) use view::cached as cached_view;

use agents::sorted_agents;
use metrics::*;
use row::first_text;
use workspaces::visible_workspace_entries;
