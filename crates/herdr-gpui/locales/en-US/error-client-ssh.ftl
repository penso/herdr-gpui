# Failures reaching a host over SSH. The "bridge" is the SSH connection
# that carries the app's traffic to Herdr on the host.

error-client-ssh-cancelled = SSH connection cancelled
error-client-ssh-closed = SSH bridge closed; check host trust, authentication, and remote Herdr installation
error-client-ssh-invalid-target = invalid SSH target (options, controls, and passwords are forbidden)
error-client-ssh-known-hosts-unavailable = SSH has no user known hosts file to save the host key to
error-client-ssh-output-limit = SSH startup output exceeds limit
# $error is why, one of the error-client-ssh-failure messages.
error-client-ssh-refused = SSH bridge closed: { $error }
error-client-ssh-timeout = SSH discovery timed out
error-client-ssh-unsupported = SSH port forwarding is not supported on this platform

# Why SSH closed the bridge, shown after "SSH bridge closed:" or on its own.
error-client-ssh-failure-auth = authentication failed; load a key into the agent or open a ControlMaster connection in a terminal
error-client-ssh-failure-herdr-missing = no Herdr is installed on the host
error-client-ssh-failure-host-key = the host key is not trusted; verify it by running ssh to the host in a terminal
error-client-ssh-failure-local-network-denied = macOS denied Herdr access to your local network. If it just asked, allow it and try again; otherwise turn on Herdr in System Settings › Privacy & Security › Local Network
error-client-ssh-failure-no-route = there is no route to the host
error-client-ssh-failure-other = check host trust, authentication, and remote Herdr installation
error-client-ssh-failure-unreachable = the host is unreachable
