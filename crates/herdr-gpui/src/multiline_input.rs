//! The prompt field for long-form text: a multi-line, wrapping,
//! native-IME-aware input. The text wraps at the field's width and the
//! field grows with it, up to [`MAX_VISIBLE`] rows, past which it scrolls
//! and keeps the caret in view. The single-line fields keep
//! [`crate::search_input`]; this one keeps the newlines. The owner keeps
//! what sits around it.

use std::ops::Range;

use gpui::{
    Bounds, ContentMask, Context, CursorStyle, FocusHandle, MouseButton, Pixels, Point, Role,
    ScrollDelta, ScrollWheelEvent, SharedString, TextAlign, TextRun, UnderlineStyle, Window,
    canvas, div, fill, point, prelude::*, px, rgb, size,
};

use crate::{
    actions,
    caret::Carets,
    config::{Config, FontConfig, Theme},
    direction,
    fonts::StyledFont,
    input::ViewInputHandler,
    text_editing::{Editing, Newlines, normalize},
    words,
};

mod handler;
mod keys;

pub struct MultilineInput {
    pub focus: FocusHandle,
    placeholder: String,
    /// Formats the placeholder in the current locale, so a field built
    /// before a language change shows the new language's hint.
    localized: Option<Box<dyn Fn() -> String>>,
    _locale: gpui::Subscription,
    /// What a screen reader calls the field when its placeholder does not
    /// name it, as when a caption beside the field does.
    label: Option<SharedString>,
    edit: Editing,
    font: FontConfig,
    theme: Theme,
    /// The width the last paint wrapped at, for sizing the next frame.
    wrap_width: Option<Pixels>,
    /// What the last paint shaped: one entry per logical line, for clicks,
    /// the input method, and scrolling.
    lines: Option<Vec<LineShape>>,
    bounds: Option<Bounds<Pixels>>,
    scroll: Pixels,
    selecting: bool,
    /// Text drawn dimmed after the caret, which the owner offers to complete
    /// what is typed; shown only while the caret ends the text.
    completion: Option<String>,
    /// The preferred x of a run of vertical moves, so Up and Down keep their
    /// screen column across rows of different widths.
    goal_x: Option<f32>,
    /// What Ctrl-K last cut, which Ctrl-Y puts back, kept apart from the
    /// clipboard as in a native macOS field.
    killed: String,
}

/// Rows of text the field shows at once; past this it scrolls.
const MAX_VISIBLE: usize = 8;
/// The wrapping width assumed before the first paint reports the real one.
const FALLBACK_WRAP_WIDTH: f32 = 640.;

/// Observer of `Changed`, for owners that want to react to edits. The chat
/// reads the field live; nothing subscribes to this yet.
pub struct Changed;

/// One logical line's shaping: the wrapped rows, and the carets of the
/// unwrapped line for resolving where a byte offset or a click lands.
struct LineShape {
    /// Byte range of the line in the whole text; no line holds the newline.
    range: Range<usize>,
    wrapped: gpui::WrappedLine,
    carets: Carets,
    /// One entry per visual row, in painting order.
    rows: Vec<RowShape>,
}

impl LineShape {
    fn len(&self) -> usize {
        self.range.end - self.range.start
    }
}

#[derive(Clone, Copy)]
struct RowShape {
    /// Byte range of the visual row within the line's own text. A row starts
    /// at its wrap boundary glyph.
    start: usize,
    end: usize,
    /// Where the row's first glyph sits, in unwrapped coordinates.
    start_x: f32,
    /// Where the row ends, in unwrapped coordinates: the next row's start,
    /// or the line's width.
    end_x: f32,
}

impl RowShape {
    fn width(&self) -> f32 {
        (self.end_x - self.start_x).max(0.)
    }
}

/// Where a byte offset of the text sits among the shaped rows.
#[derive(Clone, Copy)]
struct CaretSpot {
    line: usize,
    row: usize,
    /// The caret's x in unwrapped coordinates of its line.
    x: f32,
    /// The row's top edge, measured from the field's top, unscrolled.
    y: Pixels,
}

fn is_rtl() -> bool {
    direction::is_rtl()
}

/// The x of a row's left edge within the bounds: rows set against the
/// leading edge, which is the right one in RTL.
fn row_left(bounds: Bounds<Pixels>, row: &RowShape) -> Pixels {
    if is_rtl() {
        bounds.right() - px(row.width())
    } else {
        bounds.left()
    }
}

/// The row a byte offset of the text lands on, and where in it.
fn caret_position(lines: &[LineShape], line_height: Pixels, offset: usize) -> Option<CaretSpot> {
    let mut y = px(0.);
    for (line_index, line) in lines.iter().enumerate() {
        if offset > line.range.end {
            y += line_height * line.rows.len() as f32;
            continue;
        }
        let local = offset.saturating_sub(line.range.start).min(line.len());
        let row = line
            .rows
            .iter()
            .position(|row| local < row.end)
            .unwrap_or(line.rows.len() - 1);
        return Some(CaretSpot {
            line: line_index,
            row,
            x: f32::from(line.carets.x(local)),
            y: y + line_height * row as f32,
        });
    }
    None
}

/// Per visual row, its byte range within its line and its x extent in the
/// line's unwrapped coordinates.
fn row_table(wrapped: &gpui::WrappedLineLayout, len: usize) -> Vec<RowShape> {
    let boundaries = wrapped.wrap_boundaries();
    let mut rows = Vec::with_capacity(boundaries.len() + 1);
    let mut start = 0;
    let mut start_x = 0.0f32;
    for boundary in boundaries {
        let glyph = &wrapped.runs()[boundary.run_ix].glyphs[boundary.glyph_ix];
        let (byte, x) = (glyph.index, f32::from(glyph.position.x));
        rows.push(RowShape {
            start,
            end: byte,
            start_x,
            end_x: x,
        });
        start = byte;
        start_x = x;
    }
    rows.push(RowShape {
        start,
        end: len,
        start_x,
        end_x: f32::from(wrapped.unwrapped_layout.width),
    });
    rows
}

/// The underline for text the input method marked, over the whole text:
/// painting splits it across rows like any other glyph.
fn marked_runs(text: &str, marked: Range<usize>, base: &TextRun) -> Vec<TextRun> {
    let marked = marked.start.min(text.len())..marked.end.min(text.len());
    let underline = UnderlineStyle {
        color: Some(base.color),
        thickness: px(1.),
        wavy: false,
    };
    [
        TextRun {
            len: marked.start,
            ..base.clone()
        },
        TextRun {
            len: marked.end - marked.start,
            underline: Some(underline),
            ..base.clone()
        },
        TextRun {
            len: text.len() - marked.end,
            ..base.clone()
        },
    ]
    .into_iter()
    .filter(|run| run.len > 0)
    .collect()
}

/// Shape the whole text wrapped at `wrap_width`; one entry per logical line.
fn shape_lines(
    text: &str,
    text_color: u32,
    marked: Option<Range<usize>>,
    font_size: f32,
    wrap_width: Pixels,
    window: &Window,
) -> Option<Vec<LineShape>> {
    if text.is_empty() {
        return Some(Vec::new());
    }
    let base = TextRun {
        len: text.len(),
        font: window.text_style().font(),
        color: rgb(text_color).into(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let runs = match marked {
        Some(marked) => marked_runs(text, marked, &base),
        None => vec![base],
    };
    let wrapped =
        window
            .text_system()
            .shape_text(text.into(), px(font_size), &runs, Some(wrap_width), None);
    let Ok(wrapped) = wrapped else {
        tracing::warn!("the chat composer's text failed to shape");
        return None;
    };
    let mut lines = Vec::new();
    // The runs cover the whole text; a newline splits logical lines but
    // belongs to none of them, so byte offsets still run through it.
    let mut base = 0usize;
    for wrapped in wrapped {
        let len = wrapped.len();
        let range = base..base + len;
        let line_text = &text[range.clone()];
        let carets = Carets::new(line_text, &wrapped.unwrapped_layout);
        let rows = row_table(&wrapped, len);
        lines.push(LineShape {
            range,
            wrapped,
            carets,
            rows,
        });
        base += len + 1; // + the newline that split the lines
    }
    Some(lines)
}

/// How many visual rows the text wraps to at `wrap_width`, for sizing the
/// canvas before its laid-out bounds come back.
fn visual_rows(text: &str, font_size: f32, wrap_width: Pixels, window: &Window) -> usize {
    if text.is_empty() {
        return 1;
    }
    let run = TextRun {
        len: text.len(),
        font: window.text_style().font(),
        color: rgb(0xffffff).into(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    match window.text_system().shape_text(
        text.into(),
        px(font_size),
        &[run],
        Some(wrap_width),
        None,
    ) {
        Ok(lines) => lines
            .iter()
            .map(|line| line.wrap_boundaries().len() + 1)
            .sum(),
        Err(_) => 1,
    }
}

/// The whole visual rows of shaped text, and their y offsets from the top.
fn rows_total(lines: &[LineShape]) -> usize {
    lines.iter().map(|line| line.rows.len()).sum()
}

/// Byte range of the logical line holding `offset`.
fn line_range(text: &str, offset: usize) -> Range<usize> {
    let start = text[..offset].rfind('\n').map_or(0, |index| index + 1);
    let end = text[offset..]
        .find('\n')
        .map_or(text.len(), |index| offset + index);
    start..end
}

impl MultilineInput {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            placeholder: "Message".into(),
            localized: None,
            _locale: cx.observe_global::<crate::i18n::CurrentLocale>(|this, cx| {
                if let Some(placeholder) = this.localized.as_ref().map(|localized| localized()) {
                    this.show_placeholder(placeholder, cx);
                }
            }),
            label: None,
            edit: Editing::default(),
            font: Config::default().ui,
            theme: Theme::default(),
            wrap_width: None,
            lines: None,
            bounds: None,
            scroll: px(0.),
            selecting: false,
            completion: None,
            goal_x: None,
            killed: String::new(),
        }
    }

    pub fn text(&self) -> &str {
        &self.edit.text
    }

    /// `text` with `cursor` at its byte offset, for restoring a draft. The
    /// text keeps its newlines; the cursor snaps to a grapheme boundary.
    pub(crate) fn set_text_at(&mut self, text: &str, cursor: usize, cx: &mut Context<Self>) {
        let text = normalize(text, Newlines::Keep);
        let mut cursor = cursor.min(text.len());
        while !text.is_char_boundary(cursor) {
            cursor -= 1;
        }
        let changed = self.edit.text != text;
        self.edit = Editing {
            anchor: cursor,
            cursor,
            text,
            ..Editing::default()
        };
        self.selecting = false;
        self.goal_x = None;
        self.did_edit(changed, cx);
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        let changed = !self.edit.text.is_empty();
        self.edit = Editing::default();
        self.scroll = px(0.);
        self.selecting = false;
        self.goal_x = None;
        self.did_edit(changed, cx);
    }

    pub fn is_composing(&self) -> bool {
        self.edit.marked.is_some()
    }

    /// Shows what `placeholder` formats now, laid out again only when its
    /// text changed, and formats it again per locale.
    pub(crate) fn set_placeholder_with(
        &mut self,
        placeholder: impl Fn() -> String + 'static,
        cx: &mut Context<Self>,
    ) {
        let shown = placeholder();
        if self.placeholder != shown {
            self.show_placeholder(shown, cx);
        }
        self.localized = Some(Box::new(placeholder));
    }

    fn show_placeholder(&mut self, placeholder: String, cx: &mut Context<Self>) {
        self.placeholder = placeholder;
        cx.notify();
    }

    /// Offers `completion` after the typed text. Unchanged is a no-op, so the
    /// owner may set it from its own render.
    pub(crate) fn set_completion(&mut self, completion: Option<String>, cx: &mut Context<Self>) {
        if self.completion != completion {
            self.completion = completion;
            cx.notify();
        }
    }

    /// The completion to draw now: never while composing, or with the caret
    /// anywhere but the end of the text.
    fn shown_completion(&self) -> Option<&str> {
        let selection = self.edit.selection();
        (!self.is_composing() && selection.is_empty() && selection.end == self.edit.text.len())
            .then_some(self.completion.as_deref())
            .flatten()
    }

    pub fn set_appearance(&mut self, font: FontConfig, theme: Theme, cx: &mut Context<Self>) {
        if self.font == font && self.theme == theme {
            return;
        }
        self.font = font;
        self.theme = theme;
        cx.notify();
    }

    fn did_edit(&mut self, changed: bool, cx: &mut Context<Self>) {
        self.goal_x = None;
        if changed {
            cx.emit(Changed);
        }
        cx.notify();
    }

    /// The completion to draw after the text, dimmed.
    fn completion(&self) -> Option<&str> {
        self.shown_completion()
    }

    /// The byte offset nearest `position`, in unwrapped coordinates per
    /// row; the whole field is scanned, so a click past a row's end lands
    /// at the row's end.
    fn index_for_point(&self, position: Point<Pixels>) -> Option<usize> {
        let bounds = self.bounds?;
        let lines = self.lines.as_ref()?;
        if lines.is_empty() {
            return Some(0);
        }
        let line_height_f = self.font.line_height();
        let content_y = (position.y - bounds.top() + self.scroll).max(px(0.));
        let row = (content_y.to_f64() / f64::from(line_height_f)) as usize;
        let mut acc = 0usize;
        for line in lines {
            let rows = line.rows.len();
            if row < acc + rows {
                let row = &line.rows[row - acc];
                let origin = row_left(bounds, row);
                let local_x = (position.x - origin).max(px(0.)).min(px(row.width()));
                let byte = line.carets.index_at(local_x + px(row.start_x));
                return Some(line.range.start + byte.clamp(row.start, row.end));
            }
            acc += rows;
        }
        lines.last().map(|line| line.range.end)
    }

    /// How far the caret's row may scroll: the rows past the viewport.
    fn max_scroll(&self) -> Pixels {
        let line_height = self.font.line_height();
        let rows = self.lines.as_ref().map_or(1, |lines| rows_total(lines));
        px((rows.saturating_sub(MAX_VISIBLE)) as f32 * line_height)
    }
}

impl MultilineInput {
    fn mouse_down(
        &mut self,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus, cx);
        cx.stop_propagation();
        if self.is_composing() {
            return;
        }
        self.goal_x = None;
        let offset = self.index_for_point(event.position).unwrap_or(0);
        match event.click_count {
            0 | 1 => self.edit.select_to(offset, event.modifiers.shift),
            2 => {
                let start = words::previous_start(&self.edit.text, offset);
                let end = words::next_end(&self.edit.text, offset);
                self.edit.anchor = start;
                self.edit.cursor = end;
            }
            _ => {
                let range = line_range(&self.edit.text, offset);
                self.edit.anchor = range.start;
                self.edit.cursor = range.end;
            }
        }
        self.selecting = true;
        cx.notify();
    }

    fn mouse_move(&mut self, event: &gpui::MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.selecting
            && !self.is_composing()
            && let Some(offset) = self.index_for_point(event.position)
        {
            self.edit.select_to(offset, true);
            cx.stop_propagation();
            cx.notify();
        }
    }

    fn mouse_up(&mut self, _: &gpui::MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.selecting {
            self.selecting = false;
            cx.stop_propagation();
        }
    }
}

impl Render for MultilineInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let input = cx.entity();
        let painter = input.clone();
        let line_height_f = self.font.line_height();
        let line_height = px(line_height_f);
        let font_size = self.font.size;
        // The height follows the wrapped rows; until the first paint reports
        // the real width, assume a reasonable one.
        let rows = visual_rows(
            &self.edit.text,
            font_size,
            self.wrap_width.unwrap_or(px(FALLBACK_WRAP_WIDTH)),
            window,
        );
        let height = px(line_height_f * rows.clamp(1, MAX_VISIBLE) as f32);
        div()
            .id("multiline-input")
            .debug_selector(|| "chat-field".into())
            .role(Role::TextInput)
            .aria_value(SharedString::from(self.edit.text.clone()))
            .when_some(self.label.clone(), |field, label| field.aria_label(label))
            .when(!self.placeholder.is_empty(), |field| {
                field.aria_placeholder(SharedString::from(self.placeholder.clone()))
            })
            .w_full()
            .text_color(rgb(self.theme.foreground))
            .text_font(&self.font)
            .text_size(px(font_size))
            .line_height(line_height)
            .track_focus(&self.focus)
            .cursor(CursorStyle::IBeam)
            .on_key_down(cx.listener(Self::key_down))
            .on_action(cx.listener(|this, _: &actions::Cut, window, cx| {
                this.key_down(&actions::edit_key("x"), window, cx)
            }))
            .on_action(cx.listener(|this, _: &actions::Copy, window, cx| {
                this.key_down(&actions::edit_key("c"), window, cx)
            }))
            .on_action(cx.listener(|this, _: &actions::Paste, window, cx| {
                this.key_down(&actions::edit_key("v"), window, cx)
            }))
            .on_action(cx.listener(|this, _: &actions::SelectAll, window, cx| {
                this.key_down(&actions::edit_key("a"), window, cx)
            }))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_move(cx.listener(Self::mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, _| {
                    this.selecting = false;
                }),
            )
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                let delta = match event.delta {
                    ScrollDelta::Pixels(point) => point.y,
                    ScrollDelta::Lines(point) => px(point.y * this.font.line_height()),
                };
                let scroll = (this.scroll - delta).clamp(px(0.), this.max_scroll());
                if scroll != this.scroll {
                    this.scroll = scroll;
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .child(
                canvas(
                    move |_, _, _| (),
                    move |bounds, _, window, cx| {
                        painter.update(cx, |input, cx| {
                            paint_field(input, bounds, window, cx);
                        });
                    },
                )
                .w_full()
                .h(height),
            )
    }
}

/// Paint the field's contents: the selection's background, the text, the
/// marked range's wavy underline (carried by the text system), the ghost
/// completion, and the caret. Registers the input method for the bounds.
#[allow(clippy::too_many_arguments)]
fn paint_field(
    input: &mut MultilineInput,
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut Context<MultilineInput>,
) {
    let font_size = input.font.size;
    let line_height_f = input.font.line_height();
    let line_height = px(line_height_f);
    let rtl = is_rtl();
    let focused = input.focus.is_focused(window);

    let shape = shape_lines(
        &input.edit.text,
        input.theme.foreground,
        input.edit.marked.clone(),
        font_size,
        bounds.size.width,
        window,
    )
    .unwrap_or_default();

    // Keep the caret's row in view.
    let total_rows = rows_total(&shape);
    let max_scroll = (total_rows.saturating_sub(MAX_VISIBLE)) as f32 * line_height_f;
    if let Some(spot) = caret_position(&shape, line_height, input.edit.cursor) {
        let y = f32::from(spot.y);
        let visible = line_height_f * MAX_VISIBLE as f32;
        let current = f32::from(input.scroll);
        if y < current {
            input.scroll = px(y);
        } else if y + line_height_f > current + visible {
            input.scroll = px(y + line_height_f - visible);
        }
    }
    input.scroll = px(f32::from(input.scroll).clamp(0., max_scroll));
    let scroll = input.scroll;

    // Registering with the window before painting lets the input method
    // query the bounds we computed.
    window.handle_input(&input.focus, ViewInputHandler::new(bounds, cx.entity()), cx);

    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        let align = if rtl {
            TextAlign::Right
        } else {
            TextAlign::Left
        };
        // Each row sets against the leading edge, which is the right one in RTL.
        let align_width = Some(bounds);

        // Selection backgrounds, per visual row.
        let selection = input.edit.selection();
        if focused && selection.start != selection.end {
            let mut y = bounds.top() - scroll;
            for line in &shape {
                if selection.end > line.range.start && selection.start < line.range.end {
                    let start = selection.start.max(line.range.start);
                    let end = selection.end.min(line.range.end);
                    let spans = line
                        .carets
                        .spans(start - line.range.start..end - line.range.start);
                    for (k, row) in line.rows.iter().enumerate() {
                        let top = y + px(line_height_f * k as f32);
                        let origin = row_left(bounds, row);
                        for span in &spans {
                            let s = f32::from(span.start).max(row.start_x).min(row.end_x);
                            let e = f32::from(span.end).min(row.end_x).max(row.start_x);
                            if s < e {
                                window.paint_quad(fill(
                                    Bounds::from_corners(
                                        point(origin + px(s - row.start_x), top),
                                        point(origin + px(e - row.start_x), top + line_height),
                                    ),
                                    rgb(input.theme.active),
                                ));
                            }
                        }
                    }
                }
                y += px(line_height_f * line.rows.len() as f32);
            }
        }

        if input.edit.text.is_empty() {
            let placeholder = input.placeholder.clone();
            if !placeholder.is_empty() {
                let run = TextRun {
                    len: placeholder.len(),
                    font: window.text_style().font(),
                    color: rgb(input.theme.muted).into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                if let Ok(wrapped) = window.text_system().shape_text(
                    placeholder.into(),
                    px(font_size),
                    &[run],
                    Some(bounds.size.width),
                    None,
                ) {
                    let origin_x = if rtl {
                        bounds.right() - px(f32::from(wrapped[0].width()))
                    } else {
                        bounds.left()
                    };
                    let _ = wrapped[0].paint(
                        point(origin_x, bounds.top()),
                        line_height,
                        align,
                        align_width,
                        window,
                        cx,
                    );
                }
            }
        } else {
            let mut y = bounds.top() - scroll;
            for line in &shape {
                let _ = line.wrapped.paint(
                    point(bounds.left(), y),
                    line_height,
                    align,
                    align_width,
                    window,
                    cx,
                );
                y += px(line_height_f * line.rows.len() as f32);
            }
            // The ghost completion, dimmed, past the end of the text.
            if let Some(completion) = input.completion()
                && let Some(spot) = caret_position(&shape, line_height, input.edit.text.len())
            {
                let row = &shape[spot.line].rows[spot.row];
                let origin = row_left(bounds, row) + px(spot.x - row.start_x);
                let top = bounds.top() + spot.y - scroll;
                let run = TextRun {
                    len: completion.len(),
                    font: window.text_style().font(),
                    color: rgb(input.theme.muted).into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let ghost =
                    window
                        .text_system()
                        .shape_line(completion.into(), px(font_size), &[run], None);
                let _ = ghost.paint(
                    point(origin, top),
                    line_height,
                    TextAlign::Left,
                    None,
                    window,
                    cx,
                );
            }
        }

        if focused {
            let caret = if input.edit.text.is_empty() {
                Some((
                    if rtl {
                        bounds.right() - px(1.)
                    } else {
                        bounds.left()
                    },
                    bounds.top(),
                ))
            } else {
                caret_position(&shape, line_height, input.edit.cursor).map(|spot| {
                    let row = &shape[spot.line].rows[spot.row];
                    (
                        row_left(bounds, row) + px(spot.x - row.start_x),
                        bounds.top() + spot.y - scroll,
                    )
                })
            };
            if let Some((x, top)) = caret {
                window.paint_quad(fill(
                    Bounds::new(point(x, top), size(px(1.), line_height)),
                    rgb(input.theme.cursor),
                ));
            }
        }
    });

    input.bounds = Some(bounds);
    input.wrap_width = Some(bounds.size.width);
    input.lines = Some(shape);
}
