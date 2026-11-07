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
        Error::ReplyHost(crate::pane_move::Error::UnsupportedHost),
        Error::ReplyScript(sheprd_client::Error::ScriptCancelled),
    ]);
}

#[test]
fn herdrs_own_refusal_is_shown_as_it_came() {
    assert_verbatim(&[Error::ReplyRefused {
        code: "agent_not_ready".into(),
        message: "not ready".into(),
    }]);
}
