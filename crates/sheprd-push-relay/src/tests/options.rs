use crate::{Error, Options};
use std::{ffi::OsString, net::SocketAddr, path::PathBuf};

fn parse(arguments: &[&str]) -> crate::Result<Option<Options>> {
    Options::parse(arguments.iter().map(OsString::from))
}

#[test]
fn takes_the_required_paths_and_defaults_to_loopback() {
    let options = parse(&["--secrets", "/s", "--database", "/d"])
        .unwrap()
        .unwrap();
    assert_eq!(options.secrets, PathBuf::from("/s"));
    assert_eq!(options.database, PathBuf::from("/d"));
    assert!(options.listen.ip().is_loopback());
    assert!(!options.behind_proxy);
}

#[test]
fn takes_every_option() {
    let options = parse(&[
        "--behind-proxy",
        "--listen",
        "0.0.0.0:9000",
        "--secrets",
        "/s",
        "--database",
        "/d",
    ])
    .unwrap()
    .unwrap();
    assert!(options.behind_proxy);
    assert_eq!(
        options.listen,
        "0.0.0.0:9000".parse::<SocketAddr>().unwrap()
    );
}

#[test]
fn help_starts_nothing() {
    assert!(parse(&["--help"]).unwrap().is_none());
}

#[test]
fn bad_arguments_are_refused() {
    for arguments in [
        &[][..],
        &["--secrets", "/s"],
        &["--database", "/d"],
        &["--secrets"],
        &["--nope"],
        &["--secrets", "/s", "--database", "/d", "--listen", "here"],
    ] {
        assert!(
            matches!(parse(arguments), Err(Error::Argument { .. })),
            "{arguments:?}"
        );
    }
}
