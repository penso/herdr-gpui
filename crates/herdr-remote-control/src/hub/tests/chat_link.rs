//! A chat link against a fake program on a real private endpoint.
#![cfg(unix)]

use crate::hub::chat::Chat as Link;
use herdr_chat_protocol::{
    Change, ClientKind, ClientMessage, Command, Cursor, Rejection, ServerMessage, Transcript,
    read_frame, write_frame,
};
use herdr_client::private_endpoint::{Connection, Listener};
use std::{
    io::{BufReader, BufWriter, Write},
    time::{Duration, Instant},
};
use tokio::sync::broadcast;

fn cursor(epoch: uuid::Uuid, seq: u64) -> Cursor {
    Cursor { epoch, seq }
}

fn send(writer: &mut BufWriter<impl Write>, message: &ServerMessage) {
    write_frame(writer, message).unwrap();
    writer.flush().unwrap();
}

fn hello(reader: &mut BufReader<impl std::io::Read>) -> Option<Cursor> {
    match read_frame::<_, ClientMessage>(reader).unwrap().unwrap() {
        ClientMessage::Hello { client, since, .. } => {
            assert_eq!(client, ClientKind::Remote);
            since
        }
        other => panic!("expected hello, got {other:?}"),
    }
}

fn welcome() -> ServerMessage {
    ServerMessage::Welcome {
        agent: "Claude".into(),
        pane: None,
        version: herdr_chat_protocol::VERSION,
    }
}

async fn until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_link_follows_answers_commands_and_resumes_from_its_cursor() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("chat.sock");
    let listener = Listener::bind(&path).unwrap();
    let epoch = uuid::Uuid::new_v4();
    let program = std::thread::spawn(move || {
        // First connection: snapshot, one change, then answer a command.
        let (reader, writer, closer) = listener.accept().unwrap().split().unwrap();
        let (mut reader, mut writer) = (BufReader::new(reader), BufWriter::new(writer));
        assert_eq!(hello(&mut reader), None);
        send(&mut writer, &welcome());
        send(
            &mut writer,
            &ServerMessage::Snapshot {
                cursor: cursor(epoch, 0),
                transcript: Box::new(Transcript::new("Claude")),
            },
        );
        send(
            &mut writer,
            &ServerMessage::Change {
                change: Change::Clients { count: 2 },
                cursor: cursor(epoch, 1),
            },
        );
        let ClientMessage::Command { command, id } = read_frame::<_, ClientMessage>(&mut reader)
            .unwrap()
            .unwrap()
        else {
            panic!("expected a command");
        };
        assert_eq!(command, Command::Cancel);
        send(
            &mut writer,
            &ServerMessage::Ack {
                id,
                rejected: Some(Rejection::NotWorking),
            },
        );
        closer.close();
        drop((reader, writer));
        // Second connection: resumes after the change it saw.
        let (reader, writer, _closer) = listener.accept().unwrap().split().unwrap();
        let (mut reader, mut writer) = (BufReader::new(reader), BufWriter::new(writer));
        assert_eq!(hello(&mut reader), Some(cursor(epoch, 1)));
        send(&mut writer, &welcome());
        send(
            &mut writer,
            &ServerMessage::Change {
                change: Change::Clients { count: 3 },
                cursor: cursor(epoch, 2),
            },
        );
        // Hold the connection until the test ends.
        let _ = read_frame::<_, ClientMessage>(&mut reader);
    });

    let (all, mut every) = broadcast::channel(64);
    let link = Link::start("p1".into(), path, all).unwrap();
    until("the first change", || link.view().transcript.clients == 2).await;
    assert!(link.view().connected);
    assert_eq!(every.recv().await.unwrap().0, "p1");

    assert_eq!(
        link.command(Command::Cancel).await.unwrap(),
        Some(Rejection::NotWorking)
    );

    // The second connection resumed from the cursor (the program asserts
    // it) and applied the change it sent.
    until("the resumed change", || link.view().transcript.clients == 3).await;

    link.close();
    drop(Connection::connect(
        dir.path().join("chat.sock").as_path(),
        Duration::from_secs(1),
    ));
    drop(program);
}
