//! Starting and ending the `serve-web` child and everything it starts.
//!
//! `code serve-web` runs VS Code's server as a process of its own and leaves
//! it running when it is itself signalled; Ctrl-C in a terminal stops both
//! only because the terminal signals its whole foreground group. So on Unix
//! the child starts in a new process group, made for it alone, and ending it
//! signals that group: exactly the child and what it started, never another
//! process. The child is reaped only after the last signal, so its id, which
//! names the group, cannot be reused meanwhile. On Windows `taskkill /T` ends
//! the child's own descendants, found from its id, which the open handle
//! keeps from being reused.
use std::{
    io::{self, BufRead, BufReader, Read},
    process::{Child, ChildStdout, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

/// What `serve-web` prints once it holds its port, and only then: when the
/// port is taken it prints an error and exits instead.
const LISTENING: &[u8] = b"Web UI available at ";
/// Longer lines are read in pieces; only their start matters.
const MAX_LINE: u64 = 4096;

/// Detaches the child from the app's input, and pipes its output to
/// [`listening`] alone: `serve-web` prints the address with its token, which
/// must not reach any log.
pub(super) fn isolate(command: &mut Command) {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(command, 0);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
}

/// Asks the child and what it started to stop, without waiting. Quick
/// enough to call holding a lock.
pub(super) fn terminate(child: &Child) {
    #[cfg(unix)]
    {
        let group = rustix::process::Pid::from_child(child);
        let _ = rustix::process::kill_process_group(group, rustix::process::Signal::TERM);
    }
    // Windows has no request to stop; `kill` ends the tree at once.
    #[cfg(windows)]
    let _ = child;
}

/// Ends the child and what it started at once. Blocking on Windows, where it
/// runs `taskkill`: call it without holding a lock.
pub(super) fn kill(child: &mut Child) {
    #[cfg(unix)]
    {
        let group = rustix::process::Pid::from_child(child);
        let _ = rustix::process::kill_process_group(group, rustix::process::Signal::KILL);
    }
    #[cfg(windows)]
    {
        let mut taskkill = Command::new("taskkill");
        taskkill
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            taskkill.creation_flags(CREATE_NO_WINDOW);
        }
        let _ = taskkill.status();
    }
    let _ = child.kill();
}

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

/// Whether the child has exited, leaving it unreaped on Unix.
pub(super) fn exited(child: &mut Child) -> io::Result<bool> {
    #[cfg(unix)]
    {
        use rustix::process::{Pid, WaitId, WaitIdOptions, waitid};
        let options = WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT;
        Ok(waitid(WaitId::Pid(Pid::from_child(child)), options)?.is_some())
    }
    // The open handle keeps the id from being reused, so this may reap.
    #[cfg(windows)]
    Ok(child.try_wait()?.is_some())
}
