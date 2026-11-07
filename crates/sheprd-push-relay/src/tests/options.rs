use crate::{Backend, Error, Options};
use std::{ffi::OsString, net::SocketAddr, path::PathBuf};

fn parse(arguments: &[&str]) -> crate::Result<Option<Options>> {
    Options::parse(arguments.iter().map(OsString::from))
}

#[test]
fn takes_the_required_paths_and_defaults_to_loopback() {
    let options = parse(&["--secrets", "/s"]).unwrap().unwrap();
    assert_eq!(options.secrets, PathBuf::from("/s"));
    assert_eq!(options.backend, Backend::Memory);
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
        "--store",
        "sqlite:/d",
    ])
    .unwrap()
    .unwrap();
    assert!(options.behind_proxy);
    assert_eq!(options.backend, Backend::Sqlite(PathBuf::from("/d")));
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
        &["--store", "memory"],
        &["--secrets"],
        &["--nope"],
        &["--secrets", "/s", "--listen", "here"],
        &["--secrets", "/s", "--store", "redis"],
        &["--secrets", "/s", "--store", "sqlite:"],
        &["--secrets", "/s", "--store", "memory:/d"],
        &[
            "--secrets",
            "/s",
            "--store",
            "postgres://user:secret@host/db",
        ],
    ] {
        assert!(
            matches!(parse(arguments), Err(Error::Argument { .. })),
            "{arguments:?}"
        );
    }
}

#[test]
fn postgres_takes_no_address_on_the_command_line() {
    let options = parse(&["--secrets", "/s", "--store", "postgres"])
        .unwrap()
        .unwrap();
    assert_eq!(options.backend, Backend::Postgres);
}
