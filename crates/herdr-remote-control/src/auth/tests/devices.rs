use super::*;
use crate::auth::devices::TOUCH_PERSIST_MS;
use serde_json::json;
use std::error::Error as _;

#[test]
fn approving_stores_the_device_privately_and_it_reloads() {
    let dir = tempfile::tempdir().unwrap();
    let mut auth = auth(dir.path());
    let key = signing_key(1);
    let device = pair(&mut auth, &key, T0);

    let path = dir.path().join("devices.json");
    assert_eq!(auth.devices().path(), path);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    let reloaded = Devices::load(dir.path()).unwrap();
    let record = reloaded.get(&device).unwrap();
    assert_eq!(record.name, "iPhone");
    assert_eq!(
        record.public_key,
        PublicKey::parse(&public_key(&key)).unwrap()
    );
    assert_eq!(record.created_ms, T0);
    assert_eq!(
        reloaded.list(|id| id == &device),
        vec![control::Device {
            id: device.clone(),
            name: "iPhone".into(),
            created_ms: T0,
            last_seen_ms: None,
            push: false,
            connected: true,
        }]
    );
    // No temporary file is left beside the store.
    let names: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names, vec![std::ffi::OsString::from("devices.json")]);
}

#[test]
fn a_missing_store_is_empty() {
    let dir = tempfile::tempdir().unwrap();
    let devices = Devices::load(&dir.path().join("not-yet")).unwrap();
    assert_eq!(devices.len(), 0);
    assert!(devices.list(|_| false).is_empty());
}

#[test]
fn touching_writes_last_seen_at_most_once_a_minute() {
    let dir = tempfile::tempdir().unwrap();
    let mut devices = Devices::load(dir.path()).unwrap();
    let device = new_device(1, "iPhone");
    let id = device.id.clone();
    devices.add(device, T0).unwrap();
    let stored = |dir: &Path| {
        Devices::load(dir)
            .unwrap()
            .get(&DeviceId("device-1".into()))
            .unwrap()
            .last_seen_ms
    };

    devices.touch(&id, T0 + 1).unwrap();
    assert_eq!(stored(dir.path()), Some(T0 + 1));
    devices.touch(&id, T0 + TOUCH_PERSIST_MS).unwrap();
    assert_eq!(
        devices.get(&id).unwrap().last_seen_ms,
        Some(T0 + TOUCH_PERSIST_MS)
    );
    assert_eq!(stored(dir.path()), Some(T0 + 1));
    devices.touch(&id, T0 + 1 + TOUCH_PERSIST_MS).unwrap();
    assert_eq!(stored(dir.path()), Some(T0 + 1 + TOUCH_PERSIST_MS));

    assert!(matches!(
        devices.touch(&DeviceId("nope".into()), T0).unwrap_err(),
        Error::UnknownDevice
    ));
}

#[test]
fn revoking_forgets_the_device_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let mut devices = Devices::load(dir.path()).unwrap();
    devices.add(new_device(1, "iPhone"), T0).unwrap();
    devices.add(new_device(2, "Pixel"), T0 + 1).unwrap();
    let id = DeviceId("device-1".into());
    assert!(devices.revoke(&id).unwrap());
    assert!(!devices.revoke(&id).unwrap());
    let reloaded = Devices::load(dir.path()).unwrap();
    assert!(reloaded.get(&id).is_none());
    assert_eq!(reloaded.len(), 1);
}

#[test]
fn a_push_subscription_is_kept_opaque() {
    let dir = tempfile::tempdir().unwrap();
    let mut devices = Devices::load(dir.path()).unwrap();
    devices.add(new_device(1, "iPhone"), T0).unwrap();
    let id = DeviceId("device-1".into());
    let subscription = json!({"endpoint": "https://push.example/abc", "keys": {"p256dh": "x"}});
    devices.set_push(&id, Some(subscription.clone())).unwrap();
    let reloaded = Devices::load(dir.path()).unwrap();
    assert_eq!(reloaded.get(&id).unwrap().push, Some(subscription));
    assert!(reloaded.list(|_| false)[0].push);

    devices.set_push(&id, None).unwrap();
    assert!(!Devices::load(dir.path()).unwrap().list(|_| false)[0].push);
    assert!(matches!(
        devices
            .set_push(&DeviceId("nope".into()), None)
            .unwrap_err(),
        Error::UnknownDevice
    ));
}

#[test]
fn idle_devices_expire_and_lose_their_sessions() {
    let dir = tempfile::tempdir().unwrap();
    let mut auth = auth(dir.path());
    let idle_key = signing_key(1);
    let idle = pair(&mut auth, &idle_key, T0);
    let session = sign_in(&mut auth, &idle_key, &idle, T0);
    let active_key = signing_key(2);
    let active = pair(&mut auth, &active_key, T0);
    let day = 24 * 60 * 60 * 1000;
    auth.devices_mut().touch(&active, T0 + 5 * day).unwrap();

    assert!(auth.expire_idle(T0 + 6 * day, 7 * day).unwrap().is_empty());
    assert_eq!(
        auth.expire_idle(T0 + 7 * day, 7 * day).unwrap(),
        vec![idle.clone()]
    );
    assert!(auth.authenticate(&session.token, T0 + 7 * day).is_none());
    let reloaded = Devices::load(dir.path()).unwrap();
    assert!(reloaded.get(&idle).is_none());
    assert!(reloaded.get(&active).is_some());
}

#[test]
fn a_corrupt_store_is_a_format_error() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("devices.json"), b"{not json").unwrap();
    let error = Devices::load(dir.path()).unwrap_err();
    assert!(matches!(&error, Error::StoreFormat { path, .. } if path.ends_with("devices.json")));
    assert!(error.source().is_some());
    assert_eq!(control::ErrorCode::from(&error), control::ErrorCode::Other);
}

#[test]
fn a_stored_key_off_the_curve_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let mut off_curve = [0u8; 65];
    off_curve[0] = 4;
    let content = json!({"version": 1, "devices": [{
        "id": "d", "name": "x", "public_key": encode(&off_curve), "created_ms": 1,
    }]});
    std::fs::write(dir.path().join("devices.json"), content.to_string()).unwrap();
    assert!(matches!(
        Devices::load(dir.path()).unwrap_err(),
        Error::StoreFormat { .. }
    ));
}

#[test]
fn a_failed_write_leaves_memory_as_the_disk_has_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut devices = Devices::load(dir.path()).unwrap();
    // A directory where the file goes makes the final rename fail.
    std::fs::create_dir(dir.path().join("devices.json")).unwrap();
    let error = devices.add(new_device(1, "iPhone"), T0).unwrap_err();
    assert!(
        matches!(&error, Error::Store { operation: StoreOperation::Replace, path, .. } if path.ends_with("devices.json")),
        "{error:?}"
    );
    assert!(error.source().is_some());
    assert_eq!(devices.len(), 0);
    let leftovers = std::fs::read_dir(dir.path()).unwrap().count();
    assert_eq!(leftovers, 1, "the temporary file is removed");
}

#[test]
fn an_approval_the_store_cannot_keep_is_not_reported() {
    let dir = tempfile::tempdir().unwrap();
    let mut auth = auth(dir.path());
    std::fs::create_dir(dir.path().join("devices.json")).unwrap();
    let offer = auth.mint(T0, &origins(), FINGERPRINT).unwrap();
    let client = IpAddr::from([10, 0, 0, 2]);
    let claimed = auth
        .claim(
            client,
            &token(&offer),
            "iPhone",
            &public_key(&signing_key(1)),
            T0,
        )
        .unwrap();
    assert!(matches!(
        auth.answer(&offer.id, true, &claimed.code, T0).unwrap_err(),
        Error::Store { .. }
    ));
    assert_eq!(auth.devices().len(), 0);
    assert_eq!(
        auth.claim_status(&claimed.id, T0).unwrap(),
        PairingState::Denied
    );
}
