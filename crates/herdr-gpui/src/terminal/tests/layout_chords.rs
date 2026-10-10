use super::*;

/// A keystroke as a GPUI platform reports it: `key` names the key, `key_char`
/// is what it would type, if anything.
fn reported(key: &str, key_char: Option<&str>, modifiers: Modifiers) -> KeyDownEvent {
    KeyDownEvent {
        keystroke: Keystroke {
            modifiers,
            key: key.into(),
            key_char: key_char.map(Into::into),
        },
        is_held: false,
        prefer_character_input: false,
    }
}

fn sent(event: &KeyDownEvent, alt_keys: bool) -> Option<(ClientKeyCode, u8)> {
    match key_input(event, alt_keys)? {
        ClientPaneInputEvent::Key {
            code, modifiers, ..
        } => Some((code, modifiers)),
        other => panic!("not a key: {other:?}"),
    }
}

#[test]
fn ctrl_chords_on_non_latin_layouts_reach_the_pane_as_their_physical_key() {
    // Herdr #1079: Ctrl+ц on a Russian layout is Ctrl+W. GPUI already names
    // the physical Latin key: macOS reads the command layout when the layout
    // is not ASCII, Linux falls back to the US key at the evdev keycode
    // (`guess_ascii`), and Windows names the virtual key, whose character is
    // A-Z for letter keys on every layout. Only `key_char` differs: Linux
    // keeps the layout character, macOS and Windows drop it under Ctrl.
    let ctrl = Modifiers::control();
    for (platform, key_char) in [("macos", None), ("linux", Some("ц")), ("windows", None)] {
        for (key, ch) in [("w", 'w'), ("c", 'c'), ("u", 'u'), ("a", 'a')] {
            for alt_keys in [false, true] {
                assert_eq!(
                    sent(&reported(key, key_char, ctrl), alt_keys),
                    Some((ClientKeyCode::Char(ch), 2)),
                    "{platform} ctrl+{key}"
                );
            }
        }
    }
    // Greek, Hebrew, Arabic and Thai layouts report the same way.
    for key_char in ["ς", "ש", "ص", "ไ"] {
        assert_eq!(
            sent(&reported("w", Some(key_char), ctrl), false),
            Some((ClientKeyCode::Char('w'), 2)),
            "{key_char}"
        );
    }
}

#[test]
fn non_latin_letters_typed_without_ctrl_stay_text() {
    // Plain and Shift letters belong to the text input path, which inserts
    // `key_char`, so the pane gets ц, not w.
    for (key_char, modifiers) in [("ц", Modifiers::none()), ("Ц", Modifiers::shift())] {
        for alt_keys in [false, true] {
            assert_eq!(
                sent(&reported("w", Some(key_char), modifiers), alt_keys),
                None
            );
        }
    }
}

#[test]
fn latin_layout_ctrl_chords_keep_their_own_key() {
    // German ö sits on the US `;` key. macOS and Windows name it ö, and Herdr
    // keeps it too: on a Latin layout Ctrl+ö is not Ctrl+;. (Linux names
    // every non-ASCII key by its US position, so it reports `;` here.)
    assert_eq!(
        sent(&reported("ö", None, Modifiers::control()), false),
        Some((ClientKeyCode::Char('ö'), 2))
    );
}
