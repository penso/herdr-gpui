use std::{fmt, io, path::PathBuf};

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{at}: {problem}")]
    Effect { at: String, problem: Problem },
    /// Compile and runtime errors, with the file and line Lua adds.
    #[error("{0}")]
    Lua(#[from] mlua::Error),
    #[error(transparent)]
    Luau(#[from] herdr_luau::Error),
    #[error("mod.toml: {at}: {problem}")]
    Manifest { at: String, problem: Problem },
    #[error("{at}: {problem}")]
    Message { at: String, problem: Problem },
    #[error("the mod has no main.luau")]
    NoMain,
    #[error("the mod has no mod.toml")]
    NoManifest,
    #[error("main.luau must return herdr.mod {{ ... }}")]
    NotAMod,
    #[error("could not read {}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{file}: {problem}")]
    Source { file: String, problem: Problem },
    #[error("mod.toml is not valid TOML")]
    Toml(#[from] toml::de::Error),
    #[error("view: {at}: {problem}")]
    Tree { at: String, problem: Problem },
}

impl Error {
    /// The budget a mod ran past, if that is why it stopped.
    pub fn budget(&self) -> Option<Budget> {
        let Self::Lua(error) = self else {
            return None;
        };
        if error
            .downcast_ref::<herdr_luau::OutOfInstructions>()
            .is_some()
        {
            return Some(Budget::Instructions);
        }
        let memory = |error: &(dyn std::error::Error + 'static)| {
            matches!(
                error.downcast_ref::<mlua::Error>(),
                Some(mlua::Error::MemoryError(_))
            )
        };
        (matches!(error, mlua::Error::MemoryError(_)) || error.chain().any(memory))
            .then_some(Budget::Memory)
    }
}

/// The limits that stop a mod that runs away.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Budget {
    Instructions,
    Memory,
}

impl fmt::Display for Budget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Instructions => "instruction",
            Self::Memory => "memory",
        })
    }
}

/// What is wrong with a value a mod wrote: in its manifest, its tree, a
/// message, or an effect.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Problem {
    #[error("unknown command `{0}`")]
    Command(String),
    #[error("contains a control character")]
    Control,
    #[error("nests deeper than {limit}")]
    Depth { limit: usize },
    #[error("unknown element `{0}`")]
    Element(String),
    #[error("is empty")]
    Empty,
    #[error("has more than {limit} entries")]
    Entries { limit: usize },
    #[error("must be {expected}")]
    Expected { expected: &'static str },
    #[error("is missing")]
    Missing,
    #[error("has more than {limit} elements")]
    Nodes { limit: usize },
    #[error("must be one of {}", .0.join(", "))]
    OneOf(&'static [&'static str]),
    #[error("is larger than {limit} bytes")]
    TooLarge { limit: u64 },
    #[error("is longer than {limit} bytes")]
    TooLong { limit: usize },
    #[error("unknown key `{0}`")]
    Unknown(String),
}
