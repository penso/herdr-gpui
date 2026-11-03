// i18n: migrated
use gpui::{prelude::*, *};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

use super::{HerdrWindow, input::ViewInputHandler, words};
use crate::caret::Carets;
use crate::i18n::fl;

/// Rewrites an edit, given the text left of the range it replaces, the text
/// typed, and the text right of the range: what to insert there, and how the
/// start of the text after it changes, which a deletion can.
type Format = fn(&str, &str, &str) -> crate::worktree::Edit;

// Byte offsets internally; only the platform input boundary uses UTF-16.
#[derive(Default)]
pub(super) struct DialogInput {
    pub text: String,
    pub selection: Range<usize>,
    pub reversed: bool,
    pub marked: Option<Range<usize>>,
    pub bounds: Bounds<Pixels>,
    /// Where the last drawn text puts each caret.
    carets: Option<Carets>,
    scroll: Pixels,
    /// Rewrites each edit. Composition is shown as typed and only its commit
    /// is rewritten, so the IME is never fought mid-word.
    format: Option<Format>,
    /// What a screen reader calls the field, when a caption beside it
    /// names it rather than the field itself.
    pub label: Option<SharedString>,
}

impl DialogInput {
    pub fn new(text: String) -> Self {
        Self {
            selection: 0..text.len(),
            text,
            ..Self::default()
        }
    }

    /// The new worktree dialog's branch field, which keeps out what Git
    /// refuses as it is typed.
    pub fn branch(text: String) -> Self {
        Self {
            format: Some(crate::worktree::format_edit),
            label: Some(fl!("dialog-input-branch").into()),
            ..Self::new(text)
        }
    }

    pub fn range_from_utf16(&self, range: Range<usize>) -> Range<usize> {
        let start = byte_offset(&self.text, range.start);
        start..byte_offset(&self.text, range.end).max(start)
    }

    pub fn to_utf16(&self, range: Range<usize>) -> Range<usize> {
        self.text[..range.start].encode_utf16().count()
            ..self.text[..range.end].encode_utf16().count()
    }

    pub fn replace(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        marked: bool,
        selected: Option<Range<usize>>,
    ) {
        let range = range
            .map(|r| self.range_from_utf16(r))
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.selection.clone());
        let formatted = self.format.filter(|_| !marked).map(|format| {
            // Cut to length first, so the cut cannot undo how it was joined.
            let text: String = text.chars().take(4096).collect();
            format(&self.text[..range.start], &text, &self.text[range.end..])
        });
        let (text, range, after) = match formatted {
            // Text that is all left out changes nothing, so a stray key does
            // not also delete the selection it was typed over; a composition
            // being committed is still removed.
            Some(edit) if edit.inserted.is_empty() && !text.is_empty() && self.marked.is_none() => {
                return;
            }
            // What follows the range can give way to what now comes before it.
            // The formatter has already left out control characters.
            Some(edit) => {
                let end = range.end + edit.replaced;
                (edit.inserted, range.start..end, edit.after)
            }
            None => (
                text.chars()
                    .filter(|c| !c.is_control())
                    .take(4096)
                    .collect(),
                range,
                String::new(),
            ),
        };
        if self.text.len() - range.len() + text.len() + after.len() > 16384 {
            return;
        }
        self.text
            .replace_range(range.clone(), &format!("{text}{after}"));
        let mut end = range.start + text.len();
        if !marked {
            // What was typed can join the cluster after it, as a letter typed
            // before a combining accent does; the caret goes after the cluster.
            end = self
                .text
                .grapheme_indices(true)
                .map(|(i, _)| i)
                .find(|i| *i >= end)
                .unwrap_or(self.text.len());
        }
        self.selection = if let Some(selected) = selected {
            let start = byte_offset(&text, selected.start);
            range.start + start..range.start + byte_offset(&text, selected.end).max(start)
        } else {
            end..end
        };
        self.reversed = false;
        self.marked = (marked && !text.is_empty()).then_some(range.start..end);
        self.carets = None;
    }

    /// Ends a composition where it stands, as when the platform unmarks the
    /// text rather than committing it. A formatted field commits the text as
    /// if it had been typed, so it is kept out like any other edit.
    pub fn unmark(&mut self) {
        let Some(range) = self.marked.clone() else {
            return;
        };
        if self.format.is_none() {
            self.marked = None;
            return;
        }
        let text = self.text[range].to_owned();
        self.replace(None, &text, false, None);
        self.marked = None;
    }

    fn cursor(&self) -> usize {
        if self.reversed {
            self.selection.start
        } else {
            self.selection.end
        }
    }

    fn move_to(&mut self, offset: usize, extend: bool) {
        let anchor = if self.reversed {
            self.selection.end
        } else {
            self.selection.start
        };
        self.selection = if extend {
            anchor.min(offset)..anchor.max(offset)
        } else {
            offset..offset
        };
        self.reversed = extend && offset < anchor;
    }

    pub fn key(&mut self, key: &Keystroke, cx: &mut App) -> bool {
        if self.marked.is_some() {
            return false;
        }
        let cursor = self.cursor();
        let previous = self
            .text
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .rfind(|i| *i < cursor)
            .unwrap_or(0);
        let next = self
            .text
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .find(|i| *i > cursor)
            .unwrap_or(self.text.len());
        match key.key.as_str() {
            "a" if crate::actions::edit_modifiers(key.modifiers) => {
                self.selection = 0..self.text.len();
                self.reversed = false;
            }
            "v" if crate::actions::edit_modifiers(key.modifiers) => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    self.replace(None, &text, false, None);
                }
            }
            "c" | "x" if crate::actions::edit_modifiers(key.modifiers) => {
                if !self.selection.is_empty() {
                    cx.write_to_clipboard(ClipboardItem::new_string(
                        self.text[self.selection.clone()].into(),
                    ));
                    if key.key == "x" {
                        self.replace(None, "", false, None);
                    }
                }
            }
            "left" | "right" | "backspace" | "delete"
                if crate::actions::word_modifiers(key.modifiers) =>
            {
                let selected = !self.selection.is_empty();
                let from = match key.key.as_str() {
                    "left" if !key.modifiers.shift && selected => self.selection.start,
                    "right" if !key.modifiers.shift && selected => self.selection.end,
                    _ => cursor,
                };
                let offset = match key.key.as_str() {
                    "left" | "backspace" => words::previous_start(&self.text, from),
                    _ => words::next_end(&self.text, from),
                };
                if matches!(key.key.as_str(), "left" | "right") {
                    self.move_to(offset, key.modifiers.shift);
                } else {
                    if !selected {
                        self.selection = offset.min(cursor)..offset.max(cursor);
                    }
                    self.replace(None, "", false, None);
                }
            }
            "left" | "right" | "home" | "end" => {
                let offset = match key.key.as_str() {
                    "home" => 0,
                    "end" => self.text.len(),
                    "left" if key.modifiers.platform => 0,
                    "right" if key.modifiers.platform => self.text.len(),
                    "left" if !key.modifiers.shift && !self.selection.is_empty() => {
                        self.selection.start
                    }
                    "right" if !key.modifiers.shift && !self.selection.is_empty() => {
                        self.selection.end
                    }
                    "left" => previous,
                    _ => next,
                };
                self.move_to(offset, key.modifiers.shift);
            }
            "backspace" | "delete" => {
                if self.selection.is_empty() {
                    // Cmd deletes to the line's start or end, as macOS fields do.
                    self.selection = match (key.key.as_str(), key.modifiers.platform) {
                        ("backspace", true) => 0..cursor,
                        ("backspace", false) => previous..cursor,
                        (_, true) => cursor..self.text.len(),
                        (_, false) => cursor..next,
                    };
                }
                self.replace(None, "", false, None);
            }
            _ => return false,
        }
        true
    }

    pub fn index_at(&self, position: Point<Pixels>) -> Option<usize> {
        self.carets
            .as_ref()
            .map(|carets| carets.index_at(position.x - self.bounds.left() + self.scroll))
    }

    pub fn range_bounds(&self, range: Range<usize>) -> Option<Bounds<Pixels>> {
        let range = self.range_from_utf16(range);
        let extent = self.carets.as_ref()?.extent(range);
        let x = |x| {
            (self.bounds.left() + x - self.scroll).clamp(self.bounds.left(), self.bounds.right())
        };
        Some(Bounds::from_corners(
            point(x(extent.start), self.bounds.top()),
            point(x(extent.end), self.bounds.bottom()),
        ))
    }
}

fn byte_offset(text: &str, offset: usize) -> usize {
    let mut units = 0;
    for (byte, ch) in text.char_indices() {
        if units >= offset {
            return byte;
        }
        units += ch.len_utf16();
    }
    text.len()
}

impl HerdrWindow {
    pub(super) fn render_dialog_input(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let entity = cx.entity();
        let focus = self.menu.focus.clone();
        div()
            .id("dialog-input")
            .debug_selector(|| "dialog-input".into())
            .role(Role::TextInput)
            .aria_value(
                self.menu
                    .input
                    .as_ref()
                    .map(|input| SharedString::from(input.text.clone()))
                    .unwrap_or_default(),
            )
            .when_some(
                self.menu
                    .input
                    .as_ref()
                    .and_then(|input| input.label.clone()),
                |field, label| field.aria_label(label),
            )
            .w_full()
            .h(px(self.config.ui.line_height() + 10.))
            .px(px(6.))
            .py(px(5.))
            .rounded(px(crate::config::corners::CONTROL))
            .bg(rgb(self.theme.active))
            .overflow_hidden()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    // Another field of the same dialog may hold focus.
                    window.focus(&this.menu.focus, cx);
                    if let Some(input) = this.menu.input.as_mut()
                        && input.marked.is_none()
                        && let Some(index) = input.index_at(event.position)
                    {
                        input.move_to(index, event.modifiers.shift);
                        cx.notify();
                    }
                }),
            )
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, cx| {
                        // Unfocused, the draft reads as plain text: no caret or
                        // selection that would look like it is about to be replaced.
                        let focused = focus.is_focused(window);
                        window.handle_input(
                            &focus,
                            ViewInputHandler::new(bounds, entity.clone()),
                            cx,
                        );
                        entity.update(cx, |this, cx| {
                            let Some(input) = this.menu.input.as_mut() else {
                                return;
                            };
                            let run = TextRun {
                                len: input.text.len(),
                                font: this.config.ui.font(),
                                color: rgb(this.theme.foreground).into(),
                                background_color: None,
                                underline: None,
                                strikethrough: None,
                            };
                            let line = window.text_system().shape_line(
                                input.text.clone().into(),
                                px(this.config.ui.size),
                                &[run],
                                None,
                            );
                            let carets = Carets::new(&input.text, &line);
                            let caret = carets.x(input.cursor());
                            input.scroll = input
                                .scroll
                                .min(caret)
                                .max(caret - bounds.size.width + px(2.))
                                .max(px(0.));
                            // Right to left, a line that fits sets against
                            // the field's right, its start.
                            let room = bounds.size.width - px(2.);
                            if crate::direction::is_rtl() && line.width < room {
                                input.scroll = line.width - room;
                            }
                            let origin = bounds.origin - point(input.scroll, px(0.));
                            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                                // Across a change of direction, a selection
                                // is a stretch on each side of it.
                                if focused && !input.selection.is_empty() {
                                    for span in carets.spans(input.selection.clone()) {
                                        window.paint_quad(fill(
                                            Bounds::from_corners(
                                                point(origin.x + span.start, bounds.top()),
                                                point(origin.x + span.end, bounds.bottom()),
                                            ),
                                            rgb(this.theme.muted),
                                        ));
                                    }
                                }
                                let _ = line.paint(
                                    origin,
                                    bounds.size.height,
                                    TextAlign::Left,
                                    None,
                                    window,
                                    cx,
                                );
                                if focused {
                                    window.paint_quad(fill(
                                        Bounds::new(
                                            point(origin.x + caret, bounds.top()),
                                            size(px(1.), bounds.size.height),
                                        ),
                                        rgb(this.theme.foreground),
                                    ));
                                }
                                for span in input
                                    .marked
                                    .clone()
                                    .map(|marked| carets.spans(marked))
                                    .unwrap_or_default()
                                {
                                    window.paint_quad(fill(
                                        Bounds::new(
                                            point(origin.x + span.start, bounds.bottom() - px(1.)),
                                            size(span.end - span.start, px(1.)),
                                        ),
                                        rgb(this.theme.foreground),
                                    ));
                                }
                            });
                            input.bounds = bounds;
                            input.carets = Some(carets);
                        });
                    },
                )
                .size_full(),
            )
    }
}

#[cfg(test)]
mod tests;
