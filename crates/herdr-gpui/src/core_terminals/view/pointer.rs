//! The pointer over a herdr-core pane. A program that reads the mouse gets
//! its presses, drags, and wheel; otherwise a press selects text, a double
//! click a word and a triple click a line, and the wheel scrolls history.
//! Shift selects even over a program that reads the mouse, as terminals do.

use super::CoreTerminalView;
use crate::{
    core_terminals::selection::{Cell, Selection},
    terminal::{PLATFORM_NOTCH_LINES, wheel_rows},
    terminal_painter::{Highlight, Tint},
};
use gpui::{prelude::*, *};
use herdr_backend::{
    ClientMouseButton, ClientMouseKind, ClientMousePosition, ClientPaneInputEvent,
};
use std::num::NonZeroU16;

/// Where a press is going: to the program, or into a selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Press {
    Reported(ClientMouseButton),
    Selecting,
}

fn button(button: MouseButton) -> Option<ClientMouseButton> {
    match button {
        MouseButton::Left => Some(ClientMouseButton::Left),
        MouseButton::Right => Some(ClientMouseButton::Right),
        MouseButton::Middle => Some(ClientMouseButton::Middle),
        _ => None,
    }
}

fn modifier_bits(modifiers: Modifiers) -> u8 {
    u8::from(modifiers.shift)
        | (u8::from(modifiers.control) << 1)
        | (u8::from(modifiers.alt) << 2)
        | (u8::from(modifiers.platform) << 3)
}

impl CoreTerminalView {
    /// The cell under `position`, held inside the screen.
    fn cell_at(&self, position: Point<Pixels>) -> Option<Cell> {
        let bounds = self.bounds?;
        let frame = self.screen.frame.as_ref()?;
        if frame.width == 0 || frame.height == 0 {
            return None;
        }
        let x = f32::from(position.x - bounds.origin.x) / self.look.cell_width.max(1.);
        let y = f32::from(position.y - bounds.origin.y) / self.look.cell_height.max(1.);
        // Clamped to the screen first, so the casts cannot truncate.
        let column = x.clamp(0., f32::from(frame.width - 1)) as u16;
        let row = y.clamp(0., f32::from(frame.height - 1)) as u16;
        Some(Cell { row, column })
    }

    fn report(&self, kind: ClientMouseKind, cell: Cell, modifiers: Modifiers, lines: u16) {
        self.send(ClientPaneInputEvent::Mouse {
            kind,
            position: ClientMousePosition::Cell {
                column: cell.column,
                row: cell.row,
            },
            geometry: None,
            modifiers: modifier_bits(modifiers),
            lines,
        });
    }

    pub(super) fn mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus, cx);
        let Some(cell) = self.cell_at(event.position) else {
            return;
        };
        if self.screen.mouse_reporting && !event.modifiers.shift {
            if let Some(pressed) = button(event.button) {
                self.press = Some(Press::Reported(pressed));
                self.report(ClientMouseKind::Down(pressed), cell, event.modifiers, 1);
            }
            return;
        }
        if event.button != MouseButton::Left {
            return;
        }
        let Some(frame) = self.screen.frame.as_deref() else {
            return;
        };
        self.selection = Some(match event.click_count {
            2 => Selection::word(frame, cell),
            count if count >= 3 => Selection::line(frame, cell),
            _ => Selection::at(cell),
        });
        self.press = Some(Press::Selecting);
        cx.notify();
    }

    pub(super) fn mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(cell) = self.cell_at(event.position) else {
            return;
        };
        match self.press {
            Some(Press::Reported(pressed)) if event.pressed_button.is_some() => {
                self.report(ClientMouseKind::Drag(pressed), cell, event.modifiers, 1);
            }
            Some(Press::Selecting) if event.pressed_button == Some(MouseButton::Left) => {
                if let Some(selection) = &mut self.selection {
                    selection.extend(cell);
                    cx.notify();
                }
            }
            // Movement with no button held, for a program that asks for
            // every motion (mode 1003); the encoder drops it otherwise.
            None if event.pressed_button.is_none()
                && self.screen.mouse_reporting
                && !event.modifiers.shift
                && self.hovered != Some(cell) =>
            {
                self.hovered = Some(cell);
                self.report(ClientMouseKind::Moved, cell, event.modifiers, 1);
            }
            _ => {}
        }
    }

    pub(super) fn mouse_up(&mut self, event: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        if let Some(Press::Reported(pressed)) = self.press
            && let Some(cell) = self.cell_at(event.position)
        {
            self.report(ClientMouseKind::Up(pressed), cell, event.modifiers, 1);
        }
        self.press = None;
    }

    /// Whole rows of wheel motion, the remainder kept for the next event,
    /// as one event carrying the count: the terminal scrolls history by it,
    /// or repeats it as reports for a program reading the wheel.
    pub(super) fn scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        let Some(cell) = self.cell_at(event.position) else {
            return;
        };
        let notch = NonZeroU16::new(PLATFORM_NOTCH_LINES).unwrap_or(NonZeroU16::MIN);
        self.wheel_rest += wheel_rows(event, self.look.cell_height, notch);
        let steps = self.wheel_rest.trunc();
        self.wheel_rest -= steps;
        if steps == 0. {
            return;
        }
        let kind = if steps > 0. {
            ClientMouseKind::ScrollUp
        } else {
            ClientMouseKind::ScrollDown
        };
        // Clamped to a u16 first, so the cast cannot truncate.
        let lines = steps.abs().min(f32::from(u16::MAX)) as u16;
        self.report(kind, cell, event.modifiers, lines);
    }

    /// Copies the selection; false when nothing is selected.
    pub(super) fn copy_selection(&self, cx: &mut App) -> bool {
        let Some(frame) = self.screen.frame.as_deref() else {
            return false;
        };
        let Some(text) = self
            .selection
            .filter(|selection| !selection.is_empty())
            .map(|selection| selection.text(frame))
        else {
            return false;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        true
    }

    /// The selection as the painter tints it.
    pub(super) fn highlights(&self) -> Vec<Highlight> {
        let width = self.screen.frame.as_ref().map_or(0, |frame| frame.width);
        self.selection
            .map(|selection| {
                selection
                    .rows(width)
                    .into_iter()
                    .map(|(row, columns)| Highlight {
                        row,
                        columns,
                        tint: Tint::Selection,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}
