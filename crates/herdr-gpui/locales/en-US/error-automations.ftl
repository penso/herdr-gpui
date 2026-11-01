# Failures reading or driving herdr-automate, the automation program, on a
# host. $host is a host's name; $path is an automation script's path on it.

error-automations-approved = { $path } is already approved on { $host }
error-automations-failed = herdr-automate: the command failed
error-automations-json = herdr-automate printed output this app cannot read
error-automations-missing = herdr-automate is not installed on this host
# $platform is a system and processor, such as linux-x86_64.
error-automations-no-build = This app has no herdr-automate build for { $platform }
error-automations-no-script = { $host } has no automation script { $path }
error-automations-not-read = { $host } has not been read yet; try again in a moment
error-automations-off = Automations are turned off in Herdr UI: Settings, Sidebar, Show automations
# $found and $expected are report protocol version numbers.
error-automations-protocol = herdr-automate on this host speaks report protocol { $found }; this app reads { $expected }. Update this app.
# $error is herdr-automate's own message, kept as it is.
error-automations-refused = herdr-automate: { $error }
error-automations-run = Could not run herdr-automate: { $error }
error-automations-unknown-host = No connected host is named { $host }
error-automations-unknown-version = Could not read the version of the herdr-automate this app ships, so it cannot be installed
error-automations-unsupported-host = Automations cannot reach this kind of host
# $output is what `herdr-automate --version` printed, kept as it is.
error-automations-version = herdr-automate: unexpected version output: { $output }
