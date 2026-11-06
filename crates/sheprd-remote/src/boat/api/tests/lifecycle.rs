use super::*;
use crate::cloud::tests::server::serve;
use secrecy::SecretString;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[test]
fn cancellation_during_a_poll_cannot_resume_or_return_a_ready_sandbox() {
    for state in ["archived", "ready"] {
        let cancelled = Arc::new(AtomicBool::new(false));
        let flag = cancelled.clone();
        let (base, log) = serve(move |request| {
            assert_eq!(request.method, "GET");
            flag.store(true, Ordering::Release);
            (
                200,
                serde_json::json!({"sandbox": {
                    "id": "bx_23456789", "name": "box", "state": state
                }})
                .to_string(),
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
            wait_ready(
                &request,
                "bx_23456789",
                || cancelled.load(Ordering::Acquire),
                |_| panic!("cancelled poll")
            ),
            Err(Error::Cancelled)
        ));
        assert_eq!(log.lock().unwrap().len(), 1);
    }
}

#[test]
fn create_resume_and_extension_send_explicit_lifetimes() {
    for ttl in [None, Some(7200)] {
        let (base, log) = serve(|_| {
            (
                200,
                serde_json::json!({"sandbox": {
                    "id": "bx_23456789", "name": "box", "state": "ready"
                }})
                .to_string(),
            )
        });
        let mut settings = crate::boat::tests::settings();
        settings.base = base;
        settings.ttl_seconds = ttl;
        let key = SecretString::from("boat_fixture");
        let request = Request {
            settings: &settings,
            key: &key,
        };
        request
            .create_for_test("box", || false, |_| Ok(()))
            .unwrap();
        request.resume("bx_23456789").unwrap();
        request.extend("bx_23456789").unwrap();
        let log = log.lock().unwrap();
        assert_eq!(log.len(), 4);
        for index in [0, 2, 3] {
            let body: serde_json::Value = serde_json::from_str(&log[index].body).unwrap();
            assert!(
                body.get("ttlSeconds").is_some(),
                "omission enables Boat's one-hour default"
            );
            assert_eq!(body["ttlSeconds"], serde_json::json!(ttl));
        }
    }
}
