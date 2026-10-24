//! The status sets drawn rather than loaded from assets: Superset's wave
//! grid, the braille cells, and Orca's marks. Every animation runs on the
//! app's synced clock under one id per kind, so all busy rows move in step,
//! and caps its repaints because a sidebar can show many at once. GPUI draws
//! the first frame under reduced motion, so each first frame is a complete,
//! readable mark.

mod grid;
mod orca;

use super::STATUS_DOT_UNKNOWN;
use crate::config::StatusIcons;
use gpui::{prelude::*, *};
use herdr_client::protocol::AgentStatus;
use std::time::Duration;

/// Superset's `AsciiSpinner` frames, 80ms each.
const BRAILLE: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const BRAILLE_FRAME_MS: u64 = 80;
/// A blocked cell empties dot by dot and fills again, after Zed's long
/// spinner. It starts full, so a held cell under reduced motion is solid.
const BREATHE: [&str; 14] = [
    "⣿", "⣷", "⣶", "⣦", "⣤", "⣄", "⣀", "⡀", "⣀", "⣄", "⣤", "⣦", "⣶", "⣷",
];
const BREATHE_FRAME_MS: u64 = 70;

/// The mark `set` draws for `status` in a `size` slot, in the status `color`.
pub(super) fn mark(set: StatusIcons, status: AgentStatus, size: f32, color: u32) -> AnyElement {
    match set {
        StatusIcons::Wave => grid::mark(status, size, color),
        StatusIcons::Orca => orca::mark(status, size, color),
        StatusIcons::Braille => braille_set(status, size, color),
        // The other sets load assets; only Dots' settled marks land here.
        _ => herdr_dot(status, size, color),
    }
}

/// The braille set: every status a braille cell, so the set reads as one
/// family. Working spins, blocked breathes, and the settled ones hold still,
/// on the middle row so they sit at the row's center like the spinner.
fn braille_set(status: AgentStatus, size: f32, color: u32) -> AnyElement {
    let still = |selector: &'static str, glyph: &'static str| {
        braille_cell(selector, size)
            .child(dots(glyph, rows(&[glyph]), size, color))
            .into_any_element()
    };
    match status {
        AgentStatus::Working => {
            braille("status-braille", &BRAILLE, BRAILLE_FRAME_MS, size, color).into_any_element()
        }
        AgentStatus::Blocked => braille(
            "status-braille-fill",
            &BREATHE,
            BREATHE_FRAME_MS,
            size,
            color,
        )
        .into_any_element(),
        AgentStatus::Done => still("status-braille-done", "⠿"),
        AgentStatus::Idle => still("status-braille-idle", "⠒"),
        AgentStatus::Unknown => still("status-braille-unknown", "⠂"),
    }
}

/// Herdr's dot at `size`: filled, hollow for idle, small for unknown.
fn herdr_dot(status: AgentStatus, size: f32, color: u32) -> AnyElement {
    match status {
        AgentStatus::Unknown => disc(STATUS_DOT_UNKNOWN, color).into_any_element(),
        AgentStatus::Idle => div()
            .size(px(size))
            .flex_none()
            .rounded_full()
            .border_1()
            .border_color(rgb(color))
            .into_any_element(),
        _ => disc(size, color).into_any_element(),
    }
}

/// Orca's dots are about half the icon slot.
fn dot_size(size: f32) -> f32 {
    (size * 0.5).round().max(4.)
}

fn disc(diameter: f32, color: u32) -> Div {
    div()
        .size(px(diameter))
        .flex_none()
        .rounded_full()
        .bg(rgb(color))
}

/// The bit each dot of a braille cell sets, row by row as `[left, right]`:
/// dots 1–3 and 4–6 fill the top three rows, and 7 and 8 the fourth.
const BRAILLE_BITS: [[u32; 2]; 4] = [[0x01, 0x08], [0x02, 0x10], [0x04, 0x20], [0x40, 0x80]];

/// Whether `glyph`, a braille cell, raises the dot at `row` and `column`.
pub(super) fn raised(glyph: &str, row: usize, column: usize) -> bool {
    glyph
        .chars()
        .next()
        .and_then(|cell| (cell as u32).checked_sub(0x2800))
        .is_some_and(|bits| bits & BRAILLE_BITS[row][column] != 0)
}

/// The rows `frames` need: six-dot cells leave the fourth row empty, so they
/// draw three and center those, rather than sitting high in a cell of four.
pub(super) fn rows(frames: &[&str]) -> usize {
    let fourth = |glyph: &&str| raised(glyph, 3, 0) || raised(glyph, 3, 1);
    if frames.iter().any(fourth) { 4 } else { 3 }
}

/// A dot's diameter in a `size` slot: three rows of dots and their gaps take
/// up to seven tenths of it, in half pixels so the cell grows smoothly with
/// the font rather than in whole-pixel jumps.
pub(super) fn dot_diameter(size: f32) -> f32 {
    ((size * 0.7 - 2.) / 3. * 2.).floor().max(2.) / 2.
}

/// A slot a braille cell centers in.
fn braille_cell(selector: &'static str, size: f32) -> Div {
    div()
        .debug_selector(move || selector.into())
        .size(px(size))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
}

/// `glyph` as `rows` rows of two dots. The cell is drawn rather than set as
/// text, because fonts place a braille glyph's dots at their own heights and
/// none at the row's center. Every dot is laid out, raised or not, so the
/// cell keeps its shape from frame to frame.
fn dots(glyph: &str, rows: usize, size: f32, color: u32) -> Div {
    let dot = dot_diameter(size);
    div()
        .debug_selector(|| "status-braille-dots".into())
        .flex_none()
        .flex()
        .flex_col()
        .gap(px(1.))
        .children((0..rows).map(|row| {
            div().flex().gap(px(1.)).children((0..2).map(|column| {
                div()
                    .size(px(dot))
                    .flex_none()
                    .rounded_full()
                    .bg(rgb(color))
                    .opacity(if raised(glyph, row, column) { 1. } else { 0. })
            }))
        }))
}

/// A braille cell stepping through `frames`, one every `frame_ms`. The
/// selector names the animation too, so each kind keeps its own clock.
fn braille(
    selector: &'static str,
    frames: &'static [&'static str],
    frame_ms: u64,
    size: f32,
    color: u32,
) -> impl IntoElement {
    let rows = rows(frames);
    braille_cell(selector, size).with_animation(
        selector,
        Animation::new(crate::motion::duration(Duration::from_millis(
            frame_ms * frames.len() as u64,
        )))
        .repeat_synced()
        .with_max_fps(1000. / frame_ms as f32),
        move |cell, delta| {
            let glyph = frames[frame_index(frames.len(), delta)];
            cell.child(dots(glyph, rows, size, color))
        },
    )
}

/// The frame a looping animation of `len` frames shows at `delta`, its share
/// of the loop: each frame holds for an equal slice, the last up to the end.
pub(super) fn frame_index(len: usize, delta: f32) -> usize {
    ((delta * len as f32) as usize).min(len.saturating_sub(1))
}

#[cfg(test)]
mod tests;
