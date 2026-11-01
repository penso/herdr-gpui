//! The `serve-web` readiness marker; owned process cleanup is shared with px0.
pub(super) use crate::owned_process::{exited, isolate, kill, terminate};
use std::{
    io::{self, BufRead, BufReader, Read},
    process::ChildStdout,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

/// What `serve-web` prints once it holds its port, and only then: when the
/// port is taken it prints an error and exits instead.
const LISTENING: &[u8] = b"Web UI available at "; // i18n: data
/// Longer lines are read in pieces; only their start matters.
const MAX_LINE: u64 = 4096;

/// Reads the child's output on a thread of its own, discarding it, and
/// returns a flag that is set once the child says it listens. Until then the
/// port may be held by another program, which must never be sent the token;
/// once the child bound it, no other program can hold it while it runs.
pub(super) fn listening(stdout: ChildStdout) -> io::Result<Arc<AtomicBool>> {
    let flag = Arc::new(AtomicBool::new(false));
    let set = flag.clone();
    thread::Builder::new()
        .name("herdr-vscode-output".into())
        .spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut line = Vec::new();
            loop {
                line.clear();
                match reader.by_ref().take(MAX_LINE).read_until(b'\n', &mut line) {
                    Ok(0) | Err(_) => return,
                    Ok(_) => {
                        if line.trim_ascii_start().starts_with(LISTENING) {
                            set.store(true, Ordering::Release);
                        }
                    }
                }
            }
        })?;
    Ok(flag)
}
