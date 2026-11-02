//! A chat subscription against a real chat program: a cached transcript
//! resumes without a copy, anything else gets a snapshot, and a burst of
//! changes arrives gathered.

use super::*;
use herdr_chat_protocol::{
    Change, ClientMessage as ChatClient, Cursor, EntryId, ServerMessage as ChatServer, Transcript,
    VERSION, read_frame, write_frame,
};
use std::{os::unix::net::UnixListener, sync::mpsc as std_mpsc};
use wire::View;

const BURST: u64 = 5;

/// A chat program whose snapshot is at `cursor` with one agent entry; each
/// `()` on `burst` sends [`BURST`] appends to it back to back.
fn serve_chat(listener: UnixListener, cursor: Cursor, burst: std_mpsc::Receiver<()>) {
    std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
        let mut writer = stream;
        let _: Option<ChatClient> = read_frame(&mut reader).unwrap();
        let mut transcript = Transcript::new("Claude");
        transcript.entries = vec![herdr_chat_protocol::Entry {
            id: EntryId(1),
            kind: herdr_chat_protocol::EntryKind::Agent { text: "a".into() },
        }];
        let welcome = ChatServer::Welcome {
            agent: "Claude".into(),
            pane: Some("p1".into()),
            version: VERSION,
        };
        write_frame(&mut writer, &welcome).unwrap();
        let snapshot = ChatServer::Snapshot {
            cursor,
            transcript: Box::new(transcript),
        };
        write_frame(&mut writer, &snapshot).unwrap();
        let mut seq = cursor.seq;
        while burst.recv().is_ok() {
            for _ in 0..BURST {
                seq += 1;
                let change = ChatServer::Change {
                    change: Change::Append {
                        id: EntryId(1),
                        text: "b".into(),
                    },
                    cursor: Cursor {
                        epoch: cursor.epoch,
                        seq,
                    },
                };
                if write_frame(&mut writer, &change).is_err() {
                    return;
                }
            }
        }
    });
}

/// A session on a hub linked to the chat program, once the hub holds its
/// snapshot.
async fn linked(dir: &std::path::Path, cursor: Cursor) -> (Session, std_mpsc::Sender<()>) {
    let endpoint = EndpointId::random();
    let (burst, bursts) = std_mpsc::channel();
    let socket = herdr_chat_protocol::endpoint_path(dir, &endpoint);
    serve_chat(UnixListener::bind(socket).unwrap(), cursor, bursts);
    let hub = Hub::offline_with_chats(layout(endpoint), dir.to_path_buf());
    tokio::time::timeout(Duration::from_secs(5), async {
        while hub.chat("p1").and_then(|chat| chat.base().1).is_none() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the chat never linked");
    let state = crate::web::tests::state_with(dir, hub);
    (Session::start(state), burst)
}

fn chat(since: Option<Cursor>) -> ClientMessage {
    ClientMessage::Subscribe {
        view: View::Chat { pane: "p1".into() },
        since,
        size: None,
    }
}

#[tokio::test]
async fn a_cached_transcript_resumes_and_anything_else_gets_a_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let cursor = Cursor::new_epoch();
    let (mut session, _burst) = linked(dir.path(), Cursor { seq: 3, ..cursor }).await;
    let current = Cursor { seq: 3, ..cursor };

    session.send(chat(None)).await;
    let ServerMessage::ChatSnapshot(snapshot) = session.next().await else {
        panic!("not a snapshot");
    };
    assert_eq!(snapshot.cursor, Some(current));
    assert_eq!(snapshot.transcript.entries.len(), 1);

    session.send(chat(Some(current))).await;
    assert_eq!(
        session.next().await,
        ServerMessage::ChatResumed {
            pane: "p1".into(),
            cursor: current,
            connected: true,
        }
    );

    for stale in [Cursor { seq: 2, ..cursor }, Cursor::new_epoch()] {
        session.send(chat(Some(stale))).await;
        assert!(matches!(
            session.next().await,
            ServerMessage::ChatSnapshot(_)
        ));
    }
}

#[tokio::test]
async fn a_burst_of_changes_arrives_gathered_and_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let cursor = Cursor::new_epoch();
    let (mut session, burst) = linked(dir.path(), cursor).await;
    session.send(chat(Some(cursor))).await;
    assert!(matches!(
        session.next().await,
        ServerMessage::ChatResumed { .. }
    ));

    burst.send(()).unwrap();
    let mut seqs = Vec::new();
    let mut messages = 0;
    while seqs.len() < BURST as usize {
        let ServerMessage::ChatChanges { pane, changes } = session.next().await else {
            panic!("not changes");
        };
        assert_eq!(pane, "p1");
        messages += 1;
        seqs.extend(changes.iter().map(|step| step.cursor.seq));
    }
    assert_eq!(seqs, (1..=BURST).collect::<Vec<_>>());
    assert!(messages < BURST, "{messages} messages for {BURST} changes");
}
