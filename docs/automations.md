# Automations

Automations are short Luau scripts that run steps on a host on a schedule or
when someone starts them: pull a branch, run validation, start an agent in a
new worktree with a prompt. They are meant to be written by an agent from a
sentence ("every day at 3am, pull main, run validation, then start an agent
with this prompt") and reviewed by a person before they run.

`sheprd-automate` runs them: a small daemon per host that keeps going when
Sheprd quits and when Herdr itself is killed, and a command line for
people, agents, and Sheprd's Automations section.

Sheprd's Automations section can be turned off in Settings, Sidebar, Show
automations (`[sidebar] automations = false`). Sheprd then leaves the
section off the activity bar and neither reads nor starts any host's daemon;
daemons already running keep running their approved scripts.

## A script

Scripts live in `.herdr/automations/NAME.luau` in a repository, or in
`~/.config/herdr-automate/automations/` for personal ones.

```lua
herdr.caps { agents_per_run = 2, tokens_per_day = 2_000_000, wall_time = "3h" }

herdr.at("03:00", { tz = "Europe/Paris" }, function(fire)
  pull("main")
  local v = sh("just ci", { check = false })
  if not v.ok then
    agent {
      harness = "claude",
      worktree = "fix-validation-" .. fire.date,
      prompt = "Nightly validation failed on main:\n" .. v.log_tail .. "\nFix it.",
    }
  end
  agent { harness = "claude", worktree = "nightly-" .. fire.date, prompt = "..." }
end)

herdr.manual({ name = "deploy-preview" }, function(fire)
  sh("./scripts/deploy-preview.sh")
  notify("Preview deployed")
end)
```

`sheprd-automate check nightly.luau` prints:

```text
nightly
  caps: 2 agents per run · 2,000,000 tokens/day · 3h per run
  daily at 03:00 (Europe/Paris)
    1. pull main
    2. sh `just ci` (arbitrary command) · the script handles failure
       └ on failure, instead:
         1. agent claude in worktree fix-validation-{date}, prompt 55 chars
         2. agent claude in worktree nightly-{date}, prompt 3 chars
    3. agent claude in worktree nightly-{date}, prompt 3 chars
  deploy-preview: manual only
    1. sh `./scripts/deploy-preview.sh` (arbitrary command)
    2. notify "Preview deployed"

OK: 2 triggers. Every trigger can also be started by hand.
```

`--json` prints the same plan as JSON, or `{"ok": false, "error": ...}`.
The exit status is 0 for a valid script, 1 for an invalid or unreadable one,
and 2 for a usage error.

## Triggers

| Declaration | Fires |
| --- | --- |
| `herdr.at("03:00", { days = "mon-fri", tz = "Europe/Paris" }, fn)` | At a time of day; `days` is `daily` (default), `weekdays`, `weekends`, a range or a list |
| `herdr.every("6h", fn)` | Every interval, from 1m to 30d |
| `herdr.cron("*/15 9-17 * * mon-fri", { tz = ... }, fn)` | On a five-field cron expression |
| `herdr.manual(fn)` | Only when someone starts it |

Every trigger can also be started by hand. Each takes an optional
`{ name = "..." }`, unique within the script; a script has at most 16
triggers. Times are read in the host's zone unless `tz` names one.

The handler gets `fire`, whose `date` and `time` are tokens (see below).

## Caps

`herdr.caps` is required, once, at the top level: `agents_per_run` (0 to
16), `tokens_per_day` (up to 1,000,000,000) and `wall_time` (1m to 1d). They
print in the plan, so they are approved with it.

## Steps

| Step | Does |
| --- | --- |
| `agent { harness, prompt, worktree?, base?, wait? }` | Start an agent in a new worktree and send it the prompt |
| `notify("text")` | Post a notification |
| `pull("main")` | Fast-forward a branch in the main checkout; fails if it is dirty or has diverged |
| `run("setup")`, `run("run")` | Run the setup or run script from `.herdr/worktree.toml` |
| `sh("command")` | Run a shell command; the plan marks it as an arbitrary command |
| `worktree { name, base? }` | Create a worktree |

Branch and worktree names (`pull`, `worktree`, `agent`'s `worktree` and
`base`) take letters, digits, `. _ / -` and the `{date}` token, with git's
rules (no leading `-`, no `..`); a harness is a lowercase name such as
`claude` or `codex`.

The plan shows a step's whole command, prompt or notification under its
summary, every line, with control and invisible characters escaped.

A step that fails stops the run. With `{ check = false }` it returns
`{ ok, log_tail }` instead (and `run_id` for `agent`) and the script decides.
Steps also take `{ timeout = "30m" }`. Unknown fields, control characters,
invisible formatting characters such as bidi overrides, and over-long text are
refused with the field named.

## Tokens

What a script cannot know reaches it as tokens it can pass along but not
read: `fire.date` is `"{date}"`, `fire.time` is `"{time}"`, and a result's
`log_tail` is `"{log_tail:N}"` for step N (`run_id` likewise). The runner fills
them in only when it runs a step. Step output is untrusted, so in a `sh`
command an output token becomes a reference to a `HERDR_LOG_TAIL_N` variable
and the value travels in the environment, never in the command text. The
reference is quoted for where the token sits (`"$HERDR_LOG_TAIL_1"` bare,
`${HERDR_LOG_TAIL_1}` inside double quotes, closing and reopening single
quotes), so it always arrives as one word, unglobbed. A command that uses
an output token must close its quotes and hold no comment, heredoc, `$'…'`,
`$"…"` or `${…}`, whose quoting the runner does not model; `check` refuses it
otherwise. On Windows, where `sh`
runs `cmd /C`, a command with an output token fails instead: `cmd` would
read the value as commands.

## What a script cannot do

Steps are a script's only effects. The sandbox has no `os`, `io`, `debug`,
`require`, `loadstring`, `print`, `pcall`, `math.random` or coroutines, the
libraries are read-only, and every run starts from a freshly loaded script,
so nothing carries over between runs. Because dates and step output are
tokens, the only thing a script's path can depend on is which unchecked steps
succeeded. The plan tries every combination (at most 256 paths per trigger),
so it shows every path, and the runner halts a run that ever calls a step
its approved plan does not have next. Steps are refused at the top level,
and triggers inside a handler.

A script's own computation is bounded: about 10 million interrupt checks per
load or run, 32 MiB of memory, 32 steps per run, and 256 KiB of source.
`pcall` is gone so a stopped step or a budget always ends the run.

Luau's `--!strict` comment is accepted, but `check` does not type-check yet:
the Luau type checker is not part of the embedded interpreter.

## Running scripts

On each host:

```sh
sheprd-automate repo add ~/code/app       # load ~/code/app/.herdr/automations
sheprd-automate install-service           # keep the daemon running (or: start)
sheprd-automate approve ~/code/app/.herdr/automations/nightly.luau
sheprd-automate status
```

The daemon starts by itself once there is something to run: `approve`,
`run-now` and `repo add` (for a repository with scripts) start it when none
is running, and Sheprd starts it on any host that lists scripts with no
daemon up, retrying at most every five minutes. It rescans scripts every 30
seconds. A script runs only once it is
approved on that host, and only in the exact text approved: any change,
including one a `git pull` brings in, waits for approval again. A person
approves either at a terminal, typing the script's name after reading its
plan, or in Sheprd's dialog. `approve` refuses when its input is not a
terminal or when the pane it runs in runs an agent, and `--hash` (Sheprd's
path) is refused inside any Herdr pane. This guards against mistakes, not a
hostile agent: an agent with a shell could run commands directly.

| Command | Does |
| --- | --- |
| `status`, `watch` | Scripts, their state and next firing, recent runs; `watch` redraws live |
| `run-now SCRIPT [--trigger NAME\|N]` | Start any trigger by hand, at any time |
| `pause\|resume SCRIPT\|--all`, `disable\|enable SCRIPT` | Stop or restart scheduling |
| `runs [SCRIPT]`, `runs show RUN`, `log RUN [--step N]` | Runs, a run's steps, a step's full output |
| `stop RUN` | Stop a running run |
| `usage [SCRIPT] [--since 24h\|7d\|30d\|all] [--by script\|run\|step]` | Tokens, CPU and memory |
| `log --daemon [--lines N] [--follow]` | The daemon's own log |
| `start`, `shutdown`, `install-service`, `uninstall-service` | The daemon itself |

Every command takes `--json`; errors are then `{"ok": false, "message": ...}`.

### How a run goes

- A firing missed while the daemon was down runs once when it is back,
  marked late; one missed many times still runs once. Nothing fires for the
  time a script was paused, disabled or not yet approved, and a firing never
  runs twice. A script runs one run at a time; a firing during a run is
  skipped and logged.
- Each run loads the script afresh and is checked step by step against its
  approved plan; a step the plan does not have next halts the run.
- `pull` refuses a dirty checkout or one on another branch, and only
  fast-forwards. `run` and `sh` run as child processes with their output
  logged; a timeout or `stop` ends the whole process group. `worktree`,
  `agent` and `notify` go through the `herdr` CLI; when a step needs Herdr and
  its server is not running, the daemon starts it.
- Personal scripts have no repository, so they can use only `sh` and
  `notify`.
- Three failed runs in a row pause a script.

### Caps and usage

Besides each script's caps, the host has its own in
`~/.config/herdr-automate/config.toml`:

```toml
max_agents = 3                # agents automations may run at once
max_tokens_per_day = 10000000 # every script together
# herdr = "/path/to/herdr"    # when not on PATH
# herdr_session = "work"      # a Herdr session other than the default
```

A run halts before an agent step that would cross the script's
`agents_per_run`, the host's `max_agents`, or a daily token budget; a script
that spends its budget is also paused. Token counts come from the agents'
own session files (Claude and Codex today; other harnesses show as unknown,
never zero), sampled once a minute, so a budget can be overrun by what an
agent spends in that minute. CPU and peak memory come from the processes of
each step and agent pane.

### Keeping the daemon running

`install-service` writes and loads a launchd agent (macOS), a systemd user
unit (Linux) or a logon task (Windows), printing what it writes. No admin
rights are needed. The service runs the executable that installed it, with
that command's `PATH` and state and config directories, so a
`HERDR_AUTOMATE_HOME` or XDG directory set only in your shell still applies;
run `install-service` again after moving either. The daemon restarts after a crash; `shutdown` stops it
after its runs and it stays stopped. On a headless Linux host, a user unit
only runs while the user is logged in unless lingering is on
(`loginctl enable-linger`); `install-service` does not change that for you.

Updating the app does not update a running daemon. When a daemon runs another
version than the app ships, its host's card under Automations offers
**Update to VERSION**, on this computer as on a remote host: it copies the
app's build into `~/.local/share/herdr-automate/bin/VERSION/`, relinks
`~/.local/bin`, stops the daemon once its runs finish, and starts the new one
through a service that names the copy. On Windows, this computer's card offers
no update, because the copy needs a shell.

State lives in `~/.local/state/herdr-automate` (`%LOCALAPPDATA%` on
Windows): the database, the daemon's log, and each run's step output.

## For agents

Sheprd installs a `herdr-automations` skill beside its browser skill (once
you agree), so agents in your panes know how to write a script, check its
plan, and ask you to approve it. Its source is
[`skills/herdr-automations/SKILL.md`](../skills/herdr-automations/SKILL.md).
