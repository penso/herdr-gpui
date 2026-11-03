//! Keybindings, the app's and a host's: `error-keymap.ftl`. Keystrokes are
//! quoted as the `Display` quotes them, with `{:?}`.

use super::Localize;
use crate::{
    i18n::fl,
    keymap::{Error, daemon},
};

fn quoted(text: &str) -> String {
    format!("{text:?}")
}

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::DuplicateKeystroke {
                keystroke,
                first,
                second,
            } => fl!(
                "error-keymap-duplicate-keystroke",
                keystroke = quoted(keystroke),
                first = *first,
                second = *second
            ),
            Self::DuplicatePaneKey(keystroke) => fl!(
                "error-keymap-duplicate-pane-key",
                keystroke = quoted(keystroke)
            ),
            Self::EmptyPaneKeyText(keystroke) => fl!(
                "error-keymap-empty-pane-key-text",
                keystroke = quoted(keystroke)
            ),
            Self::InvalidKeystroke {
                command, keystroke, ..
            } => fl!(
                "error-keymap-invalid-keystroke",
                command = *command,
                keystroke = quoted(keystroke)
            ),
            Self::InvalidPaneKey { keystroke, .. } => fl!(
                "error-keymap-invalid-pane-key",
                keystroke = quoted(keystroke)
            ),
            Self::KeystrokeWithoutModifier { command, keystroke } => fl!(
                "error-keymap-keystroke-without-modifier",
                command = *command,
                keystroke = quoted(keystroke)
            ),
            Self::PaneKeyBound { keystroke, command } => fl!(
                "error-keymap-pane-key-bound",
                keystroke = quoted(keystroke),
                command = *command
            ),
            Self::PaneKeyTextTooLong { keystroke, limit } => fl!(
                "error-keymap-pane-key-text-too-long",
                keystroke = quoted(keystroke),
                limit = *limit
            ),
            Self::PaneKeyWithoutModifier(keystroke) => fl!(
                "error-keymap-pane-key-without-modifier",
                keystroke = quoted(keystroke)
            ),
            Self::TooManyKeystrokes(command) => {
                fl!("error-keymap-too-many-keystrokes", command = *command)
            }
            Self::TooManyPaneKeys(limit) => fl!("error-keymap-too-many-pane-keys", limit = *limit),
            Self::UnknownKeybinding(command) => {
                fl!(
                    "error-keymap-unknown-keybinding",
                    command = command.as_str()
                )
            }
            Self::UnsendablePaneKey { from, to } => fl!(
                "error-keymap-unsendable-pane-key",
                from = quoted(from),
                to = quoted(to)
            ),
            // keep-sorted end
        }
    }
}

impl Localize for daemon::Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::Missing => fl!("error-keymap-daemon-missing"),
            Self::NoKeys => fl!("error-keymap-daemon-no-keys"),
            // The parser's own diagnostic.
            Self::Parse(error) => fl!("error-keymap-daemon-parse", error = error.to_string()),
            Self::TooLarge { max } => fl!("error-keymap-daemon-too-large", max = *max),
            // keep-sorted end
        }
    }
}
