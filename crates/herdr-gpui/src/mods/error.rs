#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Mod {0} has too many messages waiting; try again")]
    Busy(String),
    #[error("`[mods] disabled` has an invalid name `{0}`")]
    DisabledName(String),
    #[error(transparent)]
    Mod(#[from] herdr_mods::Error),
    #[error("No mod named {0} is running")]
    NotRunning(String),
    #[error("Could not start a thread for mod {name}")]
    Spawn {
        name: String,
        #[source]
        source: std::io::Error,
    },
    #[error("`[mods] disabled` lists more than {0} mods")]
    TooManyDisabled(usize),
}
