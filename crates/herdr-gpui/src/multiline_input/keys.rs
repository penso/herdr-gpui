//! Key handling for the multi-line prompt field: the arrows move the caret
//! between visual rows, macOS keeps its Emacs keys, and the platform's edit
//! keys do what they do in every field.

use gpui::{ClipboardItem, Context, EntityInputHandler, KeyDownEvent, Window, px};

use super::{MultilineInput, caret_position, is_rtl, line_range, rows_total};
use crate::{actions, words};

impl MultilineInput {
    /// Move the caret one visual row up or down at its screen column,
    /// wrapping across logical lines. `goal_x` carries the column across a
    /// run of moves; it is reset by any other motion.
    fn vertical_move(&mut self, up: bool, extend: bool, cx: &mut Context<Self>) -> bool {
        let Some(bounds) = self.bounds else {
            return false;
        };
        let Some(lines) = self.lines.as_ref() else {
            return false;
        };
        if lines.is_empty() {
            return false;
        }
        let line_height = self.font.line_height();
        let Some(spot) = caret_position(lines, px(line_height), self.edit.cursor) else {
            return false;
        };
        let rows_before: usize = lines[..spot.line].iter().map(|line| line.rows.len()).sum();
        let total = rows_total(lines);
        let global = rows_before + spot.row;
        let target = match up {
            true => global.checked_sub(1),
            false => (global + 1 < total).then_some(global + 1),
        };
        let Some(target) = target else {
            return false;
        };
        let mut acc = 0;
        let mut found = None;
        for (line_index, line) in lines.iter().enumerate() {
            let next = acc + line.rows.len();
            if target < next {
                found = Some((line_index, target - acc));
                break;
            }
            acc = next;
        }
        let Some((line_index, row_index)) = found else {
            return false;
        };
        // The goal x in field-relative coordinates, set on the first move of
        // a run and held until any other motion.
        let goal = match self.goal_x {
            Some(goal) => goal,
            None => {
                let row = &lines[spot.line].rows[spot.row];
                let origin = if is_rtl() {
                    f32::from(bounds.size.width) - row.width()
                } else {
                    0.
                };
                let goal = origin + spot.x - row.start_x;
                self.goal_x = Some(goal);
                goal
            }
        };
        let line = &lines[line_index];
        let row = &line.rows[row_index];
        let origin = if is_rtl() {
            f32::from(bounds.size.width) - row.width()
        } else {
            0.
        };
        let local = (goal - origin).clamp(0., row.width());
        let byte = line.carets.index_at(px(local + row.start_x));
        let offset = line.range.start + byte.clamp(row.start, row.end);
        self.edit.select_to(offset, extend);
        cx.notify();
        true
    }

    /// Edits for `key` when it is one of macOS's Emacs keys: Ctrl-A and
    /// Ctrl-E to the line's ends, Ctrl-B and Ctrl-F a character back and
    /// forward, Ctrl-H and Ctrl-D delete one, Ctrl-K cuts to the line's
    /// end, and Ctrl-Y puts the cut text back. The other desktops use
    /// Ctrl for Select All, Copy, and the rest, so there these keys keep
    /// that meaning. Whether it was one.
    fn control_key(
        &mut self,
        key: &str,
        modifiers: gpui::Modifiers,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !cfg!(target_os = "macos")
            || !modifiers.control
            || modifiers.platform
            || modifiers.alt
            || modifiers.shift
            || modifiers.function
        {
            return false;
        }
        let range = line_range(&self.edit.text, self.edit.cursor);
        match key {
            "a" => self.edit.select_to(range.start, false),
            "e" => self.edit.select_to(range.end, false),
            "b" => {
                let offset = self.edit.previous();
                self.edit.select_to(offset, false);
            }
            "f" => {
                let offset = self.edit.next();
                self.edit.select_to(offset, false);
            }
            "h" | "d" => {
                if self.edit.selection().is_empty() {
                    let offset = if key == "h" {
                        self.edit.previous()
                    } else {
                        self.edit.next()
                    };
                    self.edit.select_to(offset, true);
                }
                self.replace_text_in_range(None, "", window, cx);
            }
            "k" => {
                // A selection is cut as it stands; otherwise the line's rest.
                let mut cut = self.edit.selection();
                if cut.is_empty() {
                    cut = self.edit.cursor..range.end;
                }
                if !cut.is_empty() {
                    self.killed = self.edit.text[cut.clone()].to_owned();
                    self.edit.anchor = cut.start;
                    self.edit.cursor = cut.end;
                    self.replace_text_in_range(None, "", window, cx);
                }
            }
            // With nothing cut yet, a selection stays rather than being
            // replaced by nothing.
            "y" if self.killed.is_empty() => {}
            "y" => {
                let killed = self.killed.clone();
                self.replace_text_in_range(None, &killed, window, cx);
            }
            _ => return false,
        }
        true
    }

    pub(super) fn key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Let the OS consume composition keys, including picker
        // navigation/accept/dismiss.
        if self.is_composing() {
            return;
        }
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        if key != "up" && key != "down" {
            self.goal_x = None;
        }
        // The arrows move the caret between visual rows when there is one;
        // otherwise they fall through to the window (completion menu,
        // prompt history).
        if matches!(key, "up" | "down") {
            if self.vertical_move(key == "up", modifiers.shift, cx) {
                cx.stop_propagation();
                window.prevent_default();
            }
            return;
        }
        // The window handles Enter and Escape.
        if matches!(key, "enter" | "escape") {
            return;
        }
        if self.control_key(key, modifiers, window, cx) {
            cx.stop_propagation();
            window.prevent_default();
            cx.notify();
            return;
        }
        let edit = actions::edit_modifiers(modifiers) && matches!(key, "a" | "c" | "v" | "x");
        if edit || (modifiers.platform && !modifiers.control && !modifiers.alt) {
            match key {
                // As in every macOS text field: Cmd-Left and Cmd-Right go to
                // the line's ends, Cmd-Backspace and Cmd-Delete delete to them.
                "left" | "right" => {
                    let range = line_range(&self.edit.text, self.edit.cursor);
                    let offset = if key == "left" {
                        range.start
                    } else {
                        range.end
                    };
                    self.edit.select_to(offset, modifiers.shift);
                }
                "backspace" | "delete" if !modifiers.shift => {
                    if self.edit.selection().is_empty() {
                        let range = line_range(&self.edit.text, self.edit.cursor);
                        let offset = if key == "backspace" {
                            range.start
                        } else {
                            range.end
                        };
                        self.edit.select_to(offset, true);
                    }
                    self.replace_text_in_range(None, "", window, cx);
                }
                _ if modifiers.shift && !edit => return,
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
                        "home" => line_range(&self.edit.text, self.edit.cursor).start,
                        "end" => line_range(&self.edit.text, self.edit.cursor).end,
                        "left" if !modifiers.shift && !selection.is_empty() => selection.start,
                        "right" if !modifiers.shift && !selection.is_empty() => selection.end,
                        "left" => self.edit.previous(),
                        _ => self.edit.next(),
                    };
                    self.edit.select_to(offset, modifiers.shift);
                }
                _ => return,
            }
        } else if actions::word_modifiers(modifiers)
            && matches!(key, "backspace" | "delete" | "left" | "right")
        {
            // macOS sends these as commands, never text, so the native
            // handler below would drop them.
            let selection = self.edit.selection();
            let backward = matches!(key, "backspace" | "left");
            let from = match key {
                "left" if !modifiers.shift && !selection.is_empty() => selection.start,
                "right" if !modifiers.shift && !selection.is_empty() => selection.end,
                _ => self.edit.cursor,
            };
            let offset = match backward {
                true => words::previous_start(&self.edit.text, from),
                false => words::next_end(&self.edit.text, from),
            };
            if matches!(key, "left" | "right") {
                self.edit.select_to(offset, modifiers.shift);
            } else {
                if selection.is_empty() {
                    self.edit.select_to(offset, true);
                }
                self.replace_text_in_range(None, "", window, cx);
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
