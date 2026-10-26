//! An agent's answer reaches the phone while it is being written, not
//! only once the turn ends.

use super::*;
use crate::hub::{ChatRow, Workspace};
use futures_lite::StreamExt;
use herdr_chat_protocol::{
    Change, ClientMessage, Cursor, EndpointId, Entry, EntryId, EntryKind, ServerMessage, Status,
    Transcript, VERSION, read_frame, write_frame,
};
use herdr_client::protocol::AgentStatus;
use std::{os::unix::net::UnixListener, sync::mpsc, time::Duration};

const WORDS: usize = 6;

/// A chat program that streams an answer of `WORDS` words, the next once
/// `next` says the phone has the one before, behind a long earlier step.
fn serve_chat(listener: UnixListener, next: mpsc::Receiver<()>) {
    std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
        let mut writer = stream;
        let _: Option<ClientMessage> = read_frame(&mut reader).unwrap();
        let epoch = uuid::Uuid::new_v4();
        let mut transcript = Transcript::new("Claude");
        transcript.status = Status::Working;
        transcript.entries = vec![
            Entry {
                id: EntryId(1),
                kind: EntryKind::User {
                    text: "question".into(),
                    files: 0,
                },
            },
            Entry {
                id: EntryId(2),
                kind: EntryKind::Agent {
                    text: "step ".repeat(4096),
                },
            },
            Entry {
                id: EntryId(3),
                kind: EntryKind::Agent {
                    text: "word0".into(),
                },
            },
        ];
        let send = |writer: &mut std::os::unix::net::UnixStream, message: &ServerMessage| {
            write_frame(writer, message).unwrap();
        };
        send(
            &mut writer,
            &ServerMessage::Welcome {
                agent: "Claude".into(),
                pane: Some("p1".into()),
                version: VERSION,
            },
        );
        send(
            &mut writer,
            &ServerMessage::Snapshot {
                cursor: Cursor { epoch, seq: 0 },
                transcript: Box::new(transcript),
            },
        );
        for word in 1..WORDS {
            if next.recv().is_err() {
                return;
            }
            let message = ServerMessage::Change {
                change: Change::Append {
                    id: EntryId(3),
                    text: format!(" word{word}"),
                },
                cursor: Cursor {
                    epoch,
                    seq: word as u64,
                },
            };
            if write_frame(&mut writer, &message).is_err() {
                return;
            }
        }
        // Keep the connection open while the phone reads.
        let _ = next.recv();
    });
}

#[tokio::test]
async fn an_answer_streams_to_the_phone_as_it_is_written() {
    let dir = tempfile::tempdir().unwrap();
    // The sockets sit in the directory itself, short of the path limit.
    let chats = dir.path().to_path_buf();
    let endpoint = EndpointId::random();
    let (next, waiting) = mpsc::channel();
    serve_chat(
        UnixListener::bind(herdr_chat_protocol::endpoint_path(&chats, &endpoint)).unwrap(),
        waiting,
    );
    let layout = Layout {
        connected: true,
        boot: "b".into(),
        workspaces: vec![Workspace {
            id: "w1".into(),
            label: "Work".into(),
            branch: None,
            cwd: "/".into(),
            chats: vec![ChatRow {
                pane: "p1".into(),
                tab: "t1".into(),
                endpoint,
                agent: Some("claude".into()),
                title: "Chat".into(),
                status: AgentStatus::Working,
            }],
            terminals: Vec::new(),
        }],
    };
    let state = state_with(dir.path(), Hub::offline_with_chats(layout, chats));
    let (_, token) = signed_in(&state, 4);
    let mut request = HttpRequest::get("/api/stream?view=chat&pane=p1")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    request
        .extensions_mut()
        .insert(ConnectInfo(Peer(SocketAddr::from(([10, 0, 0, 4], 50_000)))));
    let response = router(state.clone()).oneshot(request).await.unwrap();
    let mut body = response.into_body().into_data_stream();

    // Reads the next patch that carries the answer, with its last word.
    let mut patch = async || loop {
        let chunk = tokio::time::timeout(Duration::from_secs(5), body.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let chunk = String::from_utf8_lossy(&chunk).into_owned();
        if let Some(word) = (0..WORDS)
            .rev()
            .find(|word| chunk.contains(&format!("word{word}")))
        {
            return (chunk, word);
        }
    };
    let (first, word) = patch().await;
    assert_eq!(word, 0);
    assert!(first.contains(r#"id=\"chat-turn\""#), "{first}");
    for expected in 1..WORDS {
        next.send(()).unwrap();
        let (chunk, word) = patch().await;
        assert_eq!(word, expected);
        // Only the growing entry: not the turn, nor the long step before it.
        assert!(chunk.contains(r#""entry-3":"#), "{chunk}");
        assert!(
            !chunk.contains("chat-turn") && !chunk.contains("entry-2"),
            "{chunk}"
        );
        assert!(chunk.len() < 4096, "{} bytes", chunk.len());
    }
    let _ = next.send(());
}
