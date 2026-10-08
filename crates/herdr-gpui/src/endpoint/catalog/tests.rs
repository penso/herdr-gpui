#![allow(clippy::unwrap_used)]
use super::*;
use crate::endpoint::tests::host;

#[test]
fn explicit_socket_never_reads_shared_catalog() {
    let mut catalog = Catalog::new(&ConnectTarget::Socket("/unused.sock".into()));
    assert!(catalog.poll().is_none());
    assert!(catalog.pending.is_none());
    catalog.choose(LOCAL);
    assert!(catalog.queued_write.is_none());
    assert!(catalog.poll_write().is_none());
    assert!(catalog.writing.is_none());
    assert!(
        Catalog::new(&ConnectTarget::Session {
            name: "test".into(),
            development: true
        })
        .development
            == Some(true)
    );
}

#[test]
fn selection_writes_are_serialized_and_failure_keeps_the_ui_choice() {
    let mut catalog = Catalog::new(&ConnectTarget::Local);
    let (tx, rx) = mpsc::sync_channel(1);
    catalog.writing = Some(rx);
    catalog.choose("ssh:first");
    catalog.choose(LOCAL);
    catalog.choose("ssh:last");
    assert!(catalog.poll_write().is_none());
    assert!(catalog.writing.is_some());
    assert_eq!(catalog.queued_write, Some(Some("ssh:last".into())));
    // Simulate a failed worker without accessing the real user's state root.
    catalog.queued_write = None;
    tx.send(Err(std::io::Error::other("disk unavailable").into()))
        .unwrap();
    assert!(
        matches!(catalog.poll_write(), Some(Error::Io(error)) if error.to_string() == "disk unavailable")
    );
    assert!(catalog.writing.is_none());
    assert_eq!(catalog.desired.as_deref(), Some("ssh:last"));
    assert!(!catalog.restore_pending);
}

#[test]
fn desired_selection_is_client_local_and_catalog_changes_cancel_stale_restore() {
    let update = |enabled, selection| CatalogUpdate {
        hosts: vec![host("a", enabled)],
        wsl: Vec::new(),
        selection,
    };
    let mut first = Catalog::new(&ConnectTarget::Local);
    let mut second = Catalog::new(&ConnectTarget::Local);
    first.accept(&update(true, Some(Some("ssh:a".into()))));
    second.accept(&update(true, Some(Some("ssh:a".into()))));
    second.choose(LOCAL);
    first.accept(&update(true, Some(None)));
    assert_eq!(first.desired.as_deref(), Some("ssh:a"));
    assert!(first.restore_pending);
    assert_eq!(second.desired, None);
    first.accept(&update(false, None));
    assert_eq!(first.desired, None);
    assert!(!first.restore_pending);
    first.accept(&update(true, None));
    assert!(!first.restore_pending);
    let mut clicked = Catalog::new(&ConnectTarget::Local);
    clicked.choose(LOCAL);
    clicked.accept(&update(true, Some(Some("ssh:a".into()))));
    assert_eq!(
        clicked.desired, None,
        "late startup read cannot undo a click"
    );
    assert_eq!(clicked.queued_write, Some(None));
    second.choose("ssh:a");
    second.accept(&CatalogUpdate {
        hosts: vec![],
        wsl: Vec::new(),
        selection: None,
    });
    assert_eq!(second.desired, None);
}

mod wsl;
