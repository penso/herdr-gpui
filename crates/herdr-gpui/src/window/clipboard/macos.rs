//! A native pasteboard read can block or fault inside AppKit. Run it on the
//! main thread of a short-lived copy of this executable, before GPUI starts.
//! No NSPasteboard objects are shared with the GUI's foreground copy paths,
//! and a native fault cannot take the user's windows down with the reader.

#[cfg(test)]
mod tests;
mod wire;

use super::{ClipboardItem, IMAGE_LIMIT, TEXT_LIMIT};
use crate::window::clipboard;
use std::{
    ffi::OsString,
    io::{self, Read},
    os::{fd::OwnedFd, unix::net::UnixStream},
    process::{Child, Command, ExitCode, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

const HELPER: &str = "__read-clipboard";
const TIMEOUT: Duration = Duration::from_secs(3);
const READ_CHUNK: usize = 64 * 1024;
// How far a read may run past the deadline.
const READ_WAIT: Duration = Duration::from_millis(50);
// One tag followed by an image at most. Text and file paths have a smaller cap.
const OUTPUT_LIMIT: usize = IMAGE_LIMIT + 1;
const CHANGED: u8 = 10;
const INVALID_TEXT: u8 = 11;
const TEXT_TOO_LARGE: u8 = 12;
const IMAGE_TOO_LARGE: u8 = 13;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("The macOS clipboard reader exited unsuccessfully ({0}). Try pasting again.")]
    Exited(ExitStatus),
    #[error("Could not run the macOS clipboard reader")]
    Process(#[source] io::Error),
    #[error("The macOS clipboard reader returned an invalid response")]
    Response,
    #[error("Clipboard text is not valid UTF-8.")]
    TextEncoding,
    // keep-sorted end
}

impl From<Error> for crate::Error {
    fn from(error: Error) -> Self {
        clipboard::Error::Native(error).into()
    }
}

/// Blocking, but never calls AppKit in the GUI process. The existing paste
/// reservation still owns ordering, target validation, and cancellation.
pub(super) fn read(image_only: bool) -> crate::Result<Option<ClipboardItem>> {
    let executable = std::env::current_exe().map_err(Error::Process)?;
    let mut command = herdr_client::new_command(executable);
    command.args([HELPER, if image_only { "image" } else { "normal" }]);
    acquire(&mut command, Instant::now() + TIMEOUT, OUTPUT_LIMIT)
}

/// Private subprocess entry point. Invalid arguments exit without reading the
/// clipboard, loading user configuration, opening a window, or a daemon.
pub(crate) fn run_helper(args: &[OsString]) -> Option<ExitCode> {
    if args.first().is_none_or(|arg| arg != HELPER) {
        return None;
    }
    let image_only = match args {
        [_, mode] if mode == "normal" => false,
        [_, mode] if mode == "image" => true,
        _ => return Some(ExitCode::from(2)),
    };
    let Some(_main_thread) = objc2::MainThreadMarker::new() else {
        return Some(ExitCode::FAILURE);
    };
    let result = objc2::rc::autoreleasepool(|_| {
        super::read_pasteboard(
            &objc2_app_kit::NSPasteboard::generalPasteboard(),
            image_only,
        )
    });
    Some(match result {
        Ok(item) => match wire::write(item.as_ref(), &mut io::stdout().lock()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(crate::Error::Clipboard(clipboard::Error::Size { .. })) => {
                ExitCode::from(TEXT_TOO_LARGE)
            }
            Err(_) => ExitCode::FAILURE,
        },
        Err(crate::Error::Clipboard(clipboard::Error::Changed)) => ExitCode::from(CHANGED),
        Err(crate::Error::Clipboard(clipboard::Error::Encoding(_))) => ExitCode::from(INVALID_TEXT),
        Err(crate::Error::Clipboard(clipboard::Error::Size { limit })) if limit == TEXT_LIMIT => {
            ExitCode::from(TEXT_TOO_LARGE)
        }
        Err(crate::Error::Clipboard(clipboard::Error::Size { .. })) => {
            ExitCode::from(IMAGE_TOO_LARGE)
        }
        Err(_) => ExitCode::FAILURE,
    })
}

struct Reader(Child);

impl Drop for Reader {
    fn drop(&mut self) {
        // This helper launches no descendants. Only our exact child is killed
        // and reaped, including timeout, malformed output, and unwind paths.
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn acquire(
    command: &mut Command,
    deadline: Instant,
    limit: usize,
) -> crate::Result<Option<ClipboardItem>> {
    if Instant::now() >= deadline {
        return Err(clipboard::Error::Timeout.into());
    }
    // As in the Linux reader, a short read timeout needs no reader thread that
    // could outlive a hung clipboard owner. Do not retain or log stderr.
    let (mut reader, writer) = UnixStream::pair().map_err(Error::Process)?;
    // Block until output arrives rather than sleeping between reads: a socket
    // pair holds only a few kilobytes, so polling paced a large TIFF copy past
    // the deadline one small read at a time. Set it before the child can exit:
    // macOS refuses socket options once the peer has closed.
    reader
        .set_read_timeout(Some(READ_WAIT))
        .map_err(Error::Process)?;
    let spawned = command
        .stdin(Stdio::null())
        .stdout(Stdio::from(OwnedFd::from(writer)))
        .stderr(Stdio::null())
        .spawn();
    command.stdout(Stdio::null());
    let mut child = Reader(spawned.map_err(Error::Process)?);
    let mut bytes = Vec::new();
    let mut buffer = vec![0; READ_CHUNK];
    let mut eof = false;
    loop {
        if Instant::now() >= deadline {
            return Err(clipboard::Error::Timeout.into());
        }
        if !eof {
            match reader.read(&mut buffer) {
                Ok(0) => eof = true,
                Ok(count) => {
                    if count > limit.saturating_sub(bytes.len()) {
                        return Err(clipboard::Error::Size { limit }.into());
                    }
                    bytes.extend_from_slice(&buffer[..count]);
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock
                            | io::ErrorKind::TimedOut
                            | io::ErrorKind::Interrupted
                    ) => {}
                Err(error) => return Err(Error::Process(error).into()),
            }
            continue;
        }
        if let Some(status) = child.0.try_wait().map_err(Error::Process)? {
            if !status.success() {
                // Partial stdout from a reader that crashed is never pasted.
                return Err(match status.code() {
                    Some(code) if code == i32::from(CHANGED) => clipboard::Error::Changed.into(),
                    Some(code) if code == i32::from(INVALID_TEXT) => Error::TextEncoding.into(),
                    Some(code) if code == i32::from(TEXT_TOO_LARGE) => {
                        clipboard::Error::Size { limit: TEXT_LIMIT }.into()
                    }
                    Some(code) if code == i32::from(IMAGE_TOO_LARGE) => {
                        clipboard::Error::Size { limit: IMAGE_LIMIT }.into()
                    }
                    _ => Error::Exited(status).into(),
                });
            }
            return wire::read(bytes);
        }
        // Output is complete; the reader is exiting.
        thread::sleep(Duration::from_millis(5));
    }
}
