//! Agent chats and starting one: `error-chat.ftl`.

use super::Localize;
use crate::{
    chat::{Error, StartError, page, pasted, visual, widgets},
    config::chat::{MAX_ENV, MAX_ENV_BYTES},
    i18n::fl,
};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::Apply(_) => fl!("error-chat-apply"),
            Self::Busy => fl!("error-chat-busy"),
            // The chat protocol's own name for the reason.
            Self::Bye(reason) => fl!("error-chat-bye", reason = format!("{reason:?}")),
            Self::Closed => fl!("error-chat-closed"),
            Self::Connect(_) => fl!("error-chat-connect"),
            Self::EnvironmentName { name } => {
                fl!("error-chat-environment-name", name = name.as_str())
            }
            Self::EnvironmentSize => fl!(
                "error-chat-environment-size",
                count = MAX_ENV,
                bytes = MAX_ENV_BYTES
            ),
            Self::Handshake => fl!("error-chat-handshake"),
            Self::MirrorConfig => fl!("error-chat-mirror-config"),
            Self::NoEndpointDir => fl!("error-chat-no-endpoint-dir"),
            Self::NotAttached => fl!("error-chat-not-attached"),
            Self::Overflow => fl!("error-chat-overflow"),
            Self::Pasted(error) => error.localize(),
            Self::Read(_) => fl!("error-chat-read"),
            Self::ReadOnly => fl!("error-chat-read-only"),
            Self::Remote(_) => fl!("error-chat-remote"),
            Self::Spawn(_) => fl!("error-chat-spawn"),
            Self::Start(error) => error.localize(),
            // Stream positions, as the chat protocol writes them.
            Self::Stream { after, got } => fl!(
                "error-chat-stream",
                got = format!("{got:?}"),
                after = format!("{after:?}")
            ),
            Self::Version(version) => fl!("error-chat-version", version = *version),
            // keep-sorted end
        }
    }
}

impl Localize for StartError {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::AgentMissing(agent) => {
                fl!(
                    "error-chat-start-agent-missing",
                    agent = agent.display_name()
                )
            }
            Self::Disabled => fl!("error-chat-start-disabled"),
            Self::NotConnected => fl!("error-chat-start-not-connected"),
            Self::NotLocal => fl!("error-chat-start-not-local"),
            Self::NoWorkspace => fl!("error-chat-start-no-workspace"),
            Self::ProgramMissing => fl!("error-chat-start-program-missing"),
            Self::ProgramUnknown => fl!("error-chat-start-program-unknown"),
            Self::Request(error) => error.localize(),
            Self::Response => fl!("error-chat-start-response"),
            Self::SlotBusy => fl!("error-chat-start-slot-busy"),
            Self::Timeout => fl!("error-chat-start-timeout"),
            // keep-sorted end
        }
    }
}

impl Localize for pasted::Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::Folder => fl!("error-chat-pasted-folder"),
            Self::Write(_) => fl!("error-chat-pasted-write"),
            // keep-sorted end
        }
    }
}

impl Localize for page::Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::Folder => fl!("error-chat-page-folder"),
            // The system's own words.
            Self::Io(error) => error.to_string(),
            Self::NotFile => fl!("error-chat-page-not-file"),
            Self::TooLarge => fl!("error-chat-page-too-large"),
            // keep-sorted end
        }
    }
}

impl Localize for visual::Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::Base64(_) => fl!("error-chat-visual-base64"),
            Self::Decode(_) => fl!("error-chat-visual-decode"),
            Self::Diagram(_) => fl!("error-chat-visual-diagram"),
            Self::Fetch(_) => fl!("error-chat-visual-fetch"),
            // The formula renderer's own words.
            Self::Math(reason) => fl!("error-chat-visual-math", reason = reason.as_str()),
            Self::MathLimit => fl!("error-chat-visual-math-limit"),
            Self::MathParse(_) => fl!("error-chat-visual-math-parse"),
            Self::NotAnImage => fl!("error-chat-visual-not-an-image"),
            Self::Panicked => fl!("error-chat-visual-panicked"),
            Self::Raster(_) => fl!("error-chat-visual-raster"),
            // keep-sorted end
        }
    }
}

impl Localize for herdr_chat_protocol::Rejection {
    fn localize(&self) -> String {
        match self {
            Self::EmptyPrompt => fl!("error-chat-rejection-empty-prompt"),
            Self::PromptTooLong => fl!("error-chat-rejection-prompt-too-long"),
            Self::NotReady => fl!("error-chat-rejection-not-ready"),
            Self::NotWorking => fl!("error-chat-rejection-not-working"),
            Self::UnknownPermission => fl!("error-chat-rejection-unknown-permission"),
            Self::UnknownOption => fl!("error-chat-rejection-unknown-option"),
            Self::UnknownMode => fl!("error-chat-rejection-unknown-mode"),
            Self::UnknownSetting => fl!("error-chat-rejection-unknown-setting"),
            Self::UnknownQueued => fl!("error-chat-rejection-unknown-queued"),
            Self::AgentUnavailable => fl!("error-chat-rejection-agent-unavailable"),
            Self::TooManyAttachments => fl!(
                "error-chat-rejection-too-many-attachments",
                count = herdr_chat_protocol::MAX_ATTACHMENTS
            ),
            Self::QueueFull => fl!("error-chat-rejection-queue-full"),
            Self::UnknownFile => fl!("error-chat-rejection-unknown-file"),
            Self::Unsupported => fl!("error-chat-rejection-unsupported"),
            Self::Other => fl!("error-chat-rejection-other"),
        }
    }
}

impl Localize for widgets::Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::ActionGone => fl!("error-chat-card-action-gone"),
            Self::Component => fl!("error-chat-card-component"),
            Self::ComponentCount => fl!("error-chat-card-component-count"),
            Self::FieldId => fl!("error-chat-card-field-id"),
            Self::FormValue => fl!("error-chat-card-form-value"),
            Self::Json(_) => fl!("error-chat-card-json"),
            Self::Title => fl!("error-chat-card-title"),
            Self::TooLarge => fl!("error-chat-card-too-large"),
            Self::TooManyFields => fl!("error-chat-card-too-many-fields"),
            // keep-sorted end
        }
    }
}
