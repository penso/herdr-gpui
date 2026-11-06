//! Minidumps on Windows. An unhandled exception (an access violation, a stack
//! overflow, a foreign library's crash) never reaches the panic hook, so an
//! unhandled-exception filter asks for a dump, and the panic hook asks too,
//! because a release build's backtrace has no symbols while a dump can be
//! opened against the matching PDB.
//!
//! The faulting thread may have no stack left, so it never writes the dump
//! itself: a `crash-dump` thread started at install waits on an event, and
//! the filter only stores the exception, signals, and waits for it. The dump
//! holds every thread's stack and registers, not the heap; it is never put in
//! a diagnostics bundle.
//!
//! The calls are `unsafe` because no safe wrapper in the workspace, or a
//! maintained crate that would not add a second `windows-sys`, covers
//! `SetUnhandledExceptionFilter` and `MiniDumpWriteDump`; each block states
//! what it relies on.

use super::*;
use std::{
    os::windows::io::AsRawHandle as _,
    ptr,
    sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicU64, AtomicUsize, Ordering},
    time::Instant,
};
use windows_sys::Win32::{
    Foundation::HANDLE,
    System::{
        Diagnostics::Debug::{
            EXCEPTION_POINTERS, MINIDUMP_EXCEPTION_INFORMATION, MiniDumpNormal,
            MiniDumpWithThreadInfo, MiniDumpWithUnloadedModules, MiniDumpWriteDump,
            SetUnhandledExceptionFilter,
        },
        Threading::{
            CreateEventW, GetCurrentProcess, GetCurrentProcessId, GetCurrentThreadId, INFINITE,
            SetEvent, WaitForSingleObject,
        },
    },
};

/// Lets Windows' own error reporting run after ours.
const EXCEPTION_CONTINUE_SEARCH: i32 = 0;
/// A dump that takes longer than this is abandoned, and the process ends.
const DUMP_WAIT: Duration = Duration::from_secs(30);
/// A requester rechecks [`FINISHED`] at least this often. `DONE` is one
/// auto-reset event shared by every requester, so the signal for a finished
/// dump can wake a different waiter; a short sleep bounds that miss.
const RECHECK: Duration = Duration::from_millis(50);

/// The dump thread's events, as `usize` because a `HANDLE` is a pointer;
/// zero until it runs. Never closed: they live as long as the process.
static REQUESTED: AtomicUsize = AtomicUsize::new(0);
static DONE: AtomicUsize = AtomicUsize::new(0);
/// The request: an exception, or null for a dump with none (a panic).
static EXCEPTION: AtomicPtr<EXCEPTION_POINTERS> = AtomicPtr::new(ptr::null_mut());
static THREAD: AtomicU32 = AtomicU32::new(0);
/// Requests made, and the last one the dump thread finished. A requester
/// waits for its own number, so a late answer to an abandoned request never
/// releases it early.
static REQUESTS: AtomicU64 = AtomicU64::new(0);
static FINISHED: AtomicU64 = AtomicU64::new(0);
/// One dump at a time, from request until the dump thread finishes it,
/// even if its requester stopped waiting. A second request waits for it,
/// then makes its own, so each crash gets a dump and a report.
pub(super) static BUSY: AtomicBool = AtomicBool::new(false);

pub(super) fn install(dir: PathBuf, version: &'static str) {
    let (Some(requested), Some(done)) = (event(), event()) else {
        tracing::warn!(error = %io::Error::last_os_error(), "Crash dumps are off: no event");
        return;
    };
    let serving = std::thread::Builder::new()
        .name("crash-dump".into())
        .spawn(move || serve(&dir, version, requested, done));
    if let Err(error) = serving {
        tracing::warn!(%error, "Crash dumps are off: no dump thread");
        return;
    }
    REQUESTED.store(requested, Ordering::Release);
    DONE.store(done, Ordering::Release);
    // SAFETY: `filter` is a plain function that lives as long as the process
    // and only reads the pointer Windows passes it.
    #[allow(unsafe_code)]
    unsafe {
        SetUnhandledExceptionFilter(Some(filter));
    }
}

/// An auto-reset event, unsignalled.
fn event() -> Option<usize> {
    // SAFETY: no security attributes or name; Windows returns a new handle or
    // null.
    #[allow(unsafe_code)]
    let handle = unsafe { CreateEventW(ptr::null(), 0, 0, ptr::null()) };
    (!handle.is_null()).then_some(handle as usize)
}

/// From the panic hook: a dump of every thread as it is now.
pub(super) fn dump_now() {
    // SAFETY: no arguments; it cannot fail.
    #[allow(unsafe_code)]
    let thread = unsafe { GetCurrentThreadId() };
    request(ptr::null_mut(), thread);
}

#[allow(unsafe_code)]
unsafe extern "system" fn filter(info: *const EXCEPTION_POINTERS) -> i32 {
    // SAFETY: as in `dump_now`.
    let thread = unsafe { GetCurrentThreadId() };
    request(info.cast_mut(), thread);
    EXCEPTION_CONTINUE_SEARCH
}

/// Hands the dump thread a request and waits for it, first waiting out
/// another thread's dump: returning early would let Windows end the process
/// mid-dump. Atomics and system calls only: this may run on a thread with no
/// stack or a corrupt heap. One deadline covers both waits.
fn request(exception: *mut EXCEPTION_POINTERS, thread: u32) {
    let (requested, done) = (
        REQUESTED.load(Ordering::Acquire),
        DONE.load(Ordering::Acquire),
    );
    if requested == 0 || done == 0 {
        return;
    }
    let deadline = Instant::now() + DUMP_WAIT;
    while BUSY.swap(true, Ordering::AcqRel) {
        if !pause(done, deadline) {
            return;
        }
    }
    EXCEPTION.store(exception, Ordering::Release);
    THREAD.store(thread, Ordering::Release);
    let ticket = REQUESTS.fetch_add(1, Ordering::AcqRel) + 1;
    // SAFETY: both handles are events created at install and never closed.
    #[allow(unsafe_code)]
    if unsafe { SetEvent(requested as HANDLE) } == 0 {
        BUSY.store(false, Ordering::Release);
        return;
    }
    while FINISHED.load(Ordering::Acquire) < ticket {
        if !pause(done, deadline) {
            return;
        }
    }
}

/// Sleeps until `done` is signalled or [`RECHECK`] passes; false once
/// `deadline` has.
fn pause(done: usize, deadline: Instant) -> bool {
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() {
        return false;
    }
    // At most RECHECK, so it fits in a u32 and is at least 1 ms.
    let millis = left.min(RECHECK).as_millis().max(1) as u32;
    // SAFETY: an event created at install and never closed.
    #[allow(unsafe_code)]
    unsafe {
        WaitForSingleObject(done as HANDLE, millis);
    }
    true
}

fn serve(dir: &Path, version: &str, requested: usize, done: usize) {
    loop {
        // SAFETY: an event created at install and never closed.
        #[allow(unsafe_code)]
        unsafe {
            WaitForSingleObject(requested as HANDLE, INFINITE);
        }
        let ticket = REQUESTS.load(Ordering::Acquire);
        let exception = EXCEPTION.load(Ordering::Acquire);
        let thread = THREAD.load(Ordering::Acquire);
        let pointers = (!exception.is_null()).then_some((thread, exception));
        match write_dump(dir, pointers) {
            Ok(path) => tracing::error!(dump = %path.display(), "Wrote a crash dump"),
            Err(error) => tracing::error!(%error, "Could not write a crash dump"),
        }
        prune(dir, DUMP_EXTENSION, KEEP_DUMPS);
        if let Some((_, exception)) = pointers {
            // SAFETY: the faulting thread is blocked in the filter, so the
            // records Windows passed it are still alive.
            #[allow(unsafe_code)]
            let (code, address) = unsafe {
                let record = &*(*exception).ExceptionRecord;
                (record.ExceptionCode, record.ExceptionAddress)
            };
            report_exception(dir, version, code, address as usize);
        }
        FINISHED.store(ticket, Ordering::Release);
        BUSY.store(false, Ordering::Release);
        // SAFETY: as above.
        #[allow(unsafe_code)]
        unsafe {
            SetEvent(done as HANDLE);
        }
    }
}

/// The text report and log record for a crash the panic hook never saw.
fn report_exception(dir: &Path, version: &str, code: i32, address: usize) {
    let now = chrono::Local::now();
    let message = format!("unhandled exception {:#010x}", code as u32);
    let location = format!("address {address:#x}");
    let text = report(
        version,
        &now.format(TIME_FORMAT).to_string(),
        "unknown",
        &message,
        &location,
        "(none; open the crash dump of the same time)",
    );
    let name = file_name(now.to_utc(), std::process::id(), REPORT_EXTENSION);
    if let Err(error) = write_new(&dir.join(&name), &text) {
        tracing::error!(%error, "Could not write a crash report");
    }
    prune(dir, REPORT_EXTENSION, KEEP_REPORTS);
    tracing::error!(
        exception = message,
        location,
        report = name,
        "Sheprd crashed"
    );
    crate::flush(FLUSH);
}

/// Writes a dump of this process to a new file in `dir`; `exception` is the
/// faulting thread and its pointers, when there is one.
pub(super) fn write_dump(
    dir: &Path,
    exception: Option<(u32, *mut EXCEPTION_POINTERS)>,
) -> io::Result<PathBuf> {
    let path = dir.join(file_name(
        chrono::Utc::now(),
        std::process::id(),
        DUMP_EXTENSION,
    ));
    fs::create_dir_all(dir)?;
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    let info = exception.map(|(thread, pointers)| MINIDUMP_EXCEPTION_INFORMATION {
        ThreadId: thread,
        ExceptionPointers: pointers,
        ClientPointers: 0,
    });
    // SAFETY: the handles are this process and an open, writable file; `info`
    // outlives the call, and its pointers are this process's own (so
    // `ClientPointers` is false). Windows reads them and keeps nothing.
    #[allow(unsafe_code)]
    let written = unsafe {
        MiniDumpWriteDump(
            GetCurrentProcess(),
            GetCurrentProcessId(),
            file.as_raw_handle() as HANDLE,
            MiniDumpNormal | MiniDumpWithThreadInfo | MiniDumpWithUnloadedModules,
            info.as_ref().map_or(ptr::null(), ptr::from_ref),
            ptr::null(),
            ptr::null(),
        )
    };
    if written == 0 {
        let error = io::Error::last_os_error();
        drop(file);
        let _ = fs::remove_file(&path);
        return Err(error);
    }
    Ok(path)
}
