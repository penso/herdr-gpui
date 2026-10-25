//! `[pane_keys]` values that send text instead of a key.

use super::*;

fn text(text: &str) -> PaneKey {
    PaneKey::Text(text.into())
}

fn resolve(entries: &[(&str, &str)]) -> Result<Keymap> {
    Keymap::with_overrides(&BTreeMap::new(), &pane_keys(entries), &no_keys())
}

#[test]
fn a_text_value_is_sent_as_written() {
    let keymap = resolve(&[
        ("alt-7", "text:|"),
        ("alt-8", "text: [ "),
        ("cmd-shift-r", "text:\u{15}exec $SHELL"),
        ("alt-left", "text:\u{1b}b"),
    ])
    .unwrap();
    assert_eq!(sent(&keymap, "alt-7"), Some(&text("|")));
    // Spaces are part of the text.
    assert_eq!(sent(&keymap, "alt-8"), Some(&text(" [ ")));
    assert_eq!(
        sent(&keymap, "cmd-shift-r"),
        Some(&text("\u{15}exec $SHELL"))
    );
    // Text replaces a default as a key does.
    assert_eq!(sent(&keymap, "alt-left"), Some(&text("\u{1b}b")));
    assert_eq!(sent(&keymap, "alt-9"), None);
}

#[test]
fn text_must_say_something_and_stay_short() {
    assert!(matches!(
        resolve(&[("alt-7", "text:")]),
        Err(crate::Error::Keymap(Error::EmptyPaneKeyText(typed))) if typed == "alt-7"
    ));
    let longest = format!("text:{}", "a".repeat(MAX_PANE_TEXT));
    assert!(resolve(&[("alt-7", &longest)]).is_ok());
    assert!(matches!(
        resolve(&[("alt-7", &format!("{longest}a"))]),
        Err(crate::Error::Keymap(Error::PaneKeyTextTooLong {
            limit: MAX_PANE_TEXT,
            ..
        }))
    ));
    // The keystroke is still checked, and still cannot be typing.
    assert!(matches!(
        resolve(&[("7", "text:|")]),
        Err(crate::Error::Keymap(Error::PaneKeyWithoutModifier(_)))
    ));
}
