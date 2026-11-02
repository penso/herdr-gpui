use super::*;
use crate::processes::Error;

#[test]
fn every_processes_variant_has_a_message() {
    assert_localized(&[
        Error::Answer(serde_json::from_str::<u8>("x").unwrap_err()),
        Error::Busy,
        Error::Executable,
        Error::NoRoot,
        Error::Query(herdr_client::Error::ResponseType),
        Error::RootExited,
        Error::Stopped,
        Error::Worker(std::io::Error::other("disk on fire")),
    ]);
}
