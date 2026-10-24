use super::*;

#[test]
fn function_keys_can_be_configured_without_taking_ordinary_typing() {
    let keymap = with(&[("toggle_full_screen", one("f11"))]).unwrap();
    assert!(keymap.triggers(Command::ToggleFullScreen, &keystroke("f11"), false));
    for key in ["f0", "f25", "escape", "space", "shift-t"] {
        assert!(matches!(
            with(&[("new_tab", one(key))]),
            Err(crate::Error::Keymap(Error::KeystrokeWithoutModifier { .. }))
        ));
    }
}

#[test]
fn platform_defaults_are_complete_unique_and_leave_edit_keys_available() {
    let keymap = with(&[]).unwrap();
    let mut seen = HashMap::new();
    for info in COMMANDS {
        assert_eq!(
            info.default_shortcuts().len(),
            info.base_shortcuts().len(),
            "{}",
            info.name
        );
        for shortcut in info.default_shortcuts() {
            let key = keystroke(shortcut);
            assert!(
                seen.insert(identity(&key), info.name).is_none(),
                "{shortcut}"
            );
            assert!(keymap.triggers(info.command, &key, false), "{shortcut}");
            assert!(
                crate::actions::reserved_keystrokes()
                    .all(|edit| !typed_matches(&key, &keystroke(edit))),
                "{shortcut}"
            );
            if crate::controls::CTRL_SHORTCUTS {
                assert!(!key.modifiers.platform, "{shortcut}");
            } else {
                assert!(key.modifiers.platform, "{shortcut}");
            }
        }
    }
}

#[cfg(target_os = "linux")]
#[test]
fn linux_shortcuts_preserve_terminal_keys_and_shifted_actions() {
    let keymap = Keymap::default();
    for (command, key) in [
        (Command::Tab, "ctrl-shift-t"),
        (Command::NewWindow, "ctrl-alt-n"),
        (Command::CloseWindow, "ctrl-alt-w"),
        (Command::NewWorktree, "ctrl-shift-n"),
        (Command::Workspace, "ctrl-alt-shift-n"),
        (Command::Palette, "ctrl-shift-p"),
        (Command::ResumePalette, "ctrl-alt-shift-r"),
        (Command::SplitRight, "ctrl-shift-d"),
        (Command::SplitDown, "ctrl-alt-shift-d"),
        (Command::CopyMode, "ctrl-alt-shift-c"),
        (Command::ToggleFullScreen, "f11"),
    ] {
        assert_eq!(keymap.primary(command), key);
    }
    for key in [
        "ctrl-c",
        "ctrl-d",
        "ctrl-z",
        "ctrl-v",
        "ctrl-shift-c",
        "ctrl-shift-v",
        "ctrl-shift-u", // GNOME/IBus Unicode input must keep this chord.
        "cmd-t",
    ] {
        assert!(
            COMMANDS
                .iter()
                .all(|info| !keymap.triggers(info.command, &keystroke(key), false)),
            "{key}"
        );
    }
    assert!(keymap.triggers(Command::Keybinds, &keystroke("ctrl-shift-/->?"), false));
    let configured = with(&[("new_tab", one("cmd-t"))]).unwrap();
    assert_eq!(configured.primary(Command::Tab), "cmd-t");
    assert!(!configured.triggers(Command::Tab, &keystroke("ctrl-shift-t"), false));
}

#[cfg(windows)]
#[test]
fn windows_shortcuts_preserve_terminal_and_system_keys() {
    let keymap = Keymap::default();
    for (command, key) in [
        (Command::Tab, "ctrl-shift-t"),
        (Command::NewWindow, "ctrl-alt-n"),
        (Command::NewWorktree, "ctrl-shift-n"),
        (Command::Palette, "ctrl-shift-p"),
        (Command::SplitRight, "ctrl-shift-d"),
        (Command::CopyMode, "ctrl-alt-shift-c"),
        (Command::ResetFontSize, "ctrl-0"),
        (Command::Quit, "ctrl-shift-q"),
        (Command::ToggleFullScreen, "f11"),
    ] {
        assert_eq!(keymap.primary(command), key);
    }
    for key in [
        "ctrl-c",
        "ctrl-d",
        "ctrl-z",
        "ctrl-v",
        "ctrl-shift-c",
        "ctrl-shift-v",
        "ctrl-shift-0", // Windows can take this for input-language switching.
        "alt-f4",
        "cmd-d",
        "cmd-q",
    ] {
        assert!(
            COMMANDS
                .iter()
                .all(|info| !keymap.triggers(info.command, &keystroke(key), false)),
            "{key}"
        );
    }
    let configured = with(&[("new_tab", one("cmd-t"))]).unwrap();
    assert_eq!(configured.primary(Command::Tab), "cmd-t");
    assert!(!configured.triggers(Command::Tab, &keystroke("ctrl-shift-t"), false));
}
