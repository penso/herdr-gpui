use crate::core_terminals::view::keys::printable_key;
use gpui::Keystroke;
use herdr_backend::{ClientKeyCode, ClientKeyKind, ClientPaneInputEvent};

fn key(text: &str, key_char: Option<&str>) -> Keystroke {
    let mut keystroke = Keystroke::parse(text).unwrap();
    keystroke.key_char = key_char.map(str::to_owned);
    keystroke
}

#[test]
fn a_printable_key_is_a_key_event_with_its_text() {
    let Some(ClientPaneInputEvent::Key {
        code,
        modifiers,
        kind,
        shifted_codepoint,
        generated_text,
        ..
    }) = printable_key(&key("shift-a", Some("A")), false)
    else {
        panic!("a key event");
    };
    assert_eq!(code, ClientKeyCode::Char('a'));
    assert_eq!(modifiers, 1);
    assert_eq!(kind, ClientKeyKind::Press);
    assert_eq!(shifted_codepoint, Some(u32::from('A')));
    assert_eq!(generated_text.as_deref(), Some("A"));

    let held = printable_key(&key("space", Some(" ")), true);
    assert!(matches!(
        held,
        Some(ClientPaneInputEvent::Key {
            code: ClientKeyCode::Char(' '),
            kind: ClientKeyKind::Repeat,
            ..
        })
    ));
}

#[test]
fn composing_and_shortcut_keys_stay_out() {
    // A dead key types nothing yet; the input handler composes it.
    assert!(printable_key(&key("e", None), false).is_none());
    assert!(printable_key(&key("cmd-a", Some("a")), false).is_none());
    assert!(printable_key(&key("left", None), false).is_none());
    // AltGr on Windows arrives as Control-Alt, and macOS Option typing a
    // character as Alt; both type the character, so they stay text.
    assert!(printable_key(&key("ctrl-alt-q", Some("@")), false).is_none());
    assert!(printable_key(&key("alt-p", Some("π")), false).is_none());
}
