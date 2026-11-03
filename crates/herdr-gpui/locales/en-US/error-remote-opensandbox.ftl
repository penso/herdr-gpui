# Sandboxes on a self-hosted OpenSandbox server as cloud devices. $detail
# is the server's own text, kept as it is. [opensandbox] is the config
# table; api_url is its key.

# $field names a setting, such as opensandbox.api_url.
error-remote-opensandbox-url = { $field } must be https (http only on loopback) with no credentials, query, or fragment.
error-remote-opensandbox-missing-key = OpenSandbox needs an API key. Save one in Settings > Cloud devices.
error-remote-opensandbox-authentication = OpenSandbox rejected the API key.
error-remote-opensandbox-forbidden = OpenSandbox denied access.
# $id is the sandbox's ID on the server.
error-remote-opensandbox-gone = The sandbox { $id } no longer exists.
error-remote-opensandbox-stopped = The sandbox stopped: { $detail }
error-remote-opensandbox-start-timeout = The sandbox did not start within 5 minutes.
# $code is the HTTP status code, such as 500.
error-remote-opensandbox-status = OpenSandbox request failed (HTTP { $code })
error-remote-opensandbox-status-message = OpenSandbox request failed (HTTP { $code }): { $detail }
error-remote-opensandbox-network = OpenSandbox network request failed or timed out.
error-remote-opensandbox-read = OpenSandbox response could not be read within the size/time limit.
error-remote-opensandbox-size = OpenSandbox response exceeded the size limit.
error-remote-opensandbox-json = Invalid OpenSandbox JSON response.
error-remote-opensandbox-header = Invalid OpenSandbox request header.
error-remote-opensandbox-shell = The sandbox's shell refused the command: { $detail }
error-remote-opensandbox-output = Could not write the command's output.
error-remote-opensandbox-executable = Could not find this app's executable to relay through.
error-remote-opensandbox-cancelled = Cancelled.
error-remote-opensandbox-account = This device belongs to an OpenSandbox server that is no longer configured in [opensandbox] api_url.
