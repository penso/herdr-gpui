use super::*;
use herdr_control_protocol::{Error, protocol::ErrorCode};
use std::{io, path::PathBuf};

fn io() -> io::Error {
    io::Error::other("disk on fire")
}

#[test]
fn every_control_variant_has_a_message() {
    let path = PathBuf::from("/tmp/herdr-gpui.sock");
    assert_localized(&[
        Error::MessageSize { limit: 1_048_576 },
        Error::NoResponse,
        Error::PipeForeign { path: path.clone() },
        Error::SocketInUse { path: path.clone() },
        Error::Unavailable { path, source: io() },
        Error::User(io()),
    ]);
}

#[test]
fn system_and_app_text_is_shown_as_it_is() {
    assert_verbatim(&[
        Error::Io(io()),
        Error::Json(serde_json::from_str::<u8>("x").unwrap_err()),
        Error::Rejected {
            code: ErrorCode::NotFound,
            message: "no such tab".into(),
        },
    ]);
}
