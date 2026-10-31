//! Starting a core when none is running: the user's own `herdr-core`, or the
//! copy bundled beside the app.

use crate::{Error, Result};
use std::{
    io::Read,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// The core's executable name; `hc` is only a link to it.
pub const PROGRAM: &str = if cfg!(windows) {
    "herdr-core.exe"
} else {
    "herdr-core"
};

/// Output of a failed start kept for diagnostics.
const MAX_STDERR: u64 = 4 << 10;

/// Runs `binary server start`, which returns once the session's core
/// listens, and waits at most `timeout`. `configure` sets the environment,
/// such as the login shell's. Blocking: never call it on the UI thread.
pub fn start(binary: &Path, configure: impl FnOnce(&mut Command), timeout: Duration) -> Result<()> {
    let mut command = herdr_client::new_command(binary);
    command
        .args(["server", "start", "--json"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    configure(&mut command);
    let start_error = |source| Error::Start {
        binary: binary.to_owned(),
        source,
    };
    let mut child = command.spawn().map_err(start_error)?;
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait().map_err(start_error)? {
            break status;
        }
        if Instant::now() >= deadline {
            // Only the start command dies here; a core it started keeps running.
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::StartTimeout(timeout));
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    if status.success() {
        return Ok(());
    }
    let mut stderr = String::new();
    if let Some(pipe) = child.stderr.take() {
        let _ = pipe.take(MAX_STDERR).read_to_string(&mut stderr);
    }
    Err(Error::StartFailed { status, stderr })
}
