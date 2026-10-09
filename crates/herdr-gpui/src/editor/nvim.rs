//! Neovim, reused: an editor pane that runs Neovim listens on a socket in
//! this app's private state folder, and later files for the same tab open
//! in it through `nvim --server … --remote-expr`, as herdr-nvim's sidebar
//! does, instead of in another split. The socket is passed to the editor as
//! an argument, never typed as text a shell would read, and the request
//! travels as argv, so no path is ever parsed by a shell here.

use super::EditorTarget;
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

/// Unix socket paths are short: 104 bytes on macOS, 108 on Linux.
const MAX_SOCKET_BYTES: usize = 100;
/// How long one `nvim --server` call may take before it is given up on.
const DEADLINE: Duration = Duration::from_secs(5);

/// A fresh socket path for one editor pane, unique to this process. The
/// folder is made, private, by the line that starts the editor. `None` when
/// no state folder is known or the path would not fit a socket.
pub(crate) fn new_socket() -> Option<PathBuf> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = crate::preferences::state_dir()?.join("nvim");
    let name = format!(
        "{}-{}.sock",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let socket = dir.join(name);
    socket
        .to_str()
        .is_some_and(|text| text.len() <= MAX_SOCKET_BYTES && super::quotable(text))
        .then_some(socket)
}

/// Whether the first word of an editor command names Neovim.
pub(crate) fn is_nvim(command: &str) -> bool {
    command
        .split_whitespace()
        .next()
        .and_then(|program| program.rsplit('/').next())
        .is_some_and(|name| name.starts_with("nvim"))
}

/// Opens `target` in the Neovim listening on `socket`, at its line, and
/// centers it. Blocking: it runs on the background executor.
///
/// Only [`crate::Error::EditorRemote`] means that Neovim has gone. One that
/// still listens but is slow, such as one waiting on a swap-file prompt,
/// may yet run the request, so its pane is kept rather than replaced by a
/// second editor on the same file.
pub(crate) fn open(socket: &Path, target: &EditorTarget) -> crate::Result<()> {
    let request = request(target).ok_or(crate::Error::EditorPath)?;
    // Without a server `--server` edits locally and never exits, so make
    // sure one is listening first.
    if !listening(socket) {
        return Err(crate::Error::EditorRemote);
    }
    match run(nvim(socket).arg("--remote-expr").arg(request)) {
        Ok(()) => Ok(()),
        Err(_) if !listening(socket) => Err(crate::Error::EditorRemote),
        Err(error) => Err(error),
    }
}

/// One expression that opens `target` and moves to its line. Neovim runs a
/// server's requests one at a time, so two quick opens cannot interleave
/// and move each other's cursor, as separate open and jump requests could.
/// `drop` goes to a window already showing the file, and splits rather
/// than abandon unsaved changes. `None` for a path a Vim string cannot hold
/// on one line.
pub(super) fn request(target: &EditorTarget) -> Option<String> {
    let path = target
        .path
        .to_str()
        .filter(|path| !path.chars().any(char::is_control))?;
    // In a single-quoted Vim string only the quote itself is special.
    let path = path.replace('\'', "''");
    let mut commands = vec![format!("'drop ' .. fnameescape('{path}')")];
    if let Some(line) = target.line {
        commands.push(format!("'call cursor({line}, 1)'"));
        commands.push("'normal! zz'".to_owned());
    }
    Some(format!("execute([{}])", commands.join(", ")))
}

#[cfg(unix)]
fn listening(socket: &Path) -> bool {
    std::os::unix::net::UnixStream::connect(socket).is_ok()
}

/// Windows builds never start an editor (see `SUPPORTED`).
#[cfg(not(unix))]
fn listening(_: &Path) -> bool {
    false
}

fn nvim(socket: &Path) -> Command {
    let mut command = Command::new("nvim");
    command
        .env("PATH", crate::local_path::local_path())
        .args(["--headless", "--clean", "--server"])
        .arg(socket)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

/// Runs `command` to completion within [`DEADLINE`], killing it past that.
fn run(command: &mut Command) -> crate::Result<()> {
    let mut child = command.spawn().map_err(crate::Error::EditorRemoteLaunch)?;
    let deadline = Instant::now() + DEADLINE;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(_)) | Err(_) => return Err(crate::Error::EditorRemoteFailed),
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(crate::Error::EditorRemoteBusy);
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
        }
    }
}
