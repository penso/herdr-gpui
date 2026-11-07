#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the agent's log could not be read")]
    History(#[source] crate::history::Error),
    #[error("the agent's log could not be read")]
    Log(#[source] std::io::Error),
    #[error("the agent's log does not have that prompt")]
    NoPrompt,
}
