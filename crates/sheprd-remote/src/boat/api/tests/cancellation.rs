use super::*;
use crate::cloud::tests::server::serve;
use secrecy::SecretString;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[test]
fn cancellation_before_creation_sends_no_request() {
    let (base, log) = serve(|_| {
        (
            200,
            r#"{"sandbox":{"id":"bx_23456789","name":"herdr-box","state":"ready"}}"#.into(),
        )
    });
    let mut settings = crate::boat::tests::settings();
    settings.base = base;
    let key = SecretString::from("boat_fixture");
    let request = Request {
        settings: &settings,
        key: &key,
    };
    assert!(matches!(
        request.create_for_test(
            "herdr-box",
            || true,
            |_| panic!("cancelled creation must not persist")
        ),
        Err(Error::Cancelled)
    ));
    assert!(log.lock().unwrap().is_empty());
}

#[test]
fn cancellation_after_sending_creation_still_recovers_with_the_original_key() {
    let cancelled = Arc::new(AtomicBool::new(false));
    let flag = cancelled.clone();
    let attempts = std::sync::atomic::AtomicUsize::new(0);
    let (base, log) = serve(move |request| {
        let body = r#"{"sandbox":{"id":"bx_23456789","name":"herdr-box","state":"ready"}}"#;
        match request.method.as_str() {
            "POST" if attempts.fetch_add(1, Ordering::SeqCst) == 0 => {
                flag.store(true, Ordering::SeqCst);
                (503, "{}".into())
            }
            "POST" => (202, body.into()),
            "PATCH" => (200, body.into()),
            other => panic!("unexpected method {other}"),
        }
    });
    let mut settings = crate::boat::tests::settings();
    settings.base = base;
    let key = SecretString::from("boat_fixture");
    let request = Request {
        settings: &settings,
        key: &key,
    };
    let created = request
        .create_for_test("herdr-box", || cancelled.load(Ordering::SeqCst), |_| Ok(()))
        .unwrap();
    assert_eq!(created.id, "bx_23456789");
    assert!(cancelled.load(Ordering::SeqCst));
    let log = log.lock().unwrap();
    assert_eq!(log.len(), 3);
    assert_eq!(idempotency(&log[0]), idempotency(&log[1]));
}
