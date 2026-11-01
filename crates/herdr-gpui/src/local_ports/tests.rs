#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

mod en_xa;
#[cfg(unix)]
mod process;
#[cfg(unix)]
mod scan;
mod section;
mod state;

/// A listener of pid `pid` on `number`, started at `started`.
pub(super) fn listener(number: u16, pid: u32, started: u64) -> Listener {
    Listener {
        identity: Identity { pid, started },
        number,
        bind: Bind::Loopback,
        process: "node".into(),
        directory: Some(PathBuf::from("/home/dev/app")),
        paused: false,
    }
}
