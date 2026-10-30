//! The endpoint Herdr UI attaches to: one listener, and per connection a
//! reader that forwards commands and a writer that drains a bounded queue.

use std::{
    io::{BufReader, BufWriter},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::Duration,
};

use crossbeam_channel::{Sender, bounded};
use herdr_chat_protocol::{
    ByeReason, ClientMessage, Command, CommandId, EndpointId, ServerMessage, VERSION, read_frame,
    write_frame,
};
use herdr_client::private_endpoint::{Connection, Listener, release};

use crate::{
    Error, Result,
    session::{Attach, ConnId, Input},
};

mod downgrade;
pub(crate) use downgrade::Downgrade;

/// Messages queued for one client before it counts as lagging.
const CLIENT_QUEUE: usize = 4096;
const HELLO_TIMEOUT: Duration = Duration::from_secs(5);

/// Connection ids; 0 is the program's own view.
static NEXT_CONN: AtomicU64 = AtomicU64::new(1);

pub(crate) struct Server {
    address: PathBuf,
}

impl Server {
    /// Listens at the chat's address; on Unix it first removes sockets that
    /// chats which did not clean up left behind.
    pub(crate) fn listen(address: &Path, inputs: Sender<Input>) -> Result<Self> {
        #[cfg(unix)]
        if let Some(dir) = address.parent() {
            sweep(dir);
        }
        let listener = Listener::bind(address).map_err(|source| Error::Bind {
            path: address.to_owned(),
            source,
        })?;
        // The accept loop blocks for the life of the process; it is not joined.
        thread::Builder::new()
            .name("chat-accept".to_owned())
            .spawn(move || {
                loop {
                    let Ok(connection) = listener.accept() else {
                        continue;
                    };
                    let inputs = inputs.clone();
                    let _ =
                        thread::Builder::new()
                            .name("chat-client".to_owned())
                            .spawn(move || {
                                let _ = serve(connection, &inputs);
                            });
                }
            })?;
        Ok(Self {
            address: address.to_owned(),
        })
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        release(&self.address);
    }
}

/// Stale sockets looked at when starting.
#[cfg(unix)]
const SWEEP: usize = 256;

/// Removes sockets left by chats that ended without cleaning up.
#[cfg(unix)]
pub(crate) fn sweep(dir: &Path) {
    sweep_with(dir, |path| {
        std::os::unix::net::UnixStream::connect(path).map(drop)
    });
}

/// [`sweep`], probing each endpoint with `connect`: one that refuses has
/// nobody listening and is removed.
#[cfg(unix)]
fn sweep_with(dir: &Path, mut connect: impl FnMut(&Path) -> std::io::Result<()>) {
    use std::io;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten().take(SWEEP) {
        let path = entry.path();
        let is_endpoint = path.extension().is_some_and(|ext| ext == "sock")
            && path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .is_some_and(|stem| stem.parse::<EndpointId>().is_ok());
        if is_endpoint
            && let Err(error) = connect(&path)
            && error.kind() == io::ErrorKind::ConnectionRefused
        {
            let _ = std::fs::remove_file(&path);
        }
    }
}

fn serve(connection: Connection, inputs: &Sender<Input>) -> Result<()> {
    let (mut reader, writer, closer) = connection.split()?;
    reader.set_timeout(Some(HELLO_TIMEOUT))?;
    let mut reader = BufReader::new(reader);
    let mut writer = BufWriter::new(writer);
    let mut downgrade;
    let since = match read_frame::<_, ClientMessage>(&mut reader) {
        Ok(Some(ClientMessage::Hello {
            revision,
            since,
            version,
            ..
        })) if version == VERSION => {
            downgrade = Downgrade::new(revision);
            // An older client starts from a snapshot, which tells its
            // downgrade what the client holds.
            since.filter(|_| downgrade.current())
        }
        Ok(Some(ClientMessage::Hello { .. })) => {
            let reason = ByeReason::Version { supported: VERSION };
            return Ok(write_frame(&mut writer, &ServerMessage::Bye { reason })?);
        }
        Ok(None) => return Ok(()),
        Ok(Some(_)) | Err(_) => {
            let reason = ByeReason::Handshake;
            return Ok(write_frame(&mut writer, &ServerMessage::Bye { reason })?);
        }
    };
    reader.get_mut().set_timeout(None)?;
    let conn = ConnId(NEXT_CONN.fetch_add(1, Ordering::Relaxed));
    let (outbox, queue) = bounded::<ServerMessage>(CLIENT_QUEUE);
    let writer_closer = closer.try_clone()?;
    thread::Builder::new()
        .name("chat-writer".to_owned())
        .spawn(move || {
            for message in queue {
                let Some(message) = downgrade.message(message) else {
                    break;
                };
                if write_frame(&mut writer, &message).is_err() {
                    break;
                }
            }
            // The session dropped this client, or the client went away; either
            // way the reader must stop too.
            writer_closer.close();
        })?;
    let attach = Attach {
        conn,
        local: false,
        outbox,
        since,
    };
    if inputs.send(Input::Attach(attach)).is_err() {
        return Ok(());
    }
    while let Ok(Some(frame)) = read_frame::<_, serde_json::Value>(&mut reader) {
        let (command, id) = match command_frame(frame) {
            Frame::Command { command, id } => (command, id),
            // A newer client's command is refused, not taken as a broken
            // stream: dropping the connection would only have the client
            // reconnect and send it again.
            Frame::Unknown(id) => {
                if inputs.send(Input::Unsupported { conn, id }).is_err() {
                    break;
                }
                continue;
            }
            Frame::Invalid => break,
        };
        if inputs.send(Input::Command { command, conn, id }).is_err() {
            break;
        }
    }
    let _ = inputs.send(Input::Detach(conn));
    closer.close();
    Ok(())
}

/// What a client sent after its greeting.
enum Frame {
    Command {
        command: Command,
        id: CommandId,
    },
    /// A well-formed command this version does not know.
    Unknown(CommandId),
    /// Anything else, which ends the connection.
    Invalid,
}

fn command_frame(frame: serde_json::Value) -> Frame {
    let id = (frame.get("type").and_then(serde_json::Value::as_str) == Some("command"))
        .then(|| frame.get("id").cloned())
        .flatten()
        .and_then(|id| serde_json::from_value::<CommandId>(id).ok());
    match serde_json::from_value::<ClientMessage>(frame) {
        Ok(ClientMessage::Command { command, id }) => Frame::Command { command, id },
        Ok(ClientMessage::Hello { .. }) => Frame::Invalid,
        Err(_) => id.map_or(Frame::Invalid, Frame::Unknown),
    }
}

/// Where this run listens, and its id for the Herdr token.
pub(crate) fn address() -> Result<(PathBuf, EndpointId)> {
    let id = EndpointId::random();
    let address = herdr_chat_protocol::endpoint_address(&id).ok_or(Error::NoEndpointDir)?;
    Ok((address, id))
}

#[cfg(test)]
mod tests;
