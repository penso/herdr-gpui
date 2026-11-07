//! `/api/v2/tags`: a phone reads and edits the computer's tags.

use super::*;
use crate::control::tags::ComputerTags;
use serde_json::{Value, json};

fn with_bearer(token: &str, mut request: HttpRequest<Body>) -> HttpRequest<Body> {
    request.headers_mut().insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
    );
    request
}

fn put(body: &Value) -> HttpRequest<Body> {
    HttpRequest::put("/api/v2/tags")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ORIGIN, ORIGIN)
        .body(Body::from(body.to_string()))
        .unwrap()
}

#[tokio::test]
async fn tags_need_a_session() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    assert_eq!(
        send(&state, get("/api/v2/tags")).await.0,
        StatusCode::UNAUTHORIZED
    );
    let offer = json!({"tags": ["work"], "updated_ms": 1});
    assert_eq!(send(&state, put(&offer)).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(state.tags.current(), ComputerTags::default());
}

#[tokio::test]
async fn a_phone_edits_the_tags_and_the_latest_edit_stands() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let (_, token) = signed_in(&state, 1);

    let offer = json!({"tags": ["work", "WORK", " lab "], "updated_ms": 10});
    let (status, _, body) = send(&state, with_bearer(&token, put(&offer))).await;
    assert_eq!(status, StatusCode::OK);
    let stood: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(stood, json!({"tags": ["work", "lab"], "updated_ms": 10}));

    let stale = json!({"tags": ["personal"], "updated_ms": 5});
    let (_, _, body) = send(&state, with_bearer(&token, put(&stale))).await;
    let stood: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(stood["tags"], json!(["work", "lab"]));

    let (_, _, body) = send(&state, with_bearer(&token, get("/api/v2/tags"))).await;
    let read: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(read["tags"], json!(["work", "lab"]));
    assert_eq!(
        crate::app::tags::load(&crate::app::tags::file(dir.path())).tags,
        ["work", "lab"],
        "an edit is on disk before it is answered"
    );
}

#[test]
fn offers_at_once_leave_the_latest_in_memory_and_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let tags = Arc::new(Tags::load(dir.path()));
    let workers: Vec<_> = (1..=32_u64)
        .map(|edit| {
            let tags = tags.clone();
            std::thread::spawn(move || {
                let offer = ComputerTags::new([format!("tag{edit}")], edit);
                tags.offer(offer, 1_000).unwrap();
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    let latest = ComputerTags::new(["tag32"], 32);
    assert_eq!(tags.current(), latest);
    assert_eq!(
        crate::app::tags::load(&crate::app::tags::file(dir.path())),
        latest
    );
}
