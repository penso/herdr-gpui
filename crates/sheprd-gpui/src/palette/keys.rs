//! Keyboard handling while the palette has focus.
//!
//! The list moves as in Telescope and snacks.picker: arrows, Tab and
//! Shift-Tab, Ctrl-N/Ctrl-P, and Ctrl-J/Ctrl-K. Filters cycle with
//! Ctrl-Tab/Ctrl-Shift-Tab or Alt-]/Alt-[, and Alt-P toggles Go To's preview.

use super::{AgentFilter, Filter};
use crate::SheprdWindow;
use gpui::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Direction {
    Up,
    Down,
}

/// What a key does in the palette, apart from typing into its query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaletteKey {
    Move(Direction),
    CycleFilter(Direction),
    TogglePreview,
    Activate,
    Dismiss,
}

impl PaletteKey {
    fn of(keystroke: &Keystroke) -> Option<Self> {
        let modifiers = keystroke.modifiers;
        let plain = !modifiers.control && !modifiers.alt && !modifiers.platform;
        let control = modifiers.control && !modifiers.alt && !modifiers.platform;
        let alt = modifiers.alt && !modifiers.control && !modifiers.platform;
        let key = keystroke.key.as_str();
        Some(match key {
            "escape" => Self::Dismiss,
            "enter" => Self::Activate,
            "up" if plain => Self::Move(Direction::Up),
            "down" if plain => Self::Move(Direction::Down),
            "tab" if plain && modifiers.shift => Self::Move(Direction::Up),
            "tab" if plain => Self::Move(Direction::Down),
            "tab" if control && modifiers.shift => Self::CycleFilter(Direction::Up),
            "tab" if control => Self::CycleFilter(Direction::Down),
            "p" | "k" if control && !modifiers.shift => Self::Move(Direction::Up),
            "n" | "j" if control && !modifiers.shift => Self::Move(Direction::Down),
            "[" if alt => Self::CycleFilter(Direction::Up),
            "]" if alt => Self::CycleFilter(Direction::Down),
            "p" if alt && !modifiers.shift => Self::TogglePreview,
            _ => return None,
        })
    }
}

impl SheprdWindow {
    pub(crate) fn palette_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Herdr's navigate-mode keys move a Go To list as its arrows do.
        let navigation = self
            .menu
            .palette
            .as_ref()
            .filter(|palette| palette.filter == Filter::Navigation)
            .and_then(|_| self.keymap().navigates_workspace(&event.keystroke))
            .map(|up| PaletteKey::Move(if up { Direction::Up } else { Direction::Down }));
        let Some(palette) = &mut self.menu.palette else {
            return;
        };
        if palette.search.read(cx).is_composing() {
            return;
        }
        // A Go To navigation binding keeps its key; otherwise Alt and a
        // navigator letter filter by agent status without typing the letter.
        if navigation.is_none()
            && let Some(agent_filter) = AgentFilter::from_keystroke(&event.keystroke)
        {
            cx.stop_propagation();
            window.prevent_default();
            self.set_palette_agent_filter(agent_filter, cx);
            return;
        }
        let Some(key) = navigation.or_else(|| PaletteKey::of(&event.keystroke)) else {
            return;
        };
        cx.stop_propagation();
        window.prevent_default();
        match key {
            PaletteKey::Dismiss => self.dismiss_menu(window, cx),
            PaletteKey::Move(direction) => {
                let count = palette.filtered.len();
                if count == 0 {
                    return;
                }
                palette.restore = None;
                palette.selected = match direction {
                    Direction::Up => (palette.selected + count - 1) % count,
                    Direction::Down => (palette.selected + 1) % count,
                };
                palette
                    .scroll
                    .scroll_to_item(palette.selected, ScrollStrategy::Center);
                cx.notify();
            }
            PaletteKey::CycleFilter(direction) => {
                let count = Filter::ALL.len();
                let index = Filter::ALL
                    .iter()
                    .position(|filter| *filter == palette.filter)
                    .unwrap_or(0);
                let index = match direction {
                    Direction::Up => (index + count - 1) % count,
                    Direction::Down => (index + 1) % count,
                };
                self.set_palette_filter(Filter::ALL[index], cx);
            }
            PaletteKey::TogglePreview => {
                self.menu.palette_preview = !self.menu.palette_preview;
                cx.notify();
            }
            PaletteKey::Activate => {
                if let Some(entry) = palette.selected_entry() {
                    let action = entry.action.clone();
                    self.activate_palette(action, window, cx);
                }
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
