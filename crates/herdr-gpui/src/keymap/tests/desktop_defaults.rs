use super::*;

fn desktop() -> Keymap {
    Keymap::layer(
        vec![None; COMMANDS.len()],
        &Claimed::new(),
        &DaemonKeys::default(),
        Vec::new(),
        Platform::Desktop,
    )
}

#[test]
fn desktop_keymap_binds_the_desktop_table_under_herdr_chords() {
    let keymap = desktop();
    assert_eq!(list(&keymap, Command::Tab), ["ctrl-shift-t", "ctrl-b c"]);
    assert_eq!(keymap.primary(Command::TabNumber(1)), "alt-1");
    assert_eq!(
        list(&keymap, Command::NextTab),
        ["ctrl-pagedown", "ctrl-tab", "ctrl-b n"]
    );
    assert!(
        keymap
            .bindings()
            .all(|(_, keystroke)| !Keystroke::parse(keystroke).unwrap().modifiers.platform)
    );
    assert!(keymap.triggers(Command::SplitRight, &keystroke("ctrl-shift-e"), false));
    assert!(!keymap.triggers(Command::SplitRight, &keystroke("cmd-d"), false));
}

#[test]
fn a_gui_keystroke_moves_off_its_desktop_default() {
    let claimed = Claimed::from([(identity(&keystroke("ctrl-shift-t")), "new_workspace")]);
    let mut configured = vec![None; COMMANDS.len()];
    let workspace = COMMANDS
        .iter()
        .position(|info| info.command == Command::Workspace)
        .unwrap();
    configured[workspace] = Some(vec!["ctrl-shift-t".to_owned()]);
    let keymap = Keymap::layer(
        configured,
        &claimed,
        &DaemonKeys::default(),
        Vec::new(),
        Platform::Desktop,
    );
    assert_eq!(keymap.primary(Command::Workspace), "ctrl-shift-t");
    assert_eq!(list(&keymap, Command::Tab), ["ctrl-b c"]);
}
