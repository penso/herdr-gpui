use super::*;
use std::{cell::RefCell, io};

#[test]
fn a_created_sandbox_is_deleted_when_the_first_read_fails_or_is_cancelled() {
    for error in [
        Error::Cancelled,
        Error::Forbidden("role assignment did not propagate".into()),
        Error::Read(io::Error::new(
            io::ErrorKind::ConnectionReset,
            "read failed",
        )),
    ] {
        let expected = std::mem::discriminant(&error);
        let calls = RefCell::new(Vec::new());
        let result = api::create_and_check(
            || {
                calls.borrow_mut().push("create");
                Ok("created-id".into())
            },
            |id| {
                assert_eq!(id, "created-id");
                calls.borrow_mut().push("read");
                Err(error)
            },
            |id| {
                assert_eq!(id, "created-id");
                calls.borrow_mut().push("delete");
                Ok(())
            },
        );
        assert_eq!(std::mem::discriminant(&result.unwrap_err()), expected);
        assert_eq!(calls.into_inner(), ["create", "read", "delete"]);
    }
}

#[test]
fn a_failed_rollback_preserves_the_first_read_error_and_source() {
    let error = api::create_and_check(
        || Ok("created-id".into()),
        |_| {
            Err(Error::Read(io::Error::new(
                io::ErrorKind::ConnectionReset,
                "read failed",
            )))
        },
        |_| Err(Error::Forbidden("cannot delete".into())),
    )
    .unwrap_err();
    assert!(
        matches!(&error, Error::Read(source) if source.kind() == io::ErrorKind::ConnectionReset)
    );
    assert_eq!(
        std::error::Error::source(&error).unwrap().to_string(),
        "read failed"
    );
}

#[test]
fn a_failed_create_does_not_check_or_delete_an_unknown_sandbox() {
    let error = api::create_and_check(
        || Err(Error::Cancelled),
        |_| panic!("no sandbox exists to check"),
        |_| panic!("no sandbox exists to delete"),
    )
    .unwrap_err();
    assert!(matches!(error, Error::Cancelled));
}

#[test]
fn a_checked_sandbox_transfers_to_setup_without_deletion() {
    let id = api::create_and_check(
        || Ok("created-id".into()),
        |id| {
            assert_eq!(id, "created-id");
            Ok(())
        },
        |_| panic!("a checked sandbox belongs to setup"),
    )
    .unwrap();
    assert_eq!(id, "created-id");
}
