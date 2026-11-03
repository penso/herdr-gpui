use super::*;
use crate::{boat, cloud::tests::server::serve};
use secrecy::SecretString;

#[test]
fn a_failed_rename_keeps_a_default_named_sandbox_savable_and_reachable_by_id() {
    let name = "Sandbox 2026-05-31 12:00";
    let (base, log) = serve(move |request| match request.method.as_str() {
        "POST" | "GET" => (
            200,
            serde_json::json!({"sandbox": {
                "id": "bx_23456789", "name": name, "state": "ready"
            }})
            .to_string(),
        ),
        "PATCH" => (503, r#"{"message":"Rename unavailable"}"#.into()),
        other => panic!("unexpected method {other}"),
    });
    let mut settings = boat::tests::settings();
    settings.base = base;
    let key = SecretString::from("boat_fixture");
    let request = boat::tests::Request {
        settings: &settings,
        key: &key,
    };
    let created = request
        .create_for_test("herdr-box", || false, |_| Ok(()))
        .unwrap();
    let sandbox = boat::tests::wait_ready(&request, &created.id, || false, |_| {}).unwrap();
    let saved = SavedDevice {
        provider: CloudProvider::Boat,
        id: sandbox.id,
        label: sandbox.name.clone(),
        machine: sandbox.name,
        account: settings.account(),
        session: "default".into(),
        enabled: true,
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE);
    save_in(&path, saved.clone()).unwrap();
    assert_eq!(
        read(&path).unwrap().as_slice(),
        std::slice::from_ref(&saved)
    );
    assert_eq!(saved.machine, name);
    assert!(matches!(saved.target(), ConnectTarget::Cloud {
        provider: CloudProvider::Boat, id, ..
    } if id == "bx_23456789"));
    assert_eq!(log.lock().unwrap().len(), 3);
}

#[test]
fn boat_names_remain_bounded_and_identifiers_and_other_providers_remain_strict() {
    let mut saved = device("bx_23456789");
    saved.provider = CloudProvider::Boat;
    for name in ["Sandbox 2026-05-31 12:00", "workspace / café", "-draft"] {
        saved.machine = name.into();
        assert!(saved.valid());
    }
    for name in [
        "",
        " ",
        "box\nname",
        "box\0name",
        &"x".repeat(NAME_LIMIT + 1),
    ] {
        saved.machine = name.into();
        assert!(!saved.valid(), "{name:?}");
    }
    saved.machine = "Sandbox 2026-05-31 12:00".into();
    saved.id = "bad/id".into();
    assert!(!saved.valid());
    saved.id = "bx_23456789".into();
    for provider in CloudProvider::ALL {
        if *provider != CloudProvider::Boat {
            saved.provider = *provider;
            assert!(!saved.valid());
        }
    }
}
