use super::*;
use crate::boat::Error as BoatError;
use crate::cloud::ProviderError;
use crate::cloud::tests::server::{Request as HttpRequest, serve};
use std::{
    fs,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

const ID: &str = "bx_23456789";

fn reply(state: &str) -> String {
    serde_json::json!({"sandbox": {
        "id": ID, "name": "herdr-box", "state": state
    }})
    .to_string()
}

fn api(
    poll: impl Fn(&HttpRequest) -> (u16, String) + Send + 'static,
) -> (Settings, std::sync::Arc<std::sync::Mutex<Vec<HttpRequest>>>) {
    let (base, log) = serve(move |request| match request.method.as_str() {
        "POST" | "PATCH" => (200, reply("provisioning")),
        "GET" => poll(request),
        method => panic!("unexpected {method}"),
    });
    let mut settings = crate::boat::tests::settings();
    settings.base = base;
    (settings, log)
}

fn record(path: &Path, device: SavedDevice) -> crate::Result<()> {
    fs::write(path, serde_json::to_vec(&device).unwrap()).unwrap();
    Ok(())
}

fn reload(path: &Path) -> SavedDevice {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

#[test]
fn polling_failures_keep_a_durable_device_before_the_first_poll() {
    for (code, body) in [
        (503, "{}".into()),
        (404, "{}".into()),
        (200, reply("error")),
        (200, "not JSON".into()),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("saved.json");
        let persisted = path.clone();
        let (settings, log) = api(move |_| {
            assert_eq!(reload(&persisted).id, ID);
            (code, body.clone())
        });
        let key = SecretString::from("boat_fixture");
        let request = Request {
            settings: &settings,
            key: &key,
        };
        let result = finish_setup(
            &request,
            dir.path(),
            Target::Create("herdr-box".into()),
            &|| false,
            &|_| {},
            |device| record(&path, device),
            |_| panic!("failed polling must not prepare SSH or install Herdr"),
        );
        assert!(
            matches!(result, Err(crate::Error::Provider(ProviderError::Boat(BoatError::Setup { ref id, .. }))) if id == ID)
        );
        let device = reload(&path);
        assert_eq!(device.account, settings.account());
        assert_eq!(device.id, ID);
        assert!(
            matches!(device.target(), herdr_client::ConnectTarget::Cloud { id, .. } if id == ID)
        );
        assert_eq!(log.lock().unwrap().len(), 3);
    }
}

#[test]
fn cancellation_after_creation_saves_the_id_and_does_not_poll() {
    let (settings, log) = api(|_| panic!("cancelled setup must not poll"));
    let key = SecretString::from("boat_fixture");
    let request = Request {
        settings: &settings,
        key: &key,
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("saved.json");
    let cancelled = AtomicBool::new(false);
    let result = finish_setup(
        &request,
        dir.path(),
        Target::Create("herdr-box".into()),
        &|| cancelled.load(Ordering::Acquire),
        &|_| {},
        |device| {
            record(&path, device)?;
            cancelled.store(true, Ordering::Release);
            Ok(())
        },
        |_| panic!("cancelled setup must not install"),
    );
    assert!(
        matches!(result, Err(crate::Error::Provider(ProviderError::Boat(BoatError::Setup { source, .. })))
        if matches!(*source, crate::Error::Provider(ProviderError::Boat(BoatError::Cancelled))))
    );
    assert_eq!(reload(&path).id, ID);
    assert_eq!(log.lock().unwrap().len(), 2);
}

#[test]
fn ssh_and_installation_failures_preserve_the_device_and_original_error() {
    use crate::Error as AppError;
    for failure in [
        AppError::Provider(ProviderError::Boat(BoatError::Gateway)),
        AppError::Provider(ProviderError::Boat(BoatError::SshMissing(
            std::io::Error::from(std::io::ErrorKind::NotFound),
        ))),
        AppError::Cloud(cloud::Error::Install(install::Failure::Output(
            "fixture installer failure".into(),
        ))),
        AppError::Cloud(cloud::Error::InstallTimeout),
        AppError::Cloud(cloud::Error::Cancelled),
    ] {
        let (settings, _) = api(|_| (200, reply("ready")));
        let key = SecretString::from("boat_fixture");
        let request = Request {
            settings: &settings,
            key: &key,
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("saved.json");
        let diagnostic = failure.to_string();
        let kind = std::mem::discriminant(&failure);
        let error = finish_setup(
            &request,
            dir.path(),
            Target::Create("herdr-box".into()),
            &|| false,
            &|_| {},
            |device| record(&path, device),
            |_| {
                assert_eq!(reload(&path).id, ID);
                Err(failure)
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("Finish setup"));
        let AppError::Provider(ProviderError::Boat(BoatError::Setup { id, source })) = error else {
            panic!("setup context lost")
        };
        assert_eq!(id, ID);
        assert_eq!(std::mem::discriminant(&*source), kind);
        assert_eq!(source.to_string(), diagnostic);
        assert_eq!(reload(&path).id, ID);
    }
}

#[test]
fn finishing_a_saved_device_reuses_its_id_and_preserves_its_label() {
    let (settings, log) = api(|request| {
        assert_eq!(request.target, format!("/sandboxes/{ID}"));
        (200, reply("ready"))
    });
    let key = SecretString::from("boat_fixture");
    let request = Request {
        settings: &settings,
        key: &key,
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("saved.json");
    let saved = SavedDevice {
        provider: CloudProvider::Boat,
        id: ID.into(),
        label: "My chosen label".into(),
        account: settings.account(),
        machine: "old name".into(),
        session: "work".into(),
        enabled: false,
    };
    record(&path, saved.clone()).unwrap();
    let result = finish_setup(
        &request,
        dir.path(),
        Target::Saved(reload(&path)),
        &|| false,
        &|_| {},
        |device| record(&path, device),
        |_| Ok(()),
    )
    .unwrap();
    assert_eq!(result.id, saved.id);
    assert_eq!(result.label, saved.label);
    assert_eq!(result.session, saved.session);
    assert_eq!(result.enabled, saved.enabled);
    assert_eq!(result.machine, "herdr-box");
    assert_eq!(reload(&path), result);
    let log = log.lock().unwrap();
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].method, "GET", "retry must not create or rename");
}

#[test]
fn catalog_save_failures_retain_the_created_id_and_typed_storage_cause() {
    use crate::Error as AppError;
    for fail_on in [1, 2] {
        let (settings, log) = api(|_| (200, reply("ready")));
        let key = SecretString::from("boat_fixture");
        let request = Request {
            settings: &settings,
            key: &key,
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("saved.json");
        let mut writes = 0;
        let error = finish_setup(
            &request,
            dir.path(),
            Target::Create("herdr-box".into()),
            &|| false,
            &|_| {},
            |device| {
                writes += 1;
                if writes == fail_on {
                    return Err(cloud::Error::Catalog {
                        path: path.clone(),
                        source: std::io::ErrorKind::PermissionDenied.into(),
                    }
                    .into());
                }
                record(&path, device)
            },
            |_| Ok(()),
        )
        .unwrap_err();
        assert!(error.to_string().contains(ID));
        let AppError::Provider(ProviderError::Boat(BoatError::SaveDevice { id, source })) = error
        else {
            panic!("save context lost")
        };
        assert_eq!(id, ID);
        assert!(
            matches!(*source, AppError::Cloud(cloud::Error::Catalog { source, .. }) if source.kind() == std::io::ErrorKind::PermissionDenied)
        );
        if fail_on == 1 {
            assert_eq!(
                log.lock().unwrap().len(),
                1,
                "save failure must precede polling"
            );
        } else {
            assert_eq!(
                reload(&path).id,
                ID,
                "failed completion cannot erase recovery"
            );
        }
    }
}

#[test]
fn the_confirmed_id_is_recorded_before_renaming_or_polling() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("saved.json");
    let persisted = path.clone();
    let (base, log) = serve(move |request| {
        if request.method != "POST" {
            assert_eq!(reload(&persisted).id, ID);
        }
        (200, reply("ready"))
    });
    let mut settings = crate::boat::tests::settings();
    settings.base = base;
    let key = SecretString::from("boat_fixture");
    let request = Request {
        settings: &settings,
        key: &key,
    };
    let result = finish_setup(
        &request,
        dir.path(),
        Target::Create("herdr-box".into()),
        &|| false,
        &|_| {},
        |device| record(&path, device),
        |_| Ok(()),
    )
    .unwrap();
    assert_eq!(result.id, ID);
    assert_eq!(reload(&path), result);
    assert_eq!(
        log.lock()
            .unwrap()
            .iter()
            .map(|request| request.method.as_str())
            .collect::<Vec<_>>(),
        ["POST", "PATCH", "GET"]
    );
}

#[test]
fn recovery_rejects_a_different_account_before_preparing_credentials() {
    let settings = crate::boat::tests::settings();
    let device = SavedDevice {
        provider: CloudProvider::Boat,
        id: ID.into(),
        label: "box".into(),
        account: "https://other.boat.example/api/v1".into(),
        machine: "box".into(),
        session: "default".into(),
        enabled: true,
    };
    let error = add_device(
        AddRequest {
            settings,
            target: Target::Saved(device),
            install: true,
        },
        &|| false,
        &|_| panic!("wrong-account recovery must not start"),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        crate::Error::Provider(ProviderError::Boat(BoatError::Account))
    ));
}
