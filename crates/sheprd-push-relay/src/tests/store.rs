//! Every store promises the same, so one set of checks runs on each.

use super::*;
use crate::store::{Memory, Revoked, Sqlite, Store};

const ID: &str = "0123456789abcdef0123456789abcdef";

async fn finds_what_was_revoked(store: impl Store) {
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

async fn keeps_the_later_date(store: impl Store) {
    let what = Revoked::Device(PHONE);
    store.revoke(&what, NOW_S + 100).await.unwrap();
    store.revoke(&what, NOW_S).await.unwrap();
    assert_eq!(store.purge(NOW_S + 99).await.unwrap(), 0);
    assert_eq!(store.purge(NOW_S + 100).await.unwrap(), 1);
    assert!(!store.is_revoked(PHONE, ID).await.unwrap());
}

fn sqlite(dir: &tempfile::TempDir) -> Sqlite {
    Sqlite::open(dir.path().join("relay.sqlite")).unwrap()
}

#[tokio::test]
async fn memory_finds_what_was_revoked() {
    finds_what_was_revoked(Memory::default()).await;
}

#[tokio::test]
async fn memory_keeps_the_later_date() {
    keeps_the_later_date(Memory::default()).await;
}

#[tokio::test]
async fn sqlite_finds_what_was_revoked() {
    let dir = tempfile::tempdir().unwrap();
    finds_what_was_revoked(sqlite(&dir)).await;
}

#[tokio::test]
async fn sqlite_keeps_the_later_date() {
    let dir = tempfile::tempdir().unwrap();
    keeps_the_later_date(sqlite(&dir)).await;
}

#[tokio::test]
async fn sqlite_revocations_survive_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let store = sqlite(&dir);
    store.revoke(&Revoked::Device(PHONE), NOW_S).await.unwrap();
    drop(store);
    assert!(sqlite(&dir).is_revoked(PHONE, ID).await.unwrap());
}

/// Needs a database of its own: `SHEPRD_TEST_POSTGRES` is its URL.
#[cfg(feature = "postgres")]
mod postgres {
    use super::*;
    use crate::store::Postgres;

    async fn store() -> Postgres {
        let url = std::env::var("SHEPRD_TEST_POSTGRES").unwrap();
        let store = Postgres::connect(&url, None).await.unwrap();
        store.purge(u64::MAX >> 1).await.unwrap();
        store
    }

    #[tokio::test]
    #[ignore = "needs a Postgres server"]
    async fn every_promise_holds() {
        finds_what_was_revoked(store().await).await;
        keeps_the_later_date(store().await).await;
    }

    #[tokio::test]
    #[ignore = "needs a Postgres server"]
    async fn revocations_survive_a_new_connection() {
        let first = store().await;
        first.revoke(&Revoked::Device(PHONE), NOW_S).await.unwrap();
        let url = std::env::var("SHEPRD_TEST_POSTGRES").unwrap();
        let second = Postgres::connect(&url, None).await.unwrap();
        assert!(second.is_revoked(PHONE, ID).await.unwrap());
        first.purge(u64::MAX >> 1).await.unwrap();
    }

    #[tokio::test]
    #[ignore = "needs a Postgres server"]
    async fn a_lost_connection_comes_back_by_itself() {
        let store = store().await;
        let url = std::env::var("SHEPRD_TEST_POSTGRES").unwrap();
        let (admin, connection) = tokio_postgres::connect(&url, tokio_postgres::NoTls)
            .await
            .unwrap();
        tokio::spawn(connection);
        admin
            .batch_execute(
                "SELECT pg_terminate_backend(pid) FROM pg_stat_activity
                 WHERE pid <> pg_backend_pid() AND backend_type = 'client backend'",
            )
            .await
            .unwrap();
        // The first request may find out the hard way; it fails closed.
        let mut answers = Vec::new();
        for _ in 0..50 {
            let answer = store.is_revoked(PHONE, ID).await;
            let done = answer.is_ok();
            answers.push(done);
            if done {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert_eq!(answers.last(), Some(&true));
    }
}
