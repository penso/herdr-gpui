use super::*;
use herdr_client::shell_quote;

#[test]
fn fetching_uses_the_hosts_configured_ssh_key_and_environment_override() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let checkout = temp.path().join("checkout");
    std::fs::create_dir(&source).unwrap();
    git(&source, &["init", "-b", "main"]);
    git(
        &source,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "--allow-empty",
            "-m",
            "base",
        ],
    );
    git(
        temp.path(),
        &[
            "clone",
            source.to_str().unwrap(),
            checkout.to_str().unwrap(),
        ],
    );
    let head = git(&source, &["rev-parse", "HEAD"]);
    git(&source, &["update-ref", "refs/pull/51/head", &head]);
    git(
        &checkout,
        &[
            "remote",
            "set-url",
            "origin",
            &format!("ssh://git@fixture.invalid{}", source.display()),
        ],
    );
    git(&checkout, &["config", "ssh.variant", "ssh"]);

    // This shim transports Git's real protocol locally and records the key
    // selection. If the fetch overrides it with plain ssh, fail without ever
    // accessing the network.
    let bin = temp.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    std::os::unix::fs::symlink("/bin/sh", bin.join("ssh")).unwrap();
    let log = temp.path().join("ssh-args");
    let shim = temp.path().join("ssh command.sh");
    std::fs::write(
        &shim,
        format!(
            r#"set -eu
[ "$GIT_TERMINAL_PROMPT" = 0 ]
[ "$GCM_INTERACTIVE" = never ]
[ "$SSH_ASKPASS_REQUIRE" = never ]
printf '%s\n' "$@" > {}
while [ "$#" -gt 1 ]; do shift; done
exec /bin/sh -c "$1"
"#,
            shell_quote(log.to_str().unwrap()),
        ),
    )
    .unwrap();
    let command = |key: &str| {
        format!(
            "/bin/sh {} -i {} -o IdentitiesOnly=yes",
            shell_quote(shim.to_str().unwrap()),
            shell_quote(key),
        )
    };
    let configured_key = temp.path().join("repository's key");
    let configured_key = configured_key.to_str().unwrap();
    git(
        &checkout,
        &["config", "core.sshCommand", &command(configured_key)],
    );
    let item = &parse(&response(), &origin(), Kind::PullRequest).unwrap()[1];
    let script = remote::fetch_script(checkout.join(".git").to_str().unwrap(), item).unwrap();
    let path = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .unwrap();
    for override_key in [None, Some("environment's key")] {
        let mut process = std::process::Command::new("/bin/sh");
        process
            .current_dir(temp.path())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("PATH", &path)
            .env_remove("GIT_SSH")
            .env_remove("GIT_SSH_COMMAND")
            .args(["-c", &script]);
        if let Some(key) = override_key {
            process.env("GIT_SSH_COMMAND", command(key));
        }
        let output = process.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let arguments = std::fs::read_to_string(&log).unwrap();
        assert!(
            arguments.starts_with(&format!(
                "-i\n{}\n-o\nIdentitiesOnly=yes\n",
                override_key.unwrap_or(configured_key)
            )),
            "{arguments}"
        );
        assert_eq!(git(&checkout, &["rev-parse", &item.base_ref()]), head);
    }
}
