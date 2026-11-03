use super::*;

#[test]
fn serving_forwards_loopback_to_the_scripts_socket() {
    let command = serve_command("user@host", 41000, "herdr-vscode-ab12", "echo hi").unwrap();
    let args: Vec<_> = command.get_args().map(|a| a.to_str().unwrap()).collect();
    for option in [
        "BatchMode=yes",
        "StrictHostKeyChecking=yes",
        "ControlMaster=no",
        "ControlPath=none",
        "ExitOnForwardFailure=yes",
        "127.0.0.1:41000:/tmp/herdr-vscode-ab12/socket",
    ] {
        assert!(args.contains(&option), "{option} missing from {args:?}");
    }
    // Clearing forwardings would clear this one too, and a script runs.
    assert!(!args.contains(&"ClearAllForwardings=yes"));
    assert!(!args.contains(&"-N"));
    let script = args[args.len() - 1];
    assert!(script.starts_with("/bin/sh -c "), "{script}");
    assert!(script.contains("socket="), "{script}");
    assert!(script.contains("/tmp/herdr-vscode-ab12/socket"), "{script}");
    assert!(script.contains("echo hi"), "{script}");
    assert_eq!(args[args.len() - 2], "user@host");
    assert_eq!(args[args.len() - 3], "--");
}

#[test]
fn serving_refuses_bad_targets_and_socket_names() {
    assert!(serve_command("-oProxyCommand=bad", 1, "name", "true").is_err());
    for name in ["", "../etc", "a b", "UPPER", "semi;colon", &"a".repeat(65)] {
        let error = serve_command("host", 1, name, "true").unwrap_err();
        assert!(
            matches!(error, crate::Error::Ssh(Error::InvalidSocket)),
            "{name:?}: {error:?}"
        );
    }
}
