//! A local/SSH gen1 client. All transport I/O runs on a dedicated worker.
//! No reconnect/replay: commands carry the boot ID of the snapshot they act on.
//! Drain `Client::events` on a GUI background task, never block the UI thread.
#![doc = include_str!("../README.md")]

mod catalog;
mod clipboard;
mod connect;
mod discovery;
mod error;
mod event;
mod frame;
mod handle;
mod limits;
mod method;
mod options;
mod queue;
mod session;
mod sessions;
mod ssh;
mod transport;
mod upload;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;

pub use crossbeam_channel::Receiver;
pub use herdr_protocol as protocol;

pub use catalog::{
    SavedHost, load_saved_host_selection, load_saved_hosts, store_saved_host_selection,
    valid_profile_id,
};
pub use clipboard::{ClipboardImageCancellation, ClipboardImageUpload};
pub use connect::{connect, connect_with_connector, connect_with_surface_active};
pub use discovery::{ConnectTarget, session_socket};
/// Error returned when queueing commands; also available as the crate's `Error`.
pub use error::Error as SendError;
pub use error::{Error, Result, StorageOperation};
pub use event::ClientEvent;
pub use handle::{Client, ClientHandle};
pub use method::Method;
pub use options::ConnectOptions;
pub use sessions::{
    LocalSession, RemoteSession, SessionState, delete_local_session, delete_remote_session,
    list_local_sessions, list_remote_sessions,
};
#[cfg(unix)]
pub use ssh::script_command;
pub use ssh::{Destination, HostProbe, probe_host, remote_origin_url, resolve_destination};
pub use transport::Stream;
pub use upload::{remove_uploaded_files, upload_files};
