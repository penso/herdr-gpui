use super::*;
use crate::control::ErrorCode;

#[test]
fn uncertain_replies_never_create_a_fallback_copy() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("feedback.md");
    for error in [
        crate::control::Error::Rejected {
            code: ErrorCode::Timeout,
            message: "UI still has the request".into(),
        }
        .into(),
        crate::control::Error::NoResponse.into(),
        crate::Error::Io(std::io::ErrorKind::TimedOut.into()),
        crate::Error::Io(std::io::ErrorKind::BrokenPipe.into()),
    ] {
        let sent = deliver("notes".into(), Some(&path), |_| Err(error)).unwrap();
        assert!(matches!(sent, Sent::Unconfirmed { .. }));
        assert!(sent.status().contains("Delivery unconfirmed"));
        assert!(!path.exists());
    }
}

#[test]
fn an_older_apps_explicit_rejection_still_uses_the_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("feedback.md");
    let sent = deliver("notes".into(), Some(&path), |_| {
        Err(crate::control::Error::Rejected {
            code: ErrorCode::InvalidRequest,
            message: "unknown method notes.send".into(),
        }
        .into())
    })
    .unwrap();
    assert!(matches!(sent, Sent::File { .. }));
    assert_eq!(std::fs::read_to_string(path).unwrap(), "notes");
}
