//! Names Shift-drag when a drag went to a pane's program instead of
//! selecting. A program that turns on mouse reporting receives every drag,
//! and many agent TUIs do nothing with one, so copying looks broken there.
//! Shift keeps the gesture local on every platform (see
//! [`HerdrWindow::link_modifier_held`]), so the hint says only that.
//!
//! It answers the drag as a flash over the cells, the place the window
//! already answers selection gestures, and leaves forwarding unchanged. A
//! click is not a selection attempt and never raises it. It shows once per
//! app session, shared by its windows, and not at all once the user has
//! pressed with Shift over such a pane: they already know the gesture.

use super::{Flash, HerdrWindow};
use gpui::{App, Context, Global};

const TEXT: &str = "Hold Shift and drag to select text";

/// Whether this session still has the hint to give.
#[derive(Debug, Default)]
struct ShiftSelectHint {
    /// Shown already, or the user has used Shift where it would apply.
    spent: bool,
}

impl Global for ShiftSelectHint {}

/// Records that the user pressed with Shift over a mouse-reporting pane or
/// popup, which keeps the gesture local: the hint has nothing left to teach.
pub(super) fn learned(cx: &mut App) {
    cx.default_global::<ShiftSelectHint>().spent = true;
}

impl HerdrWindow {
    /// After a left drag was forwarded to a mouse-reporting program and
    /// released, shows the hint unless this session has already spent it.
    pub(super) fn hint_shift_select(&mut self, cx: &mut Context<Self>) {
        let hint = cx.default_global::<ShiftSelectHint>();
        if hint.spent {
            return;
        }
        hint.spent = true;
        self.show_flash(Flash::hint(TEXT), cx);
    }
}
