use super::*;

#[test]
fn fingerprints_are_read_from_ssh_keygen_lines() {
    assert_eq!(
        parse_fingerprint(
            "256 SHA256:eUXGGm1YGsMAS7vkcx6JOJdOGHPem5gQp4taiCfCLB8 gitlab.com (ED25519)"
        ),
        Some(HostKeyFingerprint {
            kind: "ED25519".into(),
            bits: 256,
            fingerprint: "SHA256:eUXGGm1YGsMAS7vkcx6JOJdOGHPem5gQp4taiCfCLB8".into(),
        })
    );
    // Hashed host names carry `|` and `=`; only the first and last words count.
    assert_eq!(
        parse_fingerprint("3072 SHA256:abc+/= |1|c2FsdA==|aGFzaA== (RSA)").map(|key| key.bits),
        Some(3072)
    );
    for line in [
        "",
        "256 MD5:aa:bb host (ED25519)",
        "x SHA256:abc host (ED25519)",
        "256 SHA256: host (ED25519)",
        "256 SHA256:a\u{1b}[31m host (ED25519)",
        "256 SHA256:abc host ED25519",
        "256 SHA256:abc host (ED 25519)",
    ] {
        assert_eq!(parse_fingerprint(line), None, "{line:?}");
    }
}

/// An absolute path on this platform: Windows needs a drive for one.
const ROOT: &str = if cfg!(windows) { "C:" } else { "" };

#[test]
fn known_hosts_files_come_from_ssh_g_in_order() {
    let output = format!(
        "hostname Box\nport 22\nuserknownhostsfile {ROOT}/home/me/.ssh/known_hosts {ROOT}/home/me/.ssh/known_hosts2\nglobalknownhostsfile {ROOT}/etc/ssh/ssh_known_hosts\n"
    );
    assert_eq!(
        parse_known_hosts(output.as_bytes()),
        Some(KnownHosts {
            user: vec![
                PathBuf::from(format!("{ROOT}/home/me/.ssh/known_hosts")),
                PathBuf::from(format!("{ROOT}/home/me/.ssh/known_hosts2")),
            ],
            global: vec![PathBuf::from(format!("{ROOT}/etc/ssh/ssh_known_hosts"))],
            name: "box".into(),
        })
    );
    for output in [
        "hostname box\nport 22\n".to_owned(),
        "hostname box\nport 22\nuserknownhostsfile none\n".to_owned(),
        "hostname box\nport 22\nuserknownhostsfile ~/.ssh/known_hosts\n".to_owned(),
        // A path with a space reaches `ssh -G` output as two words, which only
        // Windows drive letters can tell from two paths.
        #[cfg(unix)]
        format!("hostname box\nport 22\nuserknownhostsfile {ROOT}/home/my files/known_hosts\n"),
        format!("hostname box\nport 22\nuserknownhostsfile \"{ROOT}/home/me/known_hosts\"\n"),
        format!("userknownhostsfile {ROOT}/k\nport 22\n"),
        format!("hostname -oProxyCommand=x\nport 22\nuserknownhostsfile {ROOT}/k\n"),
    ] {
        assert_eq!(parse_known_hosts(output.as_bytes()), None, "{output:?}");
    }
}

/// What OpenSSH for Windows 9.5 prints: the profile with mixed separators,
/// and its system-wide directory as a placeholder it expands itself.
#[cfg(windows)]
#[test]
fn windows_known_hosts_files_keep_their_drive_and_expand_program_data() {
    let output = br"hostname box
port 22
userknownhostsfile C:\Users\me/.ssh/known_hosts C:\Users\me/.ssh/known_hosts2
globalknownhostsfile __PROGRAMDATA__\ssh/ssh_known_hosts __PROGRAMDATA__\ssh/ssh_known_hosts2
";
    let known = parse_known_hosts(output).unwrap();
    assert_eq!(
        known.user,
        [
            PathBuf::from(r"C:\Users\me/.ssh/known_hosts"),
            PathBuf::from(r"C:\Users\me/.ssh/known_hosts2"),
        ]
    );
    let data = PathBuf::from(std::env::var_os("ProgramData").unwrap());
    assert_eq!(
        known.global,
        [
            PathBuf::from(format!(r"{}\ssh/ssh_known_hosts", data.display())),
            PathBuf::from(format!(r"{}\ssh/ssh_known_hosts2", data.display())),
        ]
    );
    // The scan lists them for `ssh` with forward slashes, so no backslash
    // can read as an escape inside the quotes.
    let options = scan_options(Path::new(r"C:\Temp\scan\known_hosts"), &known.user);
    assert_eq!(
        options[1],
        r#"UserKnownHostsFile="C:/Temp/scan/known_hosts" "C:/Users/me/.ssh/known_hosts" "C:/Users/me/.ssh/known_hosts2""#
    );
}

/// A profile whose name has spaces reaches `ssh -G` unquoted. On Windows each
/// listed file still starts with a drive, so the words rejoin into one path
/// when the spaced directory exists, and the scan quotes it whole.
#[cfg(windows)]
#[test]
fn windows_profile_paths_with_spaces_stay_whole() {
    let root = tempfile::tempdir().unwrap();
    for profile in ["Jane Doe", "Jane  Q Doe"] {
        std::fs::create_dir(root.path().join(profile)).unwrap();
    }
    let base = root.path().display();
    let output = format!(
        "hostname box\nport 22\nuserknownhostsfile {base}\\Jane Doe/.ssh/known_hosts {base}\\Jane  Q Doe/.ssh/known_hosts2\nglobalknownhostsfile __PROGRAMDATA__\\ssh/ssh_known_hosts\n"
    );
    let known = parse_known_hosts(output.as_bytes()).unwrap();
    assert_eq!(
        known.user,
        [
            PathBuf::from(format!("{base}\\Jane Doe/.ssh/known_hosts")),
            PathBuf::from(format!("{base}\\Jane  Q Doe/.ssh/known_hosts2")),
        ]
    );
    assert_eq!(known.global.len(), 1);
    let options = scan_options(Path::new(r"C:\Temp\known_hosts"), &known.user[..1]);
    let expected = format!("{base}/Jane Doe/.ssh/known_hosts").replace('\\', "/");
    assert_eq!(
        options[1],
        format!(r#"UserKnownHostsFile="C:/Temp/known_hosts" "{expected}""#)
    );
    // A word that starts no path still cannot begin the list.
    assert!(
        parse_known_hosts(b"hostname box\nport 22\nuserknownhostsfile Doe/known_hosts\n").is_none()
    );
    // A relative file after an absolute one would join its name: refused,
    // so approval never writes to a file `ssh` does not read.
    std::fs::create_dir(root.path().join(".ssh")).unwrap();
    let ambiguous = format!(
        "hostname box\nport 22\nuserknownhostsfile {base}\\.ssh/known_hosts known_hosts.extra\n"
    );
    assert!(parse_known_hosts(ambiguous.as_bytes()).is_none());
    // So is a spaced directory that does not exist.
    let missing = format!("hostname box\nport 22\nuserknownhostsfile {base}\\No One/known_hosts\n");
    assert!(parse_known_hosts(missing.as_bytes()).is_none());
}

#[cfg(windows)]
#[test]
fn spaces_count_only_inside_existing_directories() {
    let is = |dir: &'static str| move |path: &Path| path == Path::new(dir);
    let file = Path::new(r"C:\Users\Jane Doe\.ssh\known_hosts");
    assert!(spaces_in_directories(file, is(r"C:\Users\Jane Doe")));
    assert!(!spaces_in_directories(file, |_| false));
    // A space in the file name itself is never resolved.
    assert!(!spaces_in_directories(
        Path::new(r"C:\Users\me\.ssh\known_hosts known_hosts.extra"),
        |_| true
    ));
    assert!(spaces_in_directories(
        Path::new(r"C:\Users\me\.ssh\known_hosts"),
        |_| false
    ));
}

#[test]
fn keys_are_looked_up_under_the_alias_and_port_ssh_uses() {
    let name = |output: String| parse_known_hosts(output.as_bytes()).unwrap().name;
    assert_eq!(
        name(format!(
            "hostname box\nport 2222\nuserknownhostsfile {ROOT}/k\n"
        )),
        "[box]:2222"
    );
    assert_eq!(
        name(format!(
            "hostname box\nhostkeyalias Alias\nport 22\nuserknownhostsfile {ROOT}/k\n"
        )),
        "Alias"
    );
    assert_eq!(
        name(format!(
            "hostname box\nport 22\nuserknownhostsfile {ROOT}/k\nglobalknownhostsfile none\n"
        )),
        "box"
    );
}

#[test]
fn a_host_with_any_trusted_key_is_found_in_user_or_global_files() {
    let root = tempfile::tempdir().unwrap();
    let user = root.path().join("known_hosts");
    let global = root.path().join("global");
    std::fs::write(&user, BOX_KEY.replacen("box", "other", 1)).unwrap();
    std::fs::write(&global, BOX_KEY).unwrap();
    let missing = root.path().join("missing");
    assert!(lists_host("box", [&user, &missing, &global]).unwrap());
    assert!(!lists_host("box", [&user, &missing]).unwrap());
    assert!(!lists_host("[box]:2222", [&user, &global]).unwrap());
}

#[test]
fn scan_options_record_into_scratch_and_offer_no_credential() {
    let options = scan_options(
        Path::new("/tmp/scratch/known_hosts"),
        &[PathBuf::from("/home/me/.ssh/known_hosts")],
    );
    assert_eq!(options.len(), 28);
    assert!(options.iter().step_by(2).all(|flag| flag == "-o"));
    let values: Vec<&str> = options
        .iter()
        .skip(1)
        .step_by(2)
        .map(String::as_str)
        .collect();
    assert_eq!(
        values[0],
        "UserKnownHostsFile=\"/tmp/scratch/known_hosts\" \"/home/me/.ssh/known_hosts\""
    );
    for expected in [
        "StrictHostKeyChecking=accept-new",
        "PubkeyAuthentication=no",
        "PasswordAuthentication=no",
        "KbdInteractiveAuthentication=no",
        "ControlPath=none",
        "ForwardAgent=no",
        "PermitLocalCommand=no",
    ] {
        assert!(values.contains(&expected), "{expected}");
    }
}

#[test]
fn entries_with_controls_are_refused() {
    assert_eq!(
        valid_entries("host ssh-ed25519 AAAA\n\n").unwrap(),
        "host ssh-ed25519 AAAA\n"
    );
    assert!(matches!(
        valid_entries("host ssh-ed25519 AAAA\u{1b}]52;c;x\u{7}"),
        Err(crate::Error::RemoteOutput)
    ));
}

fn key(known_hosts: PathBuf) -> NewHostKey {
    NewHostKey {
        fingerprints: Vec::new(),
        entries: "box ssh-ed25519 AAAA\n".into(),
        checked: vec![known_hosts.clone()],
        known_hosts,
        name: "box".into(),
    }
}

const BOX_KEY: &str =
    "box ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJx\n";

#[test]
fn a_key_trusted_meanwhile_is_never_replaced() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("known_hosts");
    std::fs::write(&path, BOX_KEY).unwrap();
    assert!(matches!(
        trust_host_key(&key(path.clone())),
        Err(crate::Error::Ssh(Error::Refused(SshFailure::HostKey)))
    ));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), BOX_KEY);
}

#[test]
fn an_unreadable_known_hosts_file_fails_the_check() {
    let root = tempfile::tempdir().unwrap();
    // A directory stands in for a file that cannot be checked.
    let directory = root.path().join("known_hosts");
    std::fs::create_dir(&directory).unwrap();
    assert!(matches!(
        lists_host("box", [&directory]),
        Err(crate::Error::Storage {
            operation: StorageOperation::Metadata,
            ..
        })
    ));
    assert!(!lists_host("box", [&root.path().join("missing")]).unwrap());
}

#[test]
fn trusting_appends_after_an_unterminated_last_line() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("known_hosts");
    std::fs::write(&path, "other ssh-rsa BBBB").unwrap();
    trust_host_key(&key(path.clone())).unwrap();
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "other ssh-rsa BBBB\nbox ssh-ed25519 AAAA\n"
    );
}

#[cfg(unix)]
#[test]
fn trusting_creates_a_private_directory_and_file() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join(".ssh").join("known_hosts");
    trust_host_key(&key(path.clone())).unwrap();
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "box ssh-ed25519 AAAA\n"
    );
    let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(path.parent().unwrap()), 0o700);
    assert_eq!(mode(&path), 0o600);
}

#[test]
fn a_failed_append_names_the_operation() {
    let root = tempfile::tempdir().unwrap();
    // A directory where the file should be cannot be opened for appending.
    let path = root.path().join("known_hosts");
    std::fs::create_dir(&path).unwrap();
    // Checked elsewhere, so the append itself is what fails.
    let key = NewHostKey {
        checked: Vec::new(),
        ..key(path.clone())
    };
    let error = trust_host_key(&key).unwrap_err();
    assert!(matches!(
        &error,
        crate::Error::Storage { operation: StorageOperation::Open, path: failed, .. } if failed == &path
    ));
    assert!(std::error::Error::source(&error).is_some());
}

#[test]
fn invalid_targets_are_refused_before_ssh_runs() {
    assert!(matches!(
        scan_host_key("-oProxyCommand=x"),
        Err(crate::Error::Ssh(Error::InvalidTarget))
    ));
}
