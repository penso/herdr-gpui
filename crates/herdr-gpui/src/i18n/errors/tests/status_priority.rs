use super::*;
use crate::config::Error;

#[test]
fn status_priority_errors_are_localized() {
    assert_localized(&[
        Error::DuplicateSidebarStatus("working".into()),
        Error::InvalidSidebarStatus("unknown".into()),
    ]);
}
