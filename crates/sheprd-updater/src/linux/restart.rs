//! A package restart must release the old app's process-local services before
//! the replacement binds them. A pidfd pins the old process across PID reuse;
//! READY is sent only after that handle exists, and COMMIT authorizes relaunch.
use super::*;
use rustix::{
    event::{PollFd, PollFlags, Timespec, poll},
    process::{Pid, PidfdFlags, pidfd_open},
};
use std::{
    ffi::OsString,
    io::{self, Write},
    os::fd::AsFd,
    process::{ChildStdin, ExitCode},
    time::Instant,
};

const FLAG: &str = "--herdr-relaunch-package";
const WAIT: Duration = Duration::from_secs(120);
const COMMIT: &[u8; 7] = b"COMMIT\n";

/// UI-safe: committing writes only seven bytes into a fresh pipe. Dropping an
/// uncommitted guard closes it, so the helper exits without launching anything.
pub(crate) struct RestartGuard {
    input: ChildStdin,
    committed: bool,
}

impl RestartGuard {
    pub(crate) fn commit(&mut self) -> Result<()> {
        if !self.committed {
            self.input.write_all(COMMIT)?;
            self.committed = true;
        }
        Ok(())
    }
}

pub(crate) fn relaunch(executable: &Path) -> Result<RestartGuard> {
    let mut child = sheprd_client::new_command(executable)
        .arg(FLAG)
        .arg(std::process::id().to_string())
        .args(env::args_os().skip(1))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let (Some(input), Some(mut output)) = (child.stdin.take(), child.stdout.take()) else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(Error::MissingControlPipe);
    };
    let (sender, receiver) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let mut ready = [0; 6];
        let result = output.read_exact(&mut ready).map(|()| ready);
        let _ = sender.send(result);
    });
    let ready = match receiver.recv_timeout(WAIT) {
        Ok(Ok(ready)) if ready == *b"READY\n" => Ok(()),
        Ok(Err(error)) => Err(Error::Io(error)),
        Err(mpsc::RecvTimeoutError::Timeout) => Err(Error::HelperTimeout),
        _ => Err(Error::HelperNotReady),
    };
    if let Err(error) = ready {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    // The helper itself bounds the committed wait. An uncommitted one exits
    // on EOF; neither the UI nor the updater worker waits for process teardown.
    thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(RestartGuard {
        input,
        committed: false,
    })
}

fn wait_for_exit(process: &impl AsFd, timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let timeout = Timespec::try_from(remaining).map_err(|_| Error::HelperTimeout)?;
        let mut fds = [PollFd::new(process, PollFlags::IN)];
        match poll(&mut fds, Some(&timeout)) {
            Ok(0) => return Err(Error::HelperTimeout),
            Ok(_) if fds[0].revents().contains(PollFlags::IN) => return Ok(()),
            Ok(_) => return Err(Error::HelperNotReady),
            Err(rustix::io::Errno::INTR) if Instant::now() < deadline => (),
            Err(error) => return Err(Error::Io(error.into())),
        }
    }
}

fn after_commit(
    mut input: impl Read,
    parent: &impl AsFd,
    launch: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let mut instruction = [0; 7];
    match input.read_exact(&mut instruction) {
        Ok(()) if instruction == *COMMIT => (),
        Ok(()) => return Err(Error::NotCommitted),
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
            return Err(Error::NotCommitted);
        }
        Err(error) => return Err(Error::Io(error)),
    }
    wait_for_exit(parent, WAIT)?;
    launch()
}

fn helper(args: &[OsString]) -> Result<()> {
    if rustix::process::geteuid().is_root() {
        return Err(Error::RootUser);
    }
    let [pid, args @ ..] = args else {
        return Err(Error::MalformedRequest);
    };
    let pid = pid
        .to_str()
        .and_then(|pid| pid.parse().ok())
        .and_then(Pid::from_raw)
        .ok_or(Error::MalformedRequest)?;
    if rustix::process::getppid() != Some(pid) {
        return Err(Error::MalformedRequest);
    }
    let parent = pidfd_open(pid, PidfdFlags::empty()).map_err(io::Error::from)?;
    if rustix::process::getppid() != Some(pid) {
        return Err(Error::MalformedRequest);
    }
    let executable = env::current_exe()?;
    let mut output = io::stdout();
    output.write_all(b"READY\n")?;
    output.flush()?;
    after_commit(io::stdin(), &parent, || {
        sheprd_client::new_command(executable)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        Ok(())
    })
}

pub(super) fn run_helper(args: &[OsString]) -> Option<ExitCode> {
    if args.first().is_none_or(|arg| arg != FLAG) {
        return None;
    }
    Some(match helper(&args[1..]) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = writeln!(io::stderr(), "{error}");
            ExitCode::FAILURE
        }
    })
}

#[cfg(test)]
mod tests;
