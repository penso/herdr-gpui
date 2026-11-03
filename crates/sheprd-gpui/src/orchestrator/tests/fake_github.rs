//! A GitHub GraphQL endpoint on loopback for end-to-end tests: it lists one
//! open issue and one open pull request for any repository, answers anything
//! else with an error GraphQL would send, and records the repositories asked
//! for. Calls made with its token reach it instead of GitHub.

use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
};

pub(crate) struct FakeGithub {
    /// `owner/repo` of every listing asked for, in order.
    pub(crate) asked: Arc<Mutex<Vec<String>>>,
}

pub(crate) const ISSUE_TITLE: &str = "Icons are blurry on Windows";
pub(crate) const PULL_TITLE: &str = "Read repositories without git";

impl FakeGithub {
    /// Serves until the test process ends; `token` is routed to it.
    pub(crate) fn start(token: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/graphql", listener.local_addr().unwrap());
        crate::github::fake_graphql(token, url);
        let asked = Arc::new(Mutex::new(Vec::new()));
        let log = asked.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let log = log.clone();
                std::thread::spawn(move || serve(stream, &log));
            }
        });
        Self { asked }
    }
}

fn serve(stream: TcpStream, asked: &Mutex<Vec<String>>) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut length = 0;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            return;
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0; length];
    if reader.read_exact(&mut body).is_err() {
        return;
    }
    let request: Value = serde_json::from_slice(&body).unwrap_or_default();
    let reply = answer(&request, asked).to_string();
    let mut stream = stream;
    let _ = write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
        reply.len()
    );
}

fn answer(request: &Value, asked: &Mutex<Vec<String>>) -> Value {
    let query = request["query"].as_str().unwrap_or_default();
    let variables = &request["variables"];
    let repository = format!(
        "{}/{}",
        variables["owner"].as_str().unwrap_or_default(),
        variables["repo"].as_str().unwrap_or_default()
    );
    let page = |field: &str, nodes: Value| {
        json!({"data": {"repository": {field: {
            "pageInfo": {"hasNextPage": false, "endCursor": null},
            "nodes": nodes,
        }}}})
    };
    if query.contains("issues(") {
        asked.lock().unwrap().push(repository.clone());
        return page("issues", json!([issue(&repository, 7, ISSUE_TITLE)]));
    }
    if query.contains("pullRequests(") {
        let open = variables["states"]
            .as_array()
            .is_some_and(|states| states.iter().any(|state| state == "OPEN"));
        let nodes = if open {
            let mut pull = issue(&repository, 8, PULL_TITLE);
            let fields = pull.as_object_mut().unwrap();
            for (key, value) in [
                ("isDraft", json!(false)),
                ("commits", json!({"totalCount": 1})),
                ("additions", json!(10)),
                ("deletions", json!(2)),
                ("baseRefName", json!("main")),
                ("headRefName", json!("agent/no-git")),
                ("baseRefOid", json!("1".repeat(40))),
                ("headRefOid", json!("2".repeat(40))),
                ("headRepository", json!({"nameWithOwner": repository})),
            ] {
                fields.insert(key.into(), value);
            }
            json!([pull])
        } else {
            json!([])
        };
        return page("pullRequests", nodes);
    }
    // Reviews and checks: a failure the worker keeps the last ones through.
    json!({"errors": [{"message": "not served by the fake"}]})
}

fn issue(repository: &str, number: u64, title: &str) -> Value {
    json!({
        "number": number,
        "title": title,
        "body": "",
        "state": "OPEN",
        "url": format!("https://github.com/{repository}/issues/{number}"),
        "createdAt": "2026-10-01T10:00:00Z",
        "updatedAt": "2026-10-02T10:00:00Z",
        "author": {"login": "penso"},
        "labels": {"nodes": []},
        "comments": {"totalCount": 0},
    })
}
