#![allow(clippy::unwrap_used)]

use super::*;
use crate::keymap::{Binding, DaemonKeys, PaneKeys};
use std::collections::BTreeMap;

const SHIFT_COMMAND: Held = Held {
    shift: true,
    ..COMMAND
};

fn leaves_page_for(held: Held, plain: &str, shifted: &str) -> bool {
    leaves(&typed(held, plain, shifted))
}

#[test]
fn the_default_zoom_keys_leave_a_page() {
    bind(&Keymap::default());
    // Cmd-=, Cmd--, and Cmd-0, and Cmd-+, which most layouts type as
    // Cmd-Shift-=, zoom the page through the app's bindings.
    for (held, plain, shifted) in [
        (COMMAND, "=", "="),
        (COMMAND, "-", "-"),
        (COMMAND, "0", "0"),
        (SHIFT_COMMAND, "=", "+"),
    ] {
        assert!(
            leaves_page_for(held, plain, shifted),
            "{held:?} {plain:?} {shifted:?}"
        );
    }
    // Without Command, or with Shift on minus, which types an underscore,
    // the keys stay the page's.
    for (held, plain, shifted) in [
        (Held::default(), "=", "="),
        (Held::default(), "0", "0"),
        (SHIFT_COMMAND, "-", "_"),
    ] {
        assert!(
            !leaves_page_for(held, plain, shifted),
            "{held:?} {plain:?} {shifted:?}"
        );
    }
}

#[test]
fn rebound_zoom_keys_leave_a_page_in_place_of_the_defaults() {
    let overrides = BTreeMap::from([(
        "increase_font_size".to_owned(),
        Binding::One("ctrl-=".to_owned()),
    )]);
    bind(
        &Keymap::with_overrides(&overrides, &PaneKeys::default(), &DaemonKeys::default()).unwrap(),
    );
    let control = Held {
        control: true,
        ..Held::default()
    };
    assert!(leaves_page_for(control, "=", "="));
    assert!(!leaves_page_for(COMMAND, "=", "="));
    assert!(!leaves_page_for(SHIFT_COMMAND, "=", "+"));
    // The other font size keys keep their defaults.
    assert!(leaves_page_for(COMMAND, "0", "0"));
}
