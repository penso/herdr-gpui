use super::*;
use crate::cloud::tests::server::serve_raw;
use secrecy::SecretString;
use std::sync::atomic::{AtomicUsize, Ordering};

const CREATED: &str =
    r#"{"sandbox":{"id":"bx_23456789","name":"herdr-box","state":"provisioning"}}"#;

fn reply(status: u16, body: &str, interrupted: bool) -> String {
    let length = body.len() + usize::from(interrupted);
    format!(
        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n{body}"
    )
}

#[test]
fn interrupted_creation_replies_and_pending_retries_recover_the_same_sandbox() {
    let attempts = AtomicUsize::new(0);
    let (base, log) =
        serve_raw(
            move |request| match (request.method.as_str(), request.target.as_str()) {
                ("POST", "/sandboxes") => match attempts.fetch_add(1, Ordering::SeqCst) {
                    0 | 1 => reply(202, CREATED, true),
                    2 => reply(409, r#"{"message":"Creation still in progress"}"#, false),
                    3 => reply(202, CREATED, false),
                    other => panic!("unexpected create attempt {other}"),
                },
                ("PATCH", "/sandboxes/bx_23456789") => reply(200, CREATED, false),
                other => panic!("unexpected request {other:?}"),
            },
        );
    let mut settings = crate::boat::tests::settings();
    settings.base = base;
    let key = SecretString::from("boat_fixture");
    let request = Request {
        settings: &settings,
        key: &key,
    };
    let created = request
        .create_for_test("herdr-box", || false, |_| Ok(()))
        .unwrap();
    assert_eq!(created.id, "bx_23456789");
    let log = log.lock().unwrap();
    assert_eq!(log.len(), 5);
    let original_key = idempotency(&log[0]);
    assert!(uuid::Uuid::parse_str(original_key).is_ok());
    for retry in &log[1..4] {
        assert_eq!(idempotency(retry), original_key);
        assert_eq!(retry.body, log[0].body);
    }
    assert_eq!(log[4].method, "PATCH");
}

#[test]
fn interrupted_creation_retries_are_bounded_and_preserve_the_read_error() {
    let (base, log) = serve_raw(|_| reply(202, CREATED, true));
    let mut settings = crate::boat::tests::settings();
    settings.base = base;
    let key = SecretString::from("boat_fixture");
    let request = Request {
        settings: &settings,
        key: &key,
    };
    let error = request
        .create_for_test(
            "herdr-box",
            || false,
            |_| panic!("unconfirmed creation must not persist"),
        )
        .unwrap_err();
    assert!(matches!(error, Error::Read(_)));
    assert!(std::error::Error::source(&error).is_some());
    let log = log.lock().unwrap();
    assert_eq!(log.len(), 4);
    for retry in &log[1..] {
        assert_eq!(idempotency(retry), idempotency(&log[0]));
    }
}

#[test]
fn rejected_credentials_and_malformed_json_are_not_retried() {
    for (status, body) in [(401, "{}"), (200, "not JSON")] {
        let (base, log) = serve_raw(move |_| reply(status, body, false));
        let mut settings = crate::boat::tests::settings();
        settings.base = base;
        let key = SecretString::from("boat_fixture");
        let request = Request {
            settings: &settings,
            key: &key,
        };
        let error = request
            .create_for_test(
                "herdr-box",
                || false,
                |_| panic!("failed creation must not persist"),
            )
            .unwrap_err();
        match status {
            401 => assert!(matches!(error, Error::Authentication)),
            _ => assert!(matches!(error, Error::Json(_))),
        }
        assert_eq!(log.lock().unwrap().len(), 1);
    }
}
