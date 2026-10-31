//! Keys a program asked for beyond typed text, under the kitty keyboard
//! protocol: printable keys as key events when it wants every key, and
//! releases when it wants those. By default typing stays text, which the
//! input handler commits, so layouts, dead keys, and IMEs keep working.

use super::CoreTerminalView;
use gpui::{prelude::*, *};
use herdr_backend::{ClientKeyCode, ClientKeyKind, ClientPaneInputEvent};

/// A printable key as a key event, for a program that asked for every key.
/// Only a key typed bare or with Shift: `key_input` already sends Control
/// and Alt chords it means as keys, so one with Control or Alt that reaches
/// here types a character (Windows AltGr, macOS Option typing characters)
/// and stays text, as does a key that composes (a dead key types nothing
/// yet).
pub(crate) fn printable_key(keystroke: &Keystroke, is_held: bool) -> Option<ClientPaneInputEvent> {
    let modifiers = keystroke.modifiers;
    if modifiers.platform || modifiers.function || modifiers.control || modifiers.alt {
        return None;
    }
    let text = keystroke.key_char.as_deref()?;
    let mut key = keystroke.key.chars();
    let code = match (key.next(), key.next(), keystroke.key.as_str()) {
        (_, _, "space") => ' ',
        (Some(code), None, _) => code,
        _ => return None,
    };
    let shifted = modifiers
        .shift
        .then(|| text.chars().next())
        .flatten()
        .filter(|shifted| *shifted != code)
        .map(u32::from);
    Some(ClientPaneInputEvent::Key {
        code: ClientKeyCode::Char(code),
        modifiers: u8::from(modifiers.shift),
        kind: if is_held {
            ClientKeyKind::Repeat
        } else {
            ClientKeyKind::Press
        },
        repeat_count: 1,
        shifted_codepoint: shifted,
        generated_text: Some(text.to_owned()),
        tracks_release: false,
        physical_key_id: None,
        windows_record: None,
    })
}

impl CoreTerminalView {
    /// Releases a key sent while the program asked for releases. A key
    /// whose press went elsewhere, or arrived as text, has nothing held.
    pub(super) fn key_up(&mut self, event: &KeyUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(release) = self.held_keys.release(&event.keystroke.key) else {
            return;
        };
        if self.screen.keys.releases && self.marked.is_empty() {
            self.send(release);
            cx.stop_propagation();
        }
    }
}
