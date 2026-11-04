#![allow(clippy::unwrap_used)]
use super::*;

fn args(command: &Command) -> Vec<String> {
    command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect()
}

#[test]
fn the_command_offers_only_the_chosen_key_pins_the_host_and_forwards_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let known = dir.path().join("sub").join("known");
    let headless = command(Path::new("/home/u/.ssh/id"), &known, false).unwrap();
    let argv = args(&headless);
    assert_eq!(headless.get_program(), "ssh");
    assert_eq!(argv[..3], ["-F", "/dev/null", "-T"]);
    assert_eq!(argv[argv.len() - 2..], ["--", "railway.new"]);
    let options: Vec<_> = argv.iter().skip(3).filter(|arg| *arg != "-o").collect();
    for required in [
        "IdentityFile=\"/home/u/.ssh/id\"".to_owned(),
        format!("UserKnownHostsFile=\"{}\"", known.display()),
        "IdentitiesOnly=yes".into(),
        "StrictHostKeyChecking=accept-new".into(),
        "GlobalKnownHostsFile=/dev/null".into(),
        "BatchMode=yes".into(),
        "ClearAllForwardings=yes".into(),
        "ProxyCommand=none".into(),
    ] {
        assert!(options.contains(&&required), "{required}");
    }
    assert!(!options.iter().any(|option| option.contains("Host=no")));
    assert!(
        known.parent().unwrap().is_dir(),
        "known_hosts can be written"
    );
    // Only the terminal flag differs.
    let terminal = args(&command(Path::new("/k"), &known, true).unwrap());
    assert_eq!(terminal[2], "-tt");
}

#[test]
fn paths_cannot_end_the_value_or_start_a_token() {
    let dir = tempfile::tempdir().unwrap();
    let known = &dir.path().join("known");
    let identity = |path: &str| args(&command(Path::new(path), known, false).unwrap());
    assert!(identity("/a%d/b").contains(&"IdentityFile=\"/a%%d/b\"".to_owned()));
    for bad in ["/a\"b", "/a\nHost x", "relative/key"] {
        assert!(command(Path::new(bad), known, false).is_err(), "{bad:?}");
        assert!(
            command(Path::new("/k"), Path::new(bad), false).is_err(),
            "{bad:?}"
        );
    }
}

/// OpenSSH itself must accept every option, quoted paths with spaces and `%`
/// included; `ssh -G` prints the result without connecting anywhere.
#[test]
fn the_installed_ssh_resolves_the_options_as_intended() {
    let dir = tempfile::tempdir().unwrap();
    let known = dir.path().join("known hosts");
    let built = command(Path::new("/k/my 100%/id"), &known, false).unwrap();
    let mut probe = Command::new("ssh");
    let mut arguments = args(&built);
    arguments.insert(3, "-G".into());
    probe.args(arguments);
    let Ok(output) = probe.output() else {
        return; // ssh is not installed here.
    };
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let resolved = String::from_utf8_lossy(&output.stdout);
    for expected in [
        "hostname railway.new".to_owned(),
        "identitiesonly yes".into(),
        // `-G` shows the value before token expansion, where `%%` is one `%`.
        "identityfile /k/my 100%%/id".into(),
        format!("userknownhostsfile {}", known.display()),
        "forwardagent no".into(),
        "addressfamily inet".into(),
        "stricthostkeychecking accept-new".into(),
    ] {
        assert!(resolved.lines().any(|line| line == expected), "{expected}");
    }
}
