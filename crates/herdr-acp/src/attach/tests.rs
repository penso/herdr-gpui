#![allow(clippy::unwrap_used, clippy::expect_used)]

use crossbeam_channel::Receiver;
use herdr_chat_protocol::{ClientKind, Command, CommandId, Cursor};
use herdr_client::private_endpoint::{Reader, Writer};

use super::*;

mod revisions;

const WAIT: Duration = Duration::from_secs(10);

struct Fixture {
    address: PathBuf,
    inputs: Receiver<Input>,
    /// Held to keep listening; only the Unix test reads it.
    #[cfg_attr(windows, allow(dead_code))]
    server: Option<Server>,
    #[cfg(unix)]
    dir: tempfile::TempDir,
}

fn server() -> Fixture {
    let id = EndpointId::random();
    #[cfg(unix)]
    let dir = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    let address = herdr_chat_protocol::endpoint_path(&dir.path().join("acp"), &id);
    #[cfg(windows)]
    let address = herdr_chat_protocol::endpoint_address(&id).unwrap();
    let (inputs, receiver) = bounded(16);
    let server = Server::listen(&address, inputs).unwrap();
    Fixture {
        address,
        inputs: receiver,
        server: Some(server),
        #[cfg(unix)]
        dir,
    }
}

fn connect(fixture: &Fixture) -> (Reader, Writer) {
    let (mut reader, writer, _closer) = Connection::connect(&fixture.address, WAIT)
        .unwrap()
        .split()
        .unwrap();
    reader.set_timeout(Some(WAIT)).unwrap();
    (reader, writer)
}

fn hello(writer: &mut Writer, version: u32) {
    greet(writer, version, herdr_chat_protocol::REVISION, None);
}

fn greet(writer: &mut Writer, version: u32, revision: u32, since: Option<Cursor>) {
    write_frame(
        writer,
        &ClientMessage::Hello {
            client: ClientKind::Gui,
            revision,
            since,
            version,
        },
    )
    .unwrap();
}

#[test]
fn a_client_attaches_sends_commands_and_detaches() {
    let fixture = server();
    let (mut reader, mut writer) = connect(&fixture);
    hello(&mut writer, VERSION);
    let Input::Attach(attach) = fixture.inputs.recv_timeout(WAIT).unwrap() else {
        panic!("expected attach");
    };
    assert!(!attach.local);
    attach
        .outbox
        .send(ServerMessage::Ack {
            id: CommandId::random(),
            rejected: None,
        })
        .unwrap();
    let message = read_frame::<_, ServerMessage>(&mut reader).unwrap();
    assert!(matches!(message, Some(ServerMessage::Ack { .. })));

    let id = CommandId::random();
    write_frame(
        &mut writer,
        &ClientMessage::Command {
            command: Command::Cancel,
            id,
        },
    )
    .unwrap();
    let Input::Command {
        command,
        conn,
        id: received,
    } = fixture.inputs.recv_timeout(WAIT).unwrap()
    else {
        panic!("expected a command");
    };
    assert_eq!(
        (command, conn, received),
        (Command::Cancel, attach.conn, id)
    );
    drop((reader, writer));
    assert!(matches!(
        fixture.inputs.recv_timeout(WAIT).unwrap(),
        Input::Detach(conn) if conn == attach.conn
    ));
}

#[test]
fn another_protocol_version_is_turned_away() {
    let fixture = server();
    let (mut reader, mut writer) = connect(&fixture);
    hello(&mut writer, VERSION + 1);
    let message = read_frame::<_, ServerMessage>(&mut reader).unwrap();
    assert_eq!(
        message,
        Some(ServerMessage::Bye {
            reason: ByeReason::Version { supported: VERSION }
        })
    );
    assert!(fixture.inputs.try_recv().is_err());
}

#[test]
fn a_command_before_hello_is_a_handshake_error() {
    let fixture = server();
    let (mut reader, mut writer) = connect(&fixture);
    write_frame(
        &mut writer,
        &ClientMessage::Command {
            command: Command::Cancel,
            id: CommandId::random(),
        },
    )
    .unwrap();
    let message = read_frame::<_, ServerMessage>(&mut reader).unwrap();
    assert_eq!(
        message,
        Some(ServerMessage::Bye {
            reason: ByeReason::Handshake
        })
    );
}

#[test]
fn dropping_the_session_side_closes_the_connection() {
    let fixture = server();
    let (mut reader, mut writer) = connect(&fixture);
    hello(&mut writer, VERSION);
    let Input::Attach(attach) = fixture.inputs.recv_timeout(WAIT).unwrap() else {
        panic!("expected attach");
    };
    drop(attach);
    assert_eq!(read_frame::<_, ServerMessage>(&mut reader).unwrap(), None);
}

// The dead endpoint is answered by the probe rather than made by binding a
// socket and closing it: on macOS a child another test starts meanwhile can
// inherit that listener before it is marked close-on-exec, and keep it
// listening.
#[cfg(unix)]
#[test]
fn stale_sockets_are_swept_and_the_live_one_removed_on_drop() {
    use std::io;
    let mut fixture = server();
    let chat = fixture.dir.path().join("acp");
    let stale = herdr_chat_protocol::endpoint_path(&chat, &EndpointId::random());
    std::fs::write(&stale, "").unwrap();
    let busy = herdr_chat_protocol::endpoint_path(&chat, &EndpointId::random());
    std::fs::write(&busy, "").unwrap();
    let other = chat.join("notes.sock");
    std::fs::write(&other, "").unwrap();
    sweep_with(&chat, |path| {
        if path == stale || path == other {
            Err(io::ErrorKind::ConnectionRefused.into())
        } else if path == busy {
            Err(io::ErrorKind::WouldBlock.into())
        } else {
            Ok(())
        }
    });
    assert!(!stale.exists());
    assert!(busy.exists(), "only a refusal means nobody listens");
    assert!(other.exists(), "only endpoint sockets are swept");
    assert!(fixture.address.exists(), "a live socket is kept");
    fixture.server.take();
    assert!(!fixture.address.exists());
}

#[test]
fn a_command_from_a_newer_client_is_refused_and_the_connection_stays() {
    let fixture = server();
    let (_reader, mut writer) = connect(&fixture);
    hello(&mut writer, VERSION);
    let Input::Attach(attach) = fixture.inputs.recv_timeout(WAIT).unwrap() else {
        panic!("expected attach");
    };
    let id = CommandId::random();
    write_frame(
        &mut writer,
        &serde_json::json!({"type": "command", "id": id,
            "command": {"command": "teleport", "to": "mars"}}),
    )
    .unwrap();
    let Input::Unsupported { conn, id: refused } = fixture.inputs.recv_timeout(WAIT).unwrap()
    else {
        panic!("expected a refusal");
    };
    assert_eq!((conn, refused), (attach.conn, id));
    // The connection still carries commands it knows.
    let next = CommandId::random();
    write_frame(
        &mut writer,
        &ClientMessage::Command {
            command: Command::Cancel,
            id: next,
        },
    )
    .unwrap();
    let Input::Command { id: received, .. } = fixture.inputs.recv_timeout(WAIT).unwrap() else {
        panic!("expected a command");
    };
    assert_eq!(received, next);
}

#[test]
fn a_frame_that_is_no_command_still_ends_the_connection() {
    assert!(matches!(
        command_frame(serde_json::json!({"type": "command", "command": {"command": "x"}})),
        Frame::Invalid
    ));
    assert!(matches!(
        command_frame(serde_json::json!([1, 2])),
        Frame::Invalid
    ));
}
