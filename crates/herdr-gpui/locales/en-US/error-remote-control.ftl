# Remote control's failures, shown in Settings > Remote and the pairing
# dialog. remote.port, remote.certificate, and remote.device_idle_days are
# config keys; herdr-remote-control is a program name.

# $key is the certificate's config key: cert or key.
error-remote-control-certificate-path = remote.certificate { $key } must be an absolute UTF-8 path
error-remote-control-idle-days = remote.device_idle_days must be between 0 (never) and 365
# $operation is what failed on the file or folder $path.
error-remote-control-io = { $operation ->
    [create] Could not create { $path }
    [write] Could not write { $path }
   *[remove] Could not remove { $path }
}
error-remote-control-no-state-dir = Herdr UI has no state directory for remote control
error-remote-control-pairing-address = The selected pairing address is not served by this computer
error-remote-control-pairing-url = The daemon supplied an invalid pairing URL
# $path is a file the service definition would name.
error-remote-control-path-text = { $path } cannot go in a service definition
error-remote-control-port = remote.port must be between 1 and 65535
error-remote-control-program-missing = herdr-remote-control was not found beside Herdr UI or on PATH
error-remote-control-qr = The pairing link does not fit in a QR code
# $program is the service manager command, such as launchctl or systemctl;
# $status is how it ended, in the system's words; $stderr is its own output.
error-remote-control-service = { $program } failed ({ $status }): { $stderr }
# $program is a command name, such as launchctl.
error-remote-control-spawn = Could not run { $program }
error-remote-control-unexpected = The remote control daemon gave an answer of the wrong kind
error-remote-control-unknown-user = Windows did not say who the current user is
error-remote-control-unsupported = Remote control cannot run as a service on this platform
error-remote-control-connect = could not reach the remote control daemon
# $message is the daemon's own words.
error-remote-control-daemon = the remote control daemon refused: { $message }
error-remote-control-no-answer = the remote control daemon closed the connection without answering

# Enrolling a host's remote control daemon for a phone paired elsewhere.
error-remote-control-enroll-apns-size = the APNs files sent are too large
# $path is the daemon's config file on the host.
error-remote-control-enroll-config = { $path } is not valid TOML
error-remote-control-enroll-no-home = this host has no home directory
error-remote-control-enroll-not-started = the remote control daemon on this host did not start
error-remote-control-enroll-outdated = the remote control daemon on this host is older than Herdr UI; update it
error-remote-control-enroll-unexpected = the remote control daemon gave an answer of the wrong kind
