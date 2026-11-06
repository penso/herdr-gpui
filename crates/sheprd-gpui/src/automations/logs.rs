//! The machines whose daemon log the Logs window can show: every machine a
//! window read a running `herdr-automate` on, app-wide since the Logs window
//! belongs to no one window.

use super::{HostState, Machine, fetch};
use crate::teleport::Host;
use gpui::{App, Global};
use sheprd_automation::report::LogText;
use std::sync::atomic::AtomicBool;

/// The lines of the daemon's log the Logs window asks for.
pub(crate) const DAEMON_LINES: u32 = 2000;

/// One machine's daemon log, as the Logs window offers it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LogSource {
    pub(crate) machine: Machine,
    pub(crate) label: String,
    pub(crate) host: Host,
}

impl LogSource {
    /// Reads the end of the daemon's log. Blocks: background executor only.
    pub(crate) fn read(&self, cancelled: &AtomicBool) -> Result<LogText, super::Error> {
        fetch::daemon_log(&self.host, DAEMON_LINES, cancelled)
    }
}

/// Every machine with a daemon log to show, in the order windows found them.
#[derive(Clone, Debug, Default)]
pub(crate) struct LogSources(pub(crate) Vec<LogSource>, pub(crate) bool);

impl Global for LogSources {}

impl LogSources {
    /// Adds or drops `host` after a read: listed while it has the program.
    pub(super) fn observe(host: &HostState, cx: &mut App) {
        let has_log = host.status().is_some();
        let sources = cx.default_global::<Self>();
        let at = sources
            .0
            .iter()
            .position(|source| source.machine == host.machine);
        match (at, has_log) {
            (Some(at), true) => {
                sources.0[at].label = host.label.clone();
                sources.0[at].host = host.host.clone();
            }
            (None, true) => sources.0.push(LogSource {
                machine: host.machine.clone(),
                label: host.label.clone(),
                host: host.host.clone(),
            }),
            (Some(at), false) => {
                sources.0.remove(at);
            }
            (None, false) => {}
        }
    }

    /// Turned off in the settings: no host is listed, and a Logs window
    /// reading one stops.
    pub(super) fn turn_off(cx: &mut App) {
        let sources = cx.default_global::<Self>();
        sources.0.clear();
        sources.1 = true;
    }

    pub(super) fn turn_on(cx: &mut App) {
        if cx.try_global::<Self>().is_some_and(|sources| sources.1) {
            cx.default_global::<Self>().1 = false;
        }
    }

    /// Whether automations are turned off, so no host's log may be read.
    pub(crate) fn off(cx: &App) -> bool {
        cx.try_global::<Self>().is_some_and(|sources| sources.1)
    }

    /// The sources now listed, or none before any window read one.
    pub(crate) fn current(cx: &App) -> Vec<LogSource> {
        cx.try_global::<Self>()
            .map(|sources| sources.0.clone())
            .unwrap_or_default()
    }
}
