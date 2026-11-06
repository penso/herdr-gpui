use super::*;
use crate::store::Revoked;

const ID: &str = "0123456789abcdef0123456789abcdef";

#[tokio::test]
async fn a_revoked_handle_or_device_is_found() {
    let fixture = fixture();
    let store = fixture.store;
    assert!(!store.is_revoked(PHONE, ID).await.unwrap());

    store
        .revoke(&Revoked::Handle(ID.to_owned()), NOW_S)
        .await
        .unwrap();
    assert!(store.is_revoked(PHONE, ID).await.unwrap());
    assert!(!store.is_revoked(PHONE, &"a".repeat(32)).await.unwrap());

    store.revoke(&Revoked::Device(PHONE), NOW_S).await.unwrap();
    assert!(store.is_revoked(PHONE, &"a".repeat(32)).await.unwrap());
    assert!(
        !store
            .is_revoked(OTHER_PHONE, &"a".repeat(32))
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn revocations_survive_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("relay.sqlite");
    let store = Store::open(&path).unwrap();
    store.revoke(&Revoked::Device(PHONE), NOW_S).await.unwrap();
    drop(store);
    let store = Store::open(&path).unwrap();
    assert!(store.is_revoked(PHONE, ID).await.unwrap());
}

#[tokio::test]
async fn revoking_again_keeps_the_later_date() {
    let fixture = fixture();
    let store = fixture.store;
    let what = Revoked::Device(PHONE);
    store.revoke(&what, NOW_S + 100).await.unwrap();
    store.revoke(&what, NOW_S).await.unwrap();
    assert_eq!(store.purge(NOW_S + 99).await.unwrap(), 0);
    assert_eq!(store.purge(NOW_S + 100).await.unwrap(), 1);
    assert!(!store.is_revoked(PHONE, ID).await.unwrap());
}
