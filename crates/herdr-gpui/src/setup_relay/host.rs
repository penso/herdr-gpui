//! The app's end: a single-use socket for the pane's relay, a terminal this
//! app owns, and the setup running on it as this app's child. One worker
//! thread per setup accepts, spawns, and waits; two more copy bytes, each
//! blocking on its own descriptor with one frame of buffer, so a stalled
//! side stalls the other instead of growing a queue.
use super::Error;
use super::frame::{Frame, MAX_PAYLOAD, Size};
use crate::Result;
use rustix::{
    fs::{Mode, OFlags},
    process::{Pid, Signal},
    pty::OpenptFlags,
    termios::Winsize,
};
use std::{
    fs::File,
    io::{self, Read, Write},
    os::unix::{
        fs::{DirBuilderExt, MetadataExt, PermissionsExt},
        net::{UnixListener, UnixStream},
        process::ExitStatusExt,
    },
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};

/// How long the socket waits for the pane's relay to connect. The daemon
/// creates the workspace and its shell starts first.
const CONNECT_WAIT: Duration = Duration::from_secs(120);
/// How long the connected relay may take to say its terminal size.
const HELLO_WAIT: Duration = Duration::from_secs(5);
/// How long a write to the relay may block before the session ends.
const WRITE_WAIT: Duration = Duration::from_secs(60);
/// How often the worker checks for a connection, an exit, or a lost relay.
const POLL: Duration = Duration::from_millis(50);
/// How long the last output may take to arrive after the setup exits.
const DRAIN_WAIT: Duration = Duration::from_secs(2);
/// How long a hung-up setup has to exit before it is killed.
const HANGUP_GRACE: Duration = Duration::from_secs(2);
/// The status a setup that could not start reports, as a shell does.
const NOT_STARTED: i32 = 127;
/// Used when the pane's `TERM` is missing or not a plain terminfo name.
const DEFAULT_TERM: &str = "xterm-256color";
/// The longest socket path that binds: `sun_path` holds 104 bytes on macOS
/// and 108 on Linux, including the terminating NUL.
const MAX_SOCKET_PATH: usize = 103;

/// A socket waiting for the pane's relay. Dropping it before [`keep`] closes
/// the socket, as when the workspace could not be created.
///
/// [`keep`]: Listening::keep
#[derive(Debug)]
pub(crate) struct Listening<T> {
    path: PathBuf,
    cancelled: Arc<AtomicBool>,
    /// Hands the worker what it keeps until the setup ends; `None` once kept.
    hand_over: Option<SyncSender<T>>,
}

impl<T> Listening<T> {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// The pane has its command: the socket waits on its own from here, and
    /// the setup it runs outlives this value and the dialog. `held` is kept
    /// until the setup ends, or the relay never connects, then dropped, so
    /// what it guards is released exactly when the setup is over.
    pub(crate) fn keep(&mut self, held: T) {
        if let Some(hand_over) = self.hand_over.take() {
            // A worker that already finished returns it, and it drops here.
            let _ = hand_over.try_send(held);
        }
    }
}

#[cfg(test)]
impl<T> Listening<T> {
    /// A relay that never listened, for tests of the dialog around it.
    pub(crate) fn fixture() -> Self {
        Self {
            path: PathBuf::from("/nonexistent/setup.sock"),
            cancelled: Arc::default(),
            hand_over: None,
        }
    }
}

impl<T> Drop for Listening<T> {
    fn drop(&mut self) {
        if self.hand_over.is_some() {
            self.cancelled.store(true, Ordering::Release);
        }
    }
}

/// Opens a socket in `dir`, which is made private to this user, and starts
/// the worker that runs `command` for the first relay to connect, for at
/// most `limit`. Only this user can reach the socket, and it is removed once
/// a relay connects or `CONNECT_WAIT` passes. A `dir` too long for a socket
/// path is swapped for a private one in the temporary directory. Blocks on
/// the file system: call it off the UI thread.
pub(crate) fn listen<T: Send + 'static>(
    dir: &Path,
    command: Command,
    limit: Duration,
) -> Result<Listening<T>> {
    let id = uuid::Uuid::new_v4().simple().to_string();
    let name = format!("setup-{}.sock", &id[..16]);
    let dir = socket_dir(dir, &std::env::temp_dir(), &name);
    make_private(&dir)?;
    let path = dir.join(name);
    let listener = UnixListener::bind(&path)?;
    let opened = (|| {
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)
    })();
    if let Err(error) = opened {
        let _ = std::fs::remove_file(&path);
        return Err(error.into());
    }
    let cancelled = Arc::new(AtomicBool::new(false));
    let (hand_over, held) = mpsc::sync_channel(1);
    let worker = {
        let (path, cancelled) = (path.clone(), cancelled.clone());
        thread::Builder::new()
            .name("herdr-setup-relay".into())
            .spawn(move || {
                serve(listener, &path, &cancelled, command, limit);
                // Releases what `keep` handed over, as the setup is over.
                drop(held);
            })
    };
    if let Err(error) = worker {
        let _ = std::fs::remove_file(&path);
        return Err(error.into());
    }
    Ok(Listening {
        path,
        cancelled,
        hand_over: Some(hand_over),
    })
}

/// Where a socket called `name` fits: `preferred`, else a folder of this
/// user's in `temp`, else in `/tmp`, as SSH's own control sockets do.
pub(super) fn socket_dir(preferred: &Path, temp: &Path, name: &str) -> PathBuf {
    let fits = |dir: &Path| dir.join(name).as_os_str().len() <= MAX_SOCKET_PATH;
    if fits(preferred) {
        return preferred.to_owned();
    }
    let own = format!("herdr-gpui-{}", rustix::process::getuid().as_raw());
    let temp = temp.join(&own);
    if fits(&temp) {
        temp
    } else {
        Path::new("/tmp").join(own)
    }
}

/// Creates `dir` if needed and makes it this user's alone. A shared parent
/// such as `/tmp` lets anyone create the name first, so a folder that is a
/// link or another user's is refused rather than used.
pub(super) fn make_private(dir: &Path) -> Result<()> {
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)?;
    let metadata = std::fs::symlink_metadata(dir)?;
    if !metadata.is_dir() || metadata.uid() != rustix::process::getuid().as_raw() {
        return Err(Error::Dir(dir.to_owned()).into());
    }
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}

fn serve(
    listener: UnixListener,
    path: &Path,
    cancelled: &AtomicBool,
    command: Command,
    limit: Duration,
) {
    let stream = accept(&listener, cancelled, Instant::now() + CONNECT_WAIT);
    // Single use: nothing else may attach to this setup.
    let _ = std::fs::remove_file(path);
    drop(listener);
    let Some(stream) = stream else {
        tracing::info!(category = "device_setup", "Setup relay never connected");
        return;
    };
    match run(stream, command, limit) {
        Ok(status) => tracing::info!(category = "device_setup", %status, "Setup finished"),
        Err(error) => tracing::warn!(
            category = "device_setup",
            error = %error,
            "Setup terminal failed"
        ),
    }
}

fn accept(
    listener: &UnixListener,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Option<UnixStream> {
    loop {
        match listener.accept() {
            Ok((stream, _)) => return Some(stream),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return None,
        }
        if cancelled.load(Ordering::Acquire) || Instant::now() >= deadline {
            return None;
        }
        thread::sleep(POLL);
    }
}

/// Runs the setup for one connected relay and returns how it ended.
pub(super) fn run(stream: UnixStream, command: Command, limit: Duration) -> Result<ExitStatus> {
    relay(stream, command, limit, WRITE_WAIT)
}

/// [`run`], ending the session when a write to the relay blocks for
/// `write_wait`.
pub(super) fn relay(
    stream: UnixStream,
    mut command: Command,
    limit: Duration,
    write_wait: Duration,
) -> Result<ExitStatus> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(HELLO_WAIT))?;
    let mut reader = stream.try_clone()?;
    let Some(Frame::Hello { size, term }) = Frame::read(&mut reader)? else {
        return Err(Error::Frame.into());
    };
    stream.set_read_timeout(None)?;
    // A pane that stops reading ends the session rather than holding its
    // threads forever.
    stream.set_write_timeout(Some(write_wait))?;
    let (master, terminal) = open_terminal(size)?;
    command
        .env("TERM", term.as_deref().unwrap_or(DEFAULT_TERM))
        .stdin(Stdio::from(terminal.try_clone()?))
        .stdout(Stdio::from(terminal.try_clone()?))
        .stderr(Stdio::from(terminal));
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            // Say so in the pane rather than leave it to a bare disconnect.
            let mut stream = stream;
            let message = format!("Herdr could not start the setup: {error}\r\n");
            let _ = Frame::Output(message.into_bytes()).write(&mut stream);
            let _ = Frame::Exit(NOT_STARTED).write(&mut stream);
            return Err(error.into());
        }
    };
    // The command holds this process's copies of the terminal; closing them
    // is what lets the output end once the setup exits.
    drop(command);
    let writer = Arc::new(Mutex::new(stream));
    let gone = Arc::new(AtomicBool::new(false));
    let (drained, output_done) = mpsc::sync_channel(1);
    if let Err(error) = forward(reader, master, &writer, &gone, drained) {
        // The setup is running, and the caller releases the host's claim
        // once this returns, so it must not outlive the relay.
        let _ = lock(&writer).shutdown(std::net::Shutdown::Both);
        let _ = hang_up(&mut child);
        return Err(error);
    }
    let status = wait(&mut child, &gone, Instant::now() + limit)?;
    let _ = output_done.recv_timeout(DRAIN_WAIT);
    let code = status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(0));
    let mut stream = lock(&writer);
    let _ = Frame::Exit(code).write(&mut *stream);
    // Ends the input thread's read.
    let _ = stream.shutdown(std::net::Shutdown::Both);
    Ok(status)
}

/// Starts the two threads that copy between the relay and the terminal.
/// Either sets `gone` when the relay is lost; the output one sends on
/// `drained` once the setup's output has ended.
fn forward(
    mut reader: UnixStream,
    master: File,
    writer: &Arc<Mutex<UnixStream>>,
    gone: &Arc<AtomicBool>,
    drained: SyncSender<()>,
) -> Result<()> {
    {
        let (mut master, gone) = (master.try_clone()?, gone.clone());
        thread::Builder::new()
            .name("herdr-setup-input".into())
            .spawn(move || {
                forward_input(&mut reader, &mut master);
                gone.store(true, Ordering::Release);
            })?;
    }
    let (mut master, writer, gone) = (master, writer.clone(), gone.clone());
    thread::Builder::new()
        .name("herdr-setup-output".into())
        .spawn(move || {
            forward_output(&mut master, &writer, &gone);
            let _ = drained.send(());
        })?;
    Ok(())
}

fn lock(writer: &Mutex<UnixStream>) -> std::sync::MutexGuard<'_, UnixStream> {
    writer.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A new terminal of `size`, as its controlling side and the side the setup
/// gets. Neither becomes this app's controlling terminal.
fn open_terminal(size: Size) -> Result<(File, File)> {
    let master =
        rustix::pty::openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY).map_err(io::Error::from)?;
    rustix::pty::grantpt(&master).map_err(io::Error::from)?;
    rustix::pty::unlockpt(&master).map_err(io::Error::from)?;
    let name = rustix::pty::ptsname(&master, Vec::new()).map_err(io::Error::from)?;
    let terminal = rustix::fs::open(
        name.as_c_str(),
        OFlags::RDWR | OFlags::NOCTTY | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(io::Error::from)?;
    rustix::termios::tcsetwinsize(&master, winsize(size)).map_err(io::Error::from)?;
    Ok((File::from(master), File::from(terminal)))
}

fn winsize(size: Size) -> Winsize {
    Winsize {
        ws_row: size.rows,
        ws_col: size.cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    }
}

/// Keys and sizes from the relay, until it closes or sends anything else.
fn forward_input(reader: &mut impl Read, master: &mut File) {
    while let Ok(Some(frame)) = Frame::read(reader) {
        let written = match frame {
            Frame::Input(bytes) => master.write_all(&bytes),
            Frame::Resize(size) => {
                rustix::termios::tcsetwinsize(&*master, winsize(size)).map_err(io::Error::from)
            }
            _ => return,
        };
        if written.is_err() {
            return;
        }
    }
}

/// What the setup prints, until every copy of its terminal is closed. A
/// write that fails or times out means the pane no longer sees the setup,
/// so the relay is closed and `gone` set for `wait` to hang it up.
fn forward_output(master: &mut File, writer: &Mutex<UnixStream>, gone: &AtomicBool) {
    let mut buffer = vec![0u8; MAX_PAYLOAD];
    loop {
        let read = match master.read(&mut buffer) {
            // macOS reports a terminal nobody holds any more as EIO.
            Ok(0) | Err(_) => return,
            Ok(read) => read,
        };
        let mut stream = lock(writer);
        if Frame::Output(buffer[..read].to_vec())
            .write(&mut *stream)
            .is_err()
        {
            // Also ends the input thread's read and the final exit frame.
            let _ = stream.shutdown(std::net::Shutdown::Both);
            gone.store(true, Ordering::Release);
            return;
        }
    }
}

/// Waits for the setup, hanging it up when the relay goes away or `deadline`
/// passes, as closing a terminal window would.
fn wait(child: &mut Child, gone: &AtomicBool, deadline: Instant) -> Result<ExitStatus> {
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if gone.load(Ordering::Acquire) || Instant::now() >= deadline {
            return hang_up(child);
        }
        thread::sleep(POLL);
    }
}

fn hang_up(child: &mut Child) -> Result<ExitStatus> {
    tracing::info!(category = "device_setup", "Hanging up the setup");
    // The session shim made the child the leader of its own process group.
    if let Some(group) = i32::try_from(child.id()).ok().and_then(Pid::from_raw) {
        let _ = rustix::process::kill_process_group(group, Signal::HUP);
    }
    let deadline = Instant::now() + HANGUP_GRACE;
    while Instant::now() < deadline {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        thread::sleep(POLL);
    }
    let _ = child.kill();
    Ok(child.wait()?)
}
