//! The agent's own Herdr integration reports a session for the pane, which
//! makes Herdr ignore this chat's reports until it takes the pane back.

#![cfg(unix)]

use super::*;
use std::os::unix::net::UnixListener;

/// Starts a fake API whose pane holds a session from `holder` until this
/// source's authority is cleared, and returns the requests it receives.
fn fake(
    holder: &'static str,
) -> (
    crossbeam_channel::Receiver<Value>,
    Reporter,
    tempfile::TempDir,
) {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("api.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let (sender, requests) = crossbeam_channel::unbounded::<Value>();
    thread::spawn(move || {
        let mut claimed = true;
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut line = String::new();
            BufReader::new(stream.try_clone().unwrap())
                .read_line(&mut line)
                .unwrap();
            let request: Value = serde_json::from_str(&line).unwrap();
            let id = request["id"].clone();
            let result = match request["method"].as_str().unwrap() {
                "pane.get" if claimed => json!({"type": "pane_info", "pane": {"tab_id": "t_1",
                    "agent_session": {"source": holder, "agent": "claude"}}}),
                "pane.get" => json!({"type": "pane_info", "pane": {"tab_id": "t_1"}}),
                "pane.clear_agent_authority" => {
                    claimed = false;
                    json!({"type": "ok"})
                }
                _ => json!({"type": "ok"}),
            };
            sender.send(request).unwrap();
            writeln!(stream, "{}", json!({"id": id, "result": result})).unwrap();
        }
    });
    let reporter = Reporter::start(
        Target {
            pane: "p_1".to_owned(),
            socket,
        },
        "Claude".to_owned(),
        None,
    )
    .unwrap();
    (requests, reporter, dir)
}

fn next(requests: &crossbeam_channel::Receiver<Value>) -> Value {
    requests.recv_timeout(Duration::from_secs(10)).unwrap()
}

#[test]
fn a_session_another_source_claimed_is_cleared_and_the_state_sent_again() {
    let (requests, reporter, _dir) = fake("herdr:claude");
    reporter.handle().report(Activity::Working);
    let methods: Vec<_> = (0..4).map(|_| next(&requests)).collect();
    let names: Vec<_> = methods.iter().map(|r| r["method"].clone()).collect();
    assert_eq!(
        names,
        [
            json!("pane.report_agent"),
            json!("pane.get"),
            json!("pane.clear_agent_authority"),
            json!("pane.report_agent")
        ]
    );
    // Only this chat's own authority is cleared, never the other source's.
    assert_eq!(methods[2]["params"]["source"], SOURCE);
    assert_eq!(methods[3]["params"]["state"], "working");
    let seqs: Vec<_> = [0, 2, 3]
        .iter()
        .map(|&i| methods[i]["params"]["seq"].as_u64().unwrap())
        .collect();
    assert!(seqs.windows(2).all(|pair| pair[0] < pair[1]));

    // Once the pane is back, a later state is sent once.
    assert_eq!(next(&requests)["method"], "pane.report_metadata");
    reporter.handle().report(Activity::Idle);
    assert_eq!(next(&requests)["method"], "pane.report_agent");
    assert_eq!(next(&requests)["method"], "pane.get");
    reporter.close();
    let rest: Vec<_> = requests.try_iter().collect();
    assert!(
        rest.iter()
            .all(|request| request["method"] != "pane.clear_agent_authority")
    );
}

#[test]
fn a_session_this_chat_holds_is_kept() {
    let (requests, reporter, _dir) = fake(SOURCE);
    reporter.handle().report(Activity::Working);
    assert_eq!(next(&requests)["method"], "pane.report_agent");
    assert_eq!(next(&requests)["method"], "pane.get");
    assert_eq!(next(&requests)["method"], "pane.report_metadata");
    reporter.close();
    let rest: Vec<_> = requests.try_iter().collect();
    assert!(
        rest.iter()
            .all(|request| request["method"] != "pane.clear_agent_authority")
    );
}
