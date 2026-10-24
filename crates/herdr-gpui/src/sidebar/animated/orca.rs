//! After Orca (`stablyai/orca`, `AgentStateDot` and `StatusIndicator`): a
//! ring with a gap stepping round once a second while working, a question
//! bubble when the agent asks for you, a ringed check when done, and quiet
//! dots otherwise. Nothing but the working ring moves, as in Orca.

use super::{STATUS_DOT_UNKNOWN, disc, dot_size};
use gpui::{prelude::*, *};
use herdr_client::protocol::AgentStatus;
use std::time::Duration;

/// Orca's spinner turns in twelve 30° steps a second.
pub(super) const STEPS: f32 = 12.;
/// Orca draws idle at 40% of a neutral gray; here the idle color, as quiet.
const IDLE_OPACITY: f32 = 0.4;

pub(super) fn mark(status: AgentStatus, size: f32, color: u32) -> AnyElement {
    let icon = |selector: &'static str, path: &'static str| {
        svg()
            .debug_selector(move || selector.into())
            .path(path)
            .size(px(size))
            .flex_none()
            .text_color(rgb(color))
    };
    match status {
        AgentStatus::Working => icon("status-orca-ring", "icons/status-orca-working.svg")
            .with_animation(
                "status-orca-ring",
                Animation::new(crate::motion::duration(Duration::from_secs(1)))
                    .repeat_synced()
                    .with_max_fps(STEPS),
                |ring, delta| {
                    ring.with_transformation(Transformation::rotate(percentage(step(delta))))
                },
            )
            .into_any_element(),
        AgentStatus::Blocked => {
            icon("status-orca-question", "icons/status-orca-blocked.svg").into_any_element()
        }
        AgentStatus::Done => {
            icon("status-orca-done", "icons/status-outline-done.svg").into_any_element()
        }
        AgentStatus::Idle => disc(dot_size(size), color)
            .opacity(IDLE_OPACITY)
            .into_any_element(),
        AgentStatus::Unknown => disc(STATUS_DOT_UNKNOWN, color).into_any_element(),
    }
}

/// The share of a turn the ring has made at `delta`, held between steps.
pub(super) fn step(delta: f32) -> f32 {
    (delta * STEPS).floor() / STEPS
}
