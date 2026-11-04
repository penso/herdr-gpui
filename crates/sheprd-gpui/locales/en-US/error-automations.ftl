# Failures reading or driving sheprd-automate, the automation program, on a
# host. $host is a host's name; $path is an automation script's path on it.

error-automations-approved = { $path } is already approved on { $host }
error-automations-failed = sheprd-automate: the command failed
error-automations-json = sheprd-automate printed output this app cannot read
error-automations-missing = sheprd-automate is not installed on this host
# $platform is a system and processor, such as linux-x86_64.
error-automations-no-build = This app has no sheprd-automate build for { $platform }
error-automations-no-script = { $host } has no automation script { $path }
error-automations-not-read = { $host } has not been read yet; try again in a moment
error-automations-off = Automations are turned off in Sheprd: Settings, Sidebar, Show automations
# $found and $expected are report protocol version numbers.
error-automations-protocol = sheprd-automate on this host speaks report protocol { $found }; this app reads { $expected }. Update this app.
# $error is sheprd-automate's own message, kept as it is.
error-automations-refused = sheprd-automate: { $error }
error-automations-run = Could not run sheprd-automate: { $error }
error-automations-unknown-host = No connected host is named { $host }
error-automations-unknown-version = Could not read the version of the sheprd-automate this app ships, so it cannot be installed
error-automations-unsupported-host = Automations cannot reach this kind of host
# $output is what `sheprd-automate --version` printed, kept as it is.
error-automations-version = sheprd-automate: unexpected version output: { $output }
