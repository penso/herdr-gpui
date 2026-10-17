use super::*;
use crate::keymap::{Binding, DaemonKeys, Keymap};

mod edit_keys;

/// A keymap with `command` bound to `keystroke` in the GUI's `[keybindings]`.
fn configured(command: &str, keystroke: &str) -> Keymap {
    let overrides = [(command.to_owned(), Binding::One(keystroke.into()))].into();
    Keymap::with_overrides(&overrides, &Default::default(), &DaemonKeys::default()).unwrap()
}

/// A keymap under a daemon `[keys]` table.
fn daemon(keys: &str) -> Keymap {
    let keys = DaemonKeys::from_profile(Some(keys)).unwrap();
    Keymap::with_overrides(&Default::default(), &Default::default(), &keys).unwrap()
}

fn free(keymap: &Keymap) -> Vec<&'static str> {
    free_system_keys(keymap).collect()
}

#[test]
fn a_system_key_is_kept_until_a_command_is_bound_to_it() {
    assert_eq!(free(&Keymap::default()), SYSTEM_KEYS);
    assert_eq!(
        free(&configured("focus_left", "cmd-alt-h")),
        ["cmd-h", "cmd-m"]
    );
    // The daemon's `[keys]` take one the same way, however it is spelled.
    assert_eq!(
        free(&daemon(
            "[keys]\nfocus_pane_left = [\"prefix+h\", \"alt+cmd+h\"]\nzoom = \"cmd+m\"\n"
        )),
        ["cmd-h"]
    );
    // A chord after the prefix, or the same key under other modifiers, is
    // another keystroke.
    assert_eq!(
        free(&daemon("[keys]\nfocus_pane_left = \"prefix+cmd+h\"\n")),
        SYSTEM_KEYS
    );
    assert_eq!(free(&configured("focus_left", "cmd-shift-h")), SYSTEM_KEYS);
}

#[cfg(target_os = "macos")]
#[gpui::test]
fn a_command_bound_to_a_system_key_runs_in_its_place(cx: &mut gpui::TestAppContext) {
    let typed = |keystroke: &str| [Keystroke::parse(keystroke).unwrap()];
    let bind = |keymap: Keymap, cx: &mut App| {
        let mut appearance = crate::app::InitialAppearance::default();
        appearance.config.keybindings = keymap;
        cx.set_global(appearance);
        cx.clear_key_bindings();
        bind_keys(cx);
    };
    // The binding a keystroke runs: the last one bound to it.
    let runs = |keystroke: &str, action: &dyn Action, cx: &App| {
        cx.all_bindings_for_input(&typed(keystroke))
            .first()
            .is_some_and(|binding| binding.action().partial_eq(action))
    };
    cx.update(|cx| {
        bind(Keymap::default(), cx);
        assert!(runs("cmd-alt-h", &HideOthers, cx));

        bind(
            daemon("[keys]\nfocus_pane_left = [\"prefix+h\", \"cmd+alt+h\"]\n"),
            cx,
        );
        let focus_left = RunCommand {
            command: Command::FocusLeft,
        };
        assert!(runs("cmd-alt-h", &focus_left, cx));
        // Hide Others keeps its menu item, with no shortcut to show.
        assert!(
            cx.key_bindings()
                .borrow()
                .bindings_for_action(&HideOthers)
                .next()
                .is_none()
        );
        assert!(runs("cmd-h", &Hide, cx));
        assert!(runs("cmd-m", &Minimize, cx));
    });
}
