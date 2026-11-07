//! Shift-Enter must stay distinct for programs such as Pi that bind Alt-Enter
//! to a different action (#575). Legacy newline mappings remain opt-in (#504).

use super::*;

#[test]
fn modified_enters_are_not_remapped_by_default() {
    for keymap in [
        Keymap::default(),
        Keymap::with_overrides(&BTreeMap::new(), &PaneKeys::new(), &no_keys()).unwrap(),
    ] {
        for typed in [
            "shift-enter",
            "enter",
            "ctrl-enter",
            "alt-enter",
            "ctrl-shift-enter",
        ] {
            assert_eq!(sent(&keymap, typed), None, "{typed}");
        }
    }
}

#[test]
fn explicit_shift_enter_overrides_and_the_previous_opt_out_still_work() {
    let resolve = |entries: &[(&str, &str)]| {
        Keymap::with_overrides(&BTreeMap::new(), &pane_keys(entries), &no_keys()).unwrap()
    };
    assert_eq!(sent(&resolve(&[("shift-enter", "")]), "shift-enter"), None);
    assert_eq!(
        sent(&resolve(&[("shift-enter", "alt-enter")]), "shift-enter"),
        Some(&key("alt-enter"))
    );
    assert_eq!(
        sent(&resolve(&[("shift-enter", "ctrl-j")]), "shift-enter"),
        Some(&key("ctrl-j"))
    );
    // Shift alone does not make a command shortcut, so no command can
    // already own it.
    assert!(matches!(
        Keymap::with_overrides(
            &overrides(&[("new_tab", one("shift-enter"))]),
            &BTreeMap::new(),
            &no_keys()
        ),
        Err(crate::Error::Keymap(Error::KeystrokeWithoutModifier { .. }))
    ));
}
