# The log window: the app's own log and, when automations run, each host's
# automation daemon log. Log records themselves are never translated.

log-window-title = Logs
log-window-search-placeholder = Search, namespace:herdr_gpui target:terminal_painter
# The minimum level menu's button; $level is a log level such as INFO, kept as is.
log-window-minimum = Minimum: { $level } v
log-window-minimum-menu = Minimum level
log-window-pause = Pause
log-window-resume = Resume tail
log-window-copy = Copy
log-window-working = Working...
log-window-export = Export...
log-window-entries = Log entries
log-window-empty = No matching logs.
# The status line. $path is where the app's log file is saved.
log-window-saved = Saved to { $path }. Review before sharing.
log-window-not-saved = Not saved: no state directory. Review before sharing.
log-window-choose-destination = Choose an export destination...
log-window-preparing-clipboard = Preparing clipboard...
log-window-export-cancelled = Export cancelled.
log-window-no-save-dialog = Unable to open save dialog.
log-window-copied = Filtered logs copied. Review before sharing.
log-window-exported = Filtered logs exported. Review before sharing.
log-window-export-failed = Export failed: { $error }
log-window-unreadable = Unable to read logs: { $error }
# Which log is shown: the app's own, or a host's automation daemon's.
log-window-source = Source
log-window-source-app = App
log-window-source-automations = Automations on { $host }
log-window-reading = Reading…
# $source is log-window-source-automations.
log-window-automations-off = { $source } · not read while automations are turned off
log-window-log-tail = { $source } · the end of a longer log
log-window-log-whole = { $source } · the whole log
log-window-host-unreadable = Unable to read { $host }: { $error }
# The status bar: rows shown, records dropped, whether the tail is followed,
# and $status, one of the status messages above.
log-window-summary-live = { $shown } shown | { $dropped } dropped | LIVE | { $status }
log-window-summary-paused = { $shown } shown | { $dropped } dropped | PAUSED | { $status }
