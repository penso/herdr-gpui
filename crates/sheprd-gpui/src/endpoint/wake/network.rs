//! The macOS network path monitor: counts each time a usable path comes back
//! after there was none, such as Wi-Fi joining after a wake, apart from each
//! time a usable path is reported again, such as the primary interface
//! moving or a VPN coming up, so the window can tell how much to recover.
//!
//! Network.framework has no maintained Rust binding (`objc2-network` is only
//! a reserved name), so its five C functions are declared here, and calling
//! them needs `unsafe`. The monitor runs on its own serial queue and only
//! touches atomics; the window reads the counters on its poll.

use super::NetworkChanges;
use block2::RcBlock;
use dispatch2::DispatchQueue;
use std::{
    ffi::{c_int, c_void},
    sync::{
        Once,
        atomic::{AtomicU8, AtomicU64, Ordering},
    },
};

/// `nw_path_status_satisfied`: the path can be used to send and receive.
const SATISFIED: c_int = 1;

/// The last status the monitor reported, so a report can be told apart from
/// the one before it.
const UNREPORTED: u8 = 0;
const UNUSABLE: u8 = 1;
const USABLE: u8 = 2;

static START: Once = Once::new();
static RESTORED: AtomicU64 = AtomicU64::new(0);
static CHANGED: AtomicU64 = AtomicU64::new(0);

#[allow(unsafe_code)]
#[link(name = "Network", kind = "framework")]
unsafe extern "C" {
    fn nw_path_monitor_create() -> *mut c_void;
    fn nw_path_monitor_set_queue(monitor: *mut c_void, queue: &DispatchQueue);
    fn nw_path_monitor_set_update_handler(
        monitor: *mut c_void,
        handler: &block2::Block<dyn Fn(*mut c_void)>,
    );
    fn nw_path_monitor_start(monitor: *mut c_void);
    fn nw_path_get_status(path: *mut c_void) -> c_int;
}

/// Starts the monitor once for the life of the process. Until it is started,
/// no change is ever counted.
pub(super) fn watch() {
    START.call_once(start);
}

/// The changes counted since the monitor started.
pub(super) fn changes() -> NetworkChanges {
    NetworkChanges {
        restored: RESTORED.load(Ordering::Acquire),
        changed: CHANGED.load(Ordering::Acquire),
    }
}

#[allow(unsafe_code)]
fn start() {
    let queue = DispatchQueue::new("herdr-network-path", None);
    let last = AtomicU8::new(UNREPORTED);
    let handler = RcBlock::new(move |path: *mut c_void| {
        // SAFETY: the monitor passes a valid path object that lives for the
        // duration of this handler call.
        let usable = unsafe { nw_path_get_status(path) } == SATISFIED;
        let previous = last.swap(if usable { USABLE } else { UNUSABLE }, Ordering::AcqRel);
        // The first report is the path already in use when the monitor
        // started, and an unusable path is nothing to recover over.
        let counter = match (previous, usable) {
            (UNUSABLE, true) => &RESTORED,
            (USABLE, true) => &CHANGED,
            _ => return,
        };
        counter.fetch_add(1, Ordering::AcqRel);
        tracing::debug!(
            category = "network",
            restored = previous == UNUSABLE,
            "network path changed"
        );
    });
    // SAFETY: `nw_path_monitor_create` returns a new, owned monitor or null;
    // a null one is never used. The queue and handler are copied or retained
    // by the monitor before these calls return. The monitor is never
    // cancelled or released: it runs for the life of the process, as `START`
    // makes it once.
    unsafe {
        let monitor = nw_path_monitor_create();
        if monitor.is_null() {
            tracing::warn!(category = "network", "could not watch network changes");
            return;
        }
        nw_path_monitor_set_queue(monitor, &queue);
        nw_path_monitor_set_update_handler(monitor, &handler);
        nw_path_monitor_start(monitor);
    }
}
