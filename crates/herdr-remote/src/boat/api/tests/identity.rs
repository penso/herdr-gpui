use super::*;
use crate::cloud::Value;
use crate::cloud::tests::server::serve;
use secrecy::SecretString;

#[test]
fn renaming_cannot_replace_the_created_sandbox_identity() {
    let (base, _) = serve(|request| {
        let id = if request.method == "POST" {
            "bx_23456789"
        } else {
            "bx_abcdefgh"
        };
        (
            200,
            serde_json::json!({"sandbox": {
                "id": id, "name": "herdr-box", "state": "ready"
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
    assert_eq!(
        request
            .create_for_test("herdr-box", || false, |_| Ok(()))
            .unwrap()
            .id,
        "bx_23456789"
    );
}

#[test]
fn polling_cannot_redirect_setup_to_another_sandbox() {
    let (base, _) = serve(|_| {
        (
            200,
            serde_json::json!({"sandbox": {
                "id": "bx_abcdefgh", "name": "other-box", "state": "ready"
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
        wait_ready(&request, "bx_23456789", || false, |_| {}),
        Err(Error::Field(Value::BoatSandboxId))
    ));
}
