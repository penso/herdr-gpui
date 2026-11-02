//! The chat attach protocol's failures, from `error-chat-protocol.ftl`:
//! frames a chat or a phone's remote control connection cannot use. IDs are
//! placeables, written as the protocol's `Display` writes them.

use super::Localize;
use crate::i18n::fl;
use herdr_chat_protocol::{Error, MAX_ACTIVITIES, MAX_ACTIVITY_TEXT};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::ActivityLimit => {
                fl!("error-chat-protocol-activity-limit", count = MAX_ACTIVITIES)
            }
            Self::ActivityText => {
                fl!(
                    "error-chat-protocol-activity-text",
                    bytes = MAX_ACTIVITY_TEXT
                )
            }
            Self::DuplicatePermission(id) => {
                fl!(
                    "error-chat-protocol-duplicate-permission",
                    id = format!("{id:?}")
                )
            }
            Self::EmptyFrame => fl!("error-chat-protocol-empty-frame"),
            Self::EntryOrder { id, last } => fl!(
                "error-chat-protocol-entry-order",
                id = format!("{id:?}"),
                last = format!("{last:?}")
            ),
            Self::FrameLimit { len } => fl!("error-chat-protocol-frame-limit", bytes = *len),
            Self::InvalidEndpoint => fl!("error-chat-protocol-invalid-endpoint"),
            Self::Io(_) => fl!("error-chat-protocol-io"),
            Self::Json(_) => fl!("error-chat-protocol-json"),
            Self::NoModes => fl!("error-chat-protocol-no-modes"),
            Self::NotText(id) => fl!("error-chat-protocol-not-text", id = format!("{id:?}")),
            Self::QuestionAnswersLimit(id) => fl!(
                "error-chat-protocol-question-answers-limit",
                id = format!("{id:?}")
            ),
            Self::UnknownEntry(id) => {
                fl!("error-chat-protocol-unknown-entry", id = format!("{id:?}"))
            }
            Self::UnknownPermission(id) => fl!(
                "error-chat-protocol-unknown-permission",
                id = format!("{id:?}")
            ),
            // keep-sorted end
        }
    }
}
