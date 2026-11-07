//! The crates whose events reach the log: this app and every Herdr crate it
//! links, so moving code into a crate of its own never silences it.

pub(crate) const LOG_TARGETS: &[&str] = &[
    // keep-sorted start
    "sheprd_automation",
    "sheprd_backend",
    "sheprd_chat_protocol",
    "sheprd_client",
    "sheprd_commands",
    "sheprd_control_protocol",
    "sheprd_core_client",
    "sheprd_credentials",
    "sheprd_diagnostics",
    "sheprd_github",
    "sheprd_gpui",
    "sheprd_i18n",
    "sheprd_luau",
    "sheprd_mods",
    "sheprd_orchestrator",
    "sheprd_protocol",
    "sheprd_remote",
    "sheprd_remote_control",
    "sheprd_session",
    "sheprd_terminal",
    "sheprd_updater",
    "sheprd_usage",
    "sheprd_vcs",
    // keep-sorted end
];

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
