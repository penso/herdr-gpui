# boxd machines as cloud devices. boxd is a product name and stays lowercase.
# $detail is the boxd API's own text, kept as it is. [boxd] is the config
# table; api_url is its key.

# $field names a setting, such as boxd.api_url.
error-remote-boxd-url = { $field } must be an https origin (http only on loopback) with no credentials, path, query, or fragment.
error-remote-boxd-missing-key = boxd needs an API key. Save one in Settings > Cloud devices.
error-remote-boxd-authentication = boxd rejected the API key.
error-remote-boxd-forbidden = boxd denied access. Check the API key's organization.
# $name is the machine's name or ID.
error-remote-boxd-gone = The boxd machine { $name } no longer exists.
error-remote-boxd-failed = The boxd machine { $name } failed and cannot start.
error-remote-boxd-not-ready = The boxd machine { $name } did not start in time.
# $code is a gRPC status code, such as 5.
error-remote-boxd-status-grpc = boxd request failed (gRPC status { $code })
error-remote-boxd-status-grpc-message = boxd request failed (gRPC status { $code }): { $detail }
# $code is an HTTP status code, such as 502.
error-remote-boxd-status-http = boxd request failed (HTTP { $code })
error-remote-boxd-network = boxd network request failed or timed out.
error-remote-boxd-read = boxd response could not be read within the size/time limit.
error-remote-boxd-size = boxd response exceeded the size limit.
error-remote-boxd-wire = Invalid boxd API response.
error-remote-boxd-header = Invalid boxd authorization header.
error-remote-boxd-stream = The boxd exec stream failed.
error-remote-boxd-output = Could not write the command's output.
error-remote-boxd-executable = Could not find this app's executable to relay through.
error-remote-boxd-cancelled = Cancelled.
error-remote-boxd-account = This device belongs to a boxd account that is no longer configured in [boxd] api_url.
