#![allow(clippy::unwrap_used)]

use super::*;
use crate::web::tests::{post, send, state};
use serde_json::json;

#[test]
fn a_report_is_cut_at_a_character_boundary() {
    assert_eq!(clip("short"), "short");
    let long = "é".repeat(MAX_REPORT_CHARS + 5);
    assert_eq!(clip(&long).chars().count(), MAX_REPORT_CHARS);
}

#[tokio::test]
async fn a_phone_reports_without_a_session_until_its_limit() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let report = || {
        post(
            "/api/log",
            json!({ "context": "pair", "message": "Load failed" }),
        )
    };
    for _ in 0..REPORT_BURST {
        assert_eq!(send(&state, report()).await.0, StatusCode::NO_CONTENT);
    }
    assert_eq!(
        send(&state, report()).await.0,
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[tokio::test]
async fn a_report_must_be_a_context_and_a_message() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    let (status, _, _) = send(&state, post("/api/log", json!({ "message": "x" }))).await;
    assert!(status.is_client_error());
}
