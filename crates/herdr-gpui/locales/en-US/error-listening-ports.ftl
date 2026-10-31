# Why a host's listening ports could not be read, or a tunnel to one did not open.

error-listening-ports-scan = Could not read listening ports on this host.
error-listening-ports-tool = Neither ss nor lsof is installed on this host, so listening ports cannot be read.
error-listening-ports-unsupported = Listening ports cannot be read on this platform.
# $status is how ssh exited, in the system's words.
error-listening-ports-tunnel-exited = SSH ended before the tunnel opened ({ $status }).
error-listening-ports-tunnel-port = No free local port for an SSH tunnel.
error-listening-ports-tunnel-start = Could not start ssh for a tunnel.
error-listening-ports-tunnel-timeout = The SSH tunnel did not open in time.
