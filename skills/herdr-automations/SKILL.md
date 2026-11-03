---
name: herdr-automations
description: "Write Sheprd automations: small Luau scripts that run on a schedule or by hand to pull a branch, run validation or commands, start agents in new worktrees with a prompt, and notify. Use it inside Herdr (HERDR_ENV=1) when the user asks for something to happen every day, every few hours, at a time, on a cron schedule, or on demand with one command: a nightly or recurring job, a scheduled agent, an automation, a routine, or 'every morning do X'. It covers writing the script, checking its plan with sheprd-automate check, and asking the user to approve it; never approve it yourself."
---
<!-- herdr-gpui-managed-skill v1: Sheprd keeps this file up to date. Edit a copy under another name to keep your changes. -->

# Sheprd automations

An automation is a Luau file that declares when it runs and calls steps from
a fixed list. `sheprd-automate`, a daemon on each host, runs it once the user
approves it, and Sheprd shows its runs in the Automations section. Scripts
are deterministic: steps are their only effects, and the plan
`sheprd-automate check` prints is everything the script can do.

The skill identifier, `.herdr` paths, and `herdr.*` Luau API remain stable so
existing scripts and installed skills continue to work after the rename.

## How to make one

1. Ask only what you cannot infer: when it runs (time, zone, days, or by
   hand), what each step does, which agent (`claude`, `codex`, ...), and the
   prompt. Validation is usually the repository's own command (`just ci`,
   `npm test`, `cargo test`).
2. Write `.herdr/automations/NAME.luau` in the repository it works on (or
   `~/.config/herdr-automate/automations/NAME.luau` for a personal script,
   which can only use `sh` and `notify`).
3. Run `sheprd-automate check FILE` and fix every error it names; read the
   plan it prints and check every branch says what the user asked.
4. Show the user the plan, then make sure the repository is registered:
   `sheprd-automate repo add PATH` (once per repository and host).
5. Ask the user to approve it. Run `sheprd automation approve FILE` to open
   Sheprd's approval dialog, or ask them to run `sheprd-automate approve FILE`
   in their own terminal. Never approve it yourself: approval is the user's
   review, and `sheprd-automate approve` refuses from an agent's pane.
6. After approval, `sheprd-automate run-now FILE` runs it once by hand when the
   user wants to try it; it spends tokens if it starts agents, so ask first.
   `sheprd-automate status` and `sheprd-automate runs show RUN` show what
   happened.

Any change to an approved script, or to the `.herdr/worktree.toml` scripts it
runs, needs approval again.

## A script

```lua
herdr.caps { agents_per_run = 2, tokens_per_day = 2_000_000, wall_time = "3h" }

herdr.at("03:00", { tz = "Europe/Paris", days = "mon-fri" }, function(fire)
  pull("main")
  local v = sh("just ci", { check = false, timeout = "45m" })
  if not v.ok then
    agent {
      harness = "claude",
      worktree = "fix-validation-" .. fire.date,
      prompt = "Nightly validation failed on main:\n" .. v.log_tail .. "\nFind the cause and fix it.",
    }
  end
  agent { harness = "claude", worktree = "nightly-" .. fire.date, prompt = "Review what landed on main yesterday." }
end)

herdr.manual({ name = "deploy-preview" }, function()
  sh("./scripts/deploy-preview.sh")
  notify("Preview deployed")
end)
```

## Reference

`herdr.caps { agents_per_run, tokens_per_day, wall_time }` is required, once,
at the top level: 0 to 16 agents, up to 1,000,000,000 tokens a day, `1m` to
`1d`. Pick caps that fit the job; they are part of what the user approves.

| Trigger | Runs |
| --- | --- |
| `herdr.at("HH:MM", { days = "mon-fri", tz = "Europe/Paris" }, fn)` | Daily (default), `weekdays`, `weekends`, ranges or lists |
| `herdr.every("6h", fn)` | Every interval, `1m` to `30d` |
| `herdr.cron("*/15 9-17 * * mon-fri", { tz = ... }, fn)` | Five-field cron |
| `herdr.manual(fn)` | Only by hand |

Every trigger can also be run by hand. Each takes `{ name = "..." }`
(unique). Times use the host's zone unless `tz` names one.

| Step | Does |
| --- | --- |
| `pull("main")` | Fast-forward a branch in the main checkout; fails if dirty or on another branch |
| `run("setup")`, `run("run")` | The `.herdr/worktree.toml` setup or run script |
| `sh("command")` | A shell command in the repository |
| `worktree { name, base }` | Create a worktree |
| `agent { harness, prompt, worktree, base, wait }` | Start an agent in a new worktree and send the prompt |
| `notify("text")` | A notification |

A failing step stops the run. With `{ check = false }` a step returns
`{ ok, log_tail }` (and `run_id` for `agent`) and the script decides. Steps
take `{ timeout = "30m" }`.

## What scripts cannot do

- No `os`, `io`, `require`, `print`, `pcall`, `math.random`, clock or files:
  steps are the only effects.
- `fire.date`, `fire.time`, `log_tail` and `run_id` are tokens such as
  `"{date}"` and `"{log_tail:2}"`: pass them into prompts, names and commands,
  but do not compare them, since the runner fills them in only when a step
  runs. In `sh` they arrive as quoted `$HERDR_...` variables (not on
  Windows, where `sh` runs `cmd` and such a command fails). A `sh` command
  that uses one cannot hold `#`, `<<`, `$'`, `$"` or `${`.
- Branch and worktree names take letters, digits, `. _ / -` and `{date}`.
  `harness` is a lowercase name.
- `run` takes only `setup` or `run`; other commands go through `sh`.
- Steps are refused at the top level, and triggers inside a handler.
- A trigger may have at most 256 paths: call fewer steps with
  `check = false`.

`sheprd-automate steps` lists the steps; `sheprd-automate --help` has the rest.
