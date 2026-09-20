//! A single-line, native-IME-aware input for searchable pickers.
use std::ops::Range;

use gpui::{
    App, Bounds, ClipboardItem, ContentMask, Context, CursorStyle, ElementInputHandler,
    EntityInputHandler, EventEmitter, FocusHandle, Focusable, KeyDownEvent, MouseButton, Pixels,
    Point, ShapedLine, TextRun, UTF16Selection, UnderlineStyle, Window, canvas, div, fill, point,
    prelude::*, px, rgb, size,
};

use crate::config::{Config, FontConfig, Theme};

pub struct Changed;

pub struct SearchInput {
    pub focus: FocusHandle,
    placeholder: String,
    edit: Editing,
    font: FontConfig,
    theme: Theme,
    layout: Option<ShapedLine>,
    bounds: Option<Bounds<Pixels>>,
    scroll: Pixels,
    selecting: bool,
}

// Internal offsets are UTF-8 boundaries; only the platform input API uses UTF-16.
#[derive(Default)]
struct Editing {
    text: String,
    anchor: usize,
    cursor: usize,
    marked: Option<Range<usize>>,
}

fn from_utf16(text: &str, offset: usize) -> usize {
    let mut units = 0;
    for (byte, ch) in text.char_indices() {
        if units >= offset {
            return byte;
        }
        units += ch.len_utf16();
    }
    text.len()
}

fn to_utf16(text: &str, offset: usize) -> usize {
    text[..offset].encode_utf16().count()
}

fn byte_range(text: &str, range: Range<usize>) -> Range<usize> {
    // Clamp out-of-bounds requests and round split surrogate pairs forward.
    from_utf16(text, range.start.min(range.end))..from_utf16(text, range.start.max(range.end))
}

fn single_line(text: &str) -> String {
    text.chars()
        .filter(|ch| !matches!(ch, '\r' | '\n'))
        .collect()
}

impl Editing {
    fn selection(&self) -> Range<usize> {
        self.anchor.min(self.cursor)..self.anchor.max(self.cursor)
    }

    fn select_to(&mut self, offset: usize, extend: bool) {
        self.cursor = offset;
        if !extend {
            self.anchor = offset;
        }
    }

    // Scalar boundaries keep this small input dependency-free and never split UTF-8.
    fn previous(&self) -> usize {
        self.text[..self.cursor]
            .char_indices()
            .next_back()
            .map_or(0, |(i, _)| i)
    }

    fn next(&self) -> usize {
        self.text[self.cursor..]
            .chars()
            .next()
            .map_or(self.cursor, |ch| self.cursor + ch.len_utf8())
    }

    fn replace(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        composing: bool,
        selection: Option<Range<usize>>,
    ) -> bool {
        let range = range
            .map(|r| byte_range(&self.text, r))
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.selection());
        let inserted = single_line(text);
        let changed = self.text[range.clone()] != inserted;
        self.text.replace_range(range.clone(), &inserted);
        self.marked = (composing && !inserted.is_empty())
            .then_some(range.start..range.start + inserted.len());
        self.cursor = range.start + inserted.len();
        self.anchor = self.cursor;
        if composing && let Some(selection) = selection {
            // IME selection is relative to the supplied text, not the whole buffer.
            // Map through newline removal as well as UTF-16 conversion.
            self.anchor =
                range.start + single_line(&text[..from_utf16(text, selection.start)]).len();
            self.cursor = range.start + single_line(&text[..from_utf16(text, selection.end)]).len();
        }
        changed
    }
}

impl SearchInput {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            placeholder: "Search themes...".into(),
            edit: Editing::default(),
            font: Config::default().ui,
            theme: Theme::default(),
            layout: None,
            bounds: None,
            scroll: px(0.),
            selecting: false,
        }
    }

    pub fn text(&self) -> &str {
        &self.edit.text
    }

    pub fn set_placeholder(&mut self, value: &str, cx: &mut Context<Self>) {
        self.placeholder = value.into();
        self.layout = None;
        cx.notify();
    }

    pub fn is_composing(&self) -> bool {
        self.edit.marked.is_some()
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        let changed = !self.edit.text.is_empty();
        self.edit = Editing::default();
        self.scroll = px(0.);
        self.selecting = false;
        self.did_edit(changed, cx);
    }

    pub fn set_appearance(&mut self, font: FontConfig, theme: Theme, cx: &mut Context<Self>) {
        self.font = font;
        self.theme = theme;
        self.layout = None;
        cx.notify();
    }

    fn did_edit(&mut self, changed: bool, cx: &mut Context<Self>) {
        self.layout = None;
        if changed {
            cx.emit(Changed);
        }
        cx.notify();
    }

    fn mouse_index(&self, position: Point<Pixels>) -> usize {
        match (&self.layout, self.bounds) {
            (Some(line), Some(bounds)) if !self.edit.text.is_empty() => {
                line.closest_index_for_x(position.x - bounds.left() + self.scroll)
            }
            _ => 0,
        }
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        // Let the OS consume composition keys, including picker navigation/accept/dismiss.
        if self.is_composing() {
            return;
        }
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        if matches!(key, "up" | "down" | "enter" | "escape") {
            return;
        }
        if modifiers.platform && !modifiers.control && !modifiers.alt && !modifiers.shift {
            match key {
                "a" => {
                    self.edit.anchor = 0;
                    self.edit.cursor = self.edit.text.len();
                }
                "c" | "x" => {
                    let selection = self.edit.selection();
                    if !selection.is_empty() {
                        cx.write_to_clipboard(ClipboardItem::new_string(
                            self.edit.text[selection].into(),
                        ));
                        if key == "x" {
                            self.replace_text_in_range(None, "", window, cx);
                        }
                    }
                }
                "v" => {
                    if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                        self.replace_text_in_range(None, &text, window, cx);
                    }
                }
                _ => return,
            }
        } else if !modifiers.platform && !modifiers.control && !modifiers.alt {
            match key {
                "backspace" | "delete" => {
                    if self.edit.selection().is_empty() {
                        let offset = if key == "backspace" {
                            self.edit.previous()
                        } else {
                            self.edit.next()
                        };
                        self.edit.select_to(offset, true);
                    }
                    self.replace_text_in_range(None, "", window, cx);
                }
                "left" | "right" | "home" | "end" => {
                    let selection = self.edit.selection();
                    let offset = match key {
                        "home" => 0,
                        "end" => self.edit.text.len(),
                        "left" if !modifiers.shift && !selection.is_empty() => selection.start,
                        "right" if !modifiers.shift && !selection.is_empty() => selection.end,
                        "left" => self.edit.previous(),
                        _ => self.edit.next(),
                    };
                    self.edit.select_to(offset, modifiers.shift);
                }
                _ => return,
            }
        } else {
            // Option/dead-key text also goes through the native input handler.
            return;
        }
        cx.stop_propagation();
        window.prevent_default();
        cx.notify();
    }
}

impl EventEmitter<Changed> for SearchInput {}

impl Focusable for SearchInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl EntityInputHandler for SearchInput {
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
        let changed = self.edit.replace(range, text, false, None);
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
        let changed = self.edit.replace(range, text, true, selection);
        self.did_edit(changed, cx);
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let line = self.layout.as_ref()?;
        let bounds = self.bounds?;
        let range = byte_range(self.text(), range);
        let x = |index| {
            (bounds.left() + line.x_for_index(index) - self.scroll)
                .max(bounds.left())
                .min(bounds.right())
        };
        Some(Bounds::from_corners(
            point(x(range.start), bounds.top()),
            point(x(range.end), bounds.bottom()),
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
        self.layout.as_ref()?;
        Some(to_utf16(self.text(), self.mouse_index(position)))
    }
}

impl Render for SearchInput {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let input = cx.entity();
        let painter = input.clone();
        let height = px(self.font.line_height());
        div()
            .debug_selector(|| "theme-search".into())
            .w_full()
            .px_2()
            .py_1()
            .rounded(px(5.))
            .border_1()
            .border_color(rgb(self.theme.active))
            .bg(rgb(self.theme.background))
            .text_color(rgb(self.theme.foreground))
            .font_family(self.font.family.clone())
            .text_size(px(self.font.size))
            .line_height(height)
            .track_focus(&self.focus)
            .cursor(CursorStyle::IBeam)
            .on_key_down(cx.listener(Self::key_down))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                    window.focus(&this.focus);
                    cx.stop_propagation();
                    if !this.is_composing() {
                        let offset = this.mouse_index(event.position);
                        this.edit.select_to(offset, event.modifiers.shift);
                        if event.click_count >= 2 {
                            this.edit.anchor = 0;
                            this.edit.cursor = this.edit.text.len();
                        }
                        this.selecting = true;
                    }
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                if this.selecting && !this.is_composing() {
                    this.edit.select_to(this.mouse_index(event.position), true);
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if this.selecting {
                        this.selecting = false;
                        cx.stop_propagation();
                    }
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.selecting = false),
            )
            .child(
                canvas(
                    move |_, window, cx| {
                        let input = input.read(cx);
                        let placeholder = input.text().is_empty();
                        let text: gpui::SharedString = if placeholder {
                            input.placeholder.clone().into()
                        } else {
                            input.text().to_owned().into()
                        };
                        let run = TextRun {
                            len: text.len(),
                            font: window.text_style().font(),
                            color: rgb(if placeholder {
                                input.theme.muted
                            } else {
                                input.theme.foreground
                            })
                            .into(),
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        };
                        let runs = if let Some(marked) = &input.edit.marked {
                            vec![
                                TextRun {
                                    len: marked.start,
                                    ..run.clone()
                                },
                                TextRun {
                                    len: marked.len(),
                                    underline: Some(UnderlineStyle {
                                        color: Some(run.color),
                                        thickness: px(1.),
                                        wavy: false,
                                    }),
                                    ..run.clone()
                                },
                                TextRun {
                                    len: text.len() - marked.end,
                                    ..run
                                },
                            ]
                            .into_iter()
                            .filter(|run| run.len > 0)
                            .collect()
                        } else {
                            vec![run]
                        };
                        window
                            .text_system()
                            .shape_line(text, px(input.font.size), &runs, None)
                    },
                    move |bounds, line, window, cx| {
                        painter.update(cx, |input, cx| {
                            let visible_width = (bounds.size.width - px(1.)).max(px(0.));
                            let caret = line.x_for_index(input.edit.cursor);
                            input.scroll = input
                                .scroll
                                .min((line.width - visible_width).max(px(0.)))
                                .min(caret)
                                .max(caret - visible_width)
                                .max(px(0.));
                            if input.text().is_empty() {
                                input.scroll = px(0.);
                            }
                            let origin = point(bounds.left() - input.scroll, bounds.top());
                            window.handle_input(
                                &input.focus,
                                ElementInputHandler::new(bounds, cx.entity()),
                                cx,
                            );
                            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                                let selection = input.edit.selection();
                                if input.focus.is_focused(window) && !selection.is_empty() {
                                    window.paint_quad(fill(
                                        Bounds::from_corners(
                                            point(
                                                origin.x + line.x_for_index(selection.start),
                                                bounds.top(),
                                            ),
                                            point(
                                                origin.x + line.x_for_index(selection.end),
                                                bounds.bottom(),
                                            ),
                                        ),
                                        rgb(input.theme.active),
                                    ));
                                }
                                let _ = line.paint(origin, height, window, cx);
                                if input.focus.is_focused(window) {
                                    window.paint_quad(fill(
                                        Bounds::new(
                                            point(origin.x + caret, bounds.top()),
                                            size(px(1.), height),
                                        ),
                                        rgb(input.theme.cursor),
                                    ));
                                }
                            });
                            input.bounds = Some(bounds);
                            input.layout = Some(line);
                        });
                    },
                )
                .w_full()
                .h(height),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_ranges_are_clamped_to_scalar_boundaries() {
        let text = "a\u{1f600}\u{e9}\u{4e2d}";
        assert_eq!(byte_range(text, 1..3), 1..5);
        assert_eq!(byte_range(text, 2..99), 5..text.len());
        assert_eq!(byte_range(text, Range { start: 99, end: 1 }), 1..text.len());
        for (byte, _) in text.char_indices() {
            assert_eq!(from_utf16(text, to_utf16(text, byte)), byte);
        }
    }

    #[test]
    fn replaces_unicode_selection_and_explicit_utf16_range() {
        let mut edit = Editing::default();
        assert!(edit.replace(None, "a\u{1f600}\u{e9}z", false, None));
        edit.anchor = 7;
        edit.cursor = 1;
        assert_eq!(edit.selection(), 1..7);
        assert!(edit.replace(None, "\u{4e2d}", false, None));
        assert_eq!(edit.text, "a\u{4e2d}z");
        assert_eq!(edit.cursor, 4);
        assert!(edit.replace(Some(1..2), "\u{1f600}", false, None));
        assert_eq!(edit.text, "a\u{1f600}z");
        assert_eq!(edit.cursor, 5);
        assert_eq!(edit.previous(), 1);
        edit.select_to(1, false);
        assert_eq!(edit.next(), 5);
    }

    #[test]
    fn composition_selection_is_relative_to_inserted_text() {
        let mut edit = Editing::default();
        edit.replace(None, "\u{1f600}prefix", false, None);
        let start = edit.cursor;
        edit.replace(None, "\u{4e2d}\u{1f600}a", true, Some(1..3));
        assert_eq!(edit.selection(), start + 3..start + 7);
        assert_eq!(edit.marked, Some(start..start + 8));
        edit.replace(None, "\u{e9}", true, Some(0..1));
        assert_eq!(edit.selection(), start..start + 2);
        assert_eq!(edit.marked, Some(start..start + 2));
        assert!(!edit.replace(None, "\u{e9}", false, None));
        assert_eq!(edit.marked, None);
        assert_eq!(edit.selection(), start + 2..start + 2);
        assert_eq!(edit.text, "\u{1f600}prefix\u{e9}");
    }

    #[test]
    fn strips_newlines_and_maps_composition_selection() {
        let mut edit = Editing::default();
        edit.replace(None, "a\r\n\u{1f600}\nz", true, Some(3..6));
        assert_eq!(edit.text, "a\u{1f600}z");
        assert_eq!(edit.selection(), 1..5);
        edit.replace(None, "", true, None);
        assert!(edit.text.is_empty());
        assert_eq!(edit.marked, None);
        assert_eq!(edit.selection(), 0..0);
        assert!(!edit.replace(None, "\r\n", false, None));
    }

    #[test]
    fn extending_selection_can_cross_its_anchor() {
        let mut edit = Editing::default();
        edit.replace(None, "a\u{1f600}b", false, None);
        edit.select_to(1, false);
        edit.select_to(0, true);
        assert_eq!(edit.selection(), 0..1);
        edit.select_to(5, true);
        assert_eq!(edit.selection(), 1..5);
        edit.select_to(edit.next(), true);
        assert_eq!(edit.selection(), 1..6);
    }
}
