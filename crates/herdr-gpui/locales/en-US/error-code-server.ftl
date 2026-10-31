# The VS Code server the app reaches or starts for code tabs.

# $status is how the process ended, in the system's words.
error-code-server-exited = VS Code stopped ({ $status }).
error-code-server-no-data-dir = There is no folder for the app's data. Set HOME.
error-code-server-no-free-port = Could not find a free port for VS Code.
# $status is an HTTP status code, such as 404.
error-code-server-not-server = This address is not a VS Code server (HTTP { $status }).
error-code-server-port-check = Could not check port { $port } on 127.0.0.1.
# `port` is a config key under the [code] table.
error-code-server-port-taken = Port { $port } on 127.0.0.1 is in use by another program. Quit it, or remove `port` under [code] in config-gpui.local.toml to pick another one; VS Code then starts without the settings it kept for this port.
error-code-server-silent = VS Code did not answer within { $seconds } seconds.
# $program is the path of the program that did not start.
error-code-server-spawn = Could not start { $program }.
# $status is an HTTP status code, such as 500.
error-code-server-status = The VS Code server answered HTTP { $status }.
error-code-server-token-file = Could not use the VS Code token file { $path }.
error-code-server-token-random = Could not make a VS Code connection token.
# ?tkn= is the part of the address that carries the token.
error-code-server-token-refused = The server refused the connection token. Use the address it prints, with its ?tkn=.
# $address is the server's host and port. $reason is why no answer came.
error-code-server-unreachable = { $reason ->
    [refused] Connection refused at { $address }.
    [timeout] No answer within 3 seconds at { $address }.
    [unknown-host] Unknown host at { $address }.
   *[other] No connection at { $address }.
}
error-code-server-watch = Could not watch the VS Code process.
error-code-server-worker = Could not start the VS Code worker.
