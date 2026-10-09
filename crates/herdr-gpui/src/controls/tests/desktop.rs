use super::*;
use gpui::Keystroke;
use std::collections::HashSet;

/// Keystrokes the desktop already gives a meaning: the terminal's copy and
/// paste, and IBus's Unicode entry.
const RESERVED: &[&str] = &[
    "ctrl-shift-c",
    "ctrl-shift-v",
    "ctrl-shift-u",
    "shift-insert",
];

#[test]
fn desktop_defaults_leave_super_and_terminal_keys_alone() {
    let reserved: Vec<_> = RESERVED
        .iter()
        .map(|keystroke| Keystroke::parse(keystroke).unwrap())
        .collect();
    let mut seen = HashSet::new();
    for shortcut in COMMANDS
        .iter()
        .flat_map(|info| info.defaults(Platform::Desktop))
    {
        let keystroke =
            Keystroke::parse(shortcut).unwrap_or_else(|error| panic!("{shortcut}: {error}"));
        let modifiers = keystroke.modifiers;
        assert!(!modifiers.platform, "{shortcut} needs the Super key");
        assert!(modifiers.control || modifiers.alt, "{shortcut}");
        // Ctrl or Alt alone on a letter is a control byte or Meta key that
        // the program in the pane reads.
        let letter = keystroke.key.len() == 1 && keystroke.key.chars().all(char::is_alphabetic);
        assert!(
            !(letter && !modifiers.shift && modifiers.control != modifiers.alt),
            "{shortcut} takes a key from the terminal"
        );
        assert!(
            seen.insert((modifiers, keystroke.key.clone())),
            "{shortcut} is bound twice"
        );
        assert!(!reserved.contains(&keystroke), "{shortcut} is reserved");
    }
}

#[test]
fn every_mac_default_has_a_desktop_counterpart() {
    for info in COMMANDS {
        assert_eq!(info.defaults(Platform::Mac), info.shortcuts);
        assert_eq!(
            info.shortcuts.is_empty(),
            info.defaults(Platform::Desktop).is_empty(),
            "{}",
            info.name
        );
    }
}

#[test]
fn tests_pin_the_mac_catalog() {
    assert_eq!(Platform::CURRENT, Platform::Mac);
}
