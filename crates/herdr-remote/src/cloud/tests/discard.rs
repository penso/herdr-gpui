use super::*;
use std::cell::Cell;

#[test]
fn a_failed_finish_deletes_the_machine_and_keeps_its_own_error() {
    let provider = CloudProvider::ALL[0];
    let deleted = Cell::new(0);
    let failed = discard_on_error(
        provider,
        "m-1",
        || -> crate::Result<()> { Err(Error::Cancelled.into()) },
        || {
            deleted.set(deleted.get() + 1);
            Err(Error::Invalid(Invalid::Target).into())
        },
    );
    assert!(matches!(failed, Err(crate::Error::Cloud(Error::Cancelled))));
    assert_eq!(deleted.get(), 1);
    let kept = discard_on_error(provider, "m-2", || Ok(7), || panic!("kept machines stay"));
    assert_eq!(kept.unwrap(), 7);
}
