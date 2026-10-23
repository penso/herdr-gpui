use super::*;
use crate::boat::Error as BoatError;

#[test]
fn credential_failures_preserve_saved_devices_and_do_not_call_the_api() {
    let device = new_device(
        &crate::boat::tests::settings(),
        &api::Sandbox {
            id: "bx_23456789".into(),
            name: "saved-box".into(),
            state: "ready".into(),
            error: None,
        },
        "My saved sandbox".into(),
    );
    let overview = account_overview(
        vec![device.clone()],
        Err(BoatError::Storage(Box::new(crate::Error::GitHub(
            crate::github::Error::CredentialIo(std::io::ErrorKind::PermissionDenied.into()),
        )))),
        || panic!("an unreadable key must not issue an API request"),
    );
    assert_eq!(overview.devices, vec![device]);
    assert!(overview.sandboxes.is_none());
    assert!(matches!(overview.key, Err(BoatError::Storage(source))
        if matches!(*source, crate::Error::GitHub(crate::github::Error::CredentialIo(ref error)) if error.kind() == std::io::ErrorKind::PermissionDenied)));
}
