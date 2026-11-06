# Daytona sandboxes as cloud devices. $detail is Daytona's own text, kept as
# it is. [daytona] is the config table; api_url is its key.

# $field names a setting, such as daytona.api_url.
error-remote-daytona-url = { $field } must be https (http only on loopback) with no credentials, query, or fragment.
error-remote-daytona-missing-key = Daytona needs an API key. Save one in Settings > Cloud devices.
error-remote-daytona-authentication = Daytona rejected the API key.
error-remote-daytona-forbidden = Daytona denied access. Check the API key's permissions.
# $code is the HTTP status code, such as 500.
error-remote-daytona-status = Daytona request failed (HTTP { $code })
error-remote-daytona-status-message = Daytona request failed (HTTP { $code }): { $detail }
error-remote-daytona-network = Daytona network request failed or timed out.
error-remote-daytona-read = Daytona response could not be read within the size/time limit.
error-remote-daytona-size = Daytona response exceeded the size limit.
error-remote-daytona-json = Invalid Daytona JSON response.
error-remote-daytona-header = Invalid Daytona authorization header.
error-remote-daytona-sandbox = Daytona sandbox failed: { $detail }
error-remote-daytona-deleted = The Daytona sandbox was deleted.
error-remote-daytona-snapshot = Daytona could not build the snapshot: { $detail }
error-remote-daytona-start-timeout = The Daytona sandbox did not start within 10 minutes.
error-remote-daytona-cancelled = Cancelled.
error-remote-daytona-gateway = Daytona returned an SSH command this app does not understand.
error-remote-daytona-ssh-missing = Daytona devices need ssh on the PATH; it could not be run.
error-remote-daytona-gateway-file = Could not prepare the SSH command for the Daytona gateway.
error-remote-daytona-account = This device belongs to a Daytona account that is no longer configured in [daytona] api_url.
