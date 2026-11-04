//! Why the configured keybindings or pane keys could not be resolved.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("keybindings: {keystroke:?} is bound to both {first} and {second}")]
    DuplicateKeystroke {
        keystroke: String,
        first: &'static str,
        second: &'static str,
    },
    #[error("pane_keys: {0:?} is listed twice")]
    DuplicatePaneKey(String),
    #[error("pane_keys.{0:?}: text: needs something to send")]
    EmptyPaneKeyText(String),
    #[error("keybindings.{command}: invalid keystroke {keystroke:?}")]
    InvalidKeystroke {
        command: &'static str,
        keystroke: String,
        #[source]
        source: gpui::InvalidKeystrokeError,
    },
    #[error("pane_keys: invalid keystroke {keystroke:?}")]
    InvalidPaneKey {
        keystroke: String,
        #[source]
        source: gpui::InvalidKeystrokeError,
    },
    #[error(
        "keybindings.{command}: {keystroke:?} needs a cmd, ctrl, alt, or fn modifier so typing still reaches the terminal"
    )]
    KeystrokeWithoutModifier {
        command: &'static str,
        keystroke: String,
    },
    #[error("pane_keys: {keystroke:?} is also bound to keybindings.{command}")]
    PaneKeyBound {
        keystroke: String,
        command: &'static str,
    },
    #[error("pane_keys.{keystroke:?}: text may be at most {limit} bytes")]
    PaneKeyTextTooLong { keystroke: String, limit: usize },
    #[error(
        "pane_keys: {0:?} needs a cmd, ctrl, alt, or fn modifier so typing still reaches the terminal"
    )]
    PaneKeyWithoutModifier(String),
    #[error("keybindings.{0} must list at most 8 keystrokes")]
    TooManyKeystrokes(&'static str),
    #[error("pane_keys must list at most {0} keystrokes")]
    TooManyPaneKeys(usize),
    #[error("keybindings.{0} is not a command; see the keybindings list in config-gpui.toml")]
    UnknownKeybinding(String),
    #[error("pane_keys.{from:?}: a pane cannot receive {to:?}")]
    UnsendablePaneKey { from: String, to: String },
    // keep-sorted end
}
