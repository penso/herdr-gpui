use super::MAX_PANES;
use herdr_backend::TerminalId;

/// Why a tab's panes refused a change or could not be read back.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("Terminal {0} is already a pane of this tab")]
    Duplicate(TerminalId),
    #[error("A tab holds at most {MAX_PANES} panes")]
    Full,
    #[error("Terminal {0} is not a pane of this tab")]
    Missing(TerminalId),
    // keep-sorted end
}
