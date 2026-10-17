# herdr-companion

A small headless bridge so a phone can work with the coding agents running in
Herdr without showing a terminal. It includes a web app you can install to the
home screen, so no native app is needed.

From the phone you can:

- answer permission prompts and `AskUserQuestion` forms
- read each Claude Code session as a chat
- see every agent's status
- reply, interrupt, or press keys for dialogs the hooks don't report
- start new agents
- get push notifications through ntfy

Herdr still owns every terminal. The companion only adds Claude Code hooks next
to Herdr's own integration and calls Herdr's JSON API. When nobody answers a
prompt in time, Claude Code shows its usual prompt in the pane.

```
Claude Code (in a Herdr pane) ──HTTP hooks──▶ herdr-companion ◀──HTTP── phone (web app)
                     ▲                              │
                     └── agent.prompt, pane.* ◀── Herdr JSON API socket
```

## Run it

```sh
export HERDR_COMPANION_TOKEN="$(openssl rand -hex 32)"   # also visible to agents
herdr-companion serve                                    # 127.0.0.1:8787
herdr-companion hooks > companion-hooks.json             # merge into ~/.claude/settings.json
```

`hooks` only prints settings and never edits your files. The printed hooks
send the token and the pane id from each agent's environment
(`HERDR_COMPANION_TOKEN`, `HERDR_PANE_ID`), so the settings file holds no secret.
Export the token where Herdr panes inherit it, for example in your shell profile.

### On the phone

The quickest start listens on every interface and prints a QR code at once:

```sh
herdr-companion serve --all
```

Scan it with the phone's camera and the web app opens already signed in. The
code uses the machine's Tailscale address when it has one, because WireGuard
encrypts that traffic. Otherwise it uses the LAN address and warns that plain
HTTP exposes the token to anyone on that network. Any other reachable address
is listed under the code. Addresses come from the routing table; no packets
are sent to find them.

To install the app to the home screen (Safari: Share → Add to Home Screen),
you need HTTPS, which `tailscale serve` provides:

```sh
tailscale serve --bg 8787        # https://<machine>.<tailnet>.ts.net
herdr-companion serve --public-url https://<machine>.<tailnet>.ts.net
```

The QR code uses `--public-url` whenever it is given.

- **What the code holds.** The code is a link to `/#token=…`. Browsers never
  send the part after `#` to the server, so the token stays out of request
  lines, logs, and `Referer` headers. Anyone who sees the code can use the
  companion, so treat it like a password.
- **When it prints.** The code is printed only when stderr is a terminal,
  never into a log file. `--no-qr` turns it off, and
  `herdr-companion qr --url https://<machine>.<tailnet>.ts.net` prints it again
  on demand.
- **Address bar.** In a browser tab the link stays in the address bar, so Add
  to Home Screen carries the token over; an installed iOS app does not share
  Safari's storage. The installed app removes it from the address bar once
  it has stored the token.
- **Terminal colours.** The code is drawn for a dark terminal background. On
  a light background it appears inverted, which most phone cameras still read.
- **No reachable address.** On loopback without `--public-url` there is
  nothing a phone can open, so `serve` prints a hint instead of a code.

### Push notifications

The web app can't receive notifications while it is closed, so the companion
can publish to an [ntfy](https://ntfy.sh) topic. Subscribe to the same topic in
the ntfy app.

```sh
herdr-companion serve --ntfy https://ntfy.sh/<long-random-topic> \
  --public-url https://<machine>.<tailnet>.ts.net
```

A notice is sent when a prompt needs approval, when Claude asks for input, and
when a turn finishes. By default a notice names only the tool and the project
directory. Add `--ntfy-details` to include commands and messages; whoever runs
the ntfy server can read them. Tapping a notice opens `--public-url`.

### Options for `serve`

- `--listen ADDR`: the address to bind, `127.0.0.1:8787` by default.
- `--all`: listen on every interface, `0.0.0.0:8787`. It cannot be combined
  with `--listen`; for another port, use `--listen 0.0.0.0:PORT`, which
  detects addresses the same way.
- `--decision-timeout SECS`: how long a hook waits for the phone before the
  prompt goes back to the terminal, 110 by default. `hooks` takes the same
  option and gives Claude Code's own timeout 10 seconds more.
- `--herdr-socket PATH`: Herdr's JSON API socket (`herdr.sock`), not the
  client socket. If you leave it out, the companion uses `HERDR_SOCKET_PATH`,
  then the local session's socket. Routes that need Herdr answer `502` when it
  can't be reached.
- `--public-url URL`: the address the phone uses, for the pairing QR code and
  for tapped notices.
- `--no-qr`: don't print the pairing QR code at startup.
- `--ntfy URL`, `--ntfy-details`: push notifications, as described above.

## API

The web app's files (`/`, `/app.js`, and so on) are public. Every other route
requires `Authorization: Bearer $HERDR_COMPANION_TOKEN`.

| Route | Purpose |
| --- | --- |
| `POST /hooks/claude` | Claude Code hook endpoint. A `PermissionRequest` waits here for a decision. Tool, prompt, notification, and stop hooks are recorded and answered at once. |
| `GET /v1/requests` | Prompts waiting for a decision, oldest first: `id`, `session_id`, `pane_id`, `cwd`, `tool_name`, `tool_input`. |
| `POST /v1/requests/{id}` | Decide a prompt; the decision shapes are listed below. |
| `GET /v1/events?after=N&wait=S` | The event feed after sequence `N`. With `wait`, it long-polls for up to 30 seconds. `next` is the cursor for the next call. `lost: true` means the bounded feed dropped events, so re-read `/v1/requests`. |
| `GET /v1/agents` | Herdr's agents, each with `agent_status`, `workspace_label`, `pending` prompts, its `session_id`, and `has_transcript`. |
| `POST /v1/agents` | Start an agent in a new tab or a new worktree, as shown below. |
| `GET /v1/workspaces` | Herdr's workspaces, for choosing where to start an agent. |
| `GET /v1/sessions` | Claude Code sessions the hooks have seen, most recent first. |
| `GET /v1/sessions/{id}/transcript?limit=N` | The session as chat entries (`user`, `assistant`, `tool_use`, `tool_result`), read from the end of Claude Code's transcript. 200 entries by default, at most 500. |
| `POST /v1/panes/{pane_id}/prompt` | `{"text": "..."}`. Herdr types the text into the agent's pane through `agent.prompt`. |
| `POST /v1/panes/{pane_id}/interrupt` | Sends Esc, which stops the current turn. |
| `GET /v1/panes/{pane_id}/screen` | The pane's visible text, for dialogs the hooks don't report, such as trust prompts or logins. |
| `POST /v1/panes/{pane_id}/keys` | `{"keys": ["down", "enter"]}`. Accepts up to 16 keys from a fixed list: `enter`, `esc`, `tab`, `space`, `backspace`, arrow keys, `ctrl+c`, `y`, `n`, and `1` to `9`. |

Decision bodies:

```jsonc
{"behavior": "allow"}                                   // optional updated_input, updated_permissions
{"behavior": "deny", "message": "use the staging DB"}  // Claude sees the message
{"behavior": "answer", "answers": {"Which DB?": "Postgres"}}  // AskUserQuestion only; lists for multi-select
{"behavior": "terminal"}                                // show the normal prompt in the pane instead
```

Starting an agent:

```jsonc
{"kind": "claude", "name": "auth-fix", "placement": {"in": "tab", "workspace_id": "w1", "cwd": "/repo"}}
{"kind": "codex", "name": "docs", "placement": {"in": "worktree", "cwd": "/repo", "branch": "docs-pass"}}
```

The companion creates the tab or worktree, then calls Herdr's `agent.start`,
which waits up to 30 seconds for the agent to be ready. Herdr's refusals, such
as a name already in use, come back as `422` with Herdr's message.

A request takes one decision. A late answer, after the timeout or after
someone else decided, gets a `404`.

## Limits and caveats

- While the companion is waiting, the pane shows no prompt; the terminal shows
  it only after `{"behavior": "terminal"}` or the timeout. Herdr's own hook
  still marks the agent as blocked, so the GUI still shows the attention dot.
- At most 64 prompts can wait at once. The event feed keeps the last 1024
  events, and the companion remembers the last 256 sessions. Request bodies
  are capped at 512 KiB, Herdr replies at 1 MiB, and only the last 4 MiB of a
  transcript is read. Event text and transcript entries are cut to bounded
  sizes.
- The chat view and permission handling cover Claude Code. Other agents appear
  in the agent list with Herdr's status and can be prompted, interrupted, and
  read through the screen view.
- The token is kept in the browser's local storage on the phone. "Forget
  token" in the app's settings removes it.
- Windows builds and lints, but the Herdr routes there go through Herdr's
  named pipe and have not been exercised.
