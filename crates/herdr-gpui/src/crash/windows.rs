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
    sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicUsize, Ordering},
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
const DUMP_WAIT_MS: u32 = 30_000;

/// The dump thread's events, as `usize` because a `HANDLE` is a pointer;
/// zero until it runs. Never closed: they live as long as the process.
static REQUESTED: AtomicUsize = AtomicUsize::new(0);
static DONE: AtomicUsize = AtomicUsize::new(0);
/// The request: an exception, or null for a dump with none (a panic).
static EXCEPTION: AtomicPtr<EXCEPTION_POINTERS> = AtomicPtr::new(ptr::null_mut());
static THREAD: AtomicU32 = AtomicU32::new(0);
/// One dump at a time; a second crashing thread is in the first one's dump.
static BUSY: AtomicBool = AtomicBool::new(false);

pub(super) fn install(dir: PathBuf) {
    let (Some(requested), Some(done)) = (event(), event()) else {
        tracing::warn!(error = %io::Error::last_os_error(), "Crash dumps are off: no event");
        return;
    };
    let serving = std::thread::Builder::new()
        .name("crash-dump".into())
        .spawn(move || serve(&dir, requested, done));
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

/// Hands the dump thread a request and waits for it. Atomics and two system
/// calls only: this may run on a thread with no stack or a corrupt heap.
fn request(exception: *mut EXCEPTION_POINTERS, thread: u32) {
    let (requested, done) = (
        REQUESTED.load(Ordering::Acquire),
        DONE.load(Ordering::Acquire),
    );
    if requested == 0 || done == 0 || BUSY.swap(true, Ordering::AcqRel) {
        return;
    }
    EXCEPTION.store(exception, Ordering::Release);
    THREAD.store(thread, Ordering::Release);
    // SAFETY: both handles are events created at install and never closed.
    #[allow(unsafe_code)]
    unsafe {
        if SetEvent(requested as HANDLE) != 0 {
            WaitForSingleObject(done as HANDLE, DUMP_WAIT_MS);
        }
    }
    BUSY.store(false, Ordering::Release);
}

fn serve(dir: &Path, requested: usize, done: usize) {
    loop {
        // SAFETY: an event created at install and never closed.
        #[allow(unsafe_code)]
        unsafe {
            WaitForSingleObject(requested as HANDLE, INFINITE);
        }
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
            report_exception(dir, code, address as usize);
        }
        // SAFETY: as above.
        #[allow(unsafe_code)]
        unsafe {
            SetEvent(done as HANDLE);
        }
    }
}

/// The text report and log record for a crash the panic hook never saw.
fn report_exception(dir: &Path, code: i32, address: usize) {
    let now = chrono::Local::now();
    let message = format!("unhandled exception {:#010x}", code as u32);
    let location = format!("address {address:#x}");
    let text = report(
        &now.format(TIME_FORMAT).to_string(),
        "unknown",
        &message,
        &location,
        "(none; open the crash dump of the same time)",
    );
    let name = file_name(now, std::process::id(), REPORT_EXTENSION);
    if let Err(error) = write_new(&dir.join(&name), &text) {
        tracing::error!(%error, "Could not write a crash report");
    }
    prune(dir, REPORT_EXTENSION, KEEP_REPORTS);
    tracing::error!(
        exception = message,
        location,
        report = name,
        "Herdr UI crashed"
    );
    crate::diagnostics::flush(FLUSH);
}

/// Writes a dump of this process to a new file in `dir`; `exception` is the
/// faulting thread and its pointers, when there is one.
pub(super) fn write_dump(
    dir: &Path,
    exception: Option<(u32, *mut EXCEPTION_POINTERS)>,
) -> io::Result<PathBuf> {
    let path = dir.join(file_name(
        chrono::Local::now(),
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
