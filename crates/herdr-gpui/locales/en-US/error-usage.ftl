# Why an AI service's plan usage could not be read, shown in the status bar
# and the usage panel. None of it carries account details.

error-usage-cloud-host-unsupported = This is not available for cloud machines yet.
# $kind is the step that failed. $command is a command line, such as
# amp usage, or for a step not listed here its English description.
error-usage-command = { $kind ->
    [agy-sign-in] Usage command failed: agy could not report usage; run agy to sign in.
    [agy-version] Usage command failed: agy 1.1.11 or later is needed for usage reports.
    [coderabbit] Usage command failed: read CodeRabbit usage.
    [gemini-client] Usage command failed: find the Gemini CLI's OAuth client to refresh the sign-in.
    [windsurf] Usage command failed: read the Windsurf plan cache.
   *[other] Usage command failed: { $command }.
}
error-usage-connect = Could not reach the usage service from this host.
error-usage-header = Saved sign-in cannot be sent as a header.
error-usage-json = Usage response was not the expected JSON.
error-usage-keychain-access = Reading this sign-in needs Keychain access, which macOS asks for.
error-usage-keychain-denied = Keychain access was denied, so this sign-in cannot be read.
# curl is a program's name.
error-usage-missing-curl = curl is not installed on this host, so usage cannot be read.
error-usage-mixed-secrets = Usage request mixes this machine's settings with the remote host's sign-in.
error-usage-network = Usage request failed or timed out.
error-usage-no-plan = This account has no plan with usage limits to show.
# [usage.providers] is a config table.
error-usage-not-signed-in = No sign-in found on this host. Set it up under [usage.providers] in the config.
# $kind is the step that failed. $operation is its English description,
# shown for a step not listed here.
error-usage-process = { $kind ->
    [cookie-key] Could not read the browser's cookie key.
    [keychain] Could not read a keychain item.
    [provider-command] Could not run a provider command.
    [remote-shell] Could not start the remote usage shell.
   *[other] Could not { $operation }.
}
error-usage-rate-limited = Usage is rate limited. Retrying later.
error-usage-rejected = Saved sign-in was rejected. Open the agent to sign in again.
error-usage-size = Usage output exceeded the size limit.
# $status is the HTTP status code.
error-usage-status = Usage service returned HTTP { $status }.
error-usage-timeout = Usage check timed out.
error-usage-unreachable = Could not reach this host over SSH to read usage.
error-usage-wsl-host-unsupported = WSL distributions can only be reached from Windows.
