# The activity bar's Ports section: every TCP port a process of this
# computer's user listens on, with buttons to open, pause, resume, or stop
# that process.

local-ports-title = Ports
local-ports-count = { $count ->
    [one] One port
   *[other] { $count } ports
}
local-ports-refresh = Refresh
# How the rows are ordered: by port number, busiest first, or largest first.
local-ports-sort-port = Port
local-ports-sort-cpu = CPU
local-ports-sort-memory = Memory
# The heading over listeners the operating system and its services run,
# set apart from the user's own servers.
local-ports-system = System
local-ports-unsupported = Listing and signalling this computer's ports needs macOS or Linux.
local-ports-reading = Reading this computer's ports…
local-ports-empty = Nothing on this computer is listening for connections.
# $error is why the list could not be read, in English.
local-ports-failed = Could not read the ports: { $error }
local-ports-pid = pid { $pid }
local-ports-paused = Paused
# $workspace is the name of the Herdr workspace whose terminal started it.
local-ports-workspace = Herdr: { $workspace }
# $project is the last folder of the process's working directory, $path the
# whole directory from the home folder (~/code/app).
local-ports-directory = { $project } · { $path }
local-ports-up-minutes = up { $minutes }m
local-ports-up-hours = up { $hours }h { $minutes }m
local-ports-up-days = up { $days }d { $hours }h
local-ports-open = Open
local-ports-pause = Pause
local-ports-resume = Resume
local-ports-stop = Stop
local-ports-report-stop = { $count ->
    [one] Stopped one process.
   *[other] Stopped { $count } processes.
}
local-ports-report-pause = { $count ->
    [one] Paused one process.
   *[other] Paused { $count } processes.
}
local-ports-report-resume = { $count ->
    [one] Resumed one process.
   *[other] Resumed { $count } processes.
}
local-ports-report-killed = { $count ->
    [one] One ignored the request to quit and was killed.
   *[other] { $count } ignored the request to quit and were killed.
}
local-ports-report-gone = { $count ->
    [one] One had already exited.
   *[other] { $count } had already exited.
}
local-ports-report-failed = { $count ->
    [one] One could not be signalled.
   *[other] { $count } could not be signalled.
}

# The question before stopping or pausing processes from the Ports section,
# or from `herdr-gpui ports stop|pause`.
local-ports-confirm-stop-title = { $count ->
    [one] Stop this process?
   *[other] Stop these { $count } processes?
}
local-ports-confirm-pause-title = { $count ->
    [one] Pause this process?
   *[other] Pause these { $count } processes?
}
local-ports-confirm-stop-note = Each is asked to quit, and killed if it is still running a few seconds later. Unsaved work in it may be lost.
local-ports-confirm-pause-note = Each keeps its port but stops answering until you resume it.
# $process is the process's name, $address where it listens (*:3000).
local-ports-confirm-row = { $process } (pid { $pid }) on { $address }
local-ports-confirm-cancel = Cancel
local-ports-confirm-stop = Stop
local-ports-confirm-pause = Pause
