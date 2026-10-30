//! The Control keys every native macOS text field takes from Emacs: Ctrl-A
//! and Ctrl-E to the line's ends, Ctrl-B and Ctrl-F a character back and
//! forward, Ctrl-H and Ctrl-D delete one, Ctrl-K cuts to the end, and Ctrl-Y
//! puts it back. The other desktops use Ctrl for Select All, Copy, and the
//! rest, so there these keys keep that meaning.

use gpui::{Context, EntityInputHandler, Modifiers, Window};

use super::SearchInput;

impl SearchInput {
    /// Edits for `key` when it is one of these keys; whether it was.
    pub(super) fn control_key(
        &mut self,
        key: &str,
        modifiers: Modifiers,
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
        match key {
            "a" => self.edit.select_to(0, false),
            "e" => self.edit.select_to(self.edit.text.len(), false),
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
                    cut = self.edit.cursor..self.edit.text.len();
                }
                if !cut.is_empty() {
                    // A masked secret is never kept to put back elsewhere.
                    if !self.masked {
                        self.killed = self.edit.text[cut.clone()].to_owned();
                    }
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
}
