//! One direction of a Windows pipe: a handle opened for overlapped I/O and the
//! single operation it may have in flight. The operation's `OVERLAPPED` and
//! buffer live on the heap and stay untouched until the kernel reports the
//! operation complete, so a wait can end at a deadline without cancelling it.
//! Only dropping the channel cancels, and it waits for the cancellation to
//! land before releasing the memory the kernel was writing.
//!
//! No maintained safe wrapper offers this without a runtime: `interprocess`
//! and the standard library block in `WriteFile` with no deadline, and the
//! asynchronous wrappers need Tokio or an I/O completion port reactor, both
//! more machinery than one bounded wait per call. So the four calls that
//! start, wait for, and cancel an operation are made here, and nowhere else.
#![allow(unsafe_code)]

use std::{
    cell::UnsafeCell,
    io,
    mem::ManuallyDrop,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    ptr,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{ERROR_IO_INCOMPLETE, ERROR_IO_PENDING, WAIT_TIMEOUT},
    Storage::FileSystem::{ReadFile, WriteFile},
    System::{
        IO::{CancelIoEx, GetOverlappedResultEx, OVERLAPPED},
        Threading::{CreateEventW, INFINITE},
    },
};

/// The most one operation moves. It bounds the memory a connection holds per
/// direction, and matches the slice an image write offers per poll.
pub(crate) const CHUNK: usize = 64 * 1024;

/// How long dropping a channel waits for a cancelled operation to complete.
/// Pipes complete a cancellation at once; this only bounds a driver that does
/// not, after which the operation's memory is leaked rather than freed under it.
const CANCEL_WAIT: Duration = Duration::from_secs(5);

/// What an operation did once it is no longer in flight.
pub(crate) enum Progress {
    /// Still in flight at the deadline; the operation keeps its buffer.
    Pending,
    /// Completed, moving this many bytes.
    Done(usize),
}

pub(crate) struct Channel {
    handle: OwnedHandle,
    op: ManuallyDrop<Op>,
    pending: bool,
}

/// The kernel holds raw pointers to `overlapped` and `buffer` while an
/// operation is in flight, so neither is moved, reallocated, borrowed, or
/// freed until it completes. Both are boxed, so moving the channel moves only
/// the pointers.
struct Op {
    overlapped: Box<UnsafeCell<OVERLAPPED>>,
    buffer: Box<[u8]>,
    /// Manual reset, as `GetOverlappedResultEx` requires: it is the object the
    /// wait blocks on, and each started operation resets it.
    event: OwnedHandle,
}

// SAFETY: the only pointer in `OVERLAPPED` is `hEvent`, a copy of `event`'s
// handle, which this op owns. Handles and overlapped operations are not tied
// to the thread that made them: any thread may wait for, collect, or cancel
// one, and `&mut` access keeps that to one thread at a time.
unsafe impl Send for Op {}

impl Channel {
    /// Take ownership of `handle`, which must have been opened or created with
    /// `FILE_FLAG_OVERLAPPED`; a synchronous handle would block in each start.
    pub(crate) fn new(handle: OwnedHandle) -> io::Result<Self> {
        // SAFETY: every argument may be null or false; the result is checked
        // and owned at once, so it is closed exactly once.
        let event = unsafe { CreateEventW(ptr::null(), 1, 0, ptr::null()) };
        if event.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `event` is a fresh handle nothing else owns.
        let event = unsafe { OwnedHandle::from_raw_handle(event) };
        Ok(Self {
            handle,
            op: ManuallyDrop::new(Op {
                // SAFETY: `OVERLAPPED` is plain data for which all zeroes is the
                // documented initial state.
                overlapped: Box::new(UnsafeCell::new(unsafe { std::mem::zeroed() })),
                buffer: vec![0; CHUNK].into_boxed_slice(),
                event,
            }),
            pending: false,
        })
    }

    pub(crate) fn is_pending(&self) -> bool {
        self.pending
    }

    /// The bytes a completed read left in the buffer.
    pub(crate) fn buffer(&self) -> &[u8] {
        debug_assert!(!self.pending);
        &self.op.buffer
    }

    /// Start writing `bytes`, at most one chunk of them, and return how many
    /// the operation took. They are copied, so the caller's slice is free.
    pub(crate) fn start_write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        debug_assert!(!self.pending);
        let len = bytes.len().min(CHUNK);
        self.op.buffer[..len].copy_from_slice(&bytes[..len]);
        let overlapped = self.reset();
        // SAFETY: the handle is open and overlapped. `buffer` and `overlapped`
        // are heap allocations owned by this channel and left alone until the
        // operation completes (see `Op`). `len` is within `buffer`, and the
        // byte count pointer is null, as an overlapped call permits.
        let started = unsafe {
            WriteFile(
                self.handle.as_raw_handle(),
                self.op.buffer.as_ptr(),
                len as u32,
                ptr::null_mut(),
                overlapped,
            )
        };
        self.started(started)?;
        Ok(len)
    }

    /// Start reading up to one chunk into the buffer.
    pub(crate) fn start_read(&mut self) -> io::Result<()> {
        debug_assert!(!self.pending);
        let overlapped = self.reset();
        // SAFETY: as in `start_write`; the kernel writes at most `CHUNK` bytes
        // into the buffer, which is exactly that long.
        let started = unsafe {
            ReadFile(
                self.handle.as_raw_handle(),
                self.op.buffer.as_mut_ptr(),
                CHUNK as u32,
                ptr::null_mut(),
                overlapped,
            )
        };
        self.started(started)
    }

    /// Wait until the operation in flight completes or `deadline` passes, with
    /// no deadline meaning until it completes. An operation that fails is no
    /// longer in flight either.
    pub(crate) fn wait(&mut self, deadline: Option<Instant>) -> io::Result<Progress> {
        if !self.pending {
            return Ok(Progress::Done(0));
        }
        loop {
            let millis = deadline.map_or(INFINITE, |deadline| {
                let remaining = deadline.saturating_duration_since(Instant::now());
                // Round up, and stay below `INFINITE`.
                remaining
                    .as_nanos()
                    .div_ceil(1_000_000)
                    .min(u128::from(INFINITE - 1)) as u32
            });
            match self.result(millis) {
                Ok(moved) => {
                    self.pending = false;
                    return Ok(Progress::Done(moved));
                }
                // The system timer may end a wait a little early; a deadline
                // is never reported before it passes.
                Err(error) if still_pending(&error) => {
                    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                        return Ok(Progress::Pending);
                    }
                }
                Err(error) => {
                    self.pending = false;
                    return Err(error);
                }
            }
        }
    }

    /// Clear the `OVERLAPPED` for a new operation and return the pointer the
    /// kernel receives.
    fn reset(&mut self) -> *mut OVERLAPPED {
        let overlapped = self.op.overlapped.get();
        // SAFETY: no operation is in flight, so nothing else reads or writes
        // the structure. Zero offsets are what a pipe expects.
        unsafe {
            overlapped.write(std::mem::zeroed());
            (*overlapped).hEvent = self.op.event.as_raw_handle();
        }
        overlapped
    }

    fn started(&mut self, started: i32) -> io::Result<()> {
        // A synchronous success still completes through the event, so both
        // outcomes are collected by `wait`.
        if started != 0 {
            self.pending = true;
            return Ok(());
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(ERROR_IO_PENDING as i32) {
            self.pending = true;
            return Ok(());
        }
        Err(error)
    }

    fn result(&self, millis: u32) -> io::Result<usize> {
        let mut moved = 0;
        // SAFETY: the `OVERLAPPED` belongs to the operation started on this
        // same handle, and `moved` is a live local the call writes on success.
        let done = unsafe {
            GetOverlappedResultEx(
                self.handle.as_raw_handle(),
                self.op.overlapped.get(),
                &mut moved,
                millis,
                0,
            )
        };
        if done == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(moved as usize)
    }
}

impl Drop for Channel {
    fn drop(&mut self) {
        if self.pending {
            // SAFETY: the `OVERLAPPED` identifies the operation this channel
            // started on this handle. A failure means it already completed.
            unsafe { CancelIoEx(self.handle.as_raw_handle(), self.op.overlapped.get()) };
            let millis = CANCEL_WAIT.as_millis() as u32;
            match self.result(millis) {
                Err(error) if still_pending(&error) => {
                    tracing::warn!(
                        operation = "pipe_cancel",
                        "Windows pipe operation outlived its cancellation"
                    );
                    // The kernel may still write here: leak rather than free.
                    return;
                }
                // Completed, cancelled, or failed: the kernel is done with it.
                _ => {}
            }
        }
        // SAFETY: no operation is in flight, and `op` is not used again.
        unsafe { ManuallyDrop::drop(&mut self.op) };
    }
}

fn still_pending(error: &io::Error) -> bool {
    matches!(
        error.raw_os_error(),
        Some(code) if code == WAIT_TIMEOUT as i32 || code == ERROR_IO_INCOMPLETE as i32
    )
}
