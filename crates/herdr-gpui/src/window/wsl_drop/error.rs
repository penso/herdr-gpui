//! Why files dropped on a WSL distribution's pane were not pasted.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("Another drop's paths are still being mapped in WSL. Try again in a moment.")]
    Busy,
    #[error("The pane the files were dropped on changed before their paths were ready.")]
    TargetGone,
    // keep-sorted end
}
