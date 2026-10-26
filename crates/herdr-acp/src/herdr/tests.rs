#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

mod naming;
mod titles;

#[test]
fn sequence_numbers_keep_increasing() {
    let mut sequence = Sequence::default();
    let first = sequence.next();
    let second = sequence.next();
    assert!(second > first);
    let mut restarted = Sequence::default();
    assert!(restarted.next() >= first);
}

#[cfg(unix)]
#[test]
fn reports_announce_the_agent_and_token_then_withdraw_them() {
    use std::os::unix::net::UnixListener;

    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("api.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let (sender, receiver) = crossbeam_channel::unbounded::<Value>();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut line = String::new();
            BufReader::new(stream.try_clone().unwrap())
                .read_line(&mut line)
                .unwrap();
            let request: Value = serde_json::from_str(&line).unwrap();
            let id = request["id"].clone();
            sender.send(request).unwrap();
            writeln!(stream, "{}", json!({"id": id, "result": {"type": "ok"}})).unwrap();
        }
    });
    let reporter = Reporter::start(
        Target {
            pane: "p_1".to_owned(),
            socket,
        },
        "Claude".to_owned(),
        Some("0123456789abcdef0123456789abcdef".to_owned()),
    )
    .unwrap();
    reporter.handle().report(Activity::Working);
    let report = receiver.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(report["method"], "pane.report_agent");
    assert_eq!(report["params"]["state"], "working");
    assert_eq!(report["params"]["pane_id"], "p_1");
    let metadata = receiver.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(metadata["method"], "pane.report_metadata");
    assert_eq!(metadata["params"]["display_agent"], "Claude");
    assert_eq!(
        metadata["params"]["tokens"]["acp"],
        "0123456789abcdef0123456789abcdef"
    );
    assert!(metadata["params"]["seq"].as_u64() > report["params"]["seq"].as_u64());
    reporter.close();
    let methods: Vec<_> = receiver
        .try_iter()
        .map(|request| {
            (
                request["method"].clone(),
                request["params"]["tokens"].clone(),
            )
        })
        .collect();
    assert_eq!(
        methods,
        [
            (json!("pane.report_metadata"), json!({"acp": null})),
            (json!("pane.release_agent"), Value::Null)
        ]
    );
}

#[cfg(unix)]
#[test]
fn an_api_error_is_typed() {
    use std::os::unix::net::UnixListener;

    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("api.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut line = String::new();
        BufReader::new(stream.try_clone().unwrap())
            .read_line(&mut line)
            .unwrap();
        writeln!(
            stream,
            r#"{{"id":"x","error":{{"code":"pane_not_found"}}}}"#
        )
        .unwrap();
    });
    let error = call(&socket, "pane.report_agent", json!({})).unwrap_err();
    assert!(matches!(error, Error::HerdrApi(message) if message.contains("pane_not_found")));
}

#[cfg(unix)]
#[test]
fn a_failed_announcement_is_sent_again() {
    use std::os::unix::net::UnixListener;

    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("api.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let (sender, receiver) = crossbeam_channel::unbounded::<Value>();
    thread::spawn(move || {
        let mut metadata = 0;
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut line = String::new();
            BufReader::new(stream.try_clone().unwrap())
                .read_line(&mut line)
                .unwrap();
            let request: Value = serde_json::from_str(&line).unwrap();
            let id = request["id"].clone();
            // The first announcement fails, as a slow or busy daemon's would.
            let failed = request["method"] == "pane.report_metadata" && {
                metadata += 1;
                metadata == 1
            };
            sender.send(request).unwrap();
            if failed {
                writeln!(stream, "{}", json!({"id": id, "error": {"code": "busy"}})).unwrap();
            } else {
                writeln!(stream, "{}", json!({"id": id, "result": {"type": "ok"}})).unwrap();
            }
        }
    });
    let reporter = Reporter::start(
        Target {
            pane: "p_1".to_owned(),
            socket,
        },
        "Claude".to_owned(),
        Some("0123456789abcdef0123456789abcdef".to_owned()),
    )
    .unwrap();
    reporter.handle().report(Activity::Working);
    let wait = Duration::from_secs(10);
    let methods: Vec<_> = (0..3)
        .map(|_| receiver.recv_timeout(wait).unwrap()["method"].clone())
        .collect();
    // No further activity: the retry alone sends the token again.
    assert_eq!(
        methods,
        [
            json!("pane.report_agent"),
            json!("pane.report_metadata"),
            json!("pane.report_metadata")
        ]
    );
    reporter.close();
}
