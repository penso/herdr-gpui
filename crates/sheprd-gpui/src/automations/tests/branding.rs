use crate::automations::fetch;

#[test]
fn installed_and_current_automation_versions_are_read() {
    for program in ["herdr-automate", "sheprd-automate"] {
        assert_eq!(
            fetch::parse_version(format!("{program} 20261103.1\n").as_bytes()).unwrap(),
            "20261103.1"
        );
        assert!(fetch::parse_version(format!("{program} ../../other").as_bytes()).is_err());
    }
}

#[test]
#[cfg(unix)]
fn remote_automation_commands_prefer_sheprd_and_keep_legacy_hosts_usable() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let install = |name: &str| {
        let path = dir.path().join(name);
        std::fs::write(
            &path,
            format!("#!/bin/sh\nprintf '%s\\n' '{name}' \"$@\"\n"),
        )
        .unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    };
    let invoke = || {
        let output = std::process::Command::new("/bin/sh")
            .args(["-c", &fetch::line(&["approve", "/a b/it's.luau"])])
            .env_clear()
            .env("PATH", dir.path())
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap()
    };
    install("herdr-automate");
    assert_eq!(invoke(), "herdr-automate\napprove\n/a b/it's.luau\n");
    install("sheprd-automate");
    assert_eq!(invoke(), "sheprd-automate\napprove\n/a b/it's.luau\n");
}
