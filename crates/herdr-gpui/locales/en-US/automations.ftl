# The activity bar's Automations section: the scripts herdr-automate runs on
# each host, their runs, and the dialogs that approve a script or install
# herdr-automate. $host is a host's name; $name is a script's name, as its
# author wrote it.

automations-title = Automations
automations-mode-hosts = Hosts
automations-mode-timeline = Timeline
automations-updating = Updating…
automations-refresh = Refresh
# The header's meter of tokens agents used today, against the hosts' caps.
# $used and $cap are short counts, such as 340k or 1.2M.
automations-meter = { $used } / { $cap } tokens today
# What this machine is called when nothing else names it: a host card's
# title, and inside sentences such as "this computer has not been read yet".
automations-this-computer = This computer
automations-this-computer-name = this computer

# A host's card.
automations-no-hosts = No host is connected.
automations-reading = Reading…
automations-missing-here = herdr-automate is not installed here, and this app does not ship it.
automations-missing-host = herdr-automate is not installed on this host.
automations-no-scripts = No scripts yet. Put one in .herdr/automations/ of a repository, check it with `herdr-automate check`, then approve it here.
automations-enable = Enable automations
# $version is herdr-automate's version, such as 0.4.2.
automations-update-to = Update to { $version }
automations-daemon-up = Daemon up
automations-start-daemon = Start daemon
automations-resume-all = Resume all
automations-pause-all = Pause all
# Facts after a host's name, joined with " · ".
automations-daemon-version = daemon { $version }
# $duration is how long the daemon has run, such as 2h 04m.
automations-daemon-uptime = up { $duration }
automations-daemon-stopped = herdr-automate { $version } · daemon not running
automations-all-paused = paused

# A host in the list beside a page.
automations-host-missing = Not installed
automations-host-unreachable = Unreachable
automations-host-daemon-down = Daemon down
automations-host-update = Update
# $schedule is the script's schedule as the daemon words it; $tokens a
# short count.
automations-row-detail = { $schedule } · { $tokens } tokens today

# A script's state, beside its schedule. $reason and $why are the daemon's
# own words.
automations-no-trigger = No trigger
automations-running = Running
automations-awaiting = { $reason } · awaiting approval
automations-awaiting-new = New or changed · awaiting approval
automations-invalid = Does not load
automations-paused = Paused
automations-disabled = Disabled
automations-failed = Failed: { $why }
automations-failed-unfinished = Failed: the run did not finish
# $when is a time or a date, such as 03:00 or Wed 03:00.
automations-next = Next { $when }
automations-runs-when-started = Runs when started
# $elapsed is how long the run has run, such as 12m04s.
automations-progress-step = Step { $step }/{ $steps } · { $elapsed }
automations-progress-starting = Starting · { $elapsed }
# How a run went, short: "Wed 03:00 ✓ 41m". $when is a time or a date;
# $mark is ✓, ✕, –, or …; $took a duration.
automations-result = { $when } { $mark } { $took }
automations-result-running = { $when } { $mark }
# Facts of a finished run, joined with " · ".
automations-agents = { $count ->
    [one] 1 agent
   *[other] { $count } agents
}
# $tokens is a short count, such as 340k.
automations-tokens = { $tokens } tokens
automations-interrupted = interrupted
# A script's limits. $tokens is a short count; $time a duration, such as 1h 00m.
automations-caps = Caps: { $agents ->
    [one] 1 agent
   *[other] { $agents } agents
} per run · { $tokens } tokens a day · { $time } per run

# What a button says while the host works on it.
automations-doing-approve = Approving…
automations-doing-pause = Pausing…
automations-doing-resume = Resuming…
automations-doing-run = Starting…
automations-doing-start = Starting the daemon…
automations-doing-stop = Stopping…
# A command that failed on a host. $error says why.
automations-action-failed = { $host }: { $error }

# The timeline.
automations-group-needs-you = Needs you
automations-group-now = Now
automations-group-next = Next
automations-group-today = Today
automations-group-yesterday = Yesterday
automations-group-this-week = This week
automations-group-older = Older
automations-all-hosts = All hosts
automations-recent-count = { $count ->
    [one] 1 recent run
   *[other] { $count } recent runs
}
automations-timeline-empty = Nothing has run, and nothing is scheduled.
automations-older-runs = Each host keeps its last { $count } runs; older ones are in `herdr-automate runs` on the host.
automations-review-reason = { $why } · review and approve
automations-review-new = New or changed · review and approve
automations-review = Review

# A script's and a run's pages.
automations-page-gone = This script is not on its host's last reading.
automations-run-number = Run #{ $run }
automations-earlier-lines = { $count ->
    [one] … 1 earlier line
   *[other] … { $count } earlier lines
}
automations-plan = Plan
automations-no-plan = The host could not plan this script.
automations-recent-runs = Recent runs
# $tokens is a short count, such as 1.2M.
automations-tokens-days = Tokens, last { $days } days · { $tokens }
automations-review-approve = Review and approve
automations-run-now = Run now
automations-stop-run = Stop run
automations-pause = Pause
automations-resume = Resume
automations-no-runs = No run among the host's recent runs.
automations-reading-run = Reading the run…
automations-hide-log = Hide log
automations-full-log = Full log
automations-no-step = No step has started.
automations-usage = Usage
automations-usage-tokens = Tokens
automations-usage-cpu = CPU
automations-usage-memory = Peak memory
automations-usage-agents = Agents
automations-unknown = unknown
# A step's line. $summary is the daemon's; $took a duration, such as 41s.
automations-step = { $summary } · { $took }
automations-step-exit = { $summary } · { $took } · exit { $code }
automations-open-pane = Open pane
# $worktree is a checkout's path on the host.
automations-worktree = Worktree { $worktree }
automations-reading-log = Reading the log…
automations-log-truncated = The end of a longer log; the host keeps the rest.
# Facts of a run, joined with " · ". $when is a time or a date.
automations-run-started = Started { $when } { $why ->
    [schedule] on schedule
    [late] late, after the daemon was down
   *[manual] by hand
}
automations-run-trigger = trigger { $name }
automations-run-took = took { $duration }
automations-run-running = running for { $duration }

# How long until something fires, in the timeline's Next group.
automations-in-now = now
automations-in-minutes = in { $minutes } min
automations-in-hours = { $minutes ->
    [0] in { $hours } h
   *[other] in { $hours } h { $minutes } min
}
# A duration in its two largest units: 3s, 12m04s, 2h 04m, 4 d. $seconds
# and $minutes after a larger unit are two digits.
automations-span-seconds = { $seconds }s
automations-span-minutes = { $minutes }m{ $seconds }s
automations-span-hours = { $hours }h { $minutes }m
automations-span-days = { $days } d

# A size in bytes. $value is a number, such as 3.1.
automations-size = { $unit ->
    [b] { $value } B
    [kb] { $value } KB
    [mb] { $value } MB
    [gb] { $value } GB
   *[tb] { $value } TB
}

# Notifications.
automations-notify-failed = { $name } failed on { $host }
automations-notify-unfinished = the run did not finish
automations-notify-approval = { $name } waits for approval on { $host }
automations-notify-approval-body = Review its plan in Automations before it runs.

# Approving a script.
automations-approve-title = Approve { $name } on { $host }?
automations-approve-note = Approving lets it run on its schedule with exactly this content. Any change to the file needs approval again.
automations-approve-host = Host: { $host }
# $path is the script's path on the host.
automations-approve-file = File: { $path }
# $hash is the script's SHA-256.
automations-approve-content = Content: sha256 { $hash }
automations-approve-no-plan = The host reported no plan for this script, so there is nothing to review and it cannot be approved here. Check it with `herdr-automate check` on the host.
automations-approve-confirm = Approve on { $host }
automations-cancel = Cancel
automations-close = Close

# Installing or updating herdr-automate on a host.
automations-enable-title = Enable automations on { $host }?
automations-update-title = Update automations on { $host }?
automations-enable-note = Nothing is written outside your home directory on the host, and nothing runs as root.
automations-enable-checking = Checking the host's platform…
automations-enable-installing = Copying and starting herdr-automate…
# $platform is a system and processor, such as linux-x86_64; $size a size,
# such as 3.1 MB; $hash the start of the build's SHA-256.
automations-enable-copies = Copies herdr-automate { $version } for { $platform } ({ $size }, sha256 { $hash }…) from this app.
# $program is the command's name, herdr-automate.
automations-enable-into = Into ~/.local/share/herdr-automate/bin/{ $version }/herdr-automate, linked from ~/.local/bin/{ $program }.
automations-enable-then-update = Then stops the running daemon once its runs finish (`herdr-automate shutdown`, up to about 40 s) and starts the new one (`herdr-automate start`).
automations-enable-then-install = Then installs a user service with `herdr-automate install-service` (a systemd user unit on Linux, a launchd agent on macOS) and starts the daemon.
automations-enable-confirm = Enable on { $host }
automations-update-confirm = Update on { $host }
# After installing. $message is what herdr-automate said, kept as it is.
automations-enabled = Automations are on for { $host }.
automations-enabled-said = { $host }: { $message }
