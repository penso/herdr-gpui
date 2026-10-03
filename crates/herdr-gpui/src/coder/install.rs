//! Checking for, and on explicit approval installing, Herdr inside a Coder
//! workspace the user just created or attached. This is the one place the GUI
//! installs anything remotely: it runs Herdr's published installer, which
//! verifies the release checksum, through `coder ssh`. Background reconnects
//! never call it. Output is bounded and kept only for the failure message.

use super::{Error, Result};
use std::{
    io::Read,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

const PROBE_TIMEOUT: Duration = Duration::from_secs(60);
const INSTALL_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const OUTPUT_LIMIT: usize = 8 * 1024;
const TAIL_LINES: usize = 6;
/// The installer script's exit code when the workspace has no `curl`.
const NO_CURL: i32 = 4;

/// Same search as the connection bridge, so "installed" means "the bridge
/// will find it". Paths stay in shell variables; nothing discovered is eval'd.
const PROBE: &str = r#"candidate=$(command -v herdr 2>/dev/null || :)
case "$candidate" in /*/mise/shims/herdr) candidate=;; /*) ;; *) candidate=;; esac
for path in "$candidate" "$HOME/.local/bin/herdr" /opt/homebrew/bin/herdr /usr/local/bin/herdr /home/linuxbrew/.linuxbrew/bin/herdr "$HOME/.nix-profile/bin/herdr" "/etc/profiles/per-user/$USER/bin/herdr" /nix/var/nix/profiles/default/bin/herdr /run/current-system/sw/bin/herdr; do
    if [ -n "$path" ] && [ -x "$path" ]; then exit 0; fi
done
exit 3"#;

const INSTALL: &str = r#"command -v curl >/dev/null 2>&1 || { echo "curl is required to install Herdr" >&2; exit 4; }
curl -fsSL https://herdr.dev/install.sh | sh"#;

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn remote(script: &str) -> String {
    format!("/bin/sh -c {}", quote(script))
}

struct Finished {
    code: Option<i32>,
    output: String,
}

/// Keep the last `OUTPUT_LIMIT` bytes a pipe produced, draining it to EOF so
/// a chatty child can never block on a full pipe.
fn drain(
    mut pipe: impl Read + Send + 'static,
    into: Arc<Mutex<Vec<u8>>>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut chunk = [0; 4096];
        while let Ok(read) = pipe.read(&mut chunk) {
            if read == 0 {
                break;
            }
            let mut tail = into.lock().unwrap_or_else(|error| error.into_inner());
            tail.extend_from_slice(&chunk[..read]);
            let excess = tail.len().saturating_sub(OUTPUT_LIMIT);
            tail.drain(..excess);
        }
    })
}

fn run(mut command: Command, timeout: Duration, cancelled: &impl Fn() -> bool) -> Result<Finished> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(Error::Process)?;
    let output = Arc::new(Mutex::new(Vec::new()));
    let readers = [
        child.stdout.take().map(|pipe| drain(pipe, output.clone())),
        child.stderr.take().map(|pipe| drain(pipe, output.clone())),
    ];
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait().map_err(Error::Process)? {
            break Ok(status);
        }
        if cancelled() {
            break Err(Error::Cancelled);
        }
        if Instant::now() >= deadline {
            break Err(Error::InstallTimeout);
        }
        thread::sleep(Duration::from_millis(100));
    };
    if status.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    for reader in readers.into_iter().flatten() {
        let _ = reader.join();
    }
    let status = status?;
    let bytes = output.lock().unwrap_or_else(|error| error.into_inner());
    Ok(Finished {
        code: status.code(),
        output: tail(&bytes),
    })
}

/// The last few printable lines, for a failure message a person can act on.
fn tail(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let lines: Vec<String> = text
        .lines()
        .map(|line| {
            // Drop terminal escapes and other controls from untrusted output.
            let mut clean = String::with_capacity(line.len());
            let mut escape = false;
            for c in line.chars() {
                match c {
                    '\u{1b}' => escape = true,
                    c if escape => escape = !c.is_ascii_alphabetic(),
                    c if c.is_control() => {}
                    c => clean.push(c),
                }
            }
            clean.trim().to_owned()
        })
        .filter(|line| !line.is_empty())
        .collect();
    lines[lines.len().saturating_sub(TAIL_LINES)..].join("\n")
}

/// Whether the workspace already has a Herdr the bridge can run.
/// `ssh` is a `coder ssh … <workspace>` command; the probe is appended.
pub(crate) fn installed(mut ssh: Command, cancelled: &impl Fn() -> bool) -> Result<bool> {
    ssh.arg(remote(PROBE));
    let finished = run(ssh, PROBE_TIMEOUT, cancelled)?;
    match finished.code {
        Some(0) => Ok(true),
        Some(3) => Ok(false),
        _ => Err(Error::Install(if finished.output.is_empty() {
            "coder ssh could not reach the workspace".into()
        } else {
            finished.output
        })),
    }
}

/// Run Herdr's installer in the workspace. Only called after the user approved it.
pub(crate) fn install(mut ssh: Command, cancelled: &impl Fn() -> bool) -> Result<()> {
    tracing::info!(
        category = "coder_install",
        "Installing Herdr in Coder workspace"
    );
    ssh.arg(remote(INSTALL));
    let finished = run(ssh, INSTALL_TIMEOUT, cancelled)?;
    match finished.code {
        Some(0) => Ok(()),
        Some(NO_CURL) => Err(Error::Install(
            "the workspace has no curl; add it to the template or install Herdr there manually"
                .into(),
        )),
        _ => Err(Error::Install(if finished.output.is_empty() {
            "the installer did not finish".into()
        } else {
            finished.output
        })),
    }
}

#[cfg(all(test, unix))]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    /// Stand in for `coder ssh`: run the appended remote command locally.
    fn local(home: &std::path::Path) -> Command {
        let mut command = Command::new("/bin/sh");
        command
            .arg("-c")
            .arg(r#"eval "$1""#)
            .arg("sh")
            .env_clear()
            .env("HOME", home)
            .env("PATH", "/usr/bin:/bin");
        command
    }

    #[test]
    fn the_probe_finds_herdr_where_the_bridge_looks() {
        use std::os::unix::fs::PermissionsExt;
        let home = tempfile::tempdir().unwrap();
        // The fixed roots are system-wide; this host may already have Herdr there.
        let system = [
            "/opt/homebrew/bin/herdr",
            "/usr/local/bin/herdr",
            "/home/linuxbrew/.linuxbrew/bin/herdr",
            "/nix/var/nix/profiles/default/bin/herdr",
            "/run/current-system/sw/bin/herdr",
        ]
        .iter()
        .any(|path| std::path::Path::new(path).exists());
        if !system {
            assert!(!installed(local(home.path()), &|| false).unwrap());
        }
        let bin = home.path().join(".local/bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("herdr"), "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(bin.join("herdr"), std::fs::Permissions::from_mode(0o755))
            .unwrap();
        assert!(installed(local(home.path()), &|| false).unwrap());
    }

    #[test]
    fn failures_report_the_output_tail_without_terminal_escapes() {
        // One stream: stdout and stderr drain on separate threads, so their
        // interleaving is not ordered.
        let mut failing = Command::new("/bin/sh");
        failing.args([
            "-c",
            r#"{ for i in 1 2 3 4 5 6 7 8; do echo "line $i"; done; printf '\033[31mboom\033[0m\n'; } >&2; exit 9; #"#,
        ]);
        let Err(Error::Install(text)) = install(failing, &|| false) else {
            panic!("expected an install failure");
        };
        assert!(text.ends_with("boom"), "{text}");
        assert!(!text.contains('\u{1b}'));
        assert_eq!(text.lines().count(), TAIL_LINES);
        assert!(!text.contains("line 1\n"));
    }

    #[test]
    fn a_missing_curl_is_named_and_cancellation_kills_the_child() {
        let mut no_curl = Command::new("/bin/sh");
        no_curl.args(["-c", "exit 4; #"]);
        let Err(Error::Install(text)) = install(no_curl, &|| false) else {
            panic!("expected an install failure");
        };
        assert!(text.contains("curl"));
        let mut slow = Command::new("/bin/sh");
        slow.args(["-c", "sleep 30; #"]);
        let started = Instant::now();
        assert!(matches!(install(slow, &|| true), Err(Error::Cancelled)));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn output_is_bounded() {
        let mut chatty = Command::new("/bin/sh");
        chatty.args(["-c", "yes | head -c 1000000; exit 1; #"]);
        let Err(Error::Install(text)) = install(chatty, &|| false) else {
            panic!("expected an install failure");
        };
        assert!(text.len() <= OUTPUT_LIMIT);
    }
}
