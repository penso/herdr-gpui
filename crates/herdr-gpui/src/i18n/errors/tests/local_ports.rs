use super::*;
use crate::local_ports::Error;

#[test]
fn every_local_ports_variant_has_a_message() {
    assert_localized(&[
        Error::Busy,
        Error::Scan(herdr_client::Error::Disconnected),
        Error::Stopped,
        Error::Tool,
        Error::Unsupported,
        Error::Worker(std::io::Error::other("disk on fire")),
    ]);
}
