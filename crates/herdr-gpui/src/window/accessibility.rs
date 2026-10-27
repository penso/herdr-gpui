//! The focused terminal's accessibility insertion action. Its transcript stays
//! a terminal, not a locally editable document: setting AXValue inserts text at
//! the daemon's cursor, never replaces the displayed output or selection.

use super::HerdrWindow;
use crate::terminal::{InputTarget, Transcript};
use gpui::{accesskit::ActionData, prelude::*, *};
use herdr_client::protocol::{ClientPaneInputEvent, MAX_FRAME_SIZE};

/// The input destination represented by the last rendered node. A delayed
/// accessibility request must not type into a different connection or popup.
#[derive(PartialEq, Eq)]
struct InsertionTarget {
    connection: (u64, u64),
    boot: String,
    input: InputTarget,
}

impl HerdrWindow {
    pub(super) fn accessible_terminal(
        &self,
        terminal: Stateful<Div>,
        window: &Window,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        terminal
            .track_focus(&self.focus)
            .role(Role::Terminal)
            .aria_label("Terminal")
            // AccessKit maps Terminal to AXTextArea on macOS. SetValue makes
            // AXValue settable without changing the transcript's reading role.
            .when_some(
                self.accessible_insertion_target(window),
                |terminal, target| {
                    let view = cx.weak_entity();
                    terminal.on_a11y_action(AccessibleAction::SetValue, move |data, window, cx| {
                        let _ = view.update(cx, |this, cx| {
                            this.insert_accessible_text(&target, data, window, cx);
                        });
                    })
                },
            )
            // Keep selection tools and screen readers on the existing visible
            // rows, built only while an assistive client is listening.
            .when(
                window.is_a11y_active() && self.live.surface_ready(),
                |terminal| {
                    let surface = self.live.surface.clone();
                    let selection = self.selection.clone();
                    let cell_width = self.cell_width;
                    let cell_height = self.config.terminal.line_height();
                    let origin = (
                        f32::from(self.bounds.origin.x),
                        f32::from(self.bounds.origin.y),
                    );
                    let scale = window.scale_factor();
                    terminal.a11y_synthetic_children(move |builder| {
                        if let Some(transcript) = surface.as_deref().and_then(|surface| {
                            Transcript::read(surface, selection.as_ref(), cell_width, cell_height)
                        }) {
                            transcript.expose(builder, origin, scale);
                        }
                    })
                },
            )
    }

    fn accessible_insertion_target(&self, window: &Window) -> Option<InsertionTarget> {
        if !self.focus.is_focused(window)
            || self.menu.page.is_some()
            || self.copy_mode_active()
            || !self.marked.is_empty()
            || self.mouse_focus_pending()
            || !self.live.status.is_connected()
            || !self.input_ready()
        {
            return None;
        }
        Some(InsertionTarget {
            connection: (
                self.selection_epoch,
                self.endpoints[self.selected_endpoint].generation,
            ),
            boot: self.live.snapshot.as_ref()?.boot_id.clone(),
            input: self.focused_input_target()?,
        })
    }

    fn insert_accessible_text(
        &mut self,
        target: &InsertionTarget,
        data: Option<&ActionData>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(ActionData::Value(text)) = data else {
            return;
        };
        if text.is_empty() || self.accessible_insertion_target(window).as_ref() != Some(target) {
            return;
        }
        // Bound the extra allocation before entering the semantic input queue;
        // the client also checks the size of the complete encoded message.
        if text.len() > MAX_FRAME_SIZE {
            self.local_error = Some(format!(
                "Accessibility input not sent: {}",
                herdr_client::protocol::Error::FrameLimit
            ));
            cx.notify();
            return;
        }
        self.shift_taps.cancel();
        self.clear_retained_selection(cx);
        // Paste preserves bracketed-paste policy at the daemon and the ordered,
        // bounded input queue. No clipboard read or simulated key is involved.
        self.send(ClientPaneInputEvent::Paste(text.to_string()), cx);
    }
}

#[cfg(test)]
mod tests;
