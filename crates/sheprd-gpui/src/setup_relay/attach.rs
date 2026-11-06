//! The pane's end: run in a daemon workspace, it shows the setup the app
//! runs on its own terminal. Only bytes cross the socket, so nothing here
//! touches the network or needs any permission of its own.
use super::Error;
use super::frame::{Frame, Size};
use crate::Result;
use rustix::termios::{OptionalActions, Termios};
use std::{
    io::{self, Read, Write},
    os::unix::net::UnixStream,
    path::Path,
    process::ExitCode,
    sync::{Arc, Mutex, PoisonError},
    thread,
    time::Duration,
};

/// How often the pane's size is compared with what the setup was told.
const RESIZE_POLL: Duration = Duration::from_millis(250);
/// Keys read from the pane at a time.
const INPUT_CHUNK: usize = 4096;

pub(crate) fn run(path: &Path) -> ExitCode {
    match attach(path) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("\r\n{error}");
            ExitCode::FAILURE
        }
    }
}

/// Relays until the setup ends, and returns its exit status.
fn attach(path: &Path) -> Result<u8> {
    let stdin = io::stdin();
    if !rustix::termios::isatty(&stdin) {
        return Err(Error::Terminal.into());
    }
    let mut stream = UnixStream::connect(path).map_err(Error::Connect)?;
    let size = terminal_size();
    let term = std::env::var("TERM").ok();
    Frame::Hello { size, term }.write(&mut stream)?;
    // Raw, so every key, Ctrl-C included, reaches the setup's own terminal,
    // which echoes and interprets it. Restored when this returns.
    let _raw = RawMode::enter()?;
    let writer = Arc::new(Mutex::new(stream.try_clone()?));
    {
        let writer = writer.clone();
        thread::Builder::new()
            .name("herdr-setup-keys".into())
            .spawn(move || forward_keys(&writer))?;
    }
    thread::Builder::new()
        .name("herdr-setup-size".into())
        .spawn(move || forward_size(&writer, size))?;
    let mut stdout = io::stdout().lock();
    loop {
        match Frame::read(&mut stream)? {
            Some(Frame::Output(bytes)) => {
                stdout.write_all(&bytes)?;
                stdout.flush()?;
            }
            Some(Frame::Exit(code)) => return Ok(u8::try_from(code.clamp(0, 255)).unwrap_or(1)),
            Some(_) => return Err(Error::Frame.into()),
            None => return Err(Error::Closed.into()),
        }
    }
}

fn forward_keys(writer: &Mutex<UnixStream>) {
    let mut buffer = vec![0u8; INPUT_CHUNK];
    let mut stdin = io::stdin().lock();
    loop {
        let read = match stdin.read(&mut buffer) {
            Ok(0) => return,
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return,
        };
        let mut stream = writer.lock().unwrap_or_else(PoisonError::into_inner);
        if Frame::Input(buffer[..read].to_vec())
            .write(&mut *stream)
            .is_err()
        {
            return;
        }
    }
}

/// Polls rather than handling `SIGWINCH`, which would need a signal handler.
fn forward_size(writer: &Mutex<UnixStream>, mut told: Size) {
    loop {
        thread::sleep(RESIZE_POLL);
        let size = terminal_size();
        if size == told {
            continue;
        }
        let mut stream = writer.lock().unwrap_or_else(PoisonError::into_inner);
        if Frame::Resize(size).write(&mut *stream).is_err() {
            return;
        }
        told = size;
    }
}

/// The pane's size, or 24×80 when the terminal does not say.
fn terminal_size() -> Size {
    match rustix::termios::tcgetwinsize(io::stdin()) {
        Ok(size) if size.ws_row > 0 && size.ws_col > 0 => Size {
            rows: size.ws_row,
            cols: size.ws_col,
        },
        _ => Size { rows: 24, cols: 80 },
    }
}

/// The pane's terminal in raw mode until dropped.
struct RawMode {
    saved: Termios,
}

impl RawMode {
    fn enter() -> io::Result<Self> {
        let saved = rustix::termios::tcgetattr(io::stdin())?;
        let mut raw = saved.clone();
        raw.make_raw();
        rustix::termios::tcsetattr(io::stdin(), OptionalActions::Now, &raw)?;
        Ok(Self { saved })
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        let _ = rustix::termios::tcsetattr(io::stdin(), OptionalActions::Now, &self.saved);
    }
}
