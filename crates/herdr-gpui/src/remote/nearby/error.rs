//! Why the nearby relay could not carry a frame or start its helper.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("a nearby frame is not JSON this relay reads")]
    Malformed(#[source] serde_json::Error),
    #[error("the nearby helper has no {0} pipe")]
    Pipe(&'static str),
    #[error("could not start the nearby helper")]
    Spawn(#[source] std::io::Error),
    #[error("a nearby frame is over {limit} bytes")]
    TooLarge { limit: usize },
    // keep-sorted end
}
