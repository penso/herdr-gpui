# The agent integrations a daemon's host reports and installs, as Settings >
# Integrations lists them.

integrations-no-host = No selected host
integrations-host = Daemon host: { $host }
integrations-explainer = Install updates agent configuration on this daemon's host, not necessarily this computer. No integrations are installed automatically.
integrations-working = Working on the selected daemon...
integrations-refresh = Refresh integrations
integrations-none = No integrations reported by this daemon.
integrations-no-match = No matching integrations.
integrations-not-installed = Not installed
integrations-current = Current
integrations-outdated = Outdated
# $status is one of the three states above; $command is the agent's command.
integrations-row-detail = { $status } | { $command }
integrations-unavailable = Agent not available on this host
integrations-install = Install
integrations-reinstall = Reinstall
integrations-install-unsupported = Installation not supported by this daemon
integrations-host-changed = The selected daemon changed or disconnected. Refresh to load integrations. An already queued install may have completed on the previous host.
integrations-list-changed = The host or integration list changed. Refresh before installing.
