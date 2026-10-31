use super::*;
use crate::chat::{Error, StartError};
use herdr_chat_protocol::{Agent, ByeReason, Cursor};
use std::{io, sync::Arc};

fn protocol() -> herdr_chat_protocol::Error {
    herdr_chat_protocol::Error::EmptyFrame
}

fn cursor(seq: u64) -> Cursor {
    Cursor {
        epoch: uuid::Uuid::nil(),
        seq,
    }
}

/// `Error::Pasted` is not built here: `chat::pasted` is private to `chat`,
/// so its error shows its English `Display` and cannot be named.
#[test]
fn every_chat_variant_has_a_message() {
    let mut errors = vec![
        Error::Apply(protocol()),
        Error::Busy,
        Error::Closed,
        Error::Connect(io::Error::other("refused")),
        Error::EnvironmentName {
            name: "HERDR=1".into(),
        },
        Error::EnvironmentSize,
        Error::Handshake,
        Error::MirrorConfig,
        Error::NoEndpointDir,
        Error::NotAttached,
        Error::Overflow,
        Error::Read(protocol()),
        Error::ReadOnly,
        Error::Remote(herdr_client::Error::Disconnected),
        Error::Spawn(io::Error::other("no threads")),
        Error::Start(StartError::Disabled),
        Error::Stream {
            after: Some(cursor(4)),
            got: cursor(6),
        },
        Error::Stream {
            after: None,
            got: cursor(2),
        },
        Error::Version(7),
    ];
    errors.extend(
        [
            ByeReason::Exiting,
            ByeReason::Handshake,
            ByeReason::Version { supported: 2 },
        ]
        .map(Error::Bye),
    );
    assert_localized(&errors);
}

#[test]
fn every_chat_start_variant_has_a_message() {
    let mut errors = vec![
        StartError::Disabled,
        StartError::NotConnected,
        StartError::NotLocal,
        StartError::NoWorkspace,
        StartError::ProgramMissing,
        StartError::ProgramUnknown,
        StartError::Request(Arc::new(crate::Error::NoTab)),
        StartError::Response,
        StartError::SlotBusy,
        StartError::Timeout,
    ];
    errors.extend(Agent::ALL.iter().copied().map(StartError::AgentMissing));
    assert_localized(&errors);
}
