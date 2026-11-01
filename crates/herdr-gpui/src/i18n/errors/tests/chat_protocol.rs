use super::*;
use herdr_chat_protocol::{EntryId, Error, PermissionId};

#[test]
fn every_chat_protocol_variant_has_a_message() {
    assert_localized(&[
        Error::ActivityLimit,
        Error::ActivityText,
        Error::DuplicatePermission(PermissionId(4)),
        Error::EmptyFrame,
        Error::EntryOrder {
            id: EntryId(2),
            last: EntryId(5),
        },
        Error::FrameLimit { len: 9_000_000 },
        Error::InvalidEndpoint,
        Error::Io(std::io::Error::other("disk on fire")),
        Error::Json(serde_json::from_str::<u8>("x").unwrap_err()),
        Error::NoModes,
        Error::NotText(EntryId(3)),
        Error::QuestionAnswersLimit(EntryId(7)),
        Error::UnknownEntry(EntryId(8)),
        Error::UnknownPermission(PermissionId(9)),
    ]);
}
