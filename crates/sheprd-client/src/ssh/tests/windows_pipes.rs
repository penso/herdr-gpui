//! The bridge over a child's anonymous pipes, as `ssh` runs it on Windows.
//! PowerShell stands in for `ssh` and the remote script: every Windows
//! machine has it, and it can read a line or write half of one.
use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};

const COMPATIBLE: &str = r#"{"endpoint_protocol_generation":1,"endpoint_capabilities":["surface_interest","presentation_effects_fence","health_check"],"remote_bridge_idle_timeout":true}"#;

/// A bound for waits that only a broken test would reach; PowerShell can
/// take a few seconds to start on a busy machine, and has taken 20 on a
/// hosted runner running the suite in parallel.
const STARTUP: Duration = Duration::from_secs(30);

/// `powershell.exe` running `script`, encoded so that no quoting applies.
fn powershell(script: &str) -> Command {
    let utf16: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut command = crate::new_command("powershell.exe");
    command.args([
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-EncodedCommand",
        &STANDARD.encode(utf16),
    ]);
    command
}

/// Reads exactly `expected.len()` bytes, retrying the emulated timeouts an
/// idle pipe reports, and returns them.
fn read_expected(stream: &mut Stream, expected: &[u8]) -> Vec<u8> {
    let deadline = Instant::now() + STARTUP;
    let mut read = vec![0; expected.len()];
    let mut filled = 0;
    while filled < read.len() {
        assert!(Instant::now() < deadline, "timed out after {filled} bytes");
        match stream.read(&mut read[filled..]) {
            Ok(0) => panic!("closed after {filled} bytes"),
            Ok(n) => filled += n,
            Err(error) if error.kind() == io::ErrorKind::TimedOut => {}
            Err(error) => panic!("{error}"),
        }
    }
    read
}

#[test]
fn the_handshake_and_the_bridged_stream_run_over_the_childs_pipes() {
    let script = format!(
        r#"$out = [Console]::Out
$out.Write('motd' + "`n" + '{COMPATIBLE}' + "`n`n" + 'herdr-remote-output-ready:1' + "`n")
if ([Console]::In.ReadLine() -ne 'accept-idle') {{ exit 3 }}
$hello = [Console]::In.ReadLine()
$out.Write($hello + "`n")"#
    );
    let stop = AtomicBool::new(false);
    let Ok((mut stream, _child)) = start_bridge(powershell(&script), &stop, |stream, stop| {
        // Starting PowerShell is the test's wait, not the banner's: it can
        // outlast the banner deadline. Once the first byte arrives, the
        // banner's own deadline, not the test's, bounds the rest.
        read_expected(stream, b"m");
        handshake(stream, stop)
    })
    .unwrap() else {
        panic!("the bridge closed before it was ready");
    };
    stream.write_all(b"hello\n").unwrap();
    assert_eq!(read_expected(&mut stream, b"hello\n"), b"hello\n");
    // Only the child held the pipes' other ends, so its exit reads as the end
    // of the stream rather than as one more idle timeout.
    let deadline = Instant::now() + STARTUP;
    let mut byte = [0];
    loop {
        assert!(Instant::now() < deadline, "the exit never read as closed");
        match stream.read(&mut byte) {
            Ok(0) => break,
            Ok(_) => panic!("unexpected output"),
            Err(error) if error.kind() == io::ErrorKind::TimedOut => {}
            Err(error) => panic!("{error}"),
        }
    }
}

#[test]
fn an_ssh_failure_before_the_banner_is_classified_from_its_stderr() {
    let mut command = crate::new_command("cmd.exe");
    command.args([
        "/d",
        "/c",
        "echo Host key verification failed.1>&2& exit 255",
    ]);
    let started = Instant::now();
    let Err(closed) = start_bridge(command, &AtomicBool::new(false), handshake).unwrap() else {
        panic!("a failed ssh was accepted");
    };
    assert_eq!(closed.code, Some(255));
    assert_eq!(closed.failure, SshFailure::HostKey);
    // Reported when the pipe closed, well before the banner's deadline.
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn an_idle_pipe_times_out_rather_than_closing_and_cancellation_ends_the_wait() {
    // Half a ready line, then nothing until stdin closes.
    let (mut stream, child) = spawn_bridge(powershell(
        "[Console]::Out.Write('herdr-remote'); [void][Console]::In.ReadLine()",
    ))
    .unwrap();
    stream.set_read_timeout(Some(POLL)).unwrap();
    assert_eq!(read_expected(&mut stream, b"herdr-remote"), b"herdr-remote");
    let mut byte = [0];
    let error = stream.read(&mut byte).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    // The banner never finishes; cancelling returns without the deadline.
    let stop = AtomicBool::new(true);
    assert!(matches!(
        await_ready(&mut stream, &stop, Instant::now()),
        Err(crate::Error::Ssh(Error::Cancelled))
    ));
    drop(child);
}

#[test]
fn a_banner_without_a_ready_line_is_bounded() {
    let (mut stream, _child) = spawn_bridge(powershell(
        "[Console]::Out.Write('x' * 20000); [void][Console]::In.ReadLine()",
    ))
    .unwrap();
    stream.set_read_timeout(Some(POLL)).unwrap();
    // Start the banner's clock once PowerShell runs, as in the handshake
    // test; the 19,999 bytes left still exceed the limit.
    read_expected(&mut stream, b"x");
    assert!(matches!(
        await_ready(&mut stream, &AtomicBool::new(false), Instant::now()),
        Err(crate::Error::Ssh(Error::OutputLimit))
    ));
}
