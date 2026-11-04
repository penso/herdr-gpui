//! Isolation and cleanup for HTTP editor servers owned by this app. Keep the
//! root unreaped until the final signal so its process-group ID cannot be reused.
use std::{
    io,
    process::{Child, Command, Stdio},
};

pub(crate) fn isolate(command: &mut Command) {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(command, 0);
}

pub(crate) fn terminate(child: &Child) {
    #[cfg(unix)]
    {
        let group = rustix::process::Pid::from_child(child);
        let _ = rustix::process::kill_process_group(group, rustix::process::Signal::TERM);
    }
    #[cfg(windows)]
    let _ = child;
}

/// Blocking on Windows: invoke on a worker, never under a UI-thread lock.
pub(crate) fn kill(child: &mut Child) {
    #[cfg(unix)]
    {
        let group = rustix::process::Pid::from_child(child);
        let _ = rustix::process::kill_process_group(group, rustix::process::Signal::KILL);
    }
    #[cfg(windows)]
    {
        // The open child handle keeps its PID from being reused.
        let _ = sheprd_client::new_command("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
}

pub(crate) fn exited(child: &mut Child) -> io::Result<bool> {
    #[cfg(unix)]
    {
        use rustix::process::{Pid, WaitId, WaitIdOptions, waitid};
        let options = WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT;
        Ok(waitid(WaitId::Pid(Pid::from_child(child)), options)?.is_some())
    }
    #[cfg(windows)]
    Ok(child.try_wait()?.is_some())
}
