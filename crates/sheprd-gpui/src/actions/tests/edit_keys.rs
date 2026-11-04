use super::*;

/// Linux and Windows edit with Ctrl, where Cmd is not a usable modifier
/// (Windows keeps Win+V/X/C/A for itself); macOS edits with Cmd.
const CTRL_PLATFORM: bool = cfg!(any(target_os = "linux", target_os = "windows"));

fn modifiers(control: bool, shift: bool, alt: bool, platform: bool) -> Modifiers {
    Modifiers {
        control,
        shift,
        alt,
        platform,
        ..Modifiers::default()
    }
}

#[test]
fn edit_keys_follow_the_platform_scheme() {
    let expected = if cfg!(target_os = "windows") {
        ["ctrl-shift-x", "ctrl-shift-c", "ctrl-v", "ctrl-shift-a"]
    } else if cfg!(target_os = "linux") {
        [
            "ctrl-shift-x",
            "ctrl-shift-c",
            "ctrl-shift-v",
            "ctrl-shift-a",
        ]
    } else {
        ["cmd-x", "cmd-c", "cmd-v", "cmd-a"]
    };
    assert_eq!(EDIT_KEYS, expected);
    for key in EDIT_KEYS {
        assert!(Keystroke::parse(key).is_ok(), "{key}");
        // A daemon custom command never takes an Edit key.
        assert!(
            reserved_keystrokes().any(|reserved| reserved == key),
            "{key}"
        );
    }
}

#[test]
fn fields_accept_ctrl_where_the_edit_keys_use_it() {
    let ctrl = modifiers(true, false, false, false);
    let ctrl_shift = modifiers(true, true, false, false);
    assert_eq!(edit_modifiers(ctrl), CTRL_PLATFORM);
    assert_eq!(edit_modifiers(ctrl_shift), CTRL_PLATFORM);
    assert!(edit_modifiers(modifiers(false, false, false, true)));
    for rejected in [
        modifiers(false, false, false, false),
        modifiers(false, true, false, false),
        modifiers(false, false, true, false),
        modifiers(true, false, true, false),
        modifiers(true, true, true, false),
        modifiers(true, false, false, true),
        modifiers(true, true, false, true),
        modifiers(false, true, false, true),
        modifiers(false, false, true, true),
    ] {
        assert!(!edit_modifiers(rejected), "{rejected:?}");
    }
}

#[test]
fn an_edit_menu_click_replays_the_platform_shortcut() {
    for key in ["x", "c", "v", "a"] {
        let keystroke = edit_key(key).keystroke;
        let pressed = keystroke.modifiers;
        assert_eq!(
            (
                pressed.control,
                pressed.shift,
                pressed.platform,
                pressed.alt
            ),
            (CTRL_PLATFORM, CTRL_PLATFORM, !CTRL_PLATFORM, false),
            "{key}"
        );
        assert_eq!(keystroke.key, key);
        // The focused field's own handler must take the replayed keystroke.
        assert!(edit_modifiers(pressed), "{key}");
    }
}
