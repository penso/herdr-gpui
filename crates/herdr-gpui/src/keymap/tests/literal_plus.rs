use super::*;

fn labels(keymap: &Keymap, commands: &[ClientShellCommand]) -> Vec<(String, Reach)> {
    keymap
        .custom_bindings(commands)
        .into_iter()
        .flatten()
        .map(|binding| (binding.label, binding.reach))
        .collect()
}

fn entry(label: &str, reach: Reach) -> (String, Reach) {
    (label.to_owned(), reach)
}

#[test]
fn plus_command_labels_run_and_read_as_the_plus_key() {
    let keymap = Keymap::default();
    let commands = [custom("plus", &["prefix++", "ctrl+alt++"])];
    assert_eq!(
        labels(&keymap, &commands),
        [
            entry("ctrl-b +", Reach::Runs),
            entry("ctrl-alt-+", Reach::Runs),
        ]
    );
    let find = |typed: &str, prefixed: bool| {
        keymap
            .custom_command(&commands, &keystroke(typed), prefixed)
            .map(|command| command.command_id.as_str())
    };
    assert_eq!(find("+", true), Some("plus"));
    assert_eq!(find("ctrl-alt-+", false), Some("plus"));
    assert_eq!(find("shift-=->+", true), Some("plus"));
}

#[test]
fn a_plus_prefix_labels_its_chords() {
    let keys = DaemonKeys {
        prefixes: vec![keystroke("ctrl-+")],
        ..no_keys()
    };
    let keymap = Keymap::with_overrides(&BTreeMap::new(), &Default::default(), &keys).unwrap();
    assert_eq!(
        labels(&keymap, &[custom("chord", &["prefix+y"])]),
        [entry("ctrl-+ y", Reach::Runs)]
    );
}

#[test]
fn unsupported_plus_labels_keep_the_plus_key() {
    let keymap = Keymap::default();
    let commands = [custom("hyper", &["hyper++", "prefix+ctrl+hyper++"])];
    assert_eq!(
        labels(&keymap, &commands),
        [
            entry("hyper-+", Reach::Unsupported),
            entry("ctrl-b ctrl-hyper-+", Reach::Unsupported),
        ]
    );
}

#[test]
fn a_malformed_label_never_hides_a_working_one_that_reads_alike() {
    let keymap = Keymap::default();
    // `ctrl+alt+++` is rejected, yet reads as `ctrl-alt-+` too.
    for written in [["ctrl+alt+++", "ctrl+alt++"], ["ctrl+alt++", "ctrl+alt+++"]] {
        assert_eq!(
            labels(&keymap, &[custom("plus", &written)]),
            [entry("ctrl-alt-+", Reach::Runs)],
            "{written:?}"
        );
    }
}
