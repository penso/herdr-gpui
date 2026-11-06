use super::*;

#[test]
fn quotes_paths_and_escapes_openssh_tokens() {
    let (root, path, expected) = if cfg!(windows) {
        (
            "C:/tmp/",
            "C:/tmp/boat keys %u/key",
            "\"C:/tmp/boat keys %%u/key\"",
        )
    } else {
        (
            "/tmp/",
            "/tmp/boat keys %u/key",
            "\"/tmp/boat keys %%u/key\"",
        )
    };
    assert_eq!(quoted_path(Path::new(path)).unwrap(), expected);
    assert!(matches!(
        quoted_path(Path::new("relative")),
        Err(Error::Gateway)
    ));
    for name in ["\"key", "\nkey", "\rkey", "\0key"] {
        let path = format!("{root}{name}");
        assert!(matches!(quoted_path(Path::new(&path)), Err(Error::Gateway)));
    }
}

#[test]
fn identity_paths_with_spaces_and_percent_tokens_are_passed_as_one_value() {
    let dir = tempfile::Builder::new()
        .prefix("boat keys %u ")
        .tempdir()
        .unwrap();
    let private = dir.path().join("key");
    let gateway = Gateway::new(&access(), &private).unwrap();
    let args: Vec<_> = gateway.command.get_args().collect();
    assert!(args.iter().any(|arg| {
        *arg == std::ffi::OsStr::new(&format!("IdentityFile={}", quoted_path(&private).unwrap()))
    }));
    assert!(args.iter().any(|arg| {
        *arg == std::ffi::OsStr::new(&format!(
            "UserKnownHostsFile={}",
            quoted_path(gateway._known_hosts.path()).unwrap()
        ))
    }));
}
