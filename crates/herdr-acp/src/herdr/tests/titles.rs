//! A sent prompt titles the pane and names its tab through a fake Herdr
//! API that keeps one tab's label.

#![cfg(unix)]

use super::*;
use std::os::unix::net::UnixListener;

struct Fake {
    label: Arc<Mutex<String>>,
    requests: crossbeam_channel::Receiver<Value>,
    _dir: tempfile::TempDir,
}

/// Starts a fake API whose pane `p_1` is alone in tab `t_1`, labelled `label`.
fn fake(label: &str) -> (Fake, Reporter) {
    failing(label, 0)
}

/// As `fake`, with the first `failures` renames answered by an error.
fn failing(label: &str, failures: u32) -> (Fake, Reporter) {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("api.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let (sender, requests) = crossbeam_channel::unbounded::<Value>();
    let label = Arc::new(Mutex::new(label.to_owned()));
    let shared = Arc::clone(&label);
    let mut failures = failures;
    thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut line = String::new();
            BufReader::new(stream.try_clone().unwrap())
                .read_line(&mut line)
                .unwrap();
            let request: Value = serde_json::from_str(&line).unwrap();
            let id = request["id"].clone();
            if request["method"] == "tab.rename" && failures > 0 {
                failures -= 1;
                sender.send(request).unwrap();
                writeln!(stream, "{}", json!({"id": id, "error": {"code": "busy"}})).unwrap();
                continue;
            }
            let result = match request["method"].as_str().unwrap() {
                "pane.get" => json!({"type": "pane_info", "pane": {"tab_id": "t_1"}}),
                "tab.get" => json!({"type": "tab_info", "tab": {"tab_id": "t_1",
                    "label": *shared.lock().unwrap(), "pane_count": 1}}),
                "tab.rename" => {
                    *shared.lock().unwrap() = request["params"]["label"].as_str().unwrap().into();
                    json!({"type": "tab_info"})
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
    reporter.handle().report(Activity::Idle);
    let fake = Fake {
        label,
        requests,
        _dir: dir,
    };
    fake.until("pane.report_metadata");
    (fake, reporter)
}

impl Fake {
    /// The requests up to and including the next `method`.
    fn until(&self, method: &str) -> Vec<Value> {
        let mut seen = Vec::new();
        loop {
            let request = self.requests.recv_timeout(Duration::from_secs(10)).unwrap();
            let found = request["method"] == method;
            seen.push(request);
            if found {
                return seen;
            }
        }
    }

    fn label(&self) -> String {
        self.label.lock().unwrap().clone()
    }
}

#[test]
fn a_prompt_titles_the_pane_and_names_the_tab() {
    let (fake, reporter) = fake("Claude");
    reporter.handle().prompt("fix the\ntab title".to_owned());
    let seen = fake.until("tab.rename");
    let title = seen
        .iter()
        .find(|request| request["params"]["title"].is_string())
        .unwrap();
    assert_eq!(title["method"], "pane.report_metadata");
    assert_eq!(title["params"]["title"], "fix the tab title");
    assert_eq!(fake.label(), "Claude: fix the tab title");

    reporter.handle().prompt("now the phone".to_owned());
    fake.until("tab.rename");
    assert_eq!(fake.label(), "Claude: now the phone");
    reporter.close();
    assert_eq!(fake.label(), "Claude");
}

#[test]
fn a_label_the_user_gave_stays() {
    let (fake, reporter) = fake("Claude");
    reporter.handle().prompt("first".to_owned());
    fake.until("tab.rename");
    *fake.label.lock().unwrap() = "mine".to_owned();
    reporter.handle().prompt("second".to_owned());
    let seen = fake.until("tab.get");
    assert!(seen.iter().all(|request| request["method"] != "tab.rename"));
    reporter.close();
    assert_eq!(fake.label(), "mine");
    let rest: Vec<_> = fake.requests.try_iter().collect();
    assert!(rest.iter().all(|request| request["method"] != "tab.rename"));
    let cleared = rest
        .iter()
        .find(|request| request["params"]["clear_title"] == true);
    assert!(cleared.is_some());
}

#[test]
fn a_failed_rename_is_tried_again() {
    let (fake, reporter) = failing("Claude", 1);
    reporter.handle().prompt("fix it".to_owned());
    fake.until("tab.rename");
    assert_eq!(fake.label(), "Claude");
    fake.until("tab.rename");
    assert_eq!(fake.label(), "Claude: fix it");
    reporter.close();
}
