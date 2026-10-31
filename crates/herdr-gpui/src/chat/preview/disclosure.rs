//! Real chevron coordinates and state for the isolated native disclosure probe.

use gpui::{Bounds, Div, Pixels};
use herdr_chat_protocol::EntryId;
use serde_json::{Value, json};
use std::cell::Cell;

use super::PANE;
use crate::{HerdrWindow, chat::selection::Selection};

const ENTRY: EntryId = EntryId(903);

thread_local! {
    static ARROW: Cell<Option<Bounds<Pixels>>> = const { Cell::new(None) };
}

/// The fixture's arrow gets its coordinates from actual native layout, not
/// guessed font metrics or a position on the user's desktop.
pub(crate) fn track_header(pane: &str, entry: EntryId, header: Div) -> Div {
    if pane != PANE || entry != ENTRY {
        return header;
    }
    header.on_children_prepainted(|children, _, _| {
        ARROW.set(children.first().copied());
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
        let Some(arrow) = ARROW.get() else {
            return;
        };
        let mut selection = Selection::default();
        selection.select_all(pane.prepared.text());
        let text = selection.text(pane.prepared.text()).unwrap_or_default();
        let viewport = pane.list.viewport_bounds();
        let state = json!({
            "expanded": pane.expanded.contains(&ENTRY),
            "source_visible": text.contains("Hello, café 中文"),
            "arrow": [f32::from(arrow.center().x), f32::from(arrow.center().y)],
            "viewport": {
                "x": f32::from(viewport.origin.x), "y": f32::from(viewport.origin.y),
                "width": f32::from(viewport.size.width), "height": f32::from(viewport.size.height),
            },
        });
        if self.previous.as_ref() != Some(&state) {
            eprintln!("RICH CHAT DISCLOSURE {state}");
            self.previous = Some(state);
        }
    }
}
