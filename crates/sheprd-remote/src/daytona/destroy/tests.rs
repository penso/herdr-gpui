#![allow(clippy::unwrap_used)]
use super::*;
use crate::cloud::tests::server::serve;
use secrecy::SecretString;

#[test]
fn deleting_a_sandbox_treats_one_already_gone_as_deleted() {
    let (base, log) = serve(
        |request| match (request.method.as_str(), request.target.as_str()) {
            ("DELETE", "/api/sandbox/sbx-1") => {
                (200, r#"{"id":"sbx-1","state":"destroying"}"#.into())
            }
            ("DELETE", "/api/sandbox/sbx-2") => (404, r#"{"message":"not found"}"#.into()),
            ("DELETE", "/api/sandbox/sbx-3") => (500, r#"{"message":"later"}"#.into()),
            other => panic!("unexpected {other:?}"),
        },
    );
    let mut settings = crate::daytona::tests::settings();
    settings.base = format!("{base}/api");
    let key = SecretString::from("dtn_fixture");
    let request = Request {
        settings: &settings,
        key: &key,
    };
    request.destroy("sbx-1").unwrap();
    request.destroy("sbx-2").unwrap();
    assert!(matches!(
        request.destroy("sbx-3"),
        Err(Error::Status(crate::daytona::Status { code: 500, .. }))
    ));
    assert!(matches!(request.destroy("a/b"), Err(Error::Field(_))));
    let log = log.lock().unwrap();
    assert_eq!(log.len(), 3, "an invalid ID sends nothing");
    assert_eq!(log[0].authorization.as_deref(), Some("Bearer dtn_fixture"));
}

#[test]
fn a_device_of_another_account_is_refused_before_any_request() {
    let settings = crate::daytona::tests::settings();
    assert!(matches!(
        destroy(&settings, "https://other.example.com/api", "sbx-1"),
        Err(Error::Account)
    ));
}
