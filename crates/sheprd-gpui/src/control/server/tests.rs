use super::*;

#[cfg(windows)]
mod pipe_status;

#[test]
fn a_huge_wait_is_clamped_instead_of_overflowing() {
    let now = Instant::now();
    assert_eq!(
        deadline(Duration::MAX, now),
        now + Duration::from_secs(MAX_WAIT_SECONDS)
    );
    assert_eq!(
        deadline(Duration::from_secs(5), now),
        now + Duration::from_secs(5)
    );
}

#[test]
fn every_request_names_its_window_by_the_same_field() {
    use crate::control::protocol::{Call, settings::SettingsOp};
    let caller = Caller {
        window: Some(7),
        ..Caller::default()
    };
    let update = Request::Settings(Call {
        caller: caller.clone(),
        op: SettingsOp::Update,
    });
    assert_eq!(named_window(&update), Some(7));
    assert_eq!(named_window(&Request::Status(caller)), Some(7));
    assert_eq!(named_window(&Request::Status(Caller::default())), None);
}
