use super::*;
use crate::pane_move::{Error, Unchanged};

#[test]
fn every_pane_move_variant_has_a_message() {
    assert_localized(&[
        Error::UnsupportedHost,
        Error::DevelopmentSession,
        Error::InvalidId("--help".into()),
        Error::Unchanged(Unchanged::SameTab),
        Error::Refused {
            code: "pane_not_found".into(),
            message: "pane not found".into(),
        },
        Error::MissingCli(herdr_client::Error::ResponseType),
        Error::Script(herdr_client::Error::ResponseType),
        Error::Decode(serde_json::from_str::<u8>("x").unwrap_err()),
        Error::PartialTabMove {
            pane_id: "p_2".into(),
            created_tab_id: "t_9".into(),
            source: Box::new(Error::UnsupportedHost),
        },
        Error::Rename {
            tab_id: "t_9".into(),
            source: Box::new(Error::Unchanged(Unchanged::ZoomedTab)),
        },
    ]);
}

#[test]
fn every_unchanged_reason_has_a_message() {
    assert_localized(&[Unchanged::SameTab, Unchanged::ZoomedTab, Unchanged::Other]);
}
