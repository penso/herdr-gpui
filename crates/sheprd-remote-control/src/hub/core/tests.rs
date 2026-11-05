use super::*;
use crate::hub::{Hub, layout};
use serde_json::json;
use sheprd_core_client::Endpoint;

fn record(address: &str) -> Value {
    json!({"address": address, "cols": 80, "rows": 24, "cwd": "/code/app",
        "state": "live", "pid": 123, "status": {"state": "working", "app": "claude"}})
}

#[test]
fn saved_desktop_tabs_keep_split_membership_and_ignore_other_sessions() {
    let saved = serde_json::from_value(json!({"tabs": [
        {"id": 4, "scope": "local:/test.sock", "workspace_id": "w1", "title": "build",
         "location": {"kind": "core_terminal", "panes": {"axis": "row", "first": "t1", "second": "t2"}}},
        {"id": 5, "scope": "local:/other.sock", "workspace_id": "other", "title": "wrong",
         "location": {"kind": "core_terminal", "panes": "t3"}}
    ]})).unwrap();
    let records =
        ["t1", "t2", "t3"].map(|address| serde_json::from_value(record(address)).unwrap());
    let core = projection::project("boot", &records, Some(&saved), Some("local:/test.sock"));
    let tab = &core.workspace("w1").unwrap().terminals[0];
    assert_eq!(tab.label, "build");
    assert_eq!(
        tab.panes
            .iter()
            .map(|pane| pane.pane.clone())
            .collect::<Vec<_>>(),
        vec![id("t1"), id("t2")]
    );
    assert_eq!(
        tab.agent.as_ref().unwrap().status,
        sheprd_client::protocol::AgentStatus::Working
    );
    assert!(core.workspace("other").is_none());
    assert!(core.terminal(&id("t3")).is_some());
    let daemon = layout::project(&crate::hub::tests::snapshot(json!([])));
    let merged = merge(&daemon, &core);
    assert_eq!(merged.workspace("w1").unwrap().label, "app");
    assert_eq!(
        merged.workspace("w1").unwrap().terminals.last().unwrap(),
        tab
    );
    assert_eq!(merged.boot, daemon.boot);
}

#[test]
fn connectivity_is_the_union_of_the_two_sources() {
    for daemon_connected in [false, true] {
        for core_connected in [false, true] {
            let daemon = Layout {
                connected: daemon_connected,
                ..Layout::default()
            };
            let core = Layout {
                connected: core_connected,
                ..Layout::default()
            };
            assert_eq!(
                merge(&daemon, &core).connected,
                daemon_connected || core_connected
            );
        }
    }
}

#[test]
fn merge_counts_working_agents_from_both_sources() {
    let daemon = Layout {
        working: 2,
        ..Layout::default()
    };
    let core = Layout {
        working: 3,
        ..Layout::default()
    };
    assert_eq!(merge(&daemon, &core).working, 5);
}

#[test]
fn pane_ids_do_not_collide_with_the_herdr_one_namespace_or_remote_paths() {
    assert_ne!(id("box/t1"), id("boxt1"));
    assert!(!id("box/t1").contains('/'));
    assert!(owns(&json!({"pane_id": id("t1")})));
    assert!(!owns(&json!({"pane_id": "t1"})));
}

#[cfg(unix)]
mod socket {
    use super::*;
    use sheprd_core_client::{
        Method,
        frame::{Decoder, Frame, Kind, encode},
    };
    use std::{
        io::{BufRead, BufReader, Read, Write},
        os::unix::net::{UnixListener, UnixStream},
        sync::mpsc,
        time::Instant,
    };

    mod recovery;
    mod spaces;

    struct Fake {
        _dir: tempfile::TempDir,
        inputs: mpsc::Receiver<Vec<u8>>,
        server: thread::JoinHandle<()>,
    }

    fn receive(socket: &mut UnixStream, decoder: &mut Decoder) -> Option<Frame> {
        let mut bytes = [0; 4096];
        loop {
            if let Some(frame) = decoder.next_frame().unwrap() {
                return Some(frame);
            }
            match socket.read(&mut bytes) {
                Ok(0) => return None,
                Ok(count) => decoder.push(&bytes[..count]),
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(error) => panic!("{error}"),
            }
        }
    }

    fn fake() -> (Arc<Source>, Fake) {
        fake_with_close(false)
    }

    fn fake_with_close(close: bool) -> (Arc<Source>, Fake) {
        fake_with_doc(close, Value::Null, vec![record("t1")])
    }

    fn fake_with_doc(close: bool, doc: Value, records: Vec<Value>) -> (Arc<Source>, Fake) {
        let dir = tempfile::Builder::new()
            .prefix("rc")
            .tempdir_in("/tmp")
            .unwrap();
        let path = dir.path().join("core.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let (sent, inputs) = mpsc::channel();
        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut hello = String::new();
            BufReader::new(&mut socket).read_line(&mut hello).unwrap();
            let hello: Value = serde_json::from_str(&hello).unwrap();
            let response = json!({"jsonrpc": "2.0", "id": hello["id"], "result": {
                "generation": 1, "version": "0.1.18", "framed": true,
                "core": {"id": "fake-core", "name": "test"},
                "methods": Method::ALL.iter().map(|method| method.as_str()).collect::<Vec<_>>()}});
            writeln!(socket, "{response}").unwrap();
            let mut decoder = Decoder::default();
            let mut channel = 6;
            while let Some(frame) = receive(&mut socket, &mut decoder) {
                if frame.kind != Kind::Data {
                    continue;
                }
                if frame.channel != 0 {
                    sent.send(frame.payload).unwrap();
                    if close {
                        assert_eq!(frame.channel, 8);
                    }
                    socket
                        .write_all(&encode(7, Kind::Data, b"\r\x1b[2Kreceived").unwrap())
                        .unwrap();
                    continue;
                }
                let request: Value = serde_json::from_slice(&frame.payload).unwrap();
                let result = match request["method"].as_str().unwrap() {
                    "terminal.list" => json!({"terminals": records}),
                    "doc.get" => {
                        assert_eq!(request["params"]["name"], "herdr-app");
                        json!({"value": doc, "version": if doc.is_null() { 0 } else { 1 }})
                    }
                    "terminal.attach" => {
                        assert_eq!(request["params"]["terminal"], "t1");
                        assert_eq!(request["params"]["cols"], 80);
                        {
                            channel += 1;
                            json!({"channel": channel, "terminal": record("t1")})
                        }
                    }
                    method => panic!("unexpected method {method}"),
                };
                let response = json!({"jsonrpc": "2.0", "id": request["id"], "result": result});
                socket
                    .write_all(
                        &encode(0, Kind::Data, &serde_json::to_vec(&response).unwrap()).unwrap(),
                    )
                    .unwrap();
                if request["method"] == "terminal.attach" {
                    socket
                        .write_all(
                            &encode(
                                channel,
                                Kind::Data,
                                if channel == 7 {
                                    b"\x1b[31mhello\x1b[0m\x1b[?2004h"
                                } else {
                                    b"recovered"
                                },
                            )
                            .unwrap(),
                        )
                        .unwrap();
                }
                if close && request["method"] == "terminal.attach" && channel == 7 {
                    socket
                        .write_all(&encode(7, Kind::Close, b"").unwrap())
                        .unwrap();
                }
            }
        });
        let core = Core::connect(&Endpoint {
            socket: path,
            session: "test".into(),
        })
        .unwrap();
        let source = Arc::new(Source {
            connected: AtomicBool::new(false),
            endpoint: Default::default(),
            scope: None,
            state: Mutex::new(State {
                core: Some(core),
                ..State::default()
            }),
            tabs: None,
        });
        (
            source,
            Fake {
                _dir: dir,
                inputs,
                server,
            },
        )
    }

    #[tokio::test]
    async fn a_fake_core_alone_lists_a_terminal_and_the_hub_reads_and_types() {
        let (source, fake) = fake();
        let layout = source.poll().unwrap();
        assert!(layout.connected);
        assert_eq!(layout.boot, "core:fake-core");
        let terminal = &layout.workspaces[0].terminals[0];
        assert_eq!(
            terminal.agent.as_ref().unwrap().status,
            sheprd_client::protocol::AgentStatus::Working
        );
        let pane = terminal.panes[0].pane.clone();
        let mut hub = Hub::offline_in(merge(&Layout::default(), &layout), None, None);
        hub.core = Some(source.clone());
        let hub = Arc::new(hub);
        assert!(matches!(
            hub.call(ApiMethod::WorkspaceCreate, json!({"cwd": "/code/app"}))
                .await
                .unwrap_err(),
            HubError::Core(Error::Unsupported)
        ));
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let screen = hub
                .call(ApiMethod::PaneRead, json!({"pane_id": pane}))
                .await
                .unwrap();
            if screen["read"]["text"].as_str().unwrap().contains("hello") {
                let rendered = crate::ansi::parse(screen["read"]["text"].as_str().unwrap());
                assert!(
                    rendered.lines[0]
                        .iter()
                        .any(|(style, text)| style.fg == crate::ansi::Color::Named(1)
                            && text.contains("hello"))
                );
                break;
            }
            assert!(Instant::now() < deadline, "no terminal output through hub");
            tokio::task::yield_now().await;
        }
        hub.call(
            ApiMethod::PaneSendText,
            json!({"pane_id": pane, "text": "work"}),
        )
        .await
        .unwrap();
        assert_eq!(
            fake.inputs.recv_timeout(Duration::from_secs(5)).unwrap(),
            b"\x1b[200~work\x1b[201~"
        );
        hub.call(
            ApiMethod::PaneSendKeys,
            json!({"pane_id": pane, "keys": ["ctrl+c", "enter"]}),
        )
        .await
        .unwrap();
        let mut input = fake.inputs.recv_timeout(Duration::from_secs(5)).unwrap();
        while input.len() < 2 {
            input.extend(fake.inputs.recv_timeout(Duration::from_secs(5)).unwrap());
        }
        assert_eq!(input, b"\x03\r");
        loop {
            let screen = hub
                .call(ApiMethod::PaneRead, json!({"pane_id": pane}))
                .await
                .unwrap();
            if screen["read"]["text"]
                .as_str()
                .unwrap()
                .contains("received")
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "no updated terminal output through hub"
            );
            tokio::task::yield_now().await;
        }
        drop(hub);
        drop(source);
        fake.server.join().unwrap();
    }

    #[tokio::test]
    async fn both_sources_merge_and_only_neither_is_not_connected() {
        let (source, fake) = fake();
        let core = source.poll().unwrap();
        let daemon = layout::project(&crate::hub::tests::snapshot(json!([])));
        let merged = merge(&daemon, &core);
        assert!(merged.connected);
        assert_eq!(merged.workspace("w1"), daemon.workspace("w1"));
        assert!(merged.terminal(&id("t1")).is_some());
        assert!(merge(&Layout::default(), &core).connected);
        assert!(merge(&daemon, &Layout::default()).connected);
        let neither = merge(&Layout::default(), &Layout::default());
        assert!(!neither.connected);
        let hub = Hub::offline(neither);
        assert!(matches!(
            hub.call(ApiMethod::PaneRead, json!({"pane_id": "p1"}))
                .await
                .unwrap_err(),
            HubError::NotConnected
        ));
        drop(source);
        fake.server.join().unwrap();
    }
}

mod state_context;

#[test]
fn a_failed_desktop_resolution_stays_with_the_core_source() {
    let source = Source {
        connected: AtomicBool::new(false),
        endpoint: crate::core_target::CoreTarget::InvalidSession("bad/name".into()),
        scope: None,
        state: Mutex::new(State::default()),
        tabs: None,
    };
    assert!(
        matches!(source.poll(), Err(HubError::Core(Error::Client(sheprd_core_client::Error::Session(name)))) if name == "bad/name")
    );
}
