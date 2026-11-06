# The Overview page: every host's agents, sorted by what needs the user.
# Agent, workspace, branch, and host names are shown as they are.

overview-title = Overview
# The summary beside the title, each left out at zero.
overview-stat-needs = { $count } need you
overview-stat-working = { $count } working
overview-stat-ready = { $count } ready
overview-pinned = Pinned
overview-needs-you = Needs you
overview-ready-for-review = Ready for review
overview-working = Working
overview-idle = Idle
# Shelves under the list that fold, each followed by its count.
overview-snoozed = Snoozed
overview-settled = Settled
overview-closed = Closed
overview-alerts = Recent alerts
# The row agents idle for over an hour fold into.
overview-long-idle = +{ $count } idle
# A row's second line: its workspace, then the agent and its state.
overview-row-detail = { $workspace } · { $detail }
# A snoozed row's second line; $time is when it wakes, such as 18:00 or Mon 09:00.
overview-wakes = Wakes { $time }
# A tag on a row whose snooze ended.
overview-woke = Woke
# A failed orchestrator run; $title is its issue or pull request.
overview-run-failed = Run failed: { $title }

# Row actions.
overview-reopen = Reopen
overview-forget = Forget
overview-snooze = Snooze
overview-settle = Settle
overview-pin = Pin
overview-unpin = Unpin
overview-unsettle = Unsettle
overview-wake-now = Wake now
overview-open = Open
overview-reply = Reply
overview-snooze-until = Snooze until
overview-preset-hour = In 1 hour
overview-preset-hours = In 3 hours
overview-preset-evening = This evening
overview-preset-tomorrow = Tomorrow
overview-preset-next-week = Next week
# A time on another day than today: $day is the weekday, $time the clock.
overview-weekday-time = { $day ->
        [mon] Mon
        [tue] Tue
        [wed] Wed
        [thu] Thu
        [fri] Fri
        [sat] Sat
       *[sun] Sun
    } { $time }
# How long a row has been in its state, in its largest unit.
overview-elapsed-now = now
overview-elapsed-minutes = { $minutes }m
overview-elapsed-hours = { $hours }h
overview-elapsed-days = { $days }d

overview-empty-title = No agents to look at
overview-empty = The Overview gathers the agents on every host this window connects to: those waiting on you and failed runs first, then finished work ready for review, working agents, and idle ones. Reply to a row without opening it, or snooze or settle it to put it aside; an agent you close waits under Closed to be reopened.

# The host column.
overview-hosts = Hosts
overview-connecting = Connecting…
overview-not-connected = Not connected
overview-agents = { $total ->
        [0] No agents
        [one] { $total } agent
       *[other] { $total } agents
    }
overview-agents-working = { $total ->
        [one] { $total } agent
       *[other] { $total } agents
    } · { $working } working
# Meters on a host card, abbreviated.
overview-meter-cpu = CPU
overview-meter-memory = MEM
overview-meter-gpu = GPU
overview-meter-vram = VRAM

# Reopening a closed agent.
overview-session-gone = The agent's original session is no longer connected
overview-tab-opening = Another tab is still opening
overview-host-switch = Cannot switch to the agent's host
overview-host-timeout = The agent's host did not answer in time
overview-workspace-closed = The agent's workspace is closed
overview-reopen-failed = The agent did not reopen: { $error }

# Selecting rows with Shift or Cmd/Ctrl click, and the bar that acts on them.
overview-selected = { $count } selected
overview-selection-clear = Clear

# Replying to agents without opening them.
overview-reply-title = { $count ->
        [one] Reply to { $name }
       *[other] Reply to { $count } agents
    }
overview-reply-note = Sent when the agent is back at its prompt, never on top of an approval. Replies wait in this window until then.
overview-reply-field = Reply
overview-reply-send = Queue reply
overview-reply-cancel-dialog = Cancel
overview-reply-too-long = This reply is too long to send. Shorten it, or open the agent and paste it there.
# Where a queued reply stands, shown before its text under the row.
overview-reply-waiting = Waiting for the prompt
overview-reply-sending = Sending
overview-reply-failed = Not sent: { $error }
overview-reply-cancel = Cancel
overview-reply-dropped = The agent is gone, so this reply was not sent: { $text }
overview-reply-unsupported = Replies need the herdr command on the agent's host, which this client cannot run
overview-reply-skipped = { $count ->
        [one] { $count } selected row cannot take a reply
       *[other] { $count } selected rows cannot take a reply
    }
overview-reply-not-queued = { $count ->
        [one] { $count } reply was not queued
       *[other] { $count } replies were not queued
    }

# An alert in the Recent alerts shelf whose pane is gone.
overview-alert-gone = That pane is no longer there
# The Notification History command, when no host kept an alert yet.
overview-alerts-empty = No alerts yet
