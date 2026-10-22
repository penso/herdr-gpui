use super::*;
use rustix::{
    io::Errno,
    process::{Pid, Signal, WaitOptions, kill_process, test_kill_process, waitpid},
};

struct ReaderProbe {
    directory: tempfile::TempDir,
    pid: Option<Pid>,
}

impl ReaderProbe {
    fn new() -> io::Result<Self> {
        Ok(Self {
            directory: tempfile::tempdir()?,
            pid: None,
        })
    }

    fn command(&self, text: &str) -> Command {
        // Publish our exact PID before producing output, and launch no descendants.
        let mut command = shell(
            r#"printf '%s' "$$" > "$1" || exit 1
printf '\001%s' "$2"
while :; do :; done"#,
        );
        command
            .arg("clipboard-reader-fixture")
            .arg(self.directory.path().join("pid"))
            .arg(text);
        command
    }

    fn pid(&mut self) -> io::Result<Pid> {
        if let Some(pid) = self.pid {
            return Ok(pid);
        }
        // Even with broken cleanup, a child scheduled after the acquisition
        // deadline must have time to publish its PID for the fallback below.
        let deadline = Instant::now() + TIMEOUT;
        loop {
            if let Ok(text) = std::fs::read_to_string(self.directory.path().join("pid"))
                && let Ok(raw) = text.parse::<i32>()
                && raw > 0
                && let Some(pid) = Pid::from_raw(raw)
            {
                self.pid = Some(pid);
                return Ok(pid);
            }
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "clipboard fixture did not publish its PID",
                ));
            }
            thread::sleep(Duration::from_millis(5));
        }
    }

    fn assert_gone_and_reaped(&mut self) -> io::Result<()> {
        let pid = self.pid()?;
        let existence = test_kill_process(pid);
        let status = waitpid(Some(pid), WaitOptions::NOHANG);
        // A successful wait here would mean acquire left a zombie for us to reap.
        assert!(
            matches!(status, Err(Errno::CHILD)),
            "reader {pid} was not reaped: {status:?}"
        );
        assert_eq!(existence, Err(Errno::SRCH), "reader {pid} still exists");
        Ok(())
    }
}

impl Drop for ReaderProbe {
    fn drop(&mut self) {
        let Ok(pid) = self.pid() else { return };
        loop {
            match waitpid(Some(pid), WaitOptions::NOHANG) {
                Err(Errno::INTR) => continue,
                Ok(None) => {
                    // Still our unreaped child: its PID cannot have been reused.
                    // Clean it up even when an assertion above unwinds.
                    let _ = kill_process(pid, Signal::KILL);
                    while matches!(waitpid(Some(pid), WaitOptions::empty()), Err(Errno::INTR)) {}
                }
                _ => {}
            }
            break;
        }
    }
}

#[test]
fn stalled_reader_is_killed_and_reaped() -> anyhow::Result<()> {
    let mut probe = ReaderProbe::new()?;
    let started = Instant::now();
    // Leave a loaded machine time to start the shell and publish its PID
    // before the deadline kills it, while staying well inside TIMEOUT.
    let result = acquire(
        &mut probe.command(""),
        started + Duration::from_secs(1),
        100,
    );
    probe.assert_gone_and_reaped()?;
    assert!(matches!(
        result,
        Err(crate::Error::Clipboard(ClipboardError::Timeout))
    ));
    assert!(started.elapsed() < TIMEOUT);
    assert!(acquire(&mut shell(r"printf '\000'"), Instant::now() + TIMEOUT, 100).is_ok());
    Ok(())
}

#[test]
fn oversized_reader_is_killed_and_reaped() -> anyhow::Result<()> {
    let mut probe = ReaderProbe::new()?;
    let result = acquire(&mut probe.command("too large"), Instant::now() + TIMEOUT, 4);
    probe.assert_gone_and_reaped()?;
    assert!(matches!(
        result,
        Err(crate::Error::Clipboard(ClipboardError::Size { limit: 4 }))
    ));
    assert!(acquire(&mut shell(r"printf '\000'"), Instant::now() + TIMEOUT, 100).is_ok());
    Ok(())
}
