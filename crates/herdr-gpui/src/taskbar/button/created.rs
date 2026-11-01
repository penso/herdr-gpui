//! Hears when Explorer (re)creates a window's taskbar button: the first time
//! after the window is shown, and again whenever Explorer restarts. A new
//! button has no badge or progress, so they are set again then. Explorer
//! sends the registered `TaskbarButtonCreated` message to the window itself,
//! and GPUI offers no way to see a window's messages, so each window is
//! subclassed to watch for it.
//!
//! Subclassing and the message calls have no safe wrapper, so they are
//! `unsafe`. The subclass procedure only reads the message number, sets a
//! flag, and passes every message on; the poll loop does the work.
#![allow(unsafe_code)]

use std::sync::{
    OnceLock,
    atomic::{AtomicBool, Ordering},
};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        UI::{
            Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
            WindowsAndMessaging::{
                ChangeWindowMessageFilterEx, MSGFLT_ALLOW, RegisterWindowMessageW, WM_NCDESTROY,
            },
        },
    },
    core::w,
};

/// This subclass among any other the window has.
const SUBCLASS: usize = 0x6865_7264; // "herd"

/// Set by any window's subclass when its button was created.
static CREATED: AtomicBool = AtomicBool::new(false);

/// The message number, registered once per session by name.
fn message() -> u32 {
    static MESSAGE: OnceLock<u32> = OnceLock::new();
    // SAFETY: registers a static, NUL-terminated name; returns 0 on failure,
    // which no message carries.
    *MESSAGE.get_or_init(|| unsafe { RegisterWindowMessageW(w!("TaskbarButtonCreated")) }) // i18n: data
}

/// Starts watching `hwnd`, a top-level window of this thread.
pub(super) fn watch(hwnd: HWND) {
    let message = message();
    if message == 0 {
        return;
    }
    // SAFETY: `hwnd` is a live window of this thread; the procedure is a
    // `'static` function and removes itself when the window is destroyed.
    unsafe {
        // An elevated app would otherwise never hear Explorer, which runs
        // at medium integrity.
        let _ = ChangeWindowMessageFilterEx(hwnd, message, MSGFLT_ALLOW, None);
        if !SetWindowSubclass(hwnd, Some(procedure), SUBCLASS, 0).as_bool() {
            tracing::debug!("taskbar button changes are not watched for a window");
        }
    }
}

/// Whether a button was created since the last call.
pub(super) fn take() -> bool {
    CREATED.swap(false, Ordering::Relaxed)
}

unsafe extern "system" fn procedure(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    if msg != 0 && msg == message() {
        CREATED.store(true, Ordering::Relaxed);
    }
    if msg == WM_NCDESTROY {
        // SAFETY: removes this very subclass from the window being destroyed.
        let _ = unsafe { RemoveWindowSubclass(hwnd, Some(procedure), SUBCLASS) };
    }
    // SAFETY: passes the message on unchanged, as a subclass must.
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}
