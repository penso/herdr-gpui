use super::*;
use crate::{COMMANDS, Command};
use gpui::{Keystroke, Modifiers};
use std::collections::HashMap;

mod toggle_status_bar;

type Profile = fn(Command) -> &'static [&'static str];

fn parse(shortcut: &str) -> Keystroke {
    Keystroke::parse(shortcut).unwrap_or_else(|error| panic!("{shortcut}: {error}"))
}

/// Checks a Ctrl profile on every host, not only the platform it ships on:
/// each catalog shortcut has a counterpart, none needs the platform key, and
/// no two commands share a keystroke.
fn assert_ctrl_profile(profile: Profile) {
    let mut seen = HashMap::new();
    for info in COMMANDS {
        let shortcuts = profile(info.command);
        assert!(
            shortcuts.len() >= info.base_shortcuts().len(),
            "{}",
            info.name
        );
        for shortcut in shortcuts {
            let key = parse(shortcut);
            assert!(!key.modifiers.platform, "{shortcut}");
            assert!(
                seen.insert((key.modifiers, key.key), info.name).is_none(),
                "{shortcut}"
            );
        }
    }
}

#[test]
fn linux_profile_is_complete_unique_and_free_of_super() {
    assert_ctrl_profile(linux::defaults);
}

#[test]
fn windows_profile_is_complete_unique_and_free_of_win() {
    assert_ctrl_profile(windows::defaults);
}

/// Windows keeps these for itself or for the shell's programs: plain Ctrl
/// and a letter, Ctrl-Shift-0 (an input-language hotkey), Alt-Space (the
/// window menu), Ctrl-Esc (Start), Ctrl-Shift-Esc (Task Manager),
/// Ctrl-Alt-Delete, and Alt-F4, which closes the window through the system.
#[test]
fn windows_profile_leaves_system_and_terminal_keys_alone() {
    let reserved = [
        "ctrl-shift-0",
        "alt-space",
        "ctrl-escape",
        "ctrl-shift-escape",
        "ctrl-alt-delete",
        "alt-f4",
    ];
    let defaults: Vec<_> = COMMANDS
        .iter()
        .flat_map(|info| windows::defaults(info.command))
        .map(|shortcut| parse(shortcut))
        .collect();
    for key in reserved.map(parse) {
        assert!(
            defaults
                .iter()
                .all(|default| (default.modifiers, &default.key) != (key.modifiers, &key.key)),
            "{key:?}"
        );
    }
    let ctrl = Modifiers {
        control: true,
        ..Modifiers::default()
    };
    for default in &defaults {
        let letter = default.key.len() == 1 && default.key.bytes().all(|b| b.is_ascii_lowercase());
        assert!(!(default.modifiers == ctrl && letter), "{default:?}");
    }
    assert_eq!(windows::defaults(Command::ResetFontSize), ["ctrl-0"]);
    assert_eq!(windows::defaults(Command::Palette), ["ctrl-shift-p"]);
    assert_eq!(windows::defaults(Command::Quit), ["ctrl-shift-q"]);
}

#[test]
fn ctrl_shortcuts_cover_exactly_linux_and_windows() {
    assert_eq!(CTRL_SHORTCUTS, cfg!(any(target_os = "linux", windows)));
    let ctrl = COMMANDS
        .iter()
        .flat_map(|info| info.default_shortcuts())
        .all(|shortcut| !parse(shortcut).modifiers.platform);
    assert_eq!(ctrl, CTRL_SHORTCUTS);
}
