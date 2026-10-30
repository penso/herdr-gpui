//! A local/SSH/WSL gen1 client. All transport I/O runs on a dedicated worker.
//! No reconnect/replay: commands carry the boot ID of the snapshot they act on.
//! Drain `Client::events` on a GUI background task, never block the UI thread.
#![doc = include_str!("../README.md")]
// Tests start real processes; production code builds them with `new_command`.
#![cfg_attr(test, allow(clippy::disallowed_methods))]

mod catalog;
pub mod clipboard;
mod compat;
mod connect;
#[cfg(unix)]
mod control_dir;
mod discovery;
mod environment;
mod error;
mod event;
mod forward;
mod frame;
mod handle;
mod host_theme;
mod limits;
mod local_network;
pub mod local_path;
pub mod login_env;
mod method;
mod options;
pub mod private_endpoint;
mod process;
mod queue;
mod remote_host;
mod script;
pub mod scrollback;
mod session;
pub mod sessions;
pub mod ssh;
mod surface_images;
pub mod text;
mod transport;
mod upload;
pub mod workspaces;
pub mod wsl;

#[cfg(all(test, unix))]
#[path = "../../test-support/executable.rs"]
mod test_executable;
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;

pub use crossbeam_channel::Receiver;
pub use herdr_protocol as protocol;

pub use catalog::{
    SavedHost, load_saved_host_selection, load_saved_hosts, store_saved_host,
    store_saved_host_selection, valid_profile_id,
};
pub use clipboard::{ClipboardImageCancellation, ClipboardImageUpload};
pub use compat::{MIN_HERDR_VERSION, VersionMismatch};
pub use connect::{Transport, connect, connect_with_connector, connect_with_surface_active};
#[cfg(feature = "cloud")]
pub use discovery::CloudProvider;
pub use discovery::{ConnectTarget, local_var, session_socket, set_login_config_home};
pub use environment::set_command_environment;
/// Error returned when queueing commands; also available as the crate's `Error`.
pub use error::Error as SendError;
pub use error::{Error, Result, StorageOperation};
pub use event::ClientEvent;
pub use forward::{ForwardEvent, PortForward, preferred_local_port};
pub use handle::{Client, ClientHandle};
pub use host_theme::HostTheme;
pub use local_network::{LocalNetworkAccess, local_network_access, wait_for_local_network};
pub use method::Method;
pub use options::ConnectOptions;
#[cfg(windows)]
pub use process::CREATE_NO_WINDOW;
pub use process::new_command;
pub use remote_host::RemoteHost;
pub use script::{ScriptHost, ScriptLimits, run_script, run_script_command, shell_quote};
pub use sessions::{
    LocalSession, RemoteSession, SessionState, delete_local_session, delete_remote_session,
    list_local_sessions, list_remote_sessions,
};
pub use ssh::mux::Lane as SshLane;
pub use ssh::{Bridge, SshChannel, connect_command, script_command, shared_script_command};
pub use ssh::{
    Destination, FORWARD_READY, HostKeyFingerprint, HostKeyScan, HostKeys, HostProbe,
    MAX_UNVERIFIED_HOSTS, NewHostKey, SshFailure, forward_command, probe_host, probe_host_with,
    remote_config_value, remote_origin_url, resolve_destination, scan_host_key, set_host_keys,
    set_unverified_hosts, trust_host_key,
};
pub use surface_images::{
    MAX_IMAGE_BYTES, MAX_IMAGE_SIDE, MAX_IMAGES, MAX_PLACEMENTS, SurfaceImage, SurfaceImages,
    valid_asset,
};
pub use transport::Stream;
#[cfg(windows)]
pub use transport::pipe_available;
pub use upload::{remove_uploaded_files, upload_files};
pub use wsl::{
    WslHost, WslHosts, add_wsl_host, delete_distro_session, list_distro_sessions, list_distros,
    load_wsl_hosts, probe_distro, remove_wsl_host, store_wsl_selection, valid_distro, wsl_paths,
    wsl_shell_command,
};
