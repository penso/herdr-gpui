//! A running worker follows `allow_plaintext_credentials`: the account page
//! saves to the store the setting names, so the worker must read from it too.
use super::*;
use sheprd_credentials::Store;

fn request() -> Request {
    Request {
        target: ConnectTarget::Local,
        checkout: "/nowhere".into(),
        workspace_id: "w1".into(),
        token: None,
        data_root: None,
        azure_store: Store::Keyring,
        gitlab_store: Store::Keyring,
        timing: Timing::default(),
    }
}

const FILES: Stores = Stores {
    azure: Store::File,
    gitlab: Store::File,
};

#[test]
fn new_stores_replace_the_old_and_forget_tokens_read_from_them() {
    let (_service, mut worker, _receiver) = Service::prepare(request());
    worker.azure_token = Some(("contoso".into(), None));

    worker.stores_changed(FILES);

    assert_eq!(worker.request.stores(), FILES);
    assert!(worker.azure_token.is_none(), "the next sync looks again");
}

#[test]
fn a_full_queue_reports_the_stores_as_not_taken() {
    let (service, _worker, _receiver) = Service::prepare(request());
    for _ in 0..COMMANDS {
        assert!(service.set_stores(FILES));
    }
    assert!(!service.set_stores(FILES), "the caller asks again");
}

#[test]
fn a_stopped_worker_needs_no_second_asking() {
    let (service, _worker, receiver) = Service::prepare(request());
    drop(receiver);
    assert!(service.set_stores(FILES));
}
