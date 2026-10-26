use crate::teleport;

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("could not read the existing agent metadata")]
    Answer(#[source] serde_json::Error),
    #[error(transparent)]
    Host(#[from] teleport::Error),
    #[error("could not inspect the existing agent on its host")]
    Query(#[source] herdr_client::Error),
    #[error("the existing-agent discovery worker stopped")]
    Stopped,
    #[error("could not start existing-agent discovery")]
    Worker(#[source] std::io::Error),
}
