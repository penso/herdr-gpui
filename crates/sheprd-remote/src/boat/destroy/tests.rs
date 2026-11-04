#![allow(clippy::unwrap_used)]
use super::*;
use crate::cloud::tests::server::serve;
use secrecy::SecretString;

#[test]
fn deleting_repeats_the_id_in_the_confirmation_header() {
    let (base, log) = serve(|request| {
        match (request.method.as_str(), request.target.as_str()) {
        ("DELETE", "/api/v1/sandboxes/bx_23456789") => (
            202,
            r#"{"ok":true,"type":"deletion.operation","operation":{"id":"bdop_1","status":"pending"}}"#.into(),
        ),
        ("DELETE", "/api/v1/sandboxes/bx_33456789") => (404, r#"{"message":"gone"}"#.into()),
        ("DELETE", "/api/v1/sandboxes/bx_43456789") => (409, r#"{"message":"confirm"}"#.into()),
        other => panic!("unexpected {other:?}"),
    }
    });
    let mut settings = crate::boat::tests::settings();
    settings.base = format!("{base}/api/v1");
    let key = SecretString::from("boat_fixture");
    let request = Request {
        settings: &settings,
        key: &key,
    };
    request.destroy("bx_23456789").unwrap();
    request.destroy("bx_33456789").unwrap();
    assert!(matches!(
        request.destroy("bx_43456789"),
        Err(Error::Status(crate::boat::Status { code: 409, .. }))
    ));
    assert!(matches!(request.destroy("../x"), Err(Error::Field(_))));
    let log = log.lock().unwrap();
    assert_eq!(log.len(), 3, "an invalid ID sends nothing");
    let headers = log[0].headers.to_ascii_lowercase();
    assert!(headers.contains("x-ascii-confirm-delete: bx_23456789"));
    assert_eq!(log[0].authorization.as_deref(), Some("Bearer boat_fixture"));
}

#[test]
fn a_device_of_another_account_is_refused_before_any_request() {
    let settings = crate::boat::tests::settings();
    assert!(matches!(
        destroy(&settings, "https://other.example.com/api/v1", "bx_23456789"),
        Err(Error::Account)
    ));
}
