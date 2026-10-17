//! Crash reports, kept beside the log and never uploaded. A panic hook writes
//! the message, where it happened, and a backtrace to `crash-<time>-<pid>.txt`
//! in the logs directory, and an error record to the log, which it waits for
//! the writer to flush before the process goes. Windows also writes a
//! minidump for a panic and for a crash Rust never sees, such as an access
//! violation (`windows`). Only the newest few of each are kept.
//!
//! On macOS and Linux a signal such as `SIGSEGV` leaves no report here; the
//! system's own crash reporter, if any, still sees it.

#[cfg(windows)]
mod windows;

use std::{
    backtrace::Backtrace,
    fmt::Write as _,
    fs,
    io::{self, Write as _},
    panic::PanicHookInfo,
    path::{Path, PathBuf},
    time::Duration,
};

/// Reports and dumps kept; older ones are deleted when a new one is written.
pub(crate) const KEEP_REPORTS: usize = 5;
#[cfg(windows)]
const KEEP_DUMPS: usize = 3;
pub(crate) const PREFIX: &str = "crash-";
pub(crate) const REPORT_EXTENSION: &str = "txt";
#[cfg(windows)]
pub(crate) const DUMP_EXTENSION: &str = "dmp";
const TIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S%.3f %:z";
/// How long a crashing process waits for its log record to reach the disk.
const FLUSH: Duration = Duration::from_secs(2);

/// Where reports go: the log file's directory.
pub(crate) fn dir() -> Option<PathBuf> {
    Some(crate::diagnostics::log_file()?.parent()?.to_owned())
}

/// Call once, after `diagnostics::init`. The default hook still runs after
/// ours, so a terminal still sees the panic.
pub(crate) fn install() {
    let Some(dir) = dir() else {
        return;
    };
    #[cfg(windows)]
    windows::install(dir.clone());
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        record(&dir, info);
        previous(info);
    }));
}

fn record(dir: &Path, info: &PanicHookInfo<'_>) {
    let now = chrono::Local::now();
    let backtrace = Backtrace::force_capture();
    let thread = std::thread::current();
    let thread = thread.name().unwrap_or("unnamed");
    let message = message(info.payload());
    let location = info
        .location()
        .map(|location| location.to_string())
        .unwrap_or_default();
    let text = report(
        &now.format(TIME_FORMAT).to_string(),
        thread,
        message,
        &location,
        &backtrace.to_string(),
    );
    let name = file_name(now, std::process::id(), REPORT_EXTENSION);
    let written = write_new(&dir.join(&name), &text);
    prune(dir, REPORT_EXTENSION, KEEP_REPORTS);
    match &written {
        Ok(()) => tracing::error!(
            panic = message,
            location,
            thread,
            report = name,
            backtrace = %backtrace,
            "Herdr UI panicked"
        ),
        Err(error) => tracing::error!(
            panic = message,
            location,
            thread,
            %error,
            backtrace = %backtrace,
            "Herdr UI panicked; the crash report could not be written"
        ),
    }
    #[cfg(windows)]
    windows::dump_now();
    crate::diagnostics::flush(FLUSH);
}

/// A panic's payload as text: `panic!` passes a `&str` or a `String`.
fn message(payload: &(dyn std::any::Any + Send)) -> &str {
    payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("Box<dyn Any>")
}

fn report(time: &str, thread: &str, message: &str, location: &str, backtrace: &str) -> String {
    let mut text = String::from("Herdr UI crash report\n\n");
    let _ = writeln!(text, "version: {}", crate::constants::APP_VERSION);
    let _ = writeln!(
        text,
        "platform: {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    let _ = writeln!(text, "time: {time}");
    let _ = writeln!(text, "thread: {thread}");
    let _ = writeln!(text, "location: {location}");
    let _ = writeln!(text, "message: {message}");
    let _ = write!(text, "\nbacktrace:\n{backtrace}");
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

/// `crash-20261017-230512.042-4242.txt`: names sort by time, and two
/// processes crashing in the same millisecond still differ.
fn file_name(now: chrono::DateTime<chrono::Local>, pid: u32, extension: &str) -> String {
    format!(
        "{PREFIX}{}-{pid}.{extension}",
        now.format("%Y%m%d-%H%M%S%.3f")
    )
}

/// Writes a new file, never replacing one.
fn write_new(path: &Path, text: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    // Reports can name local paths; keep them private to the user, as logs are.
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)?.write_all(text.as_bytes())
}

/// This directory's crash files with `extension`, newest first.
pub(crate) fn files(dir: &Path, extension: &str) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<_> = entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let name = entry.file_name();
            let name = name.to_str()?;
            let is_ours = name.starts_with(PREFIX)
                && Path::new(name)
                    .extension()
                    .is_some_and(|ext| ext == extension)
                && entry.file_type().ok()?.is_file();
            is_ours.then(|| entry.path())
        })
        .collect();
    // The timestamp in the name orders them; modification times can be reset.
    files.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    files
}

/// Deletes all but the newest `keep` crash files with `extension`.
fn prune(dir: &Path, extension: &str, keep: usize) {
    for old in files(dir, extension).into_iter().skip(keep) {
        let _ = fs::remove_file(old);
    }
}

#[cfg(test)]
mod tests;
