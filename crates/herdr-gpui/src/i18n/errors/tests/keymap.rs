use super::*;
use crate::keymap::{Error, daemon};

fn invalid() -> gpui::InvalidKeystrokeError {
    gpui::Keystroke::parse("cmd-n-t").unwrap_err()
}

#[test]
fn every_keymap_variant_has_a_message() {
    let keystroke = || "cmd-t".to_owned();
    assert_localized(&[
        Error::DuplicateKeystroke {
            keystroke: keystroke(),
            first: "new_tab",
            second: "close_tab",
        },
        Error::DuplicatePaneKey(keystroke()),
        Error::EmptyPaneKeyText(keystroke()),
        Error::InvalidKeystroke {
            command: "new_tab",
            keystroke: "cmd-n-t".into(),
            source: invalid(),
        },
        Error::InvalidPaneKey {
            keystroke: "cmd-n-t".into(),
            source: invalid(),
        },
        Error::KeystrokeWithoutModifier {
            command: "new_tab",
            keystroke: "n".into(),
        },
        Error::PaneKeyBound {
            keystroke: keystroke(),
            command: "new_tab",
        },
        Error::PaneKeyTextTooLong {
            keystroke: keystroke(),
            limit: 4096,
        },
        Error::PaneKeyWithoutModifier("n".into()),
        Error::TooManyKeystrokes("new_tab"),
        Error::TooManyPaneKeys(64),
        Error::UnknownKeybinding("new_tabb".into()),
        Error::UnsendablePaneKey {
            from: keystroke(),
            to: "cmd-q".into(),
        },
    ]);
}

#[test]
fn every_keymap_daemon_variant_has_a_message() {
    assert_localized(&[
        daemon::Error::Missing,
        daemon::Error::NoKeys,
        daemon::Error::Parse(toml::from_str::<toml::Table>("=").unwrap_err()),
        daemon::Error::TooLarge { max: 65536 },
    ]);
}
