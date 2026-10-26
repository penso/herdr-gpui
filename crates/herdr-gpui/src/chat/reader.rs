//! Keyboard ownership and pointer dragging in the transcript. The reader has
//! no text input handler: copying its selection must never send Ctrl-C to a CLI.

use super::{chats::ChatPane, selection::Caret};
use crate::HerdrWindow;
use gpui::*;
use herdr_chat_protocol::EntryId;
use std::time::Instant;

#[derive(Default)]
pub(crate) struct Drag {
    point: Option<Point<Pixels>>,
    stepped: Option<Instant>,
}

impl ChatPane {
    /// A held pointer beyond the list continues selecting through virtualized
    /// rows, at a bounded speed independent of frame rate.
    pub(super) fn tick_text_drag(&mut self, now: Instant) -> bool {
        let Some(point) = self.text_drag.point else {
            return false;
        };
        let selected = self
            .text_geometry
            .hit(point)
            .is_some_and(|caret| self.selection.drag(self.prepared.text(), caret));
        let viewport = self.list.viewport_bounds();
        if viewport.size.height <= px(0.) {
            return selected;
        }
        let edge = px(24.);
        let outside = if point.y < viewport.top() + edge {
            f32::from(point.y - viewport.top() - edge)
        } else if point.y > viewport.bottom() - edge {
            f32::from(point.y - viewport.bottom() + edge)
        } else {
            self.text_drag.stepped = None;
            return selected;
        };
        let seconds = self.text_drag.stepped.map_or(1. / 60., |last| {
            now.saturating_duration_since(last).as_secs_f32().min(0.05)
        });
        self.text_drag.stepped = Some(now);
        let distance = outside.clamp(-150., 150.) * 6. * seconds;
        let before = self.list.logical_scroll_top();
        self.list.scroll_by(px(distance));
        let after = self.list.logical_scroll_top();
        selected || after.item_ix != before.item_ix || after.offset_in_item != before.offset_in_item
    }
}

impl HerdrWindow {
    /// Selectable labels consume mouse-down to keep keyboard focus in the
    /// reader. A plain click still opens their disclosure; a drag never does.
    pub(in crate::chat) fn click_chat_card_text(
        &mut self,
        pane_id: &str,
        entry: EntryId,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some(pane) = self.chats.pane_mut(pane_id) else {
            return;
        };
        if !pane.text_geometry.card_click(entry, position) {
            return;
        }
        pane.toggle_expanded(entry);
        self.release_chat_text(pane_id);
        cx.stop_propagation();
        self.redraw_terminal(cx);
        cx.notify();
    }

    pub(in crate::chat) fn chat_reader_focused(&self, pane_id: &str, window: &Window) -> bool {
        self.chats
            .pane(pane_id)
            .is_some_and(|pane| pane.reading(window))
    }

    pub(in crate::chat) fn press_chat_text(
        &mut self,
        pane_id: &str,
        caret: Caret,
        clicks: usize,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pane) = self.chats.pane_mut(pane_id) else {
            return;
        };
        if !pane
            .selection
            .press(pane.prepared.text(), caret, clicks, extend)
        {
            return;
        }
        pane.text_drag = Default::default();
        let focus = pane
            .text_focus
            .get_or_insert_with(|| cx.focus_handle())
            .clone();
        window.focus(&focus, cx);
        cx.stop_propagation();
        self.note_input();
        if self.input_ready() {
            self.focus_clicked_pane(pane_id, cx);
        }
        self.redraw_terminal(cx);
        cx.notify();
    }

    pub(in crate::chat) fn drag_chat_text(
        &mut self,
        pane_id: &str,
        caret: Caret,
        cx: &mut Context<Self>,
    ) {
        let Some(pane) = self.chats.pane_mut(pane_id) else {
            return;
        };
        if pane.selection.drag(pane.prepared.text(), caret) {
            self.redraw_terminal(cx);
            cx.notify();
        }
    }

    pub(in crate::chat) fn move_chat_text(
        &mut self,
        pane_id: &str,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(pane) = self.chats.pane_mut(pane_id) else {
            return;
        };
        if !pane.selection.is_selecting() {
            return;
        }
        if event.pressed_button != Some(MouseButton::Left) {
            self.release_chat_text(pane_id);
            return;
        }
        pane.text_drag.point = Some(event.position);
        let caret = pane.text_geometry.hit(event.position);
        if let Some(caret) = caret {
            self.drag_chat_text(pane_id, caret, cx);
        }
        // The pointer can be stationary outside the viewport while the list
        // scrolls. Its next animation tick uses the newly mounted text geometry.
        self.redraw_terminal(cx);
        cx.notify();
    }

    pub(in crate::chat) fn release_chat_text(&mut self, pane_id: &str) {
        if let Some(pane) = self.chats.pane_mut(pane_id) {
            pane.selection.release();
            pane.text_geometry.release();
            pane.text_drag = Default::default();
        }
    }

    pub(in crate::chat) fn copy_chat_selection(
        &mut self,
        pane_id: &str,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.chat_reader_focused(pane_id, window) {
            return false;
        }
        if let Some(pane) = self.chats.pane(pane_id) {
            if let Some(text) = pane.selection.text(pane.prepared.text()) {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            } else if !pane.selection.is_empty() {
                self.show_flash(
                    crate::window::Flash::warning("Select less text to copy"),
                    cx,
                );
            }
        }
        cx.stop_propagation();
        true
    }

    /// Handles reader keys before any composer or hidden terminal can see them.
    pub(in crate::chat) fn chat_reader_key(
        &mut self,
        pane_id: &str,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.chat_reader_focused(pane_id, window) {
            return false;
        }
        let key = &event.keystroke;
        let edit = crate::actions::edit_modifiers(key.modifiers)
            || ((key.modifiers.platform != key.modifiers.control)
                && !key.modifiers.alt
                && !key.modifiers.function);
        if key.key == "c" && edit {
            self.copy_chat_selection(pane_id, window, cx);
        } else if let Some(pane) = self.chats.pane_mut(pane_id) {
            match key.key.as_str() {
                "a" if edit => {
                    pane.selection.select_all(pane.prepared.text());
                }
                "escape" if !key.modifiers.modified() => {
                    pane.selection.clear();
                }
                "up" | "down" | "pageup" | "pagedown" | "space"
                    if !key.modifiers.control && !key.modifiers.alt && !key.modifiers.platform =>
                {
                    let direction = if key.key == "up"
                        || key.key == "pageup"
                        || (key.key == "space" && key.modifiers.shift)
                    {
                        -1.
                    } else {
                        1.
                    };
                    let amount = if key.key == "up" || key.key == "down" {
                        36.
                    } else {
                        f32::from(pane.list.viewport_bounds().size.height) * 0.85
                    };
                    pane.list.scroll_by(px(direction * amount));
                    pane.glide.user_scrolled();
                }
                "home" if !key.modifiers.alt => {
                    pane.list.scroll_to(ListOffset {
                        item_ix: 0,
                        offset_in_item: px(0.),
                    });
                    pane.glide.user_scrolled();
                }
                "end" if !key.modifiers.alt => {
                    pane.list.scroll_to(ListOffset {
                        item_ix: pane.list.item_count(),
                        offset_in_item: px(0.),
                    });
                    pane.glide.user_scrolled();
                }
                _ => {}
            }
        }
        cx.stop_propagation();
        window.prevent_default();
        self.redraw_terminal(cx);
        cx.notify();
        true
    }
}
