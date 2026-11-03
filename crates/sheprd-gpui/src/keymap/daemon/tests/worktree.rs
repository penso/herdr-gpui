use super::*;
use crate::keymap::{Binding, Keymap};
use std::collections::BTreeMap;

/// Herdr leaves both worktree actions unbound, so they bind only what the
/// user writes, prefix chords and direct keys alike.
#[test]
fn worktree_actions_are_unbound_by_default_and_bind_what_the_user_writes() {
    let defaults = DaemonKeys::default();
    for command in [Command::OpenWorktree, Command::RemoveWorktree] {
        assert!(bound(&defaults, command).is_empty(), "{command:?}");
    }
    let keys = keys(
        r#"
        [keys]
        open_worktree = "prefix+shift+o"
        remove_worktree = ["prefix+hyper+x", "prefix+alt+x", "ctrl+alt+x"]
        "#,
    );
    assert_eq!(bound(&keys, Command::OpenWorktree), [prefixed("shift-o")]);
    // `hyper` has no GPUI equivalent, so only the later entries bind.
    assert_eq!(
        bound(&keys, Command::RemoveWorktree),
        [prefixed("alt-x"), Trigger::Direct(parsed("ctrl-alt-x"))]
    );
    let keymap = Keymap::with_overrides(&BTreeMap::new(), &Default::default(), &keys).unwrap();
    assert_eq!(
        keymap.chord(&parsed("shift-o")),
        Some(Command::OpenWorktree)
    );
    assert_eq!(
        keymap.chord(&parsed("alt-x")),
        Some(Command::RemoveWorktree)
    );
    assert!(keymap.triggers(Command::RemoveWorktree, &parsed("ctrl-alt-x"), false));
    for value in ["''", "[]"] {
        let disabled = self::keys(&format!("[keys]\nopen_worktree = {value}"));
        assert!(bound(&disabled, Command::OpenWorktree).is_empty());
    }
    // Navigate mode's pane keys stay unmapped and bind nothing.
    let navigate = self::keys("[keys]\nnavigate_pane_left = \"h\"");
    assert_eq!(navigate, DaemonKeys::default());
}

/// A `[keybindings]` entry replaces the daemon's bindings for that command,
/// and a GUI-configured keystroke wins over a daemon one for another.
#[test]
fn gui_keybindings_take_precedence_over_daemon_worktree_keys() {
    let keys = keys(
        r#"
        [keys]
        open_worktree = ["prefix+o", "ctrl+alt+o"]
        remove_worktree = "ctrl+alt+r"
        "#,
    );
    let overrides = BTreeMap::from([
        (
            "open_worktree".to_owned(),
            Binding::One("ctrl-alt-shift-o".into()),
        ),
        ("new_tab".to_owned(), Binding::One("ctrl-alt-r".into())),
    ]);
    let keymap = Keymap::with_overrides(&overrides, &Default::default(), &keys).unwrap();
    assert_eq!(
        keymap.shortcuts(Command::OpenWorktree).collect::<Vec<_>>(),
        ["ctrl-alt-shift-o"]
    );
    // `prefix+o` stays Herdr's default notification target.
    assert_eq!(
        keymap.chord(&parsed("o")),
        Some(Command::OpenNotificationTarget)
    );
    assert!(!keymap.triggers(Command::OpenWorktree, &parsed("ctrl-alt-o"), false));
    // `new_tab` took the daemon's key, so the removal is left unbound.
    assert!(keymap.triggers(Command::Tab, &parsed("ctrl-alt-r"), false));
    assert_eq!(keymap.shortcuts(Command::RemoveWorktree).count(), 0);

    // The palette and `[keybindings]` know both commands by Herdr's names.
    let named = BTreeMap::from([
        (
            "open_worktree".to_owned(),
            Binding::One("ctrl-alt-shift-o".into()),
        ),
        (
            "remove_worktree".to_owned(),
            Binding::One("ctrl-alt-shift-x".into()),
        ),
    ]);
    let keymap =
        Keymap::with_overrides(&named, &Default::default(), &DaemonKeys::default()).unwrap();
    assert_eq!(keymap.primary(Command::OpenWorktree), "ctrl-alt-shift-o");
    assert_eq!(keymap.primary(Command::RemoveWorktree), "ctrl-alt-shift-x");
}
