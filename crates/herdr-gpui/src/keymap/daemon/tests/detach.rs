use super::*;

#[test]
fn detach_uses_herdr_default_and_honors_custom_or_empty_bindings() {
    assert_eq!(
        bound(&DaemonKeys::default(), Command::Detach),
        [prefixed("q")]
    );
    let custom = keys("[keys]\nprefix = 'ctrl+a'\ndetach = 'prefix+d'");
    assert_eq!(custom.prefixes, [parsed("ctrl-a")]);
    assert_eq!(bound(&custom, Command::Detach), [prefixed("d")]);
    for value in ["''", "[]"] {
        let disabled = keys(&format!("[keys]\ndetach = {value}"));
        assert!(bound(&disabled, Command::Detach).is_empty());
    }
}

#[test]
fn detach_reads_the_remote_profile_and_does_not_override_configured_chords() {
    let profile = "[keys]\nprefix = 'ctrl+a'\ndetach = 'alt+d'";
    assert_eq!(
        DaemonKeys::from_profile(Some(profile)).unwrap(),
        keys(profile)
    );
    let configured = keys("[keys]\nnew_tab = 'prefix+q'");
    let keymap = super::super::super::Keymap::with_overrides(
        &Default::default(),
        &Default::default(),
        &configured,
    )
    .unwrap();
    assert_eq!(keymap.chord(&parsed("q")), Some(Command::Tab));
}
