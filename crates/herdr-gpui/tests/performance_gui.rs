//! Explicit native opt-in: unlike GPUI unit tests, this launches a real AppKit app.
#![cfg(target_os = "macos")]
use std::{
    io::Read,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[test]
#[ignore = "opens native fixture window; run explicitly on a macOS desktop"]
fn dense_terminal_sidebar_performance() -> Result<(), Box<dyn std::error::Error>> {
    for retained in [false, true] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_herdr-gpui"));
        command
            .arg("--performance-test")
            .env_remove("HERDR_PERF_UNCACHED")
            .env_remove("HERDR_PERF_RETAINED")
            .env_remove("HERDR_PERF_NO_RETAIN")
            .env_remove("HERDR_PERF_NO_BATCH")
            .env_remove("HERDR_PERF_SAMPLES")
            .env_remove("HERDR_PERF_P95_MS")
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        if retained {
            command.env("HERDR_PERF_RETAINED", "1");
        }
        let mut child = command.spawn()?;
        let mut stdout = child.stdout.take().ok_or("missing fixture stdout")?;
        let output = std::thread::spawn(move || {
            let mut output = String::new();
            stdout.read_to_string(&mut output).map(|_| output)
        });
        let start = Instant::now();
        loop {
            if let Some(status) = child.try_wait()? {
                assert!(
                    status.success(),
                    "native performance fixture failed (retained={retained}): {status}"
                );
                let output = output.join().map_err(|_| "stdout reader panicked")??;
                assert!(
                    output
                        .lines()
                        .any(|line| line == format!("PERF PASS retained={retained}")),
                    "native fixture exited without success marker: {output}"
                );
                break;
            }
            if start.elapsed() > Duration::from_secs(180) {
                child.kill()?;
                child.wait()?;
                let _ = output.join();
                return Err(
                    format!("native performance fixture timed out (retained={retained})").into(),
                );
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    Ok(())
}
