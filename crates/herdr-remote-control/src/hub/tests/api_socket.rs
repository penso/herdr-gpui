//! Herdr's public API socket: where it is, and one request on it.

use super::*;
use crate::hub::api::{Api, api_socket};
use std::path::Path;

#[test]
fn the_api_socket_sits_beside_the_client_socket() {
    assert_eq!(
        api_socket(Path::new("/home/me/.config/herdr/herdr-client.sock")),
        Some(PathBuf::from("/home/me/.config/herdr/herdr.sock"))
    );
    assert_eq!(
        api_socket(Path::new("/tmp/test-client.sock")),
        Some(PathBuf::from("/tmp/test.sock"))
    );
    assert_eq!(api_socket(Path::new("/tmp/-client.sock")), None);
    assert_eq!(api_socket(Path::new("/tmp/herdr.sock")), None);
}

#[cfg(unix)]
#[tokio::test]
async fn a_request_is_one_json_line_and_its_result_comes_back() {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixListener;

    // Short, so the socket path fits SUN_LEN.
    let dir = tempfile::Builder::new()
        .prefix("hr")
        .tempdir_in("/tmp")
        .unwrap();
    let listener = UnixListener::bind(dir.path().join("h.sock")).unwrap();
    let server = std::thread::spawn(move || {
        let mut answers = Vec::new();
        for answer in [
            r#"{"id":"remote","result":{"type":"ok"}}"#,
            r#"{"id":"remote","error":{"code":"pane_not_found","message":"pane not found"}}"#,
        ] {
            let (stream, _) = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(&stream).read_line(&mut line).unwrap();
            answers.push(serde_json::from_str::<Value>(&line).unwrap());
            (&stream)
                .write_all(format!("{answer}\n").as_bytes())
                .unwrap();
        }
        answers
    });
    let api = Api::new(Some(dir.path().join("h-client.sock")));
    let ok = api
        .call(
            ApiMethod::PaneSendKeys,
            json!({"pane_id": "w1:p1", "keys": ["ctrl+c"]}),
        )
        .await
        .unwrap();
    assert_eq!(ok, json!({"type": "ok"}));
    let error = api
        .call(ApiMethod::PaneClose, json!({"pane_id": "nope"}))
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Daemon(message) if message == "pane not found"));
    let requests = server.join().unwrap();
    assert_eq!(requests[0]["method"], "pane.send_keys");
    assert_eq!(requests[0]["params"]["keys"][0], "ctrl+c");
    assert_eq!(requests[1]["method"], "pane.close");
}

#[tokio::test]
async fn no_socket_means_herdr_is_not_running() {
    let api = Api::new(Some(PathBuf::from("/nonexistent/herdr-client.sock")));
    let error = api.call(ApiMethod::PaneRead, json!({})).await.unwrap_err();
    assert!(matches!(error, Error::NotConnected));
}
