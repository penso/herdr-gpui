//! One serial loader owns the shared workbench configuration, independent of
//! every window's deferred reloads. Invalid edits retain the last good config.
use super::Launcher;
use gpui::{App, Task};

pub(super) fn watch(cx: &mut App) -> Task<()> {
    crate::config::watch::follow(cx, "px0", |config| config.px0, Launcher::apply_config)
}
