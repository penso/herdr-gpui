#![allow(clippy::unwrap_used)]

use super::*;
use crate::keymap::{Binding, DaemonKeys, PaneKeys};
use std::collections::BTreeMap;

const SHIFT_COMMAND: Held = Held {
    shift: true,
    ..COMMAND
};

/// Arrows always carry Fn.
const ARROW_COMMAND: Held = Held {
    function: true,
    ..COMMAND
};

const RIGHT: &str = "\u{f703}";

fn leaves_page_for(held: Held, plain: &str, shifted: &str) -> bool {
    leaves(&typed(held, plain, shifted))
}

#[test]
fn the_default_tab_keys_leave_a_page() {
    bind(&Keymap::default());
    // Cmd-Shift-] types "}", read as "]" with Shift, as GPUI reads it.
    assert!(leaves_page_for(SHIFT_COMMAND, "]", "}"));
    assert!(leaves_page_for(SHIFT_COMMAND, "[", "{"));
    for digit in 1..=9 {
        let digit = digit.to_string();
        assert!(leaves_page_for(COMMAND, &digit, &digit), "cmd-{digit}");
    }
    // The page keeps its own shortcuts, the app's other ones included.
    for (held, plain, shifted) in [
        (COMMAND, "]", "]"),
        (COMMAND, "w", "w"),
        (COMMAND, "t", "t"),
        (COMMAND, "p", "p"),
        (SHIFT_COMMAND, "p", "P"),
        (Held::default(), "1", "1"),
        (ARROW_COMMAND, RIGHT, RIGHT),
    ] {
        assert!(
            !leaves_page_for(held, plain, shifted),
            "{held:?} {plain:?} {shifted:?}"
        );
    }
}

#[test]
fn rebound_keys_leave_a_page_in_place_of_the_defaults() {
    let one = |keystroke: &str| Binding::One(keystroke.to_owned());
    let overrides = BTreeMap::from([
        ("next_tab".to_owned(), one("cmd-right")),
        ("previous_tab".to_owned(), one("cmd-left")),
        ("next_workspace".to_owned(), one("cmd-down")),
        ("previous_workspace".to_owned(), one("cmd-up")),
        ("next_agent".to_owned(), one("f5")),
        ("previous_agent".to_owned(), one("cmd-}")),
    ]);
    let pane_keys = PaneKeys::from([
        ("cmd-left".to_owned(), String::new()),
        ("cmd-right".to_owned(), String::new()),
    ]);
    bind(&Keymap::with_overrides(&overrides, &pane_keys, &DaemonKeys::default()).unwrap());
    for arrow in ["\u{f700}", "\u{f701}", "\u{f702}", RIGHT] {
        assert!(leaves_page_for(ARROW_COMMAND, arrow, arrow), "{arrow:?}");
    }
    let f5 = "\u{f708}";
    let function = Held {
        function: true,
        ..Held::default()
    };
    assert!(leaves_page_for(function, f5, f5));
    // A shifted symbol matches however it is spelled.
    assert!(leaves_page_for(SHIFT_COMMAND, "]", "}"));
    // The default the rebinding replaced goes back to the page; a default
    // left alone keeps leaving it.
    assert!(!leaves_page_for(SHIFT_COMMAND, "[", "{"));
    assert!(leaves_page_for(COMMAND, "1", "1"));
}

#[test]
fn keys_read_as_gpui_names_them() {
    let read = |held, plain: &str, shifted: &str| typed(held, plain, shifted).unparse();
    assert_eq!(read(SHIFT_COMMAND, "]", "}"), "cmd-shift-]");
    assert_eq!(read(ARROW_COMMAND, RIGHT, RIGHT), "cmd-right");
    assert_eq!(read(COMMAND, "\u{f704}", "\u{f704}"), "cmd-f1");
    assert_eq!(read(COMMAND, "\t", "\t"), "cmd-tab");
    assert_eq!(read(SHIFT_COMMAND, "\t", "\u{19}"), "cmd-shift-tab");
    assert_eq!(read(COMMAND, "A", "A"), "cmd-a");
    // Fn on a key other than an arrow or a function key is a modifier.
    let function = Held {
        function: true,
        ..COMMAND
    };
    assert_eq!(read(function, "a", "a"), "fn-cmd-a");
}

/// Move Tab Left and Right have no default key; bound, they move the
/// page's own tab, as a web browser's keys do.
#[test]
fn bound_move_tab_keys_leave_a_page() {
    let one = |keystroke: &str| Binding::One(keystroke.to_owned());
    let overrides = BTreeMap::from([
        ("move_tab_previous".to_owned(), one("cmd-alt-left")),
        ("move_tab_next".to_owned(), one("cmd-alt-right")),
    ]);
    bind(
        &Keymap::with_overrides(&overrides, &PaneKeys::default(), &DaemonKeys::default()).unwrap(),
    );
    let option_command = Held {
        option: true,
        ..ARROW_COMMAND
    };
    assert!(leaves_page_for(option_command, RIGHT, RIGHT));
    assert!(leaves_page_for(option_command, "\u{f702}", "\u{f702}"));
    assert!(!leaves_page_for(ARROW_COMMAND, RIGHT, RIGHT));
}
