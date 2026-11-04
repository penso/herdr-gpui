# Failures reaching or serving the control socket that `sheprd <command>`
# uses to drive the running app. $path is the socket or named pipe; $error
# is the system's own words.

error-control-message-size = Control request or response exceeds { $limit } bytes
error-control-no-response = Sheprd closed the control connection without answering
error-control-pipe-foreign = { $path } is held by something other than this user's Sheprd, so it is not used
error-control-socket-in-use = Another Sheprd already serves { $path }
error-control-unavailable = Sheprd is not running, or its control socket { $path } is unreachable: { $error }
error-control-user = Could not identify the Windows user: { $error }
