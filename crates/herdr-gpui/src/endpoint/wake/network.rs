//! The macOS network path monitor: counts each time a usable path appears or
//! changes, such as Wi-Fi joining after a wake or the primary interface
//! moving, so the window can recover connections then.
//!
//! Network.framework has no binding among this app's dependencies, so its
//! five C functions are declared here, and calling them needs `unsafe`. The
//! monitor runs on its own serial queue and only touches an atomic counter;
//! the window reads that counter on its poll.
#![allow(unsafe_code)]

use block2::RcBlock;
use dispatch2::DispatchQueue;
use std::{
    ffi::{c_int, c_void},
    sync::{
        Once,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

/// `nw_path_status_satisfied`: the path can be used to send and receive.
const SATISFIED: c_int = 1;

static START: Once = Once::new();
static CHANGES: AtomicU64 = AtomicU64::new(0);

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

/// How many times a usable network path has appeared or changed, starting
/// the monitor on the first call. The path the monitor starts on is not a
/// change.
pub(super) fn changes() -> u64 {
    START.call_once(start);
    CHANGES.load(Ordering::Acquire)
}

fn start() {
    let queue = DispatchQueue::new("herdr-network-path", None);
    let started = AtomicBool::new(false);
    let handler = RcBlock::new(move |path: *mut c_void| {
        // SAFETY: the monitor passes a valid path object that lives for the
        // duration of this handler call.
        let satisfied = unsafe { nw_path_get_status(path) } == SATISFIED;
        // The first report is the path already in use when the monitor started.
        if started.swap(true, Ordering::AcqRel) && satisfied {
            CHANGES.fetch_add(1, Ordering::AcqRel);
            tracing::debug!(category = "network", "network path changed");
        }
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
