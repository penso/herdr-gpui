# Failures attaching directly to one Herdr terminal.

error-client-terminal-attach-busy = too many terminal scrolls are waiting
error-client-terminal-attach-ended = the terminal attach has ended
error-client-terminal-attach-io = terminal attach I/O failed
error-client-terminal-attach-protocol = terminal attach framing failed
error-client-terminal-attach-refused = Herdr refused the terminal attach
# $reason is Herdr's own message, kept as it is.
error-client-terminal-attach-refused-reason = Herdr refused the terminal attach: { $reason }
# $version is a protocol version number.
error-client-terminal-attach-rejected = Herdr refused the terminal hello (protocol { $version })
error-client-terminal-attach-scroll = a terminal scroll must move at least one line
error-client-terminal-attach-size = terminal size must be nonzero
error-client-terminal-attach-unexpected = Herdr answered the terminal hello with an unexpected message
