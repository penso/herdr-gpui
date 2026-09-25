//! Sidebar geometry the kit does not decide: how wide the panel may grow, how
//! its sections may split, and how long a resting pointer takes to open a menu.

use std::time::Duration;

pub(super) const SIDEBAR_WIDTH: f32 = 232.;
pub(super) const MIN_WIDTH: f32 = 160.;
pub(super) const MAX_WIDTH: f32 = 480.;
/// The terminal keeps at least this much of the window beside the sidebar.
pub(super) const TERMINAL_MIN_WIDTH: f32 = 240.;
/// Neither list may be dragged shorter than this.
pub(super) const SECTION_MIN_HEIGHT: f32 = 60.;
/// How long the pointer rests on a workspace row before its menu opens, so the
/// same actions a right click offers are reachable without one.
pub(super) const HOVER_MENU_DELAY: Duration = Duration::from_millis(600);
/// Pointer drift, in pixels, that still counts as resting on the row.
pub(super) const HOVER_MENU_SLOP: f32 = 3.;

pub(super) fn sidebar_width(preferred: Option<f32>, window_width: f32) -> f32 {
    // Keep useful label space and reserve room for the terminal.
    preferred
        .unwrap_or(SIDEBAR_WIDTH)
        .clamp(MIN_WIDTH, MAX_WIDTH)
        .min((window_width - TERMINAL_MIN_WIDTH).max(0.))
}

/// The stored share of the sidebar's height the spaces list takes.
pub(super) fn split_fraction(preferred: Option<f32>) -> f32 {
    preferred.unwrap_or(0.5).clamp(0.1, 0.9)
}
