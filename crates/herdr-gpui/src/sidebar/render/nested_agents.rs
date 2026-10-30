//! Agents listed under the workspace they run in, for
//! `agents_placement = "nested"`: the same rows as the agents section, a
//! step further in than their workspace, named by the agent alone since the
//! row above names the workspace.

use crate::{
    HerdrWindow,
    sidebar::{agents::sorted_agents, cell::RowContext},
};
use gpui::{prelude::*, *};
use herdr_client::protocol::ClientShellSnapshot;
use std::{cell::RefCell, rc::Rc};

/// A row's key and the bounds it was painted at.
type Painted = Option<(SharedString, Bounds<Pixels>)>;

/// Scrolls the focused nested agent into view. The spaces list reveals whole
/// items, and a nested agent sits somewhere inside its workspace's, so its
/// row records where it was painted and the next render scrolls by that.
#[derive(Default)]
pub(crate) struct NestedReveal {
    /// The focused nested agent's row key and its bounds, written in paint.
    painted: Rc<RefCell<Painted>>,
    /// The row last brought into view, so the user's own scrolling of an
    /// unchanged focus is left alone.
    revealed: RefCell<Option<SharedString>>,
    /// The row a frame was requested for because it had not painted yet. One
    /// retry is enough for a row that renders; one that never paints, being
    /// hidden or clipped away, must not keep the window drawing.
    waiting: RefCell<Option<SharedString>>,
}

impl NestedReveal {
    /// Whether a reveal still waits for its row to paint.
    #[cfg(test)]
    pub(in crate::sidebar) fn retrying(&self) -> bool {
        let waiting = self.waiting.borrow();
        waiting.is_some() && *waiting != *self.revealed.borrow()
    }
}

/// How far to scroll so `row` shows between `top` and `bottom`: down to its
/// bottom edge, but never past its top, and zero while it is in view.
fn reveal_shift(row: Bounds<Pixels>, top: Pixels, bottom: Pixels) -> Pixels {
    if row.top() < top {
        top - row.top()
    } else if row.bottom() > bottom {
        (bottom - row.bottom()).max(top - row.top())
    } else {
        px(0.)
    }
}

impl HerdrWindow {
    /// Rows for endpoint `index`'s agents in `workspace_id`, in the panel's
    /// order, indented `indent` past the workspace row drawn in `row_cx`,
    /// and the key of the focused one among them.
    pub(in crate::sidebar) fn nested_agent_rows(
        &self,
        index: usize,
        snapshot: &ClientShellSnapshot,
        workspace_id: &str,
        (row_cx, indent): (&RowContext, f32),
        cx: &mut Context<Self>,
    ) -> (Vec<AnyElement>, Option<SharedString>) {
        let row_cx = RowContext {
            nest: row_cx.nest + indent,
            host: None,
            ..*row_cx
        };
        let selected = index == self.selected_endpoint;
        let mut focused = None;
        let mut rows = Vec::new();
        for agent in sorted_agents(
            snapshot,
            self.agent_sort,
            &self.config.sidebar_style.status_priority,
        )
        .into_iter()
        .filter(|agent| agent.workspace_id == workspace_id)
        {
            let Some(lines) = self.configured_agent_lines(agent, snapshot, None) else {
                continue;
            };
            let gap = self.agent_row_gap(rows.len());
            let row = self.agent_cell(index, agent, snapshot, &row_cx, lines, gap, true, cx);
            if !(selected && agent.focused) {
                rows.push(row.into_any_element());
                continue;
            }
            let key = SharedString::from(format!("{index}-{}", agent.pane_id));
            let painted = self.sidebar_nested_reveal.painted.clone();
            let painted_key = key.clone();
            focused = Some(key);
            rows.push(
                row.relative()
                    .child(
                        canvas(
                            |_, _, _| (),
                            move |bounds, _, _, _| {
                                painted.replace(Some((painted_key.clone(), bounds)));
                            },
                        )
                        .absolute()
                        .inset_0()
                        .size_full(),
                    )
                    .into_any_element(),
            );
        }
        (rows, focused)
    }

    /// Brings the focused nested agent `key` into the spaces list's view,
    /// below a pinned header `cover` tall, once per newly focused agent.
    /// `None` forgets the last one, so focusing it again reveals it again.
    pub(in crate::sidebar) fn reveal_nested_agent(
        &self,
        key: Option<SharedString>,
        cover: Pixels,
        window: &mut Window,
    ) {
        let reveal = &self.sidebar_nested_reveal;
        let Some(key) = key else {
            reveal.revealed.replace(None);
            reveal.waiting.replace(None);
            return;
        };
        if reveal.revealed.borrow().as_ref() == Some(&key) {
            return;
        }
        let scroll = &self.sidebar_scroll[0];
        let viewport = scroll.bounds();
        let painted = reveal
            .painted
            .borrow()
            .clone()
            .filter(|(painted, _)| *painted == key);
        // The row has not been painted where it now sits, or the list not
        // measured: look again once this frame has painted both.
        let Some((_, row)) = painted.filter(|_| viewport.size.height > px(0.)) else {
            if reveal.waiting.replace(Some(key.clone())).as_ref() == Some(&key) {
                reveal.revealed.replace(Some(key));
            } else {
                window.request_animation_frame();
            }
            return;
        };
        reveal.waiting.replace(None);
        let shift = reveal_shift(row, viewport.top() + cover, viewport.bottom());
        if shift != px(0.) {
            let offset = scroll.offset();
            scroll.set_offset(point(offset.x, offset.y + shift));
            window.request_animation_frame();
        }
        reveal.revealed.replace(Some(key));
    }
}

#[cfg(test)]
mod tests;
