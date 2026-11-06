//! Pass-through mode: the keys the app would claim go to the focused Herdr
//! pane instead, for full-screen programs that want them, until the command's
//! own shortcut ends it. The prefix does not arm, custom commands do not run,
//! and `key_down` skips its clipboard and selection keys.
//!
//! A key GPUI's bindings hold is handed to the pane here, as the bindings
//! would otherwise run first. Every other key stays on its usual way to the
//! pane, so text keeps its IME and dead keys. A key no pane can receive, such
//! as Cmd on macOS, is left to the app, which keeps quitting within reach.
//! Only a Herdr pane in focus is served: a menu, a chat, or a herdr-core
//! terminal keeps its usual keys.
// i18n: migrated

use super::{SheprdWindow, flash::Flash, prefix::Typed};
use crate::{Command, i18n::fl, terminal::reaches_pane};
use gpui::{Context, KeyDownEvent, Window};

impl SheprdWindow {
    /// Without a key to end it the mode would hold the keyboard, so it
    /// refuses to start. A chord cannot end it either: the prefix is off.
    pub(super) fn toggle_pass_through(&mut self, cx: &mut Context<Self>) {
        if !self.pass_through && self.pass_through_unlock().is_none() {
            self.show_flash(Flash::warning(fl!("status-bar-pass-through-unbound")), cx);
            return;
        }
        self.pass_through = !self.pass_through;
        self.end_prefix();
        self.resize_mode = false;
        cx.notify();
    }

    /// A config reload or another host's keymap can take the shortcut away
    /// while the mode is on, which would leave no key to end it.
    pub(crate) fn sync_pass_through(&mut self, cx: &mut Context<Self>) {
        if !self.pass_through || self.pass_through_unlock().is_some() {
            return;
        }
        self.pass_through = false;
        self.show_flash(Flash::warning(fl!("status-bar-pass-through-ended")), cx);
    }

    /// The shortcut that ends the mode, as shown to the user.
    pub(super) fn pass_through_unlock(&self) -> Option<String> {
        self.keymap()
            .bindings()
            .find(|(command, _)| *command == Command::TogglePassThrough)
            .map(|(_, keystroke)| keystroke.to_owned())
    }

    /// Claims the unlock key, and hands a key the app's bindings hold to the
    /// pane.
    pub(super) fn pass_through_keystroke(
        &mut self,
        typed: &Typed<'_>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let keymap = self.keymap();
        if typed.matches(|keystroke| keymap.triggers(Command::TogglePassThrough, keystroke, false))
        {
            cx.stop_propagation();
            self.toggle_pass_through(cx);
            return;
        }
        let keystroke = typed.keystroke();
        if !reaches_pane(keystroke)
            || !keymap.holds_directly(keystroke)
            || self.menu.page.is_some()
            || self.copy_mode_active()
            || !self.focus.is_focused(window)
        {
            return;
        }
        cx.stop_propagation();
        let event = KeyDownEvent {
            keystroke: keystroke.clone(),
            is_held: false,
            prefer_character_input: false,
        };
        self.key_down(&event, window, cx);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
