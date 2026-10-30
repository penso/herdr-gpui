#![allow(clippy::unwrap_used)]

use super::{
    STATUS_DOT_UNKNOWN, STATUS_WIDTH, agent_name,
    agents::{
        Indicators, PANE_CAPTION_LIMIT, agent_labels, agent_place, pane_name, status_indicator,
        status_style,
    },
    layout_tests,
    render::header,
    workspace_label,
    workspaces::workspace_entries,
};
use crate::{
    config::{FontConfig, StatusIcons, Theme},
    contrast::Contrast,
};
use herdr_client::protocol::{
    AgentStatus, ClientShellAgent, ClientShellPane, ClientShellSnapshot, ClientShellWorkspace,
};

/// Every status, most urgent first.
const STATUSES: [AgentStatus; 5] = [
    AgentStatus::Working,
    AgentStatus::Blocked,
    AgentStatus::Done,
    AgentStatus::Idle,
    AgentStatus::Unknown,
];

mod agent_order;
mod change_counts;
mod hierarchy;
mod host_mark;
mod pane_names;
mod row_text;
mod status_priority;
mod statuses;
mod sticky_hosts;
mod working_marks;
