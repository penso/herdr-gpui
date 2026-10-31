//! The crates whose events reach the log: this app and every Herdr crate it
//! links, so moving code into a crate of its own never silences it.

pub(crate) const LOG_TARGETS: &[&str] = &[
    // keep-sorted start
    "herdr_automation",
    "herdr_backend",
    "herdr_chat_protocol",
    "herdr_client",
    "herdr_commands",
    "herdr_control_protocol",
    "herdr_core_client",
    "herdr_credentials",
    "herdr_diagnostics",
    "herdr_github",
    "herdr_gpui",
    "herdr_i18n",
    "herdr_luau",
    "herdr_mods",
    "herdr_orchestrator",
    "herdr_protocol",
    "herdr_remote",
    "herdr_remote_control",
    "herdr_terminal",
    "herdr_updater",
    "herdr_usage",
    "herdr_vcs",
    // keep-sorted end
];

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
