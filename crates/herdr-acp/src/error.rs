use std::io;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not start the agent `{program}`")]
    AgentSpawn {
        program: String,
        #[source]
        source: io::Error,
    },
    #[error("the agent's {0} pipe was not available")]
    AgentPipe(&'static str),
    #[error("the agent stopped reading its input")]
    AgentStalled,
    #[error("could not listen for Herdr UI at {path}")]
    Bind {
        path: std::path::PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    Chat(#[from] herdr_chat_protocol::Error),
    #[error(transparent)]
    Cli(#[from] crate::cli::Error),
    #[error("could not encode a message for the agent")]
    Encode(#[source] serde_json::Error),
    #[error("Herdr's API answered with an error: {0}")]
    HerdrApi(String),
    #[error("chat I/O failed")]
    Io(#[from] io::Error),
    #[error("no directory to listen in: neither XDG_STATE_HOME nor HOME is set")]
    NoEndpointDir,
    #[error("a line from the agent exceeded {limit} bytes")]
    LineTooLong { limit: usize },
}
