//! Local multiline editor. Lines do not soft-wrap; both axes scroll independently.
use crate::{config, input_guard::GuardedInputHandler};
use gpui::{prelude::*, *};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

const MAX_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Draft {
    text: String,
    anchor: usize,
    cursor: usize,
}

impl Draft {
    pub(super) fn text(&self) -> &str {
        &self.text
    }

    fn selection(&self) -> Range<usize> {
        self.anchor.min(self.cursor)..self.anchor.max(self.cursor)
    }

    fn select(&mut self, index: usize, extend: bool) {
        self.cursor = index;
        if !extend {
            self.anchor = index;
        }
    }

    fn previous(&self, index: usize) -> usize {
        self.text
            .grapheme_indices(true)
            .rev()
            .find_map(|(i, _)| (i < index).then_some(i))
            .unwrap_or(0)
    }

    fn next(&self, index: usize) -> usize {
        self.text
            .grapheme_indices(true)
            .find_map(|(i, _)| (i > index).then_some(i))
            .unwrap_or(self.text.len())
    }

    fn snap(&self, index: usize) -> usize {
        self.text
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .chain(std::iter::once(self.text.len()))
            .take_while(|i| *i <= index)
            .last()
            .unwrap_or(0)
    }

    fn line_range(&self) -> Range<usize> {
        let start = self.text[..self.cursor].rfind('\n').map_or(0, |i| i + 1);
        let end = self.text[self.cursor..]
            .find('\n')
            .map_or(self.text.len(), |i| self.cursor + i);
        start..end
    }

    fn vertical(&self, down: bool, column: usize) -> usize {
        let line = self.line_range();
        let target = if down {
            if line.end == self.text.len() {
                return self.cursor;
            }
            let start = line.end + 1;
            start
                ..self.text[start..]
                    .find('\n')
                    .map_or(self.text.len(), |i| start + i)
        } else {
            if line.start == 0 {
                return self.cursor;
            }
            let end = line.start - 1;
            self.text[..end].rfind('\n').map_or(0, |i| i + 1)..end
        };
        self.snap(
            self.text[target.clone()]
                .grapheme_indices(true)
                .nth(column)
                .map_or(target.end, |(i, _)| target.start + i),
        )
    }

    fn replace(&mut self, range: Range<usize>, text: &str) -> bool {
        if text.len() > MAX_BYTES.saturating_sub(self.text.len() - range.len()) {
            return false;
        }
        self.text.replace_range(range.clone(), text);
        self.select(range.start + text.len(), false);
        true
    }
}

// Round surrogate-interior offsets down, clamp out-of-bounds offsets, and normalize
// reversed ranges before slicing. IME ranges are scalar, not grapheme, boundaries.
fn from_utf16(text: &str, offset: usize) -> usize {
    let mut units = 0;
    for (index, ch) in text.char_indices() {
        if units + ch.len_utf16() > offset {
            return index;
        }
        units += ch.len_utf16();
    }
    text.len()
}

fn byte_range(text: &str, range: Range<usize>) -> Range<usize> {
    let start = from_utf16(text, range.start);
    let end = from_utf16(text, range.end);
    start.min(end)..start.max(end)
}

fn utf16_range(text: &str, range: Range<usize>) -> Range<usize> {
    text[..range.start].encode_utf16().count()..text[..range.end].encode_utf16().count()
}

pub(super) struct Submit {
    pub(super) revision: u64,
}

struct Row {
    start: usize,
    line: ShapedLine,
}

pub(super) struct Composer {
    appearance: config::FontConfig,
    font: Font,
    theme: config::Theme,
    focus: FocusHandle,
    draft: Draft,
    revision: u64,
    input_epoch: u64,
    rollback: Option<Draft>,
    marked: Option<Range<usize>>,
    enabled: bool,
    limit_error: bool,
    selecting: bool,
    preferred_column: Option<usize>,
    rows: Vec<Row>,
    bounds: Option<Bounds<Pixels>>,
    scroll: Point<Pixels>,
    reveal_caret: bool,
}

impl EventEmitter<Submit> for Composer {}

impl Composer {
    pub(super) fn new(cx: &mut Context<Self>) -> Self {
        let appearance = config::Config::default().ui;
        Self {
            font: font(appearance.family.clone()),
            appearance,
            theme: config::Theme::default(),
            focus: cx.focus_handle(),
            draft: Draft::default(),
            revision: 0,
            input_epoch: 0,
            rollback: None,
            marked: None,
            enabled: true,
            limit_error: false,
            selecting: false,
            preferred_column: None,
            rows: Vec::new(),
            bounds: None,
            scroll: point(px(0.), px(0.)),
            reveal_caret: true,
        }
    }

    pub(super) fn set_appearance(
        &mut self,
        appearance: &config::FontConfig,
        theme: &config::Theme,
        cx: &mut Context<Self>,
    ) {
        if self.appearance.family == appearance.family
            && self.appearance.size == appearance.size
            && self.theme == *theme
        {
            return;
        }
        self.appearance = appearance.clone();
        self.font = font(appearance.family.clone());
        self.theme = theme.clone();
        self.rows.clear();
        self.reveal_caret = true;
        cx.notify();
    }

    /// Snapshot committed content and selection, rolling back any visible preedit.
    pub(super) fn draft(&self) -> Draft {
        self.rollback.as_ref().unwrap_or(&self.draft).clone()
    }

    /// Visible text, including preedit. Use `draft()` for persistence or sending.
    pub(super) fn text(&self) -> &str {
        self.draft.text()
    }

    pub(super) fn is_composing(&self) -> bool {
        self.rollback.is_some()
    }

    pub(super) fn revision(&self) -> u64 {
        self.revision
    }

    pub(super) fn set_draft(&mut self, draft: Draft, cx: &mut Context<Self>) {
        if draft.text.len() > MAX_BYTES {
            self.limit_error = true;
            cx.notify();
            return;
        }
        self.input_epoch = self.input_epoch.wrapping_add(1);
        self.draft = draft;
        self.rollback = None;
        self.marked = None;
        self.selecting = false;
        self.limit_error = false;
        self.scroll = point(px(0.), px(0.));
        self.changed(cx);
    }

    pub(super) fn cancel_composition(&mut self, cx: &mut Context<Self>) {
        if let Some(draft) = self.rollback.take() {
            self.input_epoch = self.input_epoch.wrapping_add(1);
            self.draft = draft;
            self.marked = None;
            self.changed(cx);
        }
    }

    pub(super) fn set_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if self.enabled == enabled {
            return;
        }
        self.enabled = enabled;
        if !enabled {
            self.invalidate_input_session(cx);
            self.selecting = false;
        }
        cx.notify();
    }

    /// Reject callbacks from the previous native registration, even if focus
    /// returns before another frame is painted.
    pub(super) fn invalidate_input_session(&mut self, cx: &mut Context<Self>) {
        if self.is_composing() {
            self.cancel_composition(cx);
        } else {
            self.input_epoch = self.input_epoch.wrapping_add(1);
            cx.notify();
        }
    }

    fn changed(&mut self, cx: &mut Context<Self>) {
        // Also invalidate queued submits for identical draft switches and IME
        // preview/rollback changes, not just edits to committed text.
        self.revision = self.revision.wrapping_add(1);
        self.rows.clear();
        self.preferred_column = None;
        self.reveal_caret = true;
        cx.notify();
    }

    fn replace(&mut self, range: Option<Range<usize>>, text: &str, cx: &mut Context<Self>) -> bool {
        let range = range
            .map(|r| byte_range(self.text(), r))
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.draft.selection());
        if !self.draft.replace(range, text) {
            self.limit_error = true;
            cx.notify();
            return false;
        }
        self.limit_error = false;
        self.changed(cx);
        true
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        // GPUI 0.2.2 also uses propagation to decide whether to deliver native
        // text. Unhandled keys must propagate; ancestor input/action handlers
        // must ignore the Composer focus/context instead of consuming them.
        if !self.enabled {
            cx.stop_propagation();
            return;
        }
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        if modifiers.platform && key == "enter" {
            cx.stop_propagation();
            window.prevent_default();
            if !self.is_composing() {
                cx.emit(Submit {
                    revision: self.revision,
                });
            }
            return;
        }
        if self.is_composing() {
            if key == "escape" {
                cx.stop_propagation();
                window.prevent_default();
                self.cancel_composition(cx);
            }
            return;
        }
        if modifiers.platform && !modifiers.control && !modifiers.alt {
            match key {
                "a" => {
                    self.draft.anchor = 0;
                    self.draft.cursor = self.text().len();
                }
                "c" | "x" => {
                    let range = self.draft.selection();
                    if !range.is_empty() {
                        cx.write_to_clipboard(ClipboardItem::new_string(
                            self.text()[range].to_owned(),
                        ));
                        if key == "x" {
                            self.replace(None, "", cx);
                        }
                    }
                }
                "v" => {
                    if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                        self.replace(None, &text, cx);
                    }
                }
                _ => return,
            }
        } else if !modifiers.platform && !modifiers.control && !modifiers.alt {
            let cursor = self.draft.cursor;
            let selection = self.draft.selection();
            let index = match key {
                "left" => Some(if !modifiers.shift && !selection.is_empty() {
                    self.draft.snap(selection.start)
                } else {
                    self.draft.previous(cursor)
                }),
                "right" => Some(if !modifiers.shift && !selection.is_empty() {
                    self.draft.next(selection.end.saturating_sub(1))
                } else {
                    self.draft.next(cursor)
                }),
                "home" => Some(self.draft.snap(self.draft.line_range().start)),
                "end" => Some(self.draft.snap(self.draft.line_range().end)),
                "up" | "down" => {
                    let line = self.draft.line_range();
                    let column = *self.preferred_column.get_or_insert_with(|| {
                        self.draft.text[line.start..cursor].graphemes(true).count()
                    });
                    Some(self.draft.vertical(key == "down", column))
                }
                "enter" => {
                    self.replace(None, "\n", cx);
                    None
                }
                "backspace" | "delete" => {
                    let range = if selection.is_empty() {
                        if key == "backspace" {
                            self.draft.previous(cursor)..cursor
                        } else {
                            cursor..self.draft.next(cursor)
                        }
                    } else {
                        selection
                    };
                    // A native selection may bisect a grapheme: deletion still
                    // removes complete clusters rather than leaving fragments.
                    let start = self.draft.snap(range.start);
                    let end = if self.draft.snap(range.end) == range.end {
                        range.end
                    } else {
                        self.draft.next(range.end)
                    };
                    self.draft.replace(start..end, "");
                    self.limit_error = false;
                    self.changed(cx);
                    None
                }
                _ => return,
            };
            if let Some(index) = index {
                self.draft.select(index, modifiers.shift);
            }
        } else {
            return;
        }
        if key != "up" && key != "down" {
            self.preferred_column = None;
        }
        self.reveal_caret = true;
        cx.stop_propagation();
        window.prevent_default();
        cx.notify();
    }

    fn index_at(&self, position: Point<Pixels>) -> Option<usize> {
        let bounds = self.bounds?;
        let y = (position.y - bounds.top() + self.scroll.y).to_f64().max(0.);
        let row = self
            .rows
            .get((y / f64::from(self.appearance.line_height())) as usize)
            .or_else(|| self.rows.last())?;
        Some(
            self.draft.snap(
                row.start
                    + row
                        .line
                        .closest_index_for_x(position.x - bounds.left() + self.scroll.x),
            ),
        )
    }

    fn prepare(&mut self, bounds: Bounds<Pixels>, window: &mut Window) {
        self.reveal_caret |= self.bounds != Some(bounds);
        self.bounds = Some(bounds);
        if self.rows.is_empty() {
            let mut start = 0;
            for text in self.draft.text.split('\n') {
                let run = TextRun {
                    len: text.len(),
                    font: self.font.clone(),
                    color: rgb(self.theme.foreground).into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let line = window.text_system().shape_line(
                    text.to_owned().into(),
                    px(self.appearance.size),
                    &[run],
                    None,
                );
                self.rows.push(Row { start, line });
                start += text.len() + 1;
            }
        }
        if self.reveal_caret {
            let index = self
                .rows
                .partition_point(|row| row.start <= self.draft.cursor)
                .saturating_sub(1);
            if let Some(row) = self.rows.get(index) {
                let x = row.line.x_for_index(self.draft.cursor - row.start);
                let y = px(index as f32 * self.appearance.line_height());
                self.scroll.x = self.scroll.x.min(x).max(x + px(2.) - bounds.size.width);
                self.scroll.y = self
                    .scroll
                    .y
                    .min(y)
                    .max(y + px(self.appearance.line_height()) - bounds.size.height);
            }
            self.reveal_caret = false;
        }
        let width = self
            .rows
            .iter()
            .map(|row| row.line.width)
            .fold(px(0.), Pixels::max);
        self.scroll.x = self
            .scroll
            .x
            .max(px(0.))
            .min((width + px(2.) - bounds.size.width).max(px(0.)));
        self.scroll.y = self.scroll.y.max(px(0.)).min(
            (px(self.rows.len() as f32 * self.appearance.line_height()) - bounds.size.height)
                .max(px(0.)),
        );
    }

    fn paint(&self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        let row_height = self.appearance.line_height();
        let selected = self.draft.selection();
        let focused = self.enabled && self.focus.is_focused(window);
        if self.text().is_empty() {
            let placeholder = "Write a prompt...";
            let line = window.text_system().shape_line(
                placeholder.into(),
                px(self.appearance.size),
                &[TextRun {
                    len: placeholder.len(),
                    font: self.font.clone(),
                    color: rgb(self.theme.muted).into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                }],
                None,
            );
            let _ = line.paint(bounds.origin, px(row_height), window, cx);
        }
        for (index, row) in self.rows.iter().enumerate() {
            let origin = bounds.origin
                + point(
                    -self.scroll.x,
                    px(index as f32 * row_height) - self.scroll.y,
                );
            if origin.y + px(row_height) <= bounds.top() || origin.y >= bounds.bottom() {
                continue;
            }
            let end = row.start + row.line.text.len();
            if selected.start <= end && selected.end > row.start {
                let start_x = row.line.x_for_index(
                    selected
                        .start
                        .saturating_sub(row.start)
                        .min(row.line.text.len()),
                );
                let end_x = if selected.end > end {
                    row.line.width + px(7.)
                } else {
                    row.line.x_for_index(selected.end - row.start)
                };
                window.paint_quad(fill(
                    Bounds::new(
                        origin + point(start_x, px(0.)),
                        size(end_x - start_x, px(row_height)),
                    ),
                    rgb(self.theme.active),
                ));
            }
            let _ = row.line.paint(origin, px(row_height), window, cx);
            if let Some(marked) = &self.marked {
                let start = marked.start.max(row.start);
                let stop = marked.end.min(end);
                if start < stop {
                    let x = row.line.x_for_index(start - row.start);
                    let width = row.line.x_for_index(stop - row.start) - x;
                    window.paint_quad(fill(
                        Bounds::new(origin + point(x, px(row_height - 2.)), size(width, px(1.))),
                        rgb(self.theme.foreground),
                    ));
                }
            }
            if focused && self.draft.cursor >= row.start && self.draft.cursor <= end {
                let x = row.line.x_for_index(self.draft.cursor - row.start);
                window.paint_quad(fill(
                    Bounds::new(origin + point(x, px(0.)), size(px(1.), px(row_height))),
                    rgb(self.theme.cursor),
                ));
            }
        }
    }
}

impl Focusable for Composer {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl EntityInputHandler for Composer {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        if !self.enabled {
            return None;
        }
        let range = byte_range(self.text(), range);
        *actual = Some(utf16_range(self.text(), range.clone()));
        Some(self.text()[range].to_owned())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        self.enabled.then(|| UTF16Selection {
            range: utf16_range(self.text(), self.draft.selection()),
            reversed: self.draft.cursor < self.draft.anchor,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked
            .clone()
            .filter(|_| self.enabled)
            .map(|range| utf16_range(self.text(), range))
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if !self.enabled {
            return;
        }
        // Native unmark accepts the visible preedit; explicit parent cancellation
        // restores the snapshot through cancel_composition instead.
        self.marked = None;
        if self.rollback.take().is_some() {
            self.revision = self.revision.wrapping_add(1);
        }
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.enabled && self.replace(range, text, cx) {
            self.marked = None;
            self.rollback = None;
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selection: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.enabled {
            return;
        }
        if text.is_empty() {
            self.cancel_composition(cx);
            return;
        }
        let snapshot = self.draft.clone();
        if !self.replace(range, text, cx) {
            return;
        }
        self.rollback.get_or_insert(snapshot);
        let start = self.draft.cursor - text.len();
        self.marked = Some(start..self.draft.cursor);
        if let Some(selection) = selection {
            // The native selection is relative to NEW preedit, not the document
            // or the old replacement range (whose length can be different).
            let selected = byte_range(text, selection);
            self.draft.anchor = start + selected.start;
            self.draft.cursor = start + selected.end;
        }
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        if !self.enabled {
            return None;
        }
        let range = byte_range(self.text(), range);
        let bounds = self.bounds?;
        let index = self
            .rows
            .partition_point(|row| row.start <= range.start)
            .saturating_sub(1);
        let row = self.rows.get(index)?;
        let x = row.line.x_for_index(range.start - row.start);
        let end = row
            .line
            .x_for_index((range.end - row.start).min(row.line.text.len()));
        Some(Bounds::new(
            bounds.origin
                + point(
                    x - self.scroll.x,
                    px(index as f32 * self.appearance.line_height()) - self.scroll.y,
                ),
            size((end - x).max(px(1.)), px(self.appearance.line_height())),
        ))
    }

    fn character_index_for_point(
        &mut self,
        position: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        if !self.enabled {
            return None;
        }
        self.index_at(position)
            .map(|index| self.text()[..index].encode_utf16().count())
    }
}

impl Render for Composer {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let prepare = cx.entity();
        let paint = cx.entity();
        div()
            .debug_selector(|| "composer-editor".into())
            .key_context("Composer")
            .track_focus(&self.focus)
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .bg(rgb(self.theme.surface))
            .text_color(rgb(self.theme.foreground))
            .font_family(self.appearance.family.clone())
            .text_size(px(self.appearance.size))
            .on_key_down(cx.listener(Self::key_down))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    if !this.enabled {
                        return;
                    }
                    window.focus(&this.focus);
                    let composing = this.is_composing();
                    this.cancel_composition(cx);
                    if composing && let Some(bounds) = this.bounds {
                        // Hit-test the restored text, not stale preedit byte offsets.
                        this.reveal_caret = false;
                        this.prepare(bounds, window);
                    }
                    let index = this.index_at(event.position);
                    if let Some(index) = index {
                        this.draft.select(
                            this.draft.snap(index.min(this.text().len())),
                            event.modifiers.shift,
                        );
                    }
                    this.selecting = true;
                    this.preferred_column = None;
                    this.reveal_caret = true;
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if this.enabled
                    && this.selecting
                    && let Some(index) = this.index_at(event.position)
                {
                    this.draft.select(index, true);
                    this.reveal_caret = true;
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.selecting = false;
                    cx.stop_propagation();
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, _| {
                    this.selecting = false;
                }),
            )
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                cx.stop_propagation();
                if !this.enabled {
                    return;
                }
                let delta = event.delta.pixel_delta(px(this.appearance.line_height()));
                this.scroll.x -= delta.x;
                this.scroll.y -= delta.y;
                this.reveal_caret = false;
                cx.notify();
            }))
            .child(
                div()
                    .debug_selector(|| "composer-editor-viewport".into())
                    .h(px(84.))
                    .flex_none()
                    .w_full()
                    .p(px(4.))
                    .border_1()
                    .rounded_sm()
                    .border_color(rgb(self.theme.active))
                    .bg(rgb(self.theme.background))
                    .overflow_hidden()
                    .cursor(CursorStyle::IBeam)
                    .child(
                        canvas(
                            move |bounds, window, cx| {
                                prepare.update(cx, |this, _| this.prepare(bounds, window));
                            },
                            move |bounds, (), window, cx| {
                                let focus = paint.read(cx).focus.clone();
                                let register = paint.read(cx).enabled && focus.is_focused(window);
                                window.with_content_mask(Some(ContentMask { bounds }), |window| {
                                    paint.update(cx, |this, cx| this.paint(bounds, window, cx));
                                });
                                if register {
                                    let epoch = paint.read(cx).input_epoch;
                                    window.handle_input(
                                        &focus,
                                        GuardedInputHandler::new(
                                            bounds,
                                            paint.clone(),
                                            move |this: &Composer, window, _| {
                                                this.enabled
                                                    && this.input_epoch == epoch
                                                    && this.focus.is_focused(window)
                                            },
                                        ),
                                        cx,
                                    );
                                }
                            },
                        )
                        .size_full(),
                    ),
            )
            .child(div().px(px(4.)).child(if self.limit_error {
                "16 KiB limit exceeded; draft unchanged"
            } else if !self.enabled {
                "Composer unavailable"
            } else {
                "Cmd-Enter to send | Enter for newline | No soft wrap"
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    fn native_handler(editor: &Entity<Composer>, cx: &App) -> impl InputHandler + use<> {
        let epoch = editor.read(cx).input_epoch;
        GuardedInputHandler::new(
            Bounds::new(point(px(0.), px(0.)), size(px(200.), px(76.))),
            editor.clone(),
            move |editor: &Composer, window, _| {
                editor.enabled && editor.input_epoch == epoch && editor.focus.is_focused(window)
            },
        )
    }

    #[gpui::test]
    fn appearance_updates_preserve_draft_selection_preedit_and_native_registration(
        cx: &mut TestAppContext,
    ) {
        let (editor, cx) = cx.add_window_view(|window, cx| {
            let editor = Composer::new(cx);
            window.focus(&editor.focus);
            editor
        });
        cx.update(|window, cx| {
            let mut handler = native_handler(&editor, cx);
            handler.replace_text_in_range(None, "draft", window, cx);
            handler.replace_and_mark_text_in_range(None, "preedit", Some(1..3), window, cx);
            editor.update(cx, |editor, cx| {
                let draft = editor.draft();
                let visible = editor.text().to_owned();
                let marked = editor.marked.clone();
                let selection = editor.draft.selection();
                let revision = editor.revision;
                let epoch = editor.input_epoch;
                let appearance = config::FontConfig {
                    family: "Menlo".into(),
                    size: 20.,
                };
                let theme = config::Theme {
                    foreground: 0x123456,
                    ..Default::default()
                };
                editor.set_appearance(&appearance, &theme, cx);
                assert_eq!(editor.draft(), draft);
                assert_eq!(editor.text(), visible);
                assert_eq!(editor.marked, marked);
                assert_eq!(editor.draft.selection(), selection);
                assert_eq!(editor.revision, revision);
                assert_eq!(editor.input_epoch, epoch);
                assert!(editor.rows.is_empty());
                assert_eq!(editor.theme, theme);
                let bounds = Bounds::new(point(px(0.), px(0.)), size(px(200.), px(76.)));
                editor.prepare(bounds, window);
                assert_eq!(
                    editor
                        .bounds_for_range(0..1, bounds, window, cx)
                        .map(|bounds| bounds.size.height),
                    Some(px(appearance.line_height()))
                );
            });
            assert!(handler.marked_text_range(window, cx).is_some());
            handler.replace_text_in_range(None, "committed", window, cx);
            assert_eq!(editor.read(cx).text(), "draftcommitted");
        });
    }

    #[gpui::test]
    fn canceled_preedit_invalidates_old_native_handler(cx: &mut TestAppContext) {
        let (editor, cx) = cx.add_window_view(|window, cx| {
            let editor = Composer::new(cx);
            window.focus(&editor.focus);
            editor
        });
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.replace_text_in_range(None, "abc", window, cx);
                editor.draft.select(1, false);
            });
            let restored = editor.read(cx).draft();
            let epoch = editor.read(cx).input_epoch;
            let mut old = native_handler(&editor, cx);
            old.replace_and_mark_text_in_range(None, "preedit", None, window, cx);
            old.replace_and_mark_text_in_range(None, "updated", None, window, cx);
            assert_eq!(editor.read(cx).input_epoch, epoch);

            editor.update(cx, |editor, cx| editor.cancel_composition(cx));
            assert_ne!(editor.read(cx).input_epoch, epoch);
            assert_eq!(editor.read(cx).draft(), restored);
            old.replace_text_in_range(None, "canceled commit", window, cx);
            old.replace_and_mark_text_in_range(None, "canceled preedit", None, window, cx);
            old.unmark_text(window, cx);
            assert_eq!(editor.read(cx).draft(), restored);
            assert!(!editor.read(cx).is_composing());

            let mut fresh = native_handler(&editor, cx);
            fresh.replace_text_in_range(None, "!", window, cx);
            assert_eq!(editor.read(cx).text(), "a!bc");
            fresh.replace_and_mark_text_in_range(None, "x", None, window, cx);
            old.unmark_text(window, cx);
            assert!(editor.read(cx).is_composing());
            fresh.unmark_text(window, cx);
            assert_eq!(editor.read(cx).draft().text(), "a!xbc");
            assert!(!editor.read(cx).is_composing());
        });
    }

    #[gpui::test]
    fn stale_native_registration_cannot_access_replaced_draft(cx: &mut TestAppContext) {
        let (editor, cx) = cx.add_window_view(|window, cx| {
            let editor = Composer::new(cx);
            window.focus(&editor.focus);
            editor
        });
        cx.update(|window, cx| {
            let mut old = native_handler(&editor, cx);
            old.replace_text_in_range(None, "same pane text", window, cx);
            let draft = editor.read(cx).draft();
            editor.update(cx, |editor, cx| editor.set_draft(draft, cx));
            let mut current = native_handler(&editor, cx);
            current.replace_and_mark_text_in_range(None, "preedit", Some(1..2), window, cx);
            let visible = editor.read(cx).text().to_owned();
            let revision = editor.read(cx).revision();

            old.replace_text_in_range(None, "stale", window, cx);
            old.replace_and_mark_text_in_range(None, "stale", None, window, cx);
            old.unmark_text(window, cx);
            assert_eq!(editor.read(cx).text(), visible);
            assert_eq!(editor.read(cx).revision(), revision);
            assert!(editor.read(cx).is_composing());
            assert!(old.selected_text_range(true, window, cx).is_none());
            assert!(old.marked_text_range(window, cx).is_none());
            let mut adjusted = Some(9..10);
            assert!(
                old.text_for_range(0..100, &mut adjusted, window, cx)
                    .is_none()
            );
            assert_eq!(adjusted, Some(9..10));
            assert!(old.bounds_for_range(0..1, window, cx).is_none());
            assert!(
                old.character_index_for_point(point(px(0.), px(0.)), window, cx)
                    .is_none()
            );

            assert!(current.marked_text_range(window, cx).is_some());
            assert!(current.selected_text_range(false, window, cx).is_some());
            assert_eq!(
                current.text_for_range(0..100, &mut None, window, cx),
                Some(visible)
            );
            current.unmark_text(window, cx);
            assert!(!editor.read(cx).is_composing());
        });
    }

    #[gpui::test]
    fn native_epoch_survives_edits_but_not_disable_and_reenable(cx: &mut TestAppContext) {
        let (editor, cx) = cx.add_window_view(|window, cx| {
            let editor = Composer::new(cx);
            window.focus(&editor.focus);
            editor
        });
        cx.update(|window, cx| {
            let mut handler = native_handler(&editor, cx);
            let epoch = editor.read(cx).input_epoch;
            // No drawing occurs between these callbacks.
            handler.replace_text_in_range(None, "one", window, cx);
            handler.replace_text_in_range(None, " two", window, cx);
            handler.replace_and_mark_text_in_range(None, " three", None, window, cx);
            handler.unmark_text(window, cx);
            assert_eq!(editor.read(cx).text(), "one two three");
            assert_eq!(editor.read(cx).input_epoch, epoch);

            let other_focus = cx.focus_handle();
            window.focus(&other_focus);
            handler.replace_text_in_range(None, "wrong focus", window, cx);
            handler.replace_and_mark_text_in_range(None, "wrong focus", None, window, cx);
            assert!(handler.selected_text_range(true, window, cx).is_none());
            assert_eq!(editor.read(cx).text(), "one two three");
            window.focus(&editor.focus_handle(cx));
            editor.update(cx, |editor, cx| {
                editor.set_enabled(false, cx);
                editor.set_enabled(true, cx);
            });
            assert_ne!(editor.read(cx).input_epoch, epoch);
            handler.replace_text_in_range(None, "stale after menu", window, cx);
            assert!(handler.selected_text_range(true, window, cx).is_none());
            assert_eq!(editor.read(cx).text(), "one two three");
            let mut fresh = native_handler(&editor, cx);
            fresh.replace_text_in_range(None, " four", window, cx);
            assert_eq!(editor.read(cx).text(), "one two three four");
        });
    }

    #[test]
    fn grapheme_navigation_and_multiline_selection() {
        let mut draft = Draft::default();
        assert!(draft.replace(0..0, "a\u{301}\u{1f469}\u{200d}\u{1f4bb}\nlast"));
        let first = "a\u{301}".len();
        assert_eq!(draft.next(0), first);
        let newline = draft.text.find('\n').unwrap_or(0);
        assert_eq!(draft.next(first), newline);
        assert_eq!(draft.previous(newline), first);
        draft.select(first, false);
        assert_eq!(draft.vertical(true, 1), newline + 2);
        draft.select(draft.text.len(), true);
        assert_eq!(
            &draft.text[draft.selection()],
            "\u{1f469}\u{200d}\u{1f4bb}\nlast"
        );
        assert!(draft.replace(draft.selection(), "\n"));
        assert_eq!(draft.text(), "a\u{301}\n");
    }

    #[test]
    fn utf16_clamps_surrogates_and_reversed_ranges() {
        let text = "a\u{1f600}z";
        assert_eq!(from_utf16(text, 2), 1);
        assert_eq!(byte_range(text, Range { start: 3, end: 1 }), 1..5);
        assert_eq!(byte_range(text, 100..200), text.len()..text.len());
        assert_eq!(utf16_range(text, 1..5), 1..3);
    }

    #[test]
    fn oversized_edit_is_atomic_including_selection() {
        let mut draft = Draft::default();
        assert!(draft.replace(0..0, &"a".repeat(MAX_BYTES)));
        draft.anchor = 1;
        let before = draft.clone();
        assert!(!draft.replace(draft.selection(), &"b".repeat(MAX_BYTES)));
        assert_eq!(draft, before);
        assert!(draft.replace(draft.selection(), "\u{1f600}"));
        assert_eq!(draft.text(), "a\u{1f600}");
    }

    #[gpui::test]
    fn native_ime_rollback_selection_and_disabled_input(cx: &mut TestAppContext) {
        let (editor, cx) = cx.add_window_view(|_, cx| Composer::new(cx));
        editor.update_in(cx, |editor, window, cx| {
            editor.replace_text_in_range(None, "prefix \u{1f600} tail", window, cx);
            let original = editor.draft();
            editor.replace_and_mark_text_in_range(
                Some(7..9),
                "\u{1f680}ab",
                Some(2..3),
                window,
                cx,
            );
            assert!(editor.is_composing());
            assert_eq!(editor.draft(), original);
            assert_eq!(
                editor
                    .selected_text_range(false, window, cx)
                    .map(|s| s.range),
                Some(9..10)
            );
            editor.replace_and_mark_text_in_range(None, "x", Some(0..1), window, cx);
            assert_eq!(
                editor
                    .selected_text_range(false, window, cx)
                    .map(|s| s.range),
                Some(7..8)
            );
            let visible = editor.text().to_owned();
            editor.replace_and_mark_text_in_range(None, &"x".repeat(MAX_BYTES), None, window, cx);
            assert!(editor.limit_error);
            assert_eq!(editor.text(), visible);
            assert_eq!(editor.draft(), original);
            editor.cancel_composition(cx);
            assert_eq!(editor.draft(), original);
            editor.replace_and_mark_text_in_range(Some(7..9), "\u{1f680}", None, window, cx);
            editor.replace_text_in_range(None, "ok", window, cx);
            assert_eq!(editor.text(), "prefix ok tail");
            assert!(!editor.is_composing());
            editor.replace_and_mark_text_in_range(None, "temporary", None, window, cx);
            editor.set_enabled(false, cx);
            editor.set_enabled(false, cx);
            editor.replace_text_in_range(None, "ignored", window, cx);
            assert_eq!(editor.text(), "prefix ok tail");
            assert!(!editor.is_composing());
        });
    }

    #[gpui::test]
    fn unmark_commits_but_cancel_and_disable_restore_selection(cx: &mut TestAppContext) {
        let (editor, cx) = cx.add_window_view(|window, cx| {
            let editor = Composer::new(cx);
            window.focus(&editor.focus);
            editor
        });
        editor.update_in(cx, |editor, window, cx| {
            editor.replace_text_in_range(None, "a\u{1f600}z", window, cx);
            editor.draft.anchor = 5;
            editor.draft.cursor = 1;
            let original = editor.draft();
            editor.replace_and_mark_text_in_range(None, "replacement", Some(1..3), window, cx);
            editor.cancel_composition(cx);
            assert_eq!(editor.draft(), original);
            let selection = editor.selected_text_range(false, window, cx);
            assert!(
                selection.is_some_and(|selection| selection.reversed && selection.range == (1..3))
            );

            editor.replace_and_mark_text_in_range(None, "\u{1f680}", None, window, cx);
            editor.unmark_text(window, cx);
            assert!(!editor.is_composing());
            assert_eq!(editor.draft().text(), "a\u{1f680}z");
            let committed = editor.draft();
            editor.cancel_composition(cx);
            assert_eq!(editor.draft(), committed);

            editor.replace_and_mark_text_in_range(None, "temporary", None, window, cx);
            editor.set_enabled(false, cx);
            // Focus and the platform's previous handler can survive until repaint.
            assert!(editor.focus.is_focused(window));
            editor.replace_text_in_range(None, "ignored", window, cx);
            editor.replace_and_mark_text_in_range(None, "ignored", None, window, cx);
            editor.unmark_text(window, cx);
            assert_eq!(editor.draft(), committed);
            assert!(editor.selected_text_range(true, window, cx).is_none());
            assert!(editor.marked_text_range(window, cx).is_none());
            assert!(
                editor
                    .text_for_range(0..100, &mut None, window, cx)
                    .is_none()
            );
        });
    }

    #[gpui::test]
    fn caret_scroll_and_ime_geometry_use_the_same_rows(cx: &mut TestAppContext) {
        let (editor, cx) = cx.add_window_view(|_, cx| Composer::new(cx));
        editor.update_in(cx, |editor, window, cx| {
            editor.replace_text_in_range(
                None,
                "one\ntwo\nthree\nfour\nfive\na long final row",
                window,
                cx,
            );
            let bounds = Bounds::new(point(px(10.), px(20.)), size(px(50.), px(76.)));
            editor.prepare(bounds, window);
            assert!(editor.scroll.x > px(0.));
            assert!(editor.scroll.y > px(0.));
            let end = editor.text().encode_utf16().count();
            let caret = editor.bounds_for_range(end..end, bounds, window, cx);
            assert!(caret.is_some_and(|caret| {
                caret.left() >= bounds.left()
                    && caret.right() <= bounds.right()
                    && caret.top() >= bounds.top()
                    && caret.bottom() <= bounds.bottom()
            }));
            let narrower = Bounds::new(bounds.origin, size(px(25.), bounds.size.height));
            editor.prepare(narrower, window);
            assert!(
                editor
                    .bounds_for_range(end..end, narrower, window, cx)
                    .is_some_and(|caret| caret.right() <= narrower.right())
            );
            editor.scroll = point(px(0.), px(0.));
            editor.bounds = Some(bounds);
            editor.prepare(bounds, window);
            assert_eq!(editor.index_at(bounds.origin), Some(0));
            assert_eq!(
                editor.index_at(bounds.origin + point(px(0.), px(editor.appearance.line_height()))),
                Some(4)
            );
            editor.set_draft(Draft::default(), cx);
            editor.prepare(bounds, window);
            assert_eq!(editor.scroll, point(px(0.), px(0.)));
        });
    }

    #[gpui::test]
    fn clicking_preedit_hit_tests_restored_text(cx: &mut TestAppContext) {
        let (editor, cx) = cx.add_window_view(|_, cx| Composer::new(cx));
        editor.update_in(cx, |editor, window, cx| {
            editor.replace_text_in_range(None, "abcdef", window, cx);
        });
        let bounds = cx
            .debug_bounds("composer-editor-viewport")
            .unwrap_or_else(|| panic!("missing composer viewport selector"));
        assert!(cx.debug_bounds("composer-editor").is_some());
        let position = bounds.origin + point(px(19.), px(12.));
        let expected = editor.update(cx, |editor, _| editor.index_at(position));
        editor.update_in(cx, |editor, window, cx| {
            editor.replace_and_mark_text_in_range(
                Some(0..1),
                "\u{1f680}long preedit",
                Some(0..0),
                window,
                cx,
            );
        });
        cx.simulate_mouse_down(position, MouseButton::Left, Modifiers::default());
        editor.update(cx, |editor, _| {
            assert!(!editor.is_composing());
            assert_eq!(editor.text(), "abcdef");
            assert_eq!(Some(editor.draft.cursor), expected);
        });
    }

    struct Host {
        editor: Entity<Composer>,
        leaked: usize,
        submitted: usize,
    }

    impl Render for Host {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .on_key_down(cx.listener(|this, _, window, cx| {
                    if !this.editor.focus_handle(cx).is_focused(window) {
                        this.leaked += 1;
                    }
                }))
                .child(self.editor.clone())
        }
    }

    #[gpui::test]
    fn editor_keys_and_submission_stay_local(cx: &mut TestAppContext) {
        let (host, cx) = cx.add_window_view(|window, cx| {
            let editor = cx.new(Composer::new);
            window.focus(&editor.focus_handle(cx));
            cx.subscribe(&editor, |this: &mut Host, editor, event: &Submit, cx| {
                assert_eq!(event.revision, editor.read(cx).revision());
                this.submitted += 1;
            })
            .detach();
            Host {
                editor,
                leaked: 0,
                submitted: 0,
            }
        });
        cx.simulate_input("a\u{1f600}");
        cx.simulate_keystrokes("left shift-left");
        host.update(cx, |host, cx| {
            let editor = host.editor.read(cx);
            assert_eq!(&editor.text()[editor.draft.selection()], "a");
        });
        cx.simulate_keystrokes("right end enter shift-enter");
        cx.simulate_input("last");
        cx.simulate_keystrokes("cmd-enter");
        host.update(cx, |host, cx| {
            assert_eq!(host.submitted, 1);
            assert_eq!(host.leaked, 0);
            assert_eq!(host.editor.read(cx).text(), "a\u{1f600}\n\nlast");
        });
        let editor = host.update(cx, |host, _| host.editor.clone());
        editor.update_in(cx, |editor, window, cx| {
            editor.replace_and_mark_text_in_range(None, "\u{1f680}", None, window, cx);
        });
        cx.simulate_keystrokes("cmd-enter");
        host.update(cx, |host, _| assert_eq!(host.submitted, 1));
        editor.update(cx, |editor, cx| editor.cancel_composition(cx));
        cx.simulate_keystrokes("cmd-a cmd-c cmd-x cmd-v");
        editor.update(cx, |editor, _| {
            assert_eq!(editor.text(), "a\u{1f600}\n\nlast")
        });
        editor.update(cx, |editor, cx| editor.set_enabled(false, cx));
        cx.simulate_keystrokes("backspace enter cmd-enter");
        host.update(cx, |host, cx| {
            assert_eq!(host.leaked, 0);
            assert_eq!(host.submitted, 1);
            assert_eq!(host.editor.read(cx).text(), "a\u{1f600}\n\nlast");
        });
    }

    #[gpui::test]
    fn submit_revision_changes_on_identical_draft_switch_and_edit(cx: &mut TestAppContext) {
        let (editor, cx) = cx.add_window_view(|_, cx| Composer::new(cx));
        editor.update_in(cx, |editor, window, cx| {
            editor.replace_text_in_range(None, "draft", window, cx);
            let pending = Submit {
                revision: editor.revision(),
            };
            let same = editor.draft();
            editor.set_draft(same.clone(), cx);
            assert_eq!(editor.draft(), same);
            assert_ne!(pending.revision, editor.revision());

            let pending = Submit {
                revision: editor.revision(),
            };
            editor.replace_text_in_range(None, " edited", window, cx);
            assert_eq!(editor.text(), "draft edited");
            assert_ne!(pending.revision, editor.revision());

            editor.revision = u64::MAX;
            editor.set_draft(editor.draft(), cx);
            assert_eq!(editor.revision(), 0);
        });
    }
}
