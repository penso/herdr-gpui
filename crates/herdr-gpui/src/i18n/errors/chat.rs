//! Agent chats and starting one: `error-chat.ftl`.

use super::Localize;
use crate::{
    chat::{Error, StartError},
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
            // `chat::pasted` is private to `chat`, so its error cannot be
            // named here; it shows its English `Display`.
            Self::Pasted(error) => error.to_string(),
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
