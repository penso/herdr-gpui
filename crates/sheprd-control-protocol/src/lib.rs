//! The wire between `herdr-gpui <command>` and the running app: request and
//! response types, and the bounded socket (a named pipe on Windows) that
//! carries one JSON line each way. No GPUI: the app answers requests it
//! takes from a [`socket::Server`] on its UI thread.

mod error;
pub mod protocol;
pub mod socket;

pub use error::{Error, Result};
pub use protocol::{Placement, app::ActivityPanel};
