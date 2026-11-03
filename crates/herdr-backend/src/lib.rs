#![doc = include_str!("../README.md")]

mod error;
mod terminal;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;

use std::time::Duration;

pub use error::{Error, Result};
pub use herdr_protocol::{
    AgentStatus, ClientKeyCode, ClientKeyKind, ClientMouseButton, ClientMouseKind,
    ClientMousePosition, ClientPaneInputEvent, CursorState, FrameData, PaneSurfacePatchRow,
};
pub use terminal::{
    Change, Exit, KeyReporting, MAX_CELLS, MAX_SIDE, MAX_TERMINAL_ID, Role, RunState, Size, Spawn,
    SurfaceUpdate, TerminalId, TerminalInfo,
};

/// Which Herdr runs a host's terminals.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Generation {
    /// The Herdr 1 daemon: it owns workspaces, tabs, and panes and sends rendered cells.
    Herdr1,
    /// herdr-core (`hc`): it owns terminals only and streams their raw output.
    Herdr2,
}

/// One host's terminals, whichever generation runs them.
///
/// Shared between workers, so implementations are `Send + Sync`. Every
/// method may block on the backend: never call one on the UI thread.
pub trait Backend: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;
    type Terminal: Terminal<Error = Self::Error>;
    type Changes: Changes<Error = Self::Error>;

    fn generation(&self) -> Generation;

    /// The terminals running now.
    fn terminals(&self) -> Result<Vec<TerminalInfo>, Self::Error>;

    /// The terminals now, and every change after that snapshot with no gap
    /// between the two.
    fn watch(&self) -> Result<(Vec<TerminalInfo>, Self::Changes), Self::Error>;

    /// Shows `id` at `size`. An observer never types into or resizes it.
    fn attach(
        &self,
        id: &TerminalId,
        size: Size,
        role: Role,
    ) -> Result<Self::Terminal, Self::Error>;

    /// Starts a terminal; an empty `argv` starts the host's shell.
    fn spawn(&self, spawn: &Spawn) -> Result<TerminalInfo, Self::Error>;

    /// Ends `id` and its program.
    fn kill(&self, id: &TerminalId) -> Result<(), Self::Error>;
}

/// One attached terminal, owned by the worker that drains it.
pub trait Terminal: Send {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Keys, text, pastes, and mouse reports, encoded for the program's
    /// current modes by whichever side emulates the terminal.
    fn input(&mut self, input: &ClientPaneInputEvent) -> Result<(), Self::Error>;

    fn resize(&mut self, size: Size) -> Result<(), Self::Error>;

    fn focus(&mut self, focused: bool) -> Result<(), Self::Error>;

    /// What changed, waiting at most `timeout`; empty when nothing did.
    fn next(&mut self, timeout: Duration) -> Result<Vec<SurfaceUpdate>, Self::Error>;
}

/// Changes to a host's terminals after a `Backend::watch` snapshot.
pub trait Changes: Send {
    type Error: std::error::Error + Send + Sync + 'static;

    /// What changed, waiting at most `timeout`; empty when nothing did.
    fn next(&mut self, timeout: Duration) -> Result<Vec<Change>, Self::Error>;
}
