use super::*;

/// A catalog default is no Herdr action and nobody configured it, so a custom
/// command bound to its keystroke runs instead, as with `ctrl+shift+t` over
/// New Tab on Linux. A keystroke either config file assigns keeps its command,
/// even when it repeats the default.
#[test]
fn a_custom_command_takes_a_catalog_default_but_not_a_configured_keystroke() {
    let new_tab = default("cmd-t");
    let commands = [custom("create-tab", &[&new_tab.replace('-', "+")])];
    let label = keystroke(new_tab).unparse();
    let check = |keymap: &Keymap, reach: Reach, case: &str| {
        let bindings = keymap.custom_bindings(&commands);
        assert_eq!(
            bindings,
            [vec![custom::CustomBinding {
                label: label.clone(),
                reach
            }]],
            "{case}"
        );
        let found = keymap
            .custom_command(&commands, &keystroke(new_tab), false)
            .map(|command| command.command_id.as_str());
        let expected = (reach == Reach::Runs).then_some("create-tab");
        assert_eq!(found, expected, "{case}");
    };
    check(&Keymap::default(), Reach::Runs, "catalog default");
    check(&with(&[]).unwrap(), Reach::Runs, "no daemon keys");
    check(
        &with(&[("new_tab", Binding::Many(Vec::new()))]).unwrap(),
        Reach::Runs,
        "default unbound",
    );
    check(
        &with(&[("new_tab", one(new_tab))]).unwrap(),
        Reach::Shadowed,
        "default listed in [keybindings]",
    );
    check(
        &with(&[("about", one(new_tab))]).unwrap(),
        Reach::Shadowed,
        "moved by [keybindings]",
    );
    let keys = DaemonKeys {
        bindings: vec![(Command::Zoom, Trigger::Direct(keystroke(new_tab)))],
        ..no_keys()
    };
    let daemon = Keymap::with_overrides(&BTreeMap::new(), &PaneKeys::new(), &keys).unwrap();
    check(&daemon, Reach::Shadowed, "daemon [keys]");
}
