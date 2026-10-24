//! The wave set: every status drawn in the same 3×3 grid of cells, so the
//! set reads as one family. Working is Superset's `WaveSpinner`; blocked
//! chases a gap round the frame; done fills the grid; idle is the dim frame.

use gpui::{prelude::*, *};
use herdr_client::protocol::AgentStatus;
use std::time::Duration;

/// Superset's `WaveSpinner`: 24 frames of 50ms. Each diagonal fades in over
/// 3 frames, 2 frames behind the one before it, and empties from frame 11.
pub(super) const WAVE_FRAMES: f32 = 24.;
const WAVE_FADE: f32 = 3.;
const WAVE_DELAY: f32 = 2.;
const WAVE_EMPTY: f32 = 11.;
/// Starting here lights every cell, so the still frame is a full grid.
pub(super) const WAVE_FULL_FRAME: f32 = 11.;

/// The frame's cells, row by row as `row * 3 + column`, in clockwise order
/// from the top left.
pub(super) const FRAME: [usize; 8] = [0, 1, 2, 5, 8, 7, 6, 3];
const CENTER: usize = 4;
const CHASE_STEP_MS: u64 = 110;
/// The gap's cell stays faintly lit, so the frame never reads as broken.
const GAP_OPACITY: f32 = 0.15;
const DIM_OPACITY: f32 = 0.4;

pub(super) fn mark(status: AgentStatus, size: f32, color: u32) -> AnyElement {
    let cell = ((size - 2.) / 3.).floor().max(2.);
    match status {
        AgentStatus::Working => grid("status-wave", cell)
            .with_animation(
                "status-wave",
                Animation::new(crate::motion::duration(Duration::from_millis(
                    50 * WAVE_FRAMES as u64,
                )))
                .repeat_synced()
                .with_max_fps(20.),
                move |grid, delta| {
                    let frame = ((delta * WAVE_FRAMES).floor() + WAVE_FULL_FRAME) % WAVE_FRAMES;
                    grid.children(cells(cell, color, |index| {
                        wave_opacity(frame, (index / 3 + index % 3) as f32)
                    }))
                },
            )
            .into_any_element(),
        AgentStatus::Blocked => grid("status-chase", cell)
            .with_animation(
                "status-chase",
                Animation::new(crate::motion::duration(Duration::from_millis(
                    CHASE_STEP_MS * FRAME.len() as u64,
                )))
                .repeat_synced()
                .with_max_fps(1000. / CHASE_STEP_MS as f32),
                move |grid, delta| {
                    let gap = FRAME[super::frame_index(FRAME.len(), delta)];
                    grid.children(cells(cell, color, |index| chase_opacity(gap, index)))
                },
            )
            .into_any_element(),
        AgentStatus::Done => grid("status-grid-done", cell)
            .children(cells(cell, color, |_| 1.))
            .into_any_element(),
        AgentStatus::Idle => grid("status-grid-idle", cell)
            .children(cells(cell, color, |index| {
                if index == CENTER { 0. } else { DIM_OPACITY }
            }))
            .into_any_element(),
        AgentStatus::Unknown => grid("status-grid-unknown", cell)
            .children(cells(cell, color, |index| {
                if index == CENTER { DIM_OPACITY } else { 0. }
            }))
            .into_any_element(),
    }
}

/// How lit a wave cell on `diagonal` is at `frame`.
pub(super) fn wave_opacity(frame: f32, diagonal: f32) -> f32 {
    let ramp = |frames: f32| (frames / WAVE_FADE).clamp(0., 1.);
    let delay = diagonal * WAVE_DELAY;
    ramp(frame - delay) - ramp(frame - WAVE_EMPTY - delay)
}

/// A blocked cell: the frame lit but for the `gap` running round it.
pub(super) fn chase_opacity(gap: usize, index: usize) -> f32 {
    match index {
        CENTER => 0.,
        _ if index == gap => GAP_OPACITY,
        _ => 1.,
    }
}

/// Three rows of three square cells, a pixel apart, sized so the rows never
/// stretch to their parent.
fn grid(selector: &'static str, cell: f32) -> Div {
    div()
        .debug_selector(move || selector.into())
        .size(px(cell * 3. + 2.))
        .flex()
        .flex_col()
        .flex_none()
        .gap(px(1.))
}

fn cells(cell: f32, color: u32, opacity: impl Fn(usize) -> f32) -> Vec<Div> {
    (0..3)
        .map(|row| {
            div().flex().gap(px(1.)).children((0..3).map(|column| {
                div()
                    .size(px(cell))
                    .rounded(px(0.5))
                    .bg(rgb(color))
                    .opacity(opacity(row * 3 + column))
            }))
        })
        .collect()
}
