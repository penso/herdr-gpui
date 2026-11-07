//! `automation approve`: the path the app gets, and the help that says the
//! command only asks.
use super::super::{super::help, script_path};

#[test]
fn a_script_path_is_resolved_here_or_made_absolute() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("nightly.luau");
    std::fs::write(&file, "-- script").unwrap();
    let resolved = script_path(&file).unwrap();
    assert_eq!(
        std::path::Path::new(&resolved),
        file.canonicalize().unwrap()
    );
    // A remote host's file is not here; its path is kept, made absolute.
    let remote = std::path::Path::new("/nowhere/herdr-test/.herdr/automations/a.luau");
    assert_eq!(script_path(remote).unwrap(), remote.to_str().unwrap());
    let relative = script_path(std::path::Path::new("missing-dir/a.luau")).unwrap();
    assert!(std::path::Path::new(&relative).is_absolute(), "{relative}");
    assert!(relative.ends_with("a.luau"), "{relative}");
}

#[test]
fn automation_help_says_it_only_opens_the_dialog() {
    let help = help::usage(Some("automation"));
    assert!(
        help.contains("sheprd automation approve FILE [--host ID]"),
        "{help}"
    );
    assert!(
        help.contains("Nothing is\n    approved by this command"),
        "{help}"
    );
    assert!(help::usage(None).contains("automation"));
}
