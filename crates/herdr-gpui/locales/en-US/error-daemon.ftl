# Starting or talking to the herdr server. Terminal > Reconnect is a menu
# command.

error-daemon-cancelled = daemon startup cancelled
# $status is how the server ended, in the system's words.
error-daemon-exited = herdr server exited before accepting connections: { $status }. Check the Herdr server log.
# The server reported an error without saying what it was.
error-daemon-response = Invalid daemon error
# $error is the system's text.
error-daemon-spawn = Could not start herdr server: { $error }. Use Terminal > Reconnect to retry.
# $path is the server's socket.
error-daemon-timeout = Timed out waiting for herdr server at { $path }. Check the Herdr server log and use Terminal > Reconnect.
