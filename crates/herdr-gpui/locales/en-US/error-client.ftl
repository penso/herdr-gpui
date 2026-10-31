# Failures reaching a Herdr daemon (the "Herdr server") on this computer,
# over SSH, or in WSL, and talking to it once connected. Most are shown in
# a connection notice or inside another message's $error. An "endpoint" is
# the daemon's connection protocol; a "surface" is a pane's screen contents;
# a "boot" is one run of the daemon. $error is the failure underneath, as
# the system or a parser words it. $status is how a process exited, in the
# system's words. `herdr update` is a command and stays as it is.

# $version is the Herdr version on the host, as the host reports it. $fix is
# who must update: app (this app) or daemon (Herdr on the host).
error-client-bridge-incompatible = { $fix ->
    [app] Herdr on this host cannot serve this app; update Herdr UI
   *[daemon] Herdr on this host cannot serve this app; run `herdr update` and reconnect
}
error-client-bridge-incompatible-version = { $fix ->
    [app] Herdr (version { $version }) on this host cannot serve this app; update Herdr UI
   *[daemon] Herdr (version { $version }) on this host cannot serve this app; run `herdr update` and reconnect
}
error-client-cancelled = client stopped
error-client-catalog-limit = endpoint catalog exceeds storage limit
error-client-catalog-not-file = endpoint catalog is not a regular file
error-client-catalog-schema = invalid endpoint catalog schema
error-client-catalog-version-or-count = unsupported catalog version or too many profiles
# $version is the oldest Herdr release this app works with.
error-client-closed-before-welcome = Herdr server closed the connection without answering; it is likely older than { $version }, which this app requires. Run `herdr update` and reconnect
error-client-command-boot = command does not match a ready snapshot boot
error-client-disconnected = client is disconnected
error-client-empty-surface = surface dimensions must be nonzero
error-client-encoded-surface-before-baseline = encoded surface before baseline
# $server_version is the daemon's version; $generation and $ours are the
# protocol generations the daemon and this app speak. $fix is who must
# update: app (this app) or daemon (Herdr).
error-client-endpoint-generation = { $fix ->
    [app] Herdr server { $server_version } speaks endpoint generation { $generation }, this app speaks { $ours }; update Herdr UI
   *[daemon] Herdr server { $server_version } speaks endpoint generation { $generation }, this app speaks { $ours }; run `herdr update` and reconnect
}
error-client-event-receiver-dropped = event receiver dropped
error-client-expected-welcome = expected stable endpoint welcome
error-client-forward-control = could not create the port forward's private control directory
error-client-forward-exit = SSH port forward ended ({ $status }); check host trust, authentication, and the network
error-client-forward-local-port = no local port is free to forward to
error-client-forward-refused = SSH could not listen on a local port ({ $status })
error-client-forward-spawn = could not start SSH for the port forward
error-client-forward-timeout = SSH did not connect in time; check host trust and authentication
error-client-frame-length = invalid frame length
error-client-frame-prefix = invalid frame prefix
error-client-full = client command queue is full
error-client-geometry-limit = surface geometry exceeds endpoint limits
error-client-handshake-timeout = handshake/snapshot timed out
error-client-health-timeout = endpoint health check timed out
error-client-incompatible-codecs = incompatible endpoint generation/codecs
error-client-input-write-timeout = terminal input write timed out; reconnect required
error-client-invalid-git-dir = remote repository directory must be an absolute path
error-client-invalid-session = invalid session name
error-client-io = client I/O failed: { $error }
error-client-json = invalid endpoint JSON: { $error }
error-client-legacy-daemon = this Herdr server predates the endpoint protocol; run `herdr update` and reconnect
error-client-missing-boot-id = invalid client command: snapshot boot ID required
# $server_version is the version of Herdr on the SSH host.
error-client-missing-health-check = Herdr server { $server_version } on this SSH host lacks the health_check capability; run `herdr update` there and reconnect
# $server_version is the daemon's version.
error-client-missing-surface-interest = Herdr server { $server_version } lacks safe surface interest support; run `herdr update` and reconnect
error-client-no-local-socket = remote endpoints have no local socket path
error-client-partial-frame-timeout = partial frame timed out
error-client-patch-before-baseline = patch before baseline
error-client-profile-id = invalid or duplicate endpoint profile id
error-client-profile-label = invalid endpoint label
error-client-remote-command = remote command failed ({ $status })
error-client-remote-output = remote command returned unexpected output
error-client-request-timeout = endpoint request timed out; not replayed
error-client-response-boot = response boot mismatch
error-client-response-id = response ID mismatch
error-client-response-limit = response limit exceeded
error-client-response-missing-result = endpoint response carried neither a result nor an error
error-client-response-schema = invalid endpoint response
error-client-response-type = endpoint answered with a result of another method
error-client-script-cancelled = host script cancelled
error-client-script-connect = could not reach host for script: { $error }
# $stderr is the end of what the script printed on the host, as it printed it.
error-client-script-exit = host script failed ({ $status }): { $stderr }
error-client-script-input = could not read host script input
error-client-script-io = host script I/O failed
error-client-script-output = could not write host script output
error-client-script-output-limit = host script output exceeds limit
error-client-script-spawn = could not start host script
error-client-script-timeout = host script made no progress before its deadline
error-client-script-unsupported = host scripts require a Linux or macOS client
error-client-script-worker = host script worker panicked
error-client-selection-limit = endpoint selection exceeds storage limit
error-client-selection-not-file = endpoint selection is not a regular file
error-client-selection-schema = invalid endpoint selection schema
error-client-selection-unavailable = selected endpoint is absent or disabled
error-client-selection-version = unsupported endpoint selection version
error-client-snapshot-identity = endpoint boot changed or snapshot revision regressed; reconnect required
error-client-socket-closed = socket closed
error-client-storage-destination-not-file = storage destination is not a regular file
error-client-storage-path = invalid storage path
error-client-surface-before-snapshot = surface before snapshot
error-client-surface-encoding-not-negotiated = surface encoding was not advertised by endpoint
error-client-surface-identity = invalid surface identity/revision
error-client-unsolicited-response = unsolicited response
error-client-unsupported-method = method not advertised by endpoint
error-client-unsupported-surface-interest = surface interest capabilities not advertised by endpoint
error-client-upload-cancelled = SSH file transfer cancelled
# $error is why the upload failed; the temporary files it left on the host
# could not be removed either.
error-client-upload-cleanup = { $error }; remote temporary-file cleanup also failed
error-client-upload-cleanup-path = cleanup requires unchanged absolute paths returned by upload_files
error-client-upload-exit = SSH file transfer failed; check host trust, authentication, and remote storage
error-client-upload-io = SSH file transfer I/O failed
error-client-upload-name = file transfer requires a UTF-8 basename without controls
error-client-upload-not-file = file transfer source is not a regular file
error-client-upload-path-limit = SSH file transfer accepts at most 256 paths
error-client-upload-response = invalid or excessive SSH file transfer response
error-client-upload-size-overflow = file transfer total size exceeds u64
error-client-upload-source-changed = file transfer source changed length
error-client-upload-timeout = SSH file transfer made no progress for 30 seconds
error-client-upload-unsupported = SSH file transfer requires a Linux or macOS client
error-client-welcome-kind = expected endpoint.welcome.v1
# $code and $message are the daemon's reason for refusing the connection,
# in its own words.
error-client-welcome-rejected = { $code }: { $message }
