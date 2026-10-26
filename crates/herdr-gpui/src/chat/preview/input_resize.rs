//! Synthetic geometry for native pointer checks of the original input divider.

use serde_json::{Value, json};

use super::PANE;
use crate::{HerdrWindow, chat::view::pane_bounds};

#[derive(Default)]
pub(super) struct Probe {
    previous: Option<Value>,
}

impl Probe {
    pub(super) fn observe(&mut self, view: &HerdrWindow, window: &gpui::Window) {
        let Some(surface) = view.live.surface.as_ref() else {
            return;
        };
        let Some(pane) = surface.panes.iter().find(|pane| pane.pane_id == PANE) else {
            return;
        };
        let Some(chat) = view.chats.pane(PANE) else {
            return;
        };
        let Some(layout) = view.chat_terminal_layout(pane, surface) else {
            return;
        };
        let bounds = pane_bounds(
            layout.divider,
            view.cell_width,
            view.config.terminal.line_height(),
        );
        let state = json!({
            "mode": format!("{:?}", chat.terminal_size),
            "input_rows": layout.input.height,
            "conversation_rows": layout.conversation.height,
            "pointer": [f32::from(window.mouse_position().x), f32::from(window.mouse_position().y)],
            "divider": [
                f32::from(view.bounds.origin.x + bounds.origin.x) + 24.,
                f32::from(view.bounds.origin.y + bounds.origin.y) + 3.,
            ],
            "viewport": {
                "x": f32::from(view.bounds.origin.x),
                "y": f32::from(view.bounds.origin.y),
                "width": f32::from(view.bounds.size.width),
                "height": f32::from(view.bounds.size.height),
            },
        });
        if self.previous.as_ref() != Some(&state) {
            eprintln!("RICH CHAT INPUT SIZE {state}");
            self.previous = Some(state);
        }
    }
}
