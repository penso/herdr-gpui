//! The terminal's accessibility node. It reads as a terminal, and while the
//! view takes typing, setting its value pastes the text at the program's
//! cursor, as it does over a Herdr pane, so dictation tools find a writable
//! target.

use super::CoreTerminalView;
use crate::i18n::fl;
use gpui::{accesskit::ActionData, prelude::*, *};
use sheprd_backend::ClientPaneInputEvent;

impl CoreTerminalView {
    pub(in crate::core_terminals) fn accessible(
        &self,
        root: Stateful<Div>,
        window: &Window,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        root.role(Role::Terminal)
            .aria_label(fl!("window-input-terminal"))
            // AccessKit maps Terminal to AXTextArea on macOS. SetValue makes
            // AXValue settable without changing the reading role.
            .when(self.takes_accessible_text(window), |root| {
                let view = cx.weak_entity();
                root.on_a11y_action(AccessibleAction::SetValue, move |data, window, cx| {
                    let _ = view.update(cx, |this, _| this.insert_accessible_text(data, window));
                })
            })
    }

    /// Whether inserted text would reach the program now: the view has the
    /// keyboard, types rather than watches, holds no IME composition, its
    /// program still runs, and its worker has not stopped on an error.
    fn takes_accessible_text(&self, window: &Window) -> bool {
        self.focus.is_focused(window)
            && !self.observe
            && self.marked.is_empty()
            && !self.ended()
            && self.screen.failure.is_none()
    }

    /// Pastes a set value. A delayed request is checked again, so it never
    /// types into a view that has since lost the keyboard.
    pub(in crate::core_terminals) fn insert_accessible_text(
        &self,
        data: Option<&ActionData>,
        window: &Window,
    ) {
        let Some(ActionData::Value(text)) = data else {
            return;
        };
        if text.is_empty() || !self.takes_accessible_text(window) {
            return;
        }
        self.send(ClientPaneInputEvent::Paste(text.to_string()));
    }
}
