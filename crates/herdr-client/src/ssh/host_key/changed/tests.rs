use super::*;

const BOX_KEY: &str =
    "box ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJx\n";
const OTHER_KEY: &str =
    "other ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJx\n";
const NEW_ENTRY: &str = "box ssh-ed25519 AAAANEW\n";

fn known(user: &Path, global: &Path) -> KnownHosts {
    KnownHosts {
        user: vec![user.to_owned()],
        global: vec![global.to_owned()],
        name: "box".into(),
    }
}

fn changed(known: KnownHosts, trusted: Vec<HostKeyFingerprint>) -> ChangedHostKey {
    ChangedHostKey {
        fingerprints: Vec::new(),
        trusted,
        entries: NEW_ENTRY.into(),
        known_hosts: known.user[0].clone(),
        name: known.name,
        user: known.user,
        global: known.global,
    }
}

#[test]
fn the_trusted_key_is_read_from_the_users_files() {
    let root = tempfile::tempdir().unwrap();
    let user = root.path().join("known_hosts");
    std::fs::write(&user, format!("{OTHER_KEY}{BOX_KEY}")).unwrap();
    let keys = trusted_keys(&known(&user, &root.path().join("missing"))).unwrap();
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0].kind, "ED25519");
    assert!(keys[0].fingerprint.starts_with("SHA256:"));
}

#[test]
fn keys_set_system_wide_or_by_markers_are_never_replaced() {
    let root = tempfile::tempdir().unwrap();
    let user = root.path().join("known_hosts");
    let global = root.path().join("global");
    std::fs::write(&user, BOX_KEY).unwrap();
    std::fs::write(&global, BOX_KEY).unwrap();
    assert!(matches!(
        trusted_keys(&known(&user, &global)),
        Err(crate::Error::Ssh(Error::KeyNotReplaceable))
    ));
    std::fs::remove_file(&global).unwrap();
    std::fs::write(&user, format!("@revoked {BOX_KEY}")).unwrap();
    assert!(matches!(
        trusted_keys(&known(&user, &global)),
        Err(crate::Error::Ssh(Error::KeyNotReplaceable))
    ));
}

#[test]
fn a_host_with_no_trusted_key_is_not_a_changed_one() {
    let root = tempfile::tempdir().unwrap();
    let user = root.path().join("known_hosts");
    std::fs::write(&user, OTHER_KEY).unwrap();
    assert!(matches!(
        trusted_keys(&known(&user, &root.path().join("missing"))),
        Err(crate::Error::Ssh(Error::Refused(SshFailure::HostKey)))
    ));
}

#[test]
fn replacing_removes_only_the_hosts_old_key_and_appends_the_new_one() {
    let root = tempfile::tempdir().unwrap();
    let user = root.path().join("known_hosts");
    std::fs::write(&user, format!("{OTHER_KEY}{BOX_KEY}")).unwrap();
    let known = known(&user, &root.path().join("missing"));
    let trusted = trusted_keys(&known).unwrap();
    replace_host_key(&changed(known, trusted)).unwrap();
    // Windows' `ssh-keygen -R` rewrites the lines it keeps with CRLF, which
    // OpenSSH reads the same, so lines are compared without their endings.
    let lines = |path: &Path| -> Vec<String> {
        std::fs::read_to_string(path)
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect()
    };
    assert_eq!(lines(&user), [OTHER_KEY.trim_end(), NEW_ENTRY.trim_end()]);
    // `ssh-keygen -R` keeps the file as it was.
    assert_eq!(
        lines(&root.path().join("known_hosts.old")),
        [OTHER_KEY.trim_end(), BOX_KEY.trim_end()]
    );
}

#[test]
fn a_trusted_key_that_changed_since_it_was_shown_is_kept() {
    let root = tempfile::tempdir().unwrap();
    let user = root.path().join("known_hosts");
    std::fs::write(&user, BOX_KEY).unwrap();
    let known = known(&user, &root.path().join("missing"));
    let shown = vec![HostKeyFingerprint {
        kind: "ED25519".into(),
        bits: 256,
        fingerprint: "SHA256:shown".into(),
    }];
    assert!(matches!(
        replace_host_key(&changed(known, shown)),
        Err(crate::Error::Ssh(Error::Refused(SshFailure::HostKey)))
    ));
    assert_eq!(std::fs::read_to_string(&user).unwrap(), BOX_KEY);
}

#[test]
fn invalid_targets_are_refused_before_ssh_runs() {
    assert!(matches!(
        scan_changed_host_key("-oProxyCommand=x"),
        Err(crate::Error::Ssh(Error::InvalidTarget))
    ));
}
