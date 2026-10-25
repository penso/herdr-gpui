use super::*;
use std::{ffi::OsStr, path::Path};

/// The words after the session helper, which the helper `exec`s.
fn after_session(command: &Command) -> Vec<&OsStr> {
    let args: Vec<&OsStr> = command.get_args().collect();
    assert_eq!(args[0], crate::setup_relay::SESSION);
    args[1..].to_vec()
}

/// The setup runs as this app's child, through the session helper, and every
/// argument stays its own word: no shell sees the target or the label.
#[test]
fn the_setup_runs_machine_add_as_the_apps_child() -> Result<()> {
    let request = Request::new("user@host", "Alice's $(printf INJECTED); device", "work")?;
    let command = setup_command(
        OsStr::new("/Applications/Herdr.app/Contents/MacOS/Herdr"),
        OsStr::new("/a path/herdr"),
        &request,
        false,
    );
    assert_eq!(
        command.get_program(),
        "/Applications/Herdr.app/Contents/MacOS/Herdr"
    );
    let expected: Vec<&OsStr> = std::iter::once("/a path/herdr")
        .chain(request.arguments())
        .map(OsStr::new)
        .collect();
    assert_eq!(after_session(&command), expected);
    Ok(())
}

/// The user's own `ssh-copy-id`, and through it their own `ssh`, come from
/// the login `PATH`; nothing puts the system's first. The save only runs
/// once the key is installed, with its arguments untouched.
#[test]
fn key_setup_installs_the_key_with_the_users_ssh_then_saves() -> Result<()> {
    let root = tempfile::tempdir()?;
    let log = root.path().join("log");
    let fake = |name: &str, script: &str| -> Result<()> {
        crate::test_executable::write(root.path().join(name), script, 0o755)?;
        Ok(())
    };
    // Each fake records its arguments, one per line, then exits as told.
    let record = "for a in \"$@\"; do printf '%s\\n' \"$a\"; done >> \"$LOG\"";
    fake(
        "ssh-copy-id",
        &format!("#!/bin/sh\nprintf 'copy-id\\n' >> \"$LOG\"\n{record}\nexit \"$COPY_ID_EXIT\"\n"),
    )?;
    fake(
        "herdr",
        &format!("#!/bin/sh\nprintf 'herdr\\n' >> \"$LOG\"\n{record}\n"),
    )?;
    let request = Request::new("penso@m5mini.local", "Mini 'n $HOME", "")?;
    let herdr = root.path().join("herdr");
    let command = setup_command(OsStr::new("herdr-gpui"), herdr.as_os_str(), &request, true);
    let words = after_session(&command);
    assert!(
        !words
            .iter()
            .any(|word| word.to_string_lossy().contains("/usr/bin:"))
    );
    assert!(command.get_envs().all(|(key, _)| key != "PATH"));
    let run = |copy_id_exit: &str| -> Result<String> {
        let _ = std::fs::remove_file(&log);
        let status = Command::new(words[0])
            .args(&words[1..])
            .env("PATH", root.path())
            .env("LOG", &log)
            .env("COPY_ID_EXIT", copy_id_exit)
            .status()?;
        assert_eq!(status.success(), copy_id_exit == "0");
        Ok(std::fs::read_to_string(&log)?)
    };
    let mut expected = String::from("copy-id\npenso@m5mini.local\nherdr\n");
    for argument in request.arguments() {
        expected.push_str(argument);
        expected.push('\n');
    }
    assert_eq!(run("0")?, expected);
    // A key that was not installed saves nothing.
    assert_eq!(run("1")?, "copy-id\npenso@m5mini.local\n");
    Ok(())
}

/// The typed line holds only the two paths, so a long login `PATH` can never
/// push it past what a shell in canonical mode accepts (issue #288).
#[test]
fn the_typed_line_only_names_the_relay() -> Result<()> {
    let line = relay_line(
        Path::new("/Applications/Her'dr.app/Contents/MacOS/Herdr"),
        Path::new("/Users/me/.local/state/herdr/gpui/setup-0123456789abcdef.sock"),
    )?;
    assert_eq!(
        line,
        "'/Applications/Her'\\''dr.app/Contents/MacOS/Herdr' '__device-setup-attach' '/Users/me/.local/state/herdr/gpui/setup-0123456789abcdef.sock'"
    );
    assert!(line.len() < 1024);
    Ok(())
}

/// A setup that fails, as SSH refused by macOS does, used to leave its host
/// claimed for 15 minutes with nothing on screen, so adding it again said it
/// was "already being added". The relay now holds the claim and releases it
/// the moment the setup ends.
#[test]
fn a_failed_setup_releases_its_host_for_another_try() -> Result<()> {
    use crate::setup_relay::listen;
    use std::io::{Read, Write};
    let root = tempfile::tempdir()?;
    let claim = Claim::acquire(destination("failed-setup.test"), "default")?;
    let mut relay: Relay = listen(
        &root.path().join("gpui"),
        Command::new("/usr/bin/false"),
        Duration::from_secs(30),
    )?;
    relay.keep(claim);
    // The dialog is gone, but the setup still runs: the host stays claimed.
    assert!(matches!(
        Claim::acquire(destination("failed-setup.test"), "default"),
        Err(crate::Error::Devices(Error::Adding))
    ));
    let mut pane = std::os::unix::net::UnixStream::connect(relay.path())?;
    drop(relay);
    // Hello: 24 rows, 80 columns, no TERM.
    pane.write_all(&[1, 0, 0, 0, 4, 0, 24, 0, 80])?;
    // Read to the end of the session, which follows the setup's exit.
    pane.read_to_end(&mut Vec::new())?;
    let deadline = Instant::now() + Duration::from_secs(5);
    let retry = loop {
        match Claim::acquire(destination("failed-setup.test"), "default") {
            Err(crate::Error::Devices(Error::Adding)) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            result => break result,
        }
    };
    assert!(retry.is_ok());
    Ok(())
}

/// A relay nobody keeps, as when the workspace could not be created, takes
/// no claim: the dialog keeps it for a retry.
#[test]
fn an_unkept_relay_never_holds_the_host() -> Result<()> {
    let root = tempfile::tempdir()?;
    let relay: Relay = crate::setup_relay::listen(
        &root.path().join("gpui"),
        Command::new("/usr/bin/true"),
        Duration::from_secs(30),
    )?;
    let claim = Claim::acquire(destination("unkept.test"), "default")?;
    drop(relay);
    drop(claim);
    assert!(Claim::acquire(destination("unkept.test"), "default").is_ok());
    Ok(())
}
