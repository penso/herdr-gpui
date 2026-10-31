//! Coordinates of the native jump control in the isolated fixture.

use gpui::{Div, Pixels, Point};
use serde_json::{Value, json};
use std::cell::Cell;

use super::PANE;
use crate::HerdrWindow;

thread_local! {
    static CENTER: Cell<Option<Point<Pixels>>> = const { Cell::new(None) };
}

pub(crate) fn track_button(pane: &str, button: Div) -> Div {
    if pane != PANE {
        return button;
    }
    button.on_children_prepainted(|children, _, _| {
        // Symmetric padding puts the control's center at its children's union.
        CENTER.set(
            children
                .iter()
                .copied()
                .reduce(|a, b| a.union(&b))
                .map(|b| b.center()),
        );
    })
}

#[derive(Default)]
pub(super) struct Probe {
    previous: Option<Value>,
}

impl Probe {
    pub(super) fn observe(&mut self, view: &HerdrWindow) {
        let Some(pane) = view.chats.pane(PANE) else {
            return;
        };
        let needed = pane.glide.needs_jump(
            &pane.list,
            !pane.selection.is_empty() || pane.selection.is_selecting(),
        );
        let viewport = pane.list.viewport_bounds();
        let remaining = f32::from(
            pane.list.max_offset_for_scrollbar().y + pane.list.scroll_px_offset_for_scrollbar().y,
        );
        let state = json!({
            "needed": needed,
            "at_latest": remaining <= 1.,
            "jump": CENTER.get().filter(|_| needed).map(|p| [f32::from(p.x), f32::from(p.y)]),
            "viewport": {
                "x": f32::from(viewport.origin.x), "y": f32::from(viewport.origin.y),
                "width": f32::from(viewport.size.width), "height": f32::from(viewport.size.height),
            },
        });
        if self.previous.as_ref() != Some(&state) {
            eprintln!("RICH CHAT JUMP {state}");
            self.previous = Some(state);
        }
    }
}
