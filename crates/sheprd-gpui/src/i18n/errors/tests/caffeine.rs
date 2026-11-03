use super::*;
use crate::caffeine::Error;

#[test]
fn every_caffeine_variant_has_a_message() {
    assert_localized(&[
        Error::Hold(keepawake::Error::Builder(
            keepawake::BuilderError::ValidationError("no assertion".into()),
        )),
        Error::NoWorkingAgents,
        Error::Thread(std::io::Error::other("no threads")),
        Error::WorkerStopped,
    ]);
}
