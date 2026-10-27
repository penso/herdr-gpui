use super::*;
use std::{fs, os::unix::fs::symlink, path::PathBuf};

const CLIENT: &str = r#"{"endpoint_protocol_generation":1,"endpoint_capabilities":["surface_interest","presentation_effects_fence","health_check"],"remote_bridge_idle_timeout":true}"#;

struct Host {
    root: tempfile::TempDir,
    home: PathBuf,
}

impl Host {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home ' $(: > hook-ran)");
        fs::create_dir(&home).unwrap();
        // Discovery must neither source shell profiles nor evaluate mise config.
        fs::write(home.join(".profile"), ": > hook-ran\n").unwrap();
        fs::write(
            home.join("mise.toml"),
            "[env]\nSIDE_EFFECT = \"{{ exec(command=': > hook-ran') }}\"\n",
        )
        .unwrap();
        Self { root, home }
    }

    fn data(&self) -> PathBuf {
        self.home.join(".local/share/mise")
    }

    fn binary(&self, path: &Path, client: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        crate::test_executable::write(
            path,
            format!(
                r#"#!/bin/sh
case "$*" in
    'status client --json') printf '%s\n' '{client}';;
    "--session work's status server --json") printf '%s\n' '{{"running":true}}';;
    'session list --json') printf '%s\n' '{{"sessions":[]}}';;
    "--session work's remote-client-bridge --idle-timeout-v1")
        IFS= read -r hello || exit 1
        printf '%s\n' "$hello";;
    *) exit 1;;
esac
"#
            ),
            0o700,
        )
        .unwrap();
    }

    fn install(&self, data: &Path, tool: &str, executable: &str) {
        let installs = data.join("installs").join(tool);
        self.binary(&installs.join("0.9.5").join(executable), CLIENT);
        symlink("0.9.5", installs.join("latest")).unwrap();
    }

    fn shim(&self, data: &Path) -> PathBuf {
        let shims = data.join("shims");
        fs::create_dir_all(&shims).unwrap();
        for name in ["herdr", "mise"] {
            crate::test_executable::write(
                shims.join(name),
                "#!/bin/sh\n: > \"$HOME/hook-ran\"\nexit 1\n",
                0o700,
            )
            .unwrap();
        }
        shims
    }

    fn command(&self, script: &str) -> Command {
        // Hide only system install roots, so a developer's installed Herdr can
        // never be probed or bridged by these fixtures. HOME/PATH are isolated.
        let mut script = script.to_owned();
        for system in [
            "/opt/homebrew/bin/herdr",
            "/usr/local/bin/herdr",
            "/home/linuxbrew/.linuxbrew/bin/herdr",
            "/etc/profiles/per-user/$USER/bin/herdr",
            "/nix/var/nix/profiles/default/bin/herdr",
            "/run/current-system/sw/bin/herdr",
        ] {
            script = script.replace(system, self.root.path().join("absent").to_str().unwrap());
        }
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", &script])
            .env_clear()
            .env("HOME", &self.home)
            .env("PATH", self.root.path())
            .current_dir(&self.home);
        command
    }

    fn probe(&self, command: &mut Command) -> HostProbe {
        let (status, output) = run(command, Duration::from_secs(5), || false).unwrap();
        assert!(status.success());
        assert!(!self.home.join("hook-ran").exists());
        classify_probe(&output).unwrap()
    }
}

#[test]
fn mise_installs_are_found_without_shell_activation_or_shims() {
    for tool in [
        "herdr",
        "github-herdrdev-herdr",
        "github-ogulcancelik-herdr",
    ] {
        for executable in ["herdr", "bin/herdr"] {
            let host = Host::new();
            host.install(&host.data(), tool, executable);
            let mut probe = host.command(&probe_script("work's"));
            assert_eq!(host.probe(&mut probe), HostProbe::Running);
            let (status, sessions) = run(
                &mut host.command(&crate::sessions::session_list_script()),
                Duration::from_secs(5),
                || false,
            )
            .unwrap();
            assert!(status.success());
            assert_eq!(sessions, b"{\"sessions\":[]}\n");
        }
    }
}

#[test]
fn mise_data_overrides_are_quoted_and_shims_are_never_executed() {
    for variable in ["MISE_DATA_DIR", "XDG_DATA_HOME"] {
        let host = Host::new();
        let value = host.home.join("custom ' $(: > hook-ran)");
        let data = if variable == "XDG_DATA_HOME" {
            value.join("mise")
        } else {
            value.clone()
        };
        host.install(&data, "herdr", "herdr");
        let mut probe = host.command(&probe_script("work's"));
        probe.env(variable, &value).env("PATH", host.shim(&data));
        if variable == "MISE_DATA_DIR" {
            probe.env("XDG_DATA_HOME", host.home.join("unused"));
        }
        assert_eq!(host.probe(&mut probe), HostProbe::Running);
    }
}

#[test]
fn missing_or_broken_mise_installs_do_not_run_the_shim() {
    let host = Host::new();
    let installs = host.data().join("installs/herdr");
    fs::create_dir_all(&installs).unwrap();
    symlink("missing", installs.join("latest")).unwrap();
    let mut probe = host.command(&probe_script("work's"));
    probe.env("PATH", host.shim(&host.data()));
    assert_eq!(host.probe(&mut probe), HostProbe::Missing);
}

#[test]
fn relative_mise_data_is_not_resolved_against_the_project() {
    let host = Host::new();
    host.install(&host.home.join("relative"), "herdr", "herdr");
    let mut probe = host.command(&probe_script("work's"));
    probe.env("MISE_DATA_DIR", "relative");
    assert_eq!(host.probe(&mut probe), HostProbe::Missing);
}

#[test]
fn bridge_skips_incompatible_install_then_uses_mise_with_quoted_session() {
    let host = Host::new();
    host.binary(
        &host.home.join(".local/bin/herdr"),
        r#"{"endpoint_protocol_generation":0,"endpoint_capabilities":[]}"#,
    );
    host.install(&host.data(), "herdr", "bin/herdr");
    let mut command = host.command(&bridge_script("work's"));
    command.env("PATH", host.shim(&host.data()));
    let (mut stream, child) = bridge(command, &AtomicBool::new(false)).unwrap();
    stream.write_all(b"hello\n").unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut response = [0; 6];
    stream.read_exact(&mut response).unwrap();
    assert_eq!(&response, b"hello\n");
    drop(child);
    assert!(!host.home.join("hook-ran").exists());
}

#[test]
fn path_install_keeps_precedence_over_mise() {
    let host = Host::new();
    host.binary(&host.root.path().join("herdr"), CLIENT);
    let mise = host.data().join("installs/herdr/latest/herdr");
    fs::create_dir_all(mise.parent().unwrap()).unwrap();
    crate::test_executable::write(&mise, "#!/bin/sh\n: > \"$HOME/mise-ran\"\nexit 1\n", 0o700)
        .unwrap();
    let (mut stream, child) = bridge(
        host.command(&bridge_script("work's")),
        &AtomicBool::new(false),
    )
    .unwrap();
    stream.write_all(b"hello\n").unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut response = [0; 6];
    stream.read_exact(&mut response).unwrap();
    assert_eq!(&response, b"hello\n");
    drop(child);
    assert!(!host.home.join("mise-ran").exists());
}
