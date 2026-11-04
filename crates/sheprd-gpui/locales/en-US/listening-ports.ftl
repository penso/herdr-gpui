# The status bar's listening ports: a globe and :3000 :5173 after it, each
# opening its page in a browser tab, directly or through an SSH tunnel.

# What a screen reader calls the group.
listening-ports-label = Listening ports
# A port's tooltip: what listens there, then what clicking does, on its
# own line. $process is the program's name; $address is like *:3000.
listening-ports-listening = { $process } listening on { $address }
listening-ports-unnamed-listening = A process listening on { $address }
# $url is the page's address.
listening-ports-open = Open { $url }
# $target is the SSH host.
listening-ports-open-tunnel = Open through an SSH tunnel to { $target }
listening-ports-unreachable = Not reachable from this machine
# A tunnel's close button.
listening-ports-close-tunnel = Close the SSH tunnel to { $target }:{ $port }
listening-ports-tunnel-closed = Closed the SSH tunnel to :{ $port }
listening-ports-tunnel-opening = A tunnel to that port is already opening
# $error says why.
listening-ports-open-failed = Could not open :{ $port }: { $error }
