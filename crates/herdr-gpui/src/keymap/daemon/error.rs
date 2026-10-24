//! Why a host's published keybindings could not be used.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("the host did not publish its keybindings")]
    Missing,
    #[error("the host's keybindings have no [keys] table")]
    NoKeys,
    #[error("the host's keybindings are not valid TOML: {0}")]
    Parse(#[source] toml::de::Error),
    #[error("the host's keybindings exceed {max} bytes")]
    TooLarge { max: usize },
    // keep-sorted end
}
