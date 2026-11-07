use super::*;

fn ctrl_q(bindings: Vec<(Command, Trigger)>) -> DaemonKeys {
    DaemonKeys {
        prefixes: vec![keystroke("ctrl-q")],
        bindings,
        ..no_keys()
    }
}

fn resolve(entries: &[(&str, Binding)], keys: &DaemonKeys) -> Result<Keymap> {
    Keymap::with_overrides(&overrides(entries), &Default::default(), keys)
}

/// Freeing the catalog keystroke while keeping the daemon's chord is why a
/// value can spell a chord at all.
#[test]
fn a_command_can_keep_only_its_chord() {
    let keys = ctrl_q(vec![(
        Command::ClosePane,
        Trigger::Prefixed(keystroke("x")),
    )]);
    let keymap = resolve(&[("close_pane", one("prefix-x"))], &keys).unwrap();
    assert_eq!(list(&keymap, Command::ClosePane), ["ctrl-q x"]);
    assert_eq!(keymap.chord(&keystroke("x")), Some(Command::ClosePane));
    assert!(
        keymap
            .bindings()
            .all(|(command, _)| command != Command::ClosePane)
    );
    // Listing only the keystroke drops the chord, and an empty value both.
    let keymap = resolve(&[("close_pane", one(default("cmd-w")))], &keys).unwrap();
    assert_eq!(list(&keymap, Command::ClosePane), [default("cmd-w")]);
    assert_eq!(keymap.chord(&keystroke("x")), None);
    let keymap = resolve(&[("close_pane", Binding::Many(Vec::new()))], &keys).unwrap();
    assert!(list(&keymap, Command::ClosePane).is_empty());
    assert_eq!(keymap.chord(&keystroke("x")), None);
}

#[test]
fn a_configured_chord_takes_the_key_from_a_daemon_chord() {
    let keys = ctrl_q(vec![
        (Command::CloseTab, Trigger::Prefixed(keystroke("x"))),
        (Command::ClosePane, Trigger::Prefixed(keystroke("y"))),
    ]);
    let keymap = resolve(&[("close_pane", one("prefix-x"))], &keys).unwrap();
    assert_eq!(keymap.chord(&keystroke("x")), Some(Command::ClosePane));
    assert_eq!(keymap.chord(&keystroke("y")), None);
    assert_eq!(list(&keymap, Command::CloseTab), [default("cmd-shift-w")]);
}

#[test]
fn two_commands_cannot_claim_one_chord() {
    let error = resolve(
        &[
            ("close_pane", one("prefix-x")),
            ("new_tab", one("prefix-x")),
        ],
        &ctrl_q(Vec::new()),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        crate::Error::Keymap(Error::DuplicateKeystroke { ref keystroke, .. })
            if keystroke == "prefix-x"
    ));
    // A chord and the same key as a keystroke are different things to type.
    resolve(
        &[
            ("close_pane", one("prefix-alt-x")),
            ("new_tab", one("alt-x")),
        ],
        &ctrl_q(Vec::new()),
    )
    .unwrap();
}

#[test]
fn a_chord_key_must_parse_but_needs_no_modifier() {
    assert!(matches!(
        resolve(&[("close_pane", one("prefix-"))], &ctrl_q(Vec::new())),
        Err(crate::Error::Keymap(Error::ChordWithoutKey("close_pane")))
    ));
    let error = resolve(&[("close_pane", one("prefix-nope-x"))], &ctrl_q(Vec::new())).unwrap_err();
    assert!(matches!(
        error,
        crate::Error::Keymap(Error::InvalidKeystroke { command: "close_pane", ref keystroke, .. })
            if keystroke == "prefix-nope-x"
    ));
    for good in ["prefix-x", "prefix-shift-x", "prefix-tab", "prefix--"] {
        let keymap = resolve(&[("close_pane", one(good))], &ctrl_q(Vec::new())).unwrap();
        assert_eq!(list(&keymap, Command::ClosePane).len(), 1, "{good}");
    }
}

/// The prefix is the daemon's and can differ per device, so a chord it cannot
/// reach is no config error.
#[test]
fn a_chord_the_prefix_cannot_reach_is_left_out() {
    let keymap = resolve(&[("close_pane", one("prefix-ctrl-q"))], &ctrl_q(Vec::new())).unwrap();
    assert!(list(&keymap, Command::ClosePane).is_empty());
    assert_eq!(keymap.chord(&keystroke("ctrl-q")), None);
    // The GUI config holds the only prefix, so no chord can start.
    let keymap = resolve(
        &[("close_pane", one("prefix-x")), ("themes", one("ctrl-q"))],
        &ctrl_q(Vec::new()),
    )
    .unwrap();
    assert!(list(&keymap, Command::ClosePane).is_empty());
}

#[test]
fn a_chord_follows_whichever_prefix_the_daemon_has() {
    let entries = [("close_pane", one("prefix-x"))];
    let keymap = resolve(&entries, &no_keys()).unwrap();
    assert_eq!(list(&keymap, Command::ClosePane), ["ctrl-b x"]);
    let keymap = resolve(&entries, &ctrl_q(Vec::new())).unwrap();
    assert_eq!(list(&keymap, Command::ClosePane), ["ctrl-q x"]);
}

/// The shortcut editor starts from these, so saving one unchanged must bind
/// what it read, whatever the key: symbols, `-`, a literal plus, shifted keys.
#[test]
fn every_entry_reads_back_as_the_shortcut_it_came_from() {
    let mut keys = DaemonKeys::default();
    keys.bindings.push((
        Command::Themes,
        Trigger::Prefixed(Keystroke {
            modifiers: Modifiers::default(),
            key: "+".to_owned(),
            key_char: None,
        }),
    ));
    let keymap = Keymap::with_overrides(&BTreeMap::new(), &Default::default(), &keys).unwrap();
    let written: BTreeMap<String, Binding> = COMMANDS
        .iter()
        .map(|info| {
            let entries = keymap.entries(info.command).collect::<Vec<_>>();
            assert!(
                entries.iter().all(|entry| !entry.contains(' ')),
                "{entries:?}"
            );
            (info.name.to_owned(), Binding::Many(entries))
        })
        .collect();
    let read = Keymap::with_overrides(&written, &Default::default(), &keys).unwrap();
    assert!(list(&keymap, Command::Themes).contains(&"ctrl-b +"));
    for info in COMMANDS {
        assert_eq!(
            list(&read, info.command),
            list(&keymap, info.command),
            "{}",
            info.name
        );
    }
    assert_eq!(read.chord(&keystroke("+")), Some(Command::Themes));
    assert_eq!(read.chord(&keystroke("-")), Some(Command::SplitDown));
}
