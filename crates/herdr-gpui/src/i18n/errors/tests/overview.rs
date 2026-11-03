use super::*;
use crate::overview::Error;
use std::sync::Arc;

#[test]
fn every_overview_variant_has_a_message() {
    assert_localized(&[
        Error::InvalidMarks,
        Error::Json(serde_json::from_str::<u8>("x").unwrap_err()),
        Error::ReopenRequest(Arc::new(crate::Error::NotConnected)),
        Error::ReopenResponse,
    ]);
}
