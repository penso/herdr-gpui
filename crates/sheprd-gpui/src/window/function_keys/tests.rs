use super::*;
use crate::terminal::key_input;
use gpui::KeyDownEvent;
use sheprd_client::protocol::{ClientKeyCode, ClientPaneInputEvent};

const SHIFT: Modifiers = Modifiers {
    control: false,
    alt: false,
    shift: true,
    platform: false,
    function: false,
};

fn typed(key: &str, modifiers: Modifiers) -> Keystroke {
    Keystroke {
        modifiers,
        key: key.into(),
        key_char: Some(key.into()),
    }
}

fn function_key(key: &str, modifiers: Modifiers) -> Keystroke {
    Keystroke {
        modifiers,
        key: key.into(),
        key_char: None,
    }
}

#[test]
fn a_key_code_names_the_function_key() {
    assert_eq!(
        rename(&typed("", Modifiers::default()), 90, false),
        Some(function_key("f20", Modifiers::default()))
    );
    assert_eq!(
        rename(&typed(DATA_LINK_ESCAPE, Modifiers::default()), 122, false),
        Some(function_key("f1", Modifiers::default()))
    );
}

/// GPUI reports Shift-F1 as U+0010 without Shift, having taken the character
/// for a shifted symbol, and Shift-F20 as an empty key with it. Both are
/// shifted once named, and the other modifiers stay as GPUI read them.
#[test]
fn shift_comes_from_the_event_and_other_modifiers_from_the_keystroke() {
    let control = Modifiers {
        control: true,
        ..Modifiers::default()
    };
    assert_eq!(
        rename(&typed(DATA_LINK_ESCAPE, control), 122, true),
        Some(function_key(
            "f1",
            Modifiers {
                control: true,
                ..SHIFT
            }
        ))
    );
    assert_eq!(
        rename(&typed("", SHIFT), 90, true),
        Some(function_key("f20", SHIFT))
    );
}

#[test]
fn every_function_key_code_has_its_own_name() {
    let codes = [
        122, 120, 99, 118, 96, 97, 98, 100, 101, 109, 103, 111, 105, 107, 113, 106, 64, 79, 80, 90,
    ];
    for (index, code) in codes.into_iter().enumerate() {
        assert_eq!(name(code), Some(format!("f{}", index + 1).as_str()));
    }
}

#[test]
fn another_key_code_is_left_alone() {
    // 35 is P: Ctrl+P types U+0010 too, but arrives as `p` with control.
    assert_eq!(
        rename(&typed(DATA_LINK_ESCAPE, Modifiers::default()), 35, false),
        None
    );
}

#[test]
fn only_a_keystroke_without_a_name_is_looked_up() {
    // F17 to F20 on a layout with no entry for them, and F1 to F16 on one
    // that has.
    assert!(unnamed(&typed("", Modifiers::default())));
    assert!(unnamed(&typed(DATA_LINK_ESCAPE, Modifiers::default())));
    for key in ["f20", "p", "escape"] {
        assert!(!unnamed(&Keystroke::parse(key).unwrap()));
        let press = || unreachable!("{key} has a name, so no event is read for it");
        assert_eq!(named(&Keystroke::parse(key).unwrap(), press), None);
    }
    // Without an event to read, an unnamed keystroke stays as it came.
    assert_eq!(named(&typed("", Modifiers::default()), || None), None);
}

/// The pane's key path sends nothing for the keystroke as macOS delivered
/// it, and the function key once it is named.
#[test]
fn a_pane_receives_the_named_key_and_not_the_unnamed_one() {
    let sent = |keystroke: Keystroke| {
        let event = KeyDownEvent {
            keystroke,
            is_held: false,
            prefer_character_input: false,
        };
        match key_input(&event, false) {
            Some(ClientPaneInputEvent::Key {
                code, modifiers, ..
            }) => Some((code, modifiers)),
            _ => None,
        }
    };
    for unnamed in ["", DATA_LINK_ESCAPE] {
        assert_eq!(sent(typed(unnamed, Modifiers::default())), None);
    }
    let named = rename(&typed("", Modifiers::default()), 90, false).unwrap();
    assert_eq!(sent(named), Some((ClientKeyCode::F(20), 0)));
    let shifted = rename(&typed(DATA_LINK_ESCAPE, Modifiers::default()), 105, true).unwrap();
    assert_eq!(sent(shifted), Some((ClientKeyCode::F(13), 1)));
}
