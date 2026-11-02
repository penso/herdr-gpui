use super::*;
use herdr_client::terminal_attach::Error;

#[test]
fn every_terminal_attach_variant_has_a_message() {
    assert_localized(&[
        Error::Busy,
        Error::Ended,
        Error::Io(std::io::Error::other("disk on fire")),
        Error::Protocol(herdr_client::protocol::Error::FrameLimit),
        Error::Refused(None),
        Error::Refused(Some("another client holds the terminal".into())),
        Error::Rejected {
            version: 2,
            reason: "unsupported".into(),
        },
        Error::Scroll,
        Error::Size,
        Error::Unexpected,
    ]);
    assert_localized(&[herdr_client::Error::TerminalAttach(Error::Ended)]);
}
