use super::*;

fn refused(code: control::ErrorCode) -> Result<Response, control::Error> {
    Err(control::Error::Daemon {
        code,
        message: String::new(),
    })
}

#[test]
fn an_answer_is_offered_again_until_the_daemon_records_it() {
    assert!(retry_report(&refused(control::ErrorCode::Unsaved)));
    assert!(retry_report(&Err(control::Error::NoAnswer)));
    assert!(retry_report(&Err(control::Error::Connect(
        std::io::ErrorKind::ConnectionRefused.into()
    ))));
}

#[test]
fn a_recorded_or_refused_answer_is_not_sent_again() {
    assert!(!retry_report(&Ok(Response::Done)));
    assert!(!retry_report(&refused(control::ErrorCode::Other)));
    assert!(!retry_report(&refused(control::ErrorCode::UnknownDevice)));
}
