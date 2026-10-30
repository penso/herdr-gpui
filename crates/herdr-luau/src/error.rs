pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Lua(#[from] mlua::Error),
}

/// Raised through Lua when a state runs past its interrupt budget. Callers
/// find it with `mlua::Error::downcast_ref`, wherever Lua wrapped it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("the script ran past its instruction budget")]
pub struct OutOfInstructions;
