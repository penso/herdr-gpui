use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};

const COMPATIBLE: &str = r#"{"endpoint_protocol_generation":1,"endpoint_capabilities":["surface_interest","presentation_effects_fence","health_check"],"remote_bridge_idle_timeout":true,"remote_host_bridge":true}"#;

/// The script a `powershell.exe -EncodedCommand` line runs.
fn decoded(command: &str) -> String {
    let encoded = command
        .strip_prefix("powershell.exe -NoLogo -NoProfile -NonInteractive -EncodedCommand ")
        .unwrap();
    let bytes = STANDARD.decode(encoded).unwrap();
    let utf16 = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect::<Vec<_>>();
    String::from_utf16(&utf16).unwrap()
}

/// One probe block as a Windows host prints it, with CRLF line endings.
fn block(path: &str, client: &str, server: &str) -> String {
    format!(
        "herdr-probe:candidate\r\nherdr-windows-path:{}\r\n{client}\r\n{server}\r\n",
        STANDARD.encode(path)
    )
}

#[test]
fn only_a_remote_shell_failure_retries_through_powershell() {
    // `cmd.exe` and PowerShell report a missing `/bin/sh` with their own codes.
    for code in [1, 9009 & 0xff] {
        assert!(powershell::rejected_posix(Some(code)), "{code}");
    }
    // `ssh` itself failed, no Herdr was found, or nothing exited.
    for code in [Some(0), Some(127), Some(255), None] {
        assert!(!powershell::rejected_posix(code), "{code:?}");
    }
}

#[cfg(unix)]
#[test]
fn a_shell_that_cannot_run_the_bridge_reports_its_exit_code() {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", "printf '%s\\n' 'not recognized' >&2; exit 1"]);
    let closed = start_bridge(command, &AtomicBool::new(false), handshake)
        .unwrap()
        .err()
        .unwrap();
    assert_eq!(closed.code, Some(1));
    assert_eq!(closed.failure, SshFailure::Other);
    assert!(powershell::rejected_posix(closed.code));
}

#[test]
fn windows_probe_blocks_name_each_candidate_and_survive_crlf() {
    let path = r"C:\Users\O'Brien\测试\.herdr\packages\standalone\releases\0.9.0\herdr.exe";
    let output = format!(
        "profile noise\r\n{}{}herdr-probe:candidate\r\nherdr-windows-path:not base64!\r\n{COMPATIBLE}\r\n\r\nherdr-probe:done\r\n",
        block(
            r"C:\old\herdr.exe",
            r#"{"endpoint_protocol_generation":0}"#,
            ""
        ),
        block(path, COMPATIBLE, r#"{"running":true}"#),
    );
    let candidates = powershell::candidates(output.as_bytes());
    let paths = candidates
        .iter()
        .map(|c| c.path.as_str())
        .collect::<Vec<_>>();
    // The block whose path does not decode cannot be bridged and is dropped.
    assert_eq!(paths, [r"C:\old\herdr.exe", path]);
    assert_eq!(powershell::compatible(&candidates[0].status), None);
    assert_eq!(powershell::compatible(&candidates[1].status), Some(true));
    assert_eq!(
        classify_probe_with(output.as_bytes(), powershell::compatible),
        Some(HostProbe::Running)
    );
}

#[test]
fn windows_hosts_also_need_the_remote_host_bridge() {
    let without = COMPATIBLE.replace(r#","remote_host_bridge":true"#, "");
    let status = serde_json::from_str(&without).unwrap();
    assert_eq!(compatible(&status), Some(true));
    assert_eq!(powershell::compatible(&status), None);
    let output = format!("{}herdr-probe:done\r\n", block(r"C:\h.exe", &without, ""));
    assert_eq!(
        classify_probe_with(output.as_bytes(), powershell::compatible),
        Some(HostProbe::Outdated)
    );
    assert_eq!(
        classify_probe_with(b"herdr-probe:done\r\n", powershell::compatible),
        Some(HostProbe::Missing)
    );
    // Output that never finished is not a host without Herdr.
    assert_eq!(
        classify_probe_with(
            block(r"C:\h.exe", COMPATIBLE, "").as_bytes(),
            powershell::compatible
        ),
        None
    );
}

#[test]
fn powershell_ends_its_ready_banner_with_crlf() {
    let mut stream = scripted(bytes(b"\r\nherdr-remote-output-ready:1\r\n"));
    let output = await_ready(&mut stream, &AtomicBool::new(false), Instant::now()).unwrap();
    assert_eq!(output, b"\r\n");
    let mut stray = scripted(bytes(b"herdr-remote-output-ready:1\r\r\n"));
    assert!(matches!(
        await_ready(&mut stray, &AtomicBool::new(false), Instant::now()),
        Err(crate::Error::Ssh(Error::Closed))
    ));
}

#[test]
fn the_windows_bridge_quotes_its_path_and_arguments() {
    let script = decoded(&powershell::bridge_command(
        r"C:\Program Files\O'Brien\herdr.exe",
        "work",
        true,
    ));
    assert!(script.starts_with(
        "[Console]::Out.WriteLine(); [Console]::Out.WriteLine('herdr-remote-output-ready:1'); [Console]::Out.Flush(); "
    ));
    assert!(script.contains(
        r"Start-Process -FilePath 'C:\Program Files\O''Brien\herdr.exe' -ArgumentList '--session work remote-client-bridge --idle-timeout-v1' -NoNewWindow -PassThru"
    ));
    assert!(script.ends_with("exit $process.ExitCode"));
    let plain = decoded(&powershell::bridge_command(r"C:\h.exe", "default", false));
    assert!(plain.contains("-ArgumentList '--session default remote-client-bridge' "));
}

#[test]
fn the_windows_probe_asks_each_candidate_for_the_session() {
    let script = decoded(&powershell::probe_command("work"));
    assert!(script.contains("& $herdrPath --session work status server --json"));
    assert!(script.contains("'herdr-probe:candidate'"));
    assert!(script.ends_with("[Console]::Out.WriteLine('herdr-probe:done'); exit 0"));
    // cmd.exe caps a command line at 8191 characters.
    assert!(powershell::probe_command("work").len() < 8191);
}
