# Fly Sprites as cloud devices. $detail is the Sprites API's own text, kept
# as it is. [sprites] is the config table; api_url is its key. A sprite is
# what Sprites calls one machine.

# $field names a setting, such as sprites.api_url.
error-remote-sprites-url = { $field } must be https (http only on loopback) with no credentials, query, or fragment.
error-remote-sprites-missing-token = Sprites needs an API token. Save one in Settings > Cloud devices.
error-remote-sprites-authentication = Sprites rejected the API token.
error-remote-sprites-forbidden = Sprites denied access. Check the token's organization.
# $name is the sprite's name.
error-remote-sprites-gone = The sprite { $name } no longer exists.
# $code is the HTTP status code, such as 500.
error-remote-sprites-status = Sprites request failed (HTTP { $code })
error-remote-sprites-status-message = Sprites request failed (HTTP { $code }): { $detail }
error-remote-sprites-network = Sprites network request failed or timed out.
error-remote-sprites-read = Sprites response could not be read within the size/time limit.
error-remote-sprites-size = Sprites response exceeded the size limit.
error-remote-sprites-json = Invalid Sprites JSON response.
error-remote-sprites-header = Invalid Sprites authorization header.
error-remote-sprites-output = Could not write the command's output.
error-remote-sprites-executable = Could not find this app's executable to relay through.
error-remote-sprites-cancelled = Cancelled.
error-remote-sprites-account = This device belongs to a Sprites account that is no longer configured in [sprites] api_url.
