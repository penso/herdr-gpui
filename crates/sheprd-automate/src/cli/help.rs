pub const HELP: &str = "\
sheprd-automate: scheduled and manual scripts for Herdr

Writing scripts:
  check FILE                 Load a script and print its plan, or what is wrong
  steps                      List the steps a script can call

Running them on this host:
  approve FILE [--hash H]    Approve a script's exact text after reading its plan
  status | list              Scripts, their state and next firing, recent runs
  run-now SCRIPT [--trigger NAME|N]
                             Start a trigger by hand (any trigger, any time)
  pause|resume SCRIPT|--all  Stop or restart scheduling
  disable|enable SCRIPT      Turn a script off or back on
  runs [SCRIPT] | runs show RUN
                             Recent runs; one run's steps
  log RUN [--step N] | log --daemon [--lines N] [--follow]
                             A run's output; the daemon's own log
  stop RUN                   Stop a running run
  usage [SCRIPT] [--since 24h|7d|30d|all] [--by script|run|step]
                             Tokens, CPU and memory
  repo add|remove|list [PATH]
                             Repositories whose .herdr/automations are loaded
  watch                      A live dashboard; Ctrl-C to leave

The daemon:
  serve                      Run the scheduler in the foreground
  start | shutdown           Start it detached, or stop it and its runs
  install-service | uninstall-service
                             Keep it running across logins and crashes

Every command takes --json; errors are then {\"ok\": false, \"message\": ...}.

A script is a Luau file in a repository's .herdr/automations or in the
personal folder (~/.config/herdr-automate/automations). Its top level
declares caps and triggers; each trigger's function calls steps, in order.
Steps are a script's only effects: there is no clock, randomness, file,
network, process or require.

  herdr.caps { agents_per_run = 1, tokens_per_day = 2_000_000, wall_time = \"3h\" }

  herdr.at(\"03:00\", { days = \"mon-fri\", tz = \"Europe/Paris\" }, function(fire) ... end)
  herdr.every(\"6h\", function(fire) ... end)
  herdr.cron(\"*/15 9-17 * * mon-fri\", function(fire) ... end)
  herdr.manual({ name = \"deploy\" }, function(fire) ... end)

Every trigger can also be started by hand; herdr.manual has no schedule.
Triggers take an optional { name = \"...\" }.

A step that fails stops the run, unless called with { check = false }: then
it returns { ok, log_tail } (and run_id for agent) and the script decides.
Steps also take { timeout = \"30m\" }.

What a script cannot know reaches it as tokens it can pass along but not
read: fire.date is \"{date}\", fire.time is \"{time}\", and a result's log_tail
is \"{log_tail:N}\" for step N. The runner fills them in when it runs a step;
in a sh command they become \"$HERDR_...\" variables. So the plan, which tries
every combination of unchecked results, is every path the script can take,
and a run that leaves it is halted.

A script runs only once approved on this host, and only in the exact form
approved: any change, including from a git pull, needs approval again. An
agent cannot approve its own script: approval needs a person at a terminal
or Sheprd's dialog.

Exit status: 0 success, 1 failure (an invalid script, an unknown run, ...),
2 usage.
";
