# herdr-acp

Runs a coding agent through the [Agent Client Protocol](https://agentclientprotocol.com)
in a terminal pane, draws the conversation there, and lets Herdr UI attach to it
and show it as a native chat.

```sh
herdr-acp claude             # or codex, copilot, gemini, opencode
herdr-acp -- goose acp       # any ACP agent command
herdr-acp --load ID codex    # continue one of the agent's sessions
```

`herdr-acp --help` lists every option and key.

## Why a program in the pane

Herdr owns terminals, not conversations. The agent's structured conversation
has to live in something Herdr already keeps running, so it lives in this
program: Herdr starts it in a pane like any other command, keeps it alive when
Herdr UI closes, and the Herdr TUI shows its chat because it is an ordinary
terminal program. Normal ACP sessions read no agent logs or terminal screen;
the optional read-only mirror below reads an explicitly selected native log.

`herdr-acp` is the only ACP client of the agent. Every view is a client of
`herdr-acp` instead:

- **The pane itself.** The chat drawn in the terminal, so the session works in
  the Herdr TUI, a plain terminal, or over SSH.
- **Herdr UI.** On a local daemon, it finds the pane's `acp` metadata token,
  connects to the endpoint it names, and draws the same conversation natively
  over the pane. Both views stay live: a prompt or a permission answer from
  either one shows in both, and the first answer to a permission request wins.
- **Anything else that speaks the attach protocol** in
  `crates/herdr-chat-protocol`.

## Viewing an existing CLI session

Herdr UI can automatically render existing Linux Codex/Claude sessions with
`[chat] render_existing = true`. It resolves exact session and process identity,
then runs a bounded `herdr-acp observe AGENT SESSION_ID PID START_TICKS --pane ID`
child over standard streams. The observer pins and validates the session file;
it exits if the original process ends or its identity changes. It starts no
agent and writes no Herdr lifecycle metadata. The GUI retains the original
terminal editor and approval interface directly below the rich conversation.
No separate conversation, prompt injection or terminal/view round trip is needed.

The manual, read-only historical mirror remains available separately:

`mirror` watches one existing Codex rollout or Claude Code JSONL transcript.
It starts no agent, resumes no session, and sends no Herdr metadata or activity
reports. Prompts, approvals, cancellation, and settings stay in the original
terminal; the mirror refuses every command even from another attach client.

Start the watcher separately, leaving the running agent alone:

```sh
herdr-acp mirror codex /absolute/path/to/rollout.jsonl \
  --pane PANE_ID --endpoint 0123456789abcdef0123456789abcdef
# Or: herdr-acp mirror claude /absolute/path/to/session.jsonl ...
```

The endpoint is 32 lowercase hexadecimal digits. Omit `--endpoint` to generate
one; the watcher prints its ID once ready. Stop that watcher with Ctrl+C when
finished. This only stops the viewer. The GUI can show it over the selected
existing pane using its own config (normally `config-gpui.toml`):

```toml
[features]
acp_chats = true

[[chat.mirrors]]
pane = "PANE_ID"
endpoint = "0123456789abcdef0123456789abcdef"
# host = "my-saved-ssh-target"  # For a watcher running on that exact SSH host.
```

The pane must already exist on the matching host. An ordinary ACP endpoint
advertised by the pane takes precedence. Remote watchers run beside their
transcript on the SSH host and use the existing `herdr-acp connect ID` relay;
the GUI does not launch them or deploy a binary. Remove the mirror's config
entry when finished. No daemon configuration needs changing.

The viewer refreshes once a second when the file changes. It reads at most the
last 8 MiB and retains at most 2,000 entries / 2 MiB of text / 8 MiB of image
data. Individual text fields are capped at 128 KiB. Records are limited to
1 MiB, except Codex messages containing typed images, which may use the full
8 MiB read window. Within retained records, unsupported images or encoded image
data over the 5 MiB attachment limit show an unavailable notice while their captions
remain visible. Other oversized records, unsupported record kinds, and malformed
lines are skipped. A partially written final line waits for its newline. The
displayed tool history is descriptive and never an approval UI.
Messages, tool text, and bounded embedded Codex raster images are supported;
linked files, complete session settings, and live permission state are not
reconstructed from logs. Image paths and external image URLs are never fetched
by the observer. Codex uses `response_item` records so matching `event_msg`
records are not repeated;
Claude sidechain records are excluded. Choose the exact session file yourself:
the watcher does not guess from folders, recent timestamps, or terminal text.

Claude records marked as internal task notifications or scheduled reminders
appear as compact notices. Expand a notice to inspect or copy its retained
source text; instructions, output-file paths, and links in it are inert.
Ordinary user messages containing similar text remain user messages.

Codex environment updates use the content's explicit harness provenance to
become compact notices, keeping adjacent user text separate. Recognized image
wrappers around a typed embedded image also become compact source notices;
the image and its user caption remain visible. Literal user-pasted XML is not
reclassified by its spelling. Expanded notice details retain the original text.

The mirror's native-log format support is best effort because those storage
formats can change. A missing or unreadable source ends the watcher with an
error. It never repairs or writes to the transcript.

## What it tells Herdr

Inside a Herdr pane (`HERDR_PANE_ID` and `HERDR_SOCKET_PATH` set), it reports
through Herdr's JSON API:

- `pane.report_agent` with the source `herdr-acp`, the agent label `acp`, and
  the state `idle`, `working`, or `blocked`, so the sidebar and notifications
  are exact rather than detected from the screen;
- `pane.report_metadata` with the agent's display name and the `acp` token, the
  attach socket's id;
- each prompt the agent received, on one line and cut to 80 characters, as
  the pane's `title`, which Herdr UI's sidebar and the phone's chat list
  show; a report or rename Herdr fails is tried again a few times;
- `tab.rename` of its tab to the agent and that prompt (`Claude: fix the…`,
  at most 40 characters), only while the chat is the tab's one pane and the
  tab still has the agent's name, a number, or the label the chat gave it, so
  a name the user typed stays (Herdr has no conditional rename, so a name
  saved in the instant between reading the label and renaming is replaced);
- on exit, the token and title are cleared, the tab gets the agent's name
  back unless the user renamed it, and the agent is released.

It also writes OSC 7501 program status to the terminal, for terminals that read
it; terminals that do not ignore it.

## Agents

| Name | Started as |
| --- | --- |
| `claude` | `claude` itself (see below); with `--acp`, `claude-agent-acp`, else `npx -y @agentclientprotocol/claude-agent-acp` |
| `codex` | `codex app-server` (see below); with `--acp`, `codex-acp`, else `npx -y @agentclientprotocol/codex-acp` |
| `copilot` | `copilot --acp` |
| `gemini` | `gemini --acp` |
| `opencode` | `opencode acp` |
| `pi` | `pi --mode rpc` (see below); with `--acp`, `pi-acp`, else `npx -y pi-acp` |

Arguments after `--` go to the agent. Each agent signs in its own way; when it
reports that it needs authentication, the chat says how.

### Claude without the adapter

Where `claude` is installed, `herdr-acp claude` runs it directly in its
`stream-json` mode, the JSON lines Anthropic's own Agent SDKs speak, and
translates between that and ACP on a thread of its own (`src/claude.rs`).
There is no Node process or `npx` download in between, and a chat is ready
about twice as fast. A turn takes as long either way: the model's time
dominates it. The control messages on that pipe (the handshake, interrupt,
permission questions, and setting changes) are not a published interface;
they follow Anthropic's Python Agent SDK. `--acp`, or `HERDR_ACP_ADAPTER=1`
in the environment, goes back to the ACP adapter, for when a Claude Code
release changes them.

What it offers: Claude Code's permission modes (Default, Accept Edits, Plan,
Bypass Permissions), its models and effort levels from its handshake, its
slash commands, the to-do list as the plan, diffs for edits, permission
questions with Claude's own "don't ask again" suggestion, a plan to approve
when leaving plan mode, and questions with choices. `--load` continues a
session with its context, but the chat starts empty: the earlier messages
are not shown again.

### Codex and pi without an adapter

The same holds for two more agents, each through a translator of its own
(`src/codex.rs`, `src/pi.rs`) behind the `Translator` trait in
`src/harness.rs`, which Claude's uses too:

- **Codex** runs as `codex app-server`, the JSON-RPC protocol OpenAI's editor
  extension and Python SDK drive; OpenAI marks it experimental. Its modes are
  Codex's approval presets (Read Only, Auto, Full Access), its models and
  reasoning efforts come from `model/list` and go with each turn, and command,
  file-change, and extra-access approvals become permission requests. A
  configured model the account does not offer gives way to Codex's default.
- **pi** (`@earendil-works/pi-coding-agent`) runs as `pi --mode rpc`, the JSON
  lines its `docs/rpc.md` documents. pi asks no tool approvals of its own; an
  extension's `select` and `confirm` dialogs become permission requests, and
  its models and thinking levels are the session's settings. `--load` passes
  pi's `--session`.

### Enterprise endpoints

`herdr-acp` starts the agent with its own environment, untouched, so each
agent reaches whatever endpoint its configuration names, exactly as it does
when run by hand in the same pane:

- Copilot on GitHub Enterprise Cloud with data residency: `GH_HOST` set to
  the enterprise host (such as `mycompany.ghe.com`) after `copilot login
  --host`, or a token in `COPILOT_GITHUB_TOKEN`; on GitHub Enterprise
  Server, `COPILOT_PROVIDER_GHES_HOST` and its token.
- Claude: `ANTHROPIC_BASE_URL` for an LLM gateway, `CLAUDE_CODE_USE_BEDROCK`
  or `CLAUDE_CODE_USE_VERTEX`, `HTTPS_PROXY`, and `NODE_EXTRA_CA_CERTS`, or
  the same keys in the `env` block of Claude Code's settings; both the direct
  path and the ACP adapter read them.
- Codex: a `[model_providers]` entry with its `base_url` in
  `~/.codex/config.toml`. Gemini: `GOOGLE_CLOUD_PROJECT` and Vertex AI
  settings, or `.gemini/.env`. OpenCode: a provider's `baseURL`, or its
  GitHub Enterprise URL when signing in to Copilot.

These must be set where the pane's shell sees them: the shell startup files
of the machine the chat runs on, which for a chat on an SSH host is that host.

## The chat

The header shows the agent, whether it works and for how long, its model,
thinking level, and mode, how full its context window is, and what the
session has cost when the agent says. Each finished turn leaves a line saying
how long it took and the tokens it used. Settings the agent offers through
ACP's session config options (model, thinking level, mode, and any others)
change from **Ctrl+O** here and from the header's buttons in Herdr UI; the
agent's slash commands complete after `/`, and Up and Down step through the
session's earlier prompts, whichever client sent them.

## Behaviour

- Prompts sent while the agent works are queued and sent in order; cancelling
  ends the turn, declines pending permission requests, and drops the queue.
- The agent runs in its own process group and stops with the program.
- The program advertises no file system, terminal, or elicitation support, so
  agents use their own tools; it answers only permission requests.
- The transcript keeps at most 2,000 entries and 2 MiB of text, dropping the
  oldest, so a snapshot always fits one frame.

## Limits

- Herdr UI attaches through `herdr_client::private_endpoint`: on Unix a
  socket in `$XDG_STATE_HOME/herdr/acp` (or `~/.local/state/herdr/acp`), on
  Windows the named pipe `\\.\pipe\herdr-acp-<id>`, either one reachable by
  this user alone, so only from a Herdr UI on the same machine.
- A chat that is killed rather than quit leaves its Unix socket file behind;
  the next chat to start removes sockets nothing answers on. A pipe goes away
  with its process.
- Resuming after a restart needs `--load` with the agent's session id, and an
  agent that supports `session/load`.
