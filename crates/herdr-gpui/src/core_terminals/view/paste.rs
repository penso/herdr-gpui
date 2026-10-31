//! Pasting into a herdr-core pane: the clipboard's text goes to the
//! program as a paste, which the terminal brackets when the program asks.
//! The same keys paste as over a Herdr pane, and the Edit menu's Paste
//! lands here rather than in the Herdr pane behind this one.

use super::CoreTerminalView;
use gpui::{App, Keystroke};
use herdr_backend::ClientPaneInputEvent;

/// Whether `keystroke` pastes: Cmd-V, Ctrl-Shift-V, or Shift-Insert, and
/// plain Ctrl-V where it pastes (Windows) while the clipboard holds text.
pub(crate) fn pastes(
    keystroke: &Keystroke,
    ctrl_v_pastes: bool,
    clipboard_has_text: impl FnOnce() -> bool,
) -> bool {
    let modifiers = keystroke.modifiers;
    let v = keystroke.key.eq_ignore_ascii_case("v");
    let insert = keystroke.key == "insert";
    if modifiers.alt || modifiers.function {
        return false;
    }
    if (modifiers.platform && !modifiers.control && v)
        || (modifiers.control && modifiers.shift && (v || insert))
        || (modifiers.shift && !modifiers.control && !modifiers.platform && insert)
    {
        return true;
    }
    let plain_ctrl_v = modifiers.control && !modifiers.shift && !modifiers.platform && v;
    ctrl_v_pastes && plain_ctrl_v && clipboard_has_text()
}

impl CoreTerminalView {
    /// Sends the clipboard's text as a paste; false when it holds none or
    /// this view only watches.
    pub(super) fn paste_clipboard(&self, cx: &App) -> bool {
        if self.observe {
            return false;
        }
        let Some(text) = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .filter(|text| !text.is_empty())
        else {
            return false;
        };
        self.send(ClientPaneInputEvent::Paste(text));
        true
    }
}
