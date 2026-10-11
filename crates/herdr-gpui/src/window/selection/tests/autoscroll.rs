//! How far a drag held past a pane's top or bottom scrolls the pane per step.
//! Each step is a `pane.scroll` request; the daemon's answer, not the frame
//! that shows it, is what allows the next step.

use super::*;
use crate::window::{HerdrWindow, MockPeer};
use gpui::{Entity, VisualTestContext};
use serde_json::{Value, json};

const VIEWPORT_ROWS: u16 = 24;

/// A window with one scrollable pane and a selection press held inside it.
struct Drag<'a> {
    view: Entity<HerdrWindow>,
    cx: &'a mut VisualTestContext,
    peer: MockPeer,
    origin: Point<Pixels>,
    cell: (f32, f32),
    /// The id of the `pane.scroll` the daemon has received and not answered.
    unanswered: Option<String>,
}

impl<'a> Drag<'a> {
    /// Presses on row 5 of a pane showing `offset` rows up from the bottom of
    /// `max_offset` rows of history, then moves the pointer to `pointer_row`
    /// (fractional rows; below 0 is above the pane, 24 and up is below it).
    fn hold(
        cx: &'a mut TestAppContext,
        offset: u64,
        max_offset: u64,
        pointer_row: f32,
    ) -> Drag<'a> {
        let peer = MockPeer::advertising(&["pane.scroll", "pane.selection.read"]);
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = fixture_window(window, cx);
            peer.prepare(&mut view, cx);
            view.live.supports_selection_read = true;
            let frame = surface(&["x"; VIEWPORT_ROWS as usize], 80);
            let live = Arc::make_mut(view.live.surface.as_mut().unwrap());
            live.frame = frame.frame;
            live.panes[0].mouse_reporting = false;
            live.panes[0].scroll = Some(PaneSurfaceScrollMetrics {
                offset_from_bottom: offset,
                max_offset_from_bottom: max_offset,
                viewport_rows: u64::from(VIEWPORT_ROWS),
            });
            view
        });
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        let (origin, cell) = view.read_with(cx, |view, _| {
            (
                view.bounds.origin,
                (view.cell_width, view.config.terminal.line_height()),
            )
        });
        let drag = Drag {
            view,
            cx,
            peer,
            origin,
            cell,
            unanswered: None,
        };
        let press = drag.at(3.2, 5.5);
        drag.cx
            .simulate_mouse_down(press, MouseButton::Left, Modifiers::default());
        let hold = drag.at(3.2, pointer_row);
        drag.cx
            .simulate_mouse_move(hold, MouseButton::Left, Modifiers::default());
        drag
    }

    /// The pointer `rows` rows above the pane's top edge, measured to the
    /// middle of the row it is in: 1 is the row just above the pane.
    fn above(rows: u16) -> f32 {
        -f32::from(rows) + 0.5
    }

    /// The pointer `rows` rows below the pane's bottom edge, measured the same way.
    fn below(rows: u16) -> f32 {
        f32::from(VIEWPORT_ROWS) + f32::from(rows) - 0.5
    }

    fn at(&self, column: f32, row: f32) -> Point<Pixels> {
        self.origin + point(px(column * self.cell.0), px(row * self.cell.1))
    }

    fn follow(&mut self) {
        self.view
            .update(self.cx, |view, cx| view.follow_selection(cx));
    }

    /// Whether a scroll request is still waiting for its answer.
    fn scroll_in_flight(&mut self) -> bool {
        self.view
            .read_with(self.cx, |view, _| view.live.drag_request.is_some())
    }

    /// The next `pane.scroll` the daemon receives.
    fn next_scroll(&mut self) -> Value {
        loop {
            let ClientMessage::ClientShellEndpointRequest { request, .. } = self.peer.receive()
            else {
                continue;
            };
            let request: Value = serde_json::from_str(&request).unwrap();
            let id = request["id"].as_str().unwrap().to_owned();
            if request["method"] == "pane.scroll" {
                assert_eq!(request["params"]["pane_id"], "w1:p1");
                self.unanswered = Some(id);
                return request;
            }
            self.peer
                .respond("boot-v1", &id, &json!({"id": id, "result": {"type": "ok"}}));
        }
    }

    /// Follows the held pointer and returns the offset the pane is asked to
    /// scroll to.
    fn requested_offset(&mut self) -> u64 {
        self.follow();
        self.next_scroll()["params"]["offset_from_bottom"]
            .as_u64()
            .unwrap()
    }

    /// The daemon answers the scroll and the throttle passes, but the frame
    /// showing the new offset has not arrived: the surface is unchanged.
    fn answer_before_the_frame(&mut self) {
        let id = self.unanswered.take().unwrap();
        self.peer
            .respond("boot-v1", &id, &json!({"id": id, "result": {"type": "ok"}}));
        self.view.update(self.cx, |view, _| {
            view.live.drag_request = None;
            view.selection_follow.scrolled = None;
        });
    }
}

/// The further past the edge the pointer is held, the faster the pane
/// scrolls, beyond the old five-row ceiling.
#[gpui::test]
fn holding_far_above_the_pane_scrolls_faster_than_five_rows_per_step(cx: &mut TestAppContext) {
    let mut drag = Drag::hold(cx, 0, 100, Drag::above(10));
    assert_eq!(drag.requested_offset(), 10);
}

/// A step never moves more than one pane height, however far out the pointer is.
#[gpui::test]
fn holding_very_far_above_the_pane_scrolls_at_most_one_pane_height(cx: &mut TestAppContext) {
    let mut drag = Drag::hold(cx, 0, 100, Drag::above(100));
    assert_eq!(drag.requested_offset(), u64::from(VIEWPORT_ROWS));
}

/// A pointer barely outside the pane still scrolls, by one row.
#[gpui::test]
fn holding_barely_above_the_pane_scrolls_one_row(cx: &mut TestAppContext) {
    let mut drag = Drag::hold(cx, 0, 100, -0.2);
    assert_eq!(drag.requested_offset(), 1);
}

/// An answer that beats the frame showing it must not make the next step
/// start from the stale offset and repeat the last one.
#[gpui::test]
fn an_answer_before_its_frame_does_not_repeat_the_upward_step(cx: &mut TestAppContext) {
    let mut drag = Drag::hold(cx, 0, 100, Drag::above(2));
    assert_eq!(drag.requested_offset(), 2);
    drag.answer_before_the_frame();
    assert_eq!(drag.requested_offset(), 4);
}

/// The pane is never asked to scroll past its history, and once it has been
/// asked for the last row nothing more is sent.
#[gpui::test]
fn holding_above_stops_at_the_end_of_the_history(cx: &mut TestAppContext) {
    let mut drag = Drag::hold(cx, 0, 3, Drag::above(10));
    assert_eq!(drag.requested_offset(), 3);
    drag.answer_before_the_frame();
    drag.follow();
    assert!(!drag.scroll_in_flight());
}

/// Held below the pane, the offset shrinks toward the bottom the same way,
/// and an answer ahead of its frame does not repeat the step.
#[gpui::test]
fn holding_below_the_pane_scrolls_back_down_without_repeating_a_step(cx: &mut TestAppContext) {
    let mut drag = Drag::hold(cx, 30, 100, Drag::below(10));
    assert_eq!(drag.requested_offset(), 20);
    drag.answer_before_the_frame();
    assert_eq!(drag.requested_offset(), 10);
}

/// A pane already at the bottom has nowhere to scroll to, so a pointer held
/// below it sends nothing.
#[gpui::test]
fn holding_below_a_pane_at_the_bottom_sends_no_scroll(cx: &mut TestAppContext) {
    let mut drag = Drag::hold(cx, 0, 100, Drag::below(3));
    drag.follow();
    assert!(!drag.scroll_in_flight());
}
