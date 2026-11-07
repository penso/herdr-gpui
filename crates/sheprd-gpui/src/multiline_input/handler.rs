//! The platform text input boundary and the focus handle for the field.

use std::ops::Range;

use gpui::{
    App, Bounds, Context, EntityInputHandler, EventEmitter, FocusHandle, Focusable, Pixels, Point,
    UTF16Selection, Window, point, px,
};

use super::{CaretSpot, Changed, MultilineInput, caret_position, row_left};
use crate::text_editing::{Newlines, byte_range, to_utf16};

impl EventEmitter<Changed> for MultilineInput {}

impl Focusable for MultilineInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl EntityInputHandler for MultilineInput {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = byte_range(self.text(), range);
        *actual = Some(to_utf16(self.text(), range.start)..to_utf16(self.text(), range.end));
        Some(self.edit.text[range].into())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let selection = self.edit.selection();
        Some(UTF16Selection {
            range: to_utf16(self.text(), selection.start)..to_utf16(self.text(), selection.end),
            reversed: self.edit.cursor < self.edit.anchor,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.edit
            .marked
            .as_ref()
            .map(|r| to_utf16(self.text(), r.start)..to_utf16(self.text(), r.end))
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.edit.marked = None;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let changed = self.edit.replace(range, text, false, None, Newlines::Keep);
        self.did_edit(changed, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selection: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let changed = self
            .edit
            .replace(range, text, true, selection, Newlines::Keep);
        self.did_edit(changed, cx);
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let bounds = self.bounds?;
        let lines = self.lines.as_ref()?;
        let line_height = px(self.font.line_height());
        let range = byte_range(self.text(), range);
        let start = caret_position(lines, line_height, range.start)?;
        let end = caret_position(lines, line_height, range.end)?;
        let abs_x = |spot: CaretSpot| {
            let line = &lines[spot.line];
            let row = &line.rows[spot.row];
            row_left(bounds, row) + px(spot.x - row.start_x)
        };
        let line_height_f = self.font.line_height();
        let top = |spot: CaretSpot| bounds.top() + spot.y - self.scroll;
        if start.line == end.line && start.row == end.row {
            return Some(Bounds::from_corners(
                point(abs_x(start), top(start)),
                point(abs_x(end), top(end) + px(line_height_f)),
            ));
        }
        // A range across rows: box the whole thing; an input method's
        // candidate window is happy with a box.
        Some(Bounds::from_corners(
            point(bounds.left(), top(start)),
            point(bounds.right(), top(end) + px(line_height_f)),
        ))
    }

    fn character_index_for_point(
        &mut self,
        position: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let bounds = self.bounds?;
        if position.y < bounds.top() || position.y > bounds.bottom() {
            return None;
        }
        Some(to_utf16(self.text(), self.index_for_point(position)?))
    }
}
