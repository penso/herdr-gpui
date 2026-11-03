# Sheprd feature inventory

Source-backed inventory for the sheprd.org mockups. Researched read-only at
commit `8e03793da` (2026-11-02, latest release `v20261102.2`) on 2026-11-03.
The mockups' feature directory (`assets/features.js`) is a condensed view of
this file; every row there maps to a row here.

Source abbreviations: **R** = `README.md`, **GR** = `crates/herdr-gpui/README.md`,
**CLI** = `docs/cli.md`, **EX** = `crates/herdr-gpui/config-gpui.example.toml`,
**FTL** = `crates/herdr-gpui/locales/en-US/*.ftl`. Line numbers are at `8e03793da`.

Status legend used across the site concepts:

| Badge | Meaning |
| --- | --- |
| Shipped | In a published release and on by default (or one click away) |
| Opt-in | In a published release, off by default (a setting or `[features]` flag) |
| Experimental | Docs or the UI call it experimental / proof of concept, or it ships unverified on real devices |
| TestFlight beta | iOS builds uploaded to TestFlight (internal testers); no public link |
| Planned | Plan doc or "not yet" only; no shipped implementation |

A platform caveat is listed separately from status (for example "macOS and Linux clients").

## Codebase measurement

Used only as supporting evidence of engineering depth, never as the headline.

Method: `tokei` 13 over `git ls-files` at `8e03793da`, first-party source only.
Excluded: `site/`, `docs/`, `crates/embed-resource` (no-op stub), lockfiles,
generated files, images and other assets, and all third-party dependencies
(none are vendored except the attributed parts of `herdr-protocol`, which are counted).
"Test" Rust files are paths under a `tests/` directory, `tests.rs`, `*_tests.rs`,
`crates/test-support/` and `benches/`; production files hold no test bodies
(AGENTS.md "Verification"). Code = non-blank, non-comment lines.

| Measure | Value |
| --- | --- |
| First-party Rust crates (excluding the stub and test-support) | 26 |
| Production Rust, code lines | 378,965 (1,941 files) |
| Rust test code, code lines | 224,291 (1,951 files) |
| Test functions (`#[test]`, `#[gpui::test]`, … attributes) | 8,493 |
| Swift (iOS app, extensions, macOS nearby helper, icon tooling), code lines | 18,900 (170 files) |
| Python (release/packaging scripts), code lines | 9,071 |
| Fluent UI strings (`.ftl`, en-US) | 288 files |

Reproduce:

```sh
git ls-files | grep -E '\.(rs|swift|lua|luau|sh|py|ts|js|css|html|ftl)$' \
  | grep -v -E '^(site/|docs/|crates/embed-resource/)' > files.txt
grep '\.rs$' files.txt | grep -E '(/tests/|/tests\.rs$|_tests\.rs$|^crates/test-support/|/benches/)' > rs-test.txt
grep '\.rs$' files.txt | grep -vxf rs-test.txt > rs-prod.txt
tokei -t Rust $(cat rs-prod.txt); tokei -t Rust $(cat rs-test.txt)
git grep -c -E '#\[(gpui::)?test' -- '*.rs' | awk -F: '{s+=$2} END {print s}'
```

Suggested public phrasing: "26 Rust crates, roughly 380k lines of production
Rust and 8,000+ tests" — always next to what those crates *do*.

---

## Desktop · 0. Identity, positioning, and the Herdr relationship

| Fact | Source |
| --- | --- |
| Current product name: "Herdr UI". Repo and package: `herdr-gpui`. macOS bundle: `Herdr.app`, ID `so.pen.herdr-gpui`. Windows AppUserModelID: `so.pen.herdr-gpui`. Homebrew cask name: "Herdr". | R:5, GR:3604-3606, `homebrew/Casks/herdr-gpui.rb` |
| Tagline: "A native Rust/GPUI client for macOS, Linux, and Windows, for a Herdr daemon you installed yourself. It paints the daemon's terminal cells, split panes included, without running another terminal emulator or wrapping the TUI." | R:22-24 |
| Disclaimer, quoted exactly: "**Unaffiliated project.** Not affiliated with, endorsed by, or supported by Herdr or herdr.dev." | R:26-27; also `site/llms.txt` |
| The daemon owns terminals and session state. Closing or detaching the GUI leaves the daemon and its terminals running. | R:403-425, AGENTS.md "Architecture" |
| The app never installs, stops, migrates, or upgrades a personal daemon. It only starts an already-installed `herdr server` when the session is absent. If Herdr is missing, an Install button opens herdr.dev and never downloads anything. | R:133-137, GR:787-803 |
| The one remote install: Herdr's own installer (`curl -fsSL https://herdr.dev/install.sh \| sh`) runs on new cloud machines, and only with the user's "Install Herdr" switch on. | GR:488-494 |
| Upstream projects: Herdr at https://github.com/herdrdev/herdr (herdr.dev), and herdr-core ("Herdr 2") at https://github.com/herdrdev/herdr-core. | NOTICE, `crates/herdr-core-client/README.md:3` |
| Attribution: Apache-2.0 (Copyright 2026 Fabien Penso). Parts of `herdr-protocol`, the shared settings schema, theme palettes, status indicators, and the Done/Request sounds are adapted from Herdr (Apache-2.0). Usage providers are ported from CodexBar (MIT). GitHub integration references Arbor (`GITHUB-NOTICE.md`). | R:497-501, `NOTICE`, `crates/herdr-protocol/NOTICE.md`, `docs/usage-providers.md:10-13` |
| Built on Rust 1.96.1 and GPUI 0.3.6 (the `gpui-pre` crate). | R:273-276 |
| Versions are calendar-based: `YYYYMMDD.COUNTER`, tagged `vYYYYMMDD.COUNTER`. | GR:146-150, `scripts/release/package-windows.py:40` |
| The current site at herdrui.dev uses the title "Herdr UI: a native window for the Herdr daemon" and the H1 "Herdr's TUI is great." Its description: "the window adds dispatch from issues, side-by-side review, checkpoints, Teleport and a Devices overview." | `site/index.html` |

---

## Desktop · 1. Platforms, install, and distribution

| Platform | Requirements and artifacts | Install command (exactly as documented) | Source |
| --- | --- | --- | --- |
| macOS | macOS 14.2 Sonoma or newer, Apple Silicon or Intel. A signed, notarized universal app, shipped as `Herdr-VERSION-universal-apple-darwin.dmg`. | `brew install penso/tap/herdr-gpui` then `open -a Herdr`. Alternative: `brew tap penso/tap` then `brew install herdr-gpui`. | R:141-180, cask file |
| Linux | x86_64 and ARM64. Ships as `.deb`, `.rpm`, an Arch `.pkg.tar.zst`, and a tarball. Needs glibc 2.39+ (Ubuntu 24.04, Debian 13, Fedora 40, current Arch), a Vulkan driver, ALSA, and `wl-clipboard` or `xclip`. | `curl -fsSL https://herdrui.dev/install.sh \| sh`. By hand: `sudo apt install ./Herdr-VERSION-x86_64-unknown-linux-gnu.deb` (or `dnf` / `pacman -U`). | R:182-221, 311-327, `site/install.sh` |
| NixOS / Nix | Builds from source with the flake, x86_64 and ARM64 Linux. Never updates itself. | `nix run github:penso/herdr-gpui` | R:223-232 |
| Windows | x86_64 and ARM64 (native). Ships `Herdr-VERSION-<arch>-pc-windows-msvc.zip` and `-setup.exe` (per-user Inno Setup, no admin rights). Both install to `%LOCALAPPDATA%\Programs\Herdr UI`. | `irm https://herdrui.dev/install.ps1 \| iex`. Uninstall: `& ([scriptblock]::Create((irm https://herdrui.dev/install.ps1))) -Uninstall` | R:234-269, 377-401 |
| winget | Package ID `Penso.HerdrUI`. Submitted only after Windows signing is set up and a first manual submission. | (none documented for users yet) | `scripts/release/README.md:302-336` |
| From source | Any platform. | `git clone https://github.com/penso/herdr-gpui.git && cd herdr-gpui && just run` (or `just install`) | R:271-305 |

**Companion binaries.** Every package ships five programs beside the app: `herdr-acp`, `herdr-remote-control`, `herdr-automate`, `herdr-nearby`, and a pinned `herdr-core` 0.1.13 fetched from releases.herdr.dev. Sources: `scripts/release/package-macos.sh:9-15`, `package-linux.sh:9-14`, `package-windows.py:8-13,48-50`, `scripts/release/herdr-core.json`.

**Release integrity.** Releases carry checksums, Sigstore signatures, build provenance, and an SBOM. Updates are checked against an Ed25519-signed manifest. Source: R:380-392, `docs/updating.md`, `scripts/release/generate-sbom.py`.

**Beta channel.** Settings > General > Install beta releases (`[updates] channel = "beta"`). Betas never reach the Homebrew tap or winget. Source: R:176-180.

---

## Desktop · 2. Terminals and the Herdr connection

| Feature | Plain-language description | Source | Status |
| --- | --- | --- | --- |
| Native terminal painting | Draws the daemon's terminal cells (tabs, splits, colors, cursor shapes) natively. Box-drawing and prompt glyphs are pixel-aligned. | R:22-24, GR:38-49, 4497-4499 | Shipped |
| Auto-start the local daemon | Starts an installed `herdr server` if none is running, and never installs one. | GR:6-13, 787-803 | Shipped |
| Reconnect and detach | Each host reconnects with backoff. A dropped host keeps its last frame dimmed, with a "Reconnect now" card. Detach disconnects without stopping anything. SSH links are checked after the machine sleeps. | GR:58-81 | Shipped |
| Version mismatch notice | Says whether the daemon or the app needs updating, and offers a "Copy Command" for `herdr update` (it never runs it). | GR:793-800 | Shipped |
| Session restore (shared Herdr setting) | Toggles Herdr's `[experimental] pane_history` so scrollback survives a daemon restart, for each host. | GR:805-821 | Opt-in (Herdr's own experimental flag) |
| Multiple windows and window restore | Reopens up to 64 windows with their size, position, and display. | GR:119-129 | Shipped |
| Herdr 2 (herdr-core) terminals | "New Herdr 2 Terminal" emulates raw-stream terminals locally (alacritty_terminal), with native splits, zoom, resize, and a mirror (watch-only) view. Herdr 1 and Herdr 2 tabs share workspaces. Without Herdr 1 installed, the window opens a herdr-core shell instead of asking you to install Herdr. | GR:1757-1790, AGENTS.md table (`herdr-core-client`, `herdr-terminal`, `herdr-backend`) | Shipped, bundled. Opt-in as the default for new tabs (`herdr_2_terminals = true`) |
| Selection and copy | Shift-drag selects over mouse-aware TUIs. Copy on release, with a flash. Cell-accurate wide characters. Concealed text copies as blanks. | GR:1792-1878 | Shipped |
| OSC 52 clipboard | Programs that copy through OSC 52 reach the system clipboard (bounded UTF-8 text only). | GR:1849-1853 | Shipped |
| Find in scrollback | Cmd-F searches the whole scrollback through the daemon, showing "3 of 17". | GR:4209-4220 | Shipped (needs a daemon that offers it) |
| Keyboard copy mode | Vim-like copy mode (Cmd-Shift-C) with word motions, `/` and `?` search, and visual marks. | GR:4221-4244 | Shipped |
| Open scrollback in editor | Opens a pane's history in your editor. | GR:4264-4267 | Shipped (needs daemon support) |
| Smooth scrolling | Pixel-smooth wheel and trackpad scrollback. | GR:4485-4496 | Shipped (on by default) |
| IME and Unicode input | Inline composition, dead keys, Option-as-Alt modes, AltGr on Windows. | GR:4502-4518, 4623-4629 | Shipped |
| `[pane_keys]` remaps | Mac-style Cmd-Left/Right/Backspace and Option-word motion, plus Shift-Enter as a newline. | GR:4352-4374 | Shipped |
| Images (clipboard and drop) | Pastes or drops images into agents through Herdr's image bridge. Oversized images are recompressed or downscaled, and TIFF is converted to PNG. | GR:2517-2602 | Shipped (Windows: no SSH upload yet, GR:2599-2600) |
| File drops | Dropping files pastes escaped paths. On SSH hosts the files are uploaded first. On WSL the paths are mapped. | GR:1904-1927 | Shipped |
| SSH file copies | Streams dropped files to SSH hosts with a progress card and Cancel. | GR:1929-1959 | macOS/Linux. Not on Windows yet (GR:4721-4722) |
| Clickable links and paths | Cmd-click opens URLs and `file:line` paths (verified before opening). Untrusted schemes ask first. Links can open in an in-app browser tab. | GR:2638-2713 | Shipped |
| Terminal bell | Requests attention (bounces the Dock) and can play an optional system sound. | GR:1490-1509 | Shipped |
| Accessibility | The terminal is exposed to VoiceOver, Narrator, and NVDA, and accepts dictation (AXValue). | GR:1887-1902, 4806-4888 | Shipped (gaps listed at GR:4874-4888) |
| Herdr plugins interop | Runs plugin actions with the selected text, link handlers, popups, sidebar tokens, and the Settings > Plugins token switches. | GR:2715-2776 | Shipped |
| Daemon integrations | Installs agent hooks and integrations on the selected host from Settings > Integrations. | GR:1070-1077 | Shipped |
| Product announcements and release notes | Shows the daemon's announcement card and "what's new" as plain text. | GR:1079-1088 | Shipped |
| Documented limitations | No bundled Nerd Font. No image rendering in cells. Simple two-pass painter. No notification history. | GR:4610-4635 | (note for honesty) |

---

## Desktop · 3. Workspaces, worktrees, sidebar, and navigation

| Feature | Description | Source | Status |
| --- | --- | --- | --- |
| Workspace and worktree sidebar | Linked worktrees nest under their main checkout. Rows show the daemon's status dots (idle, working, blocked, done), PR number, diff counts, ahead/behind arrows, and the owner's avatar. | GR:3790-3831, R:32-35 | Shipped |
| Telling worktrees apart | Tree guides, a branch glyph, and an optional tinted rule along the tab strip (`[worktrees]`). | GR:2041-2080 | Shipped (the terminal rule is opt-in) |
| New worktree dialog | Proposes a branch, cleans up branch names, and checks out PRs (forks too) or issues as branches. "Opens in" starts an agent such as `claude` or `codex` in the new checkout. | GR:3859-3914, 4172-4194, 2265-2280 | Shipped ("Opens in" works on macOS/Linux only) |
| Open existing worktree | A searchable picker of the checkouts a repository already has. | GR:3915-3941 | Shipped |
| Delete worktree checkout | One confirmation, with Force remove for dirty checkouts. | GR:4133-4140 | Shipped |
| Safe workspace close | Checks for uncommitted or unpushed work and asks you to type `close` when it finds any. | GR:3876-3883, 1097-1105 | Shipped |
| Remove worktrees of merged PRs | Cleans up a linked worktree once its PR merges, if the checkout is clean. | GR:1106-1115 | Opt-in (`remove_merged_worktrees`) |
| Worktree scripts | `.herdr/worktree.toml` holds setup, run, and archive scripts. Each runs in a visible tab and only after you trust it. | GR:1961-2019 | Shipped (not on WSL) |
| Worktree notes | One-line reminders on a checkout, searchable from the palette. | GR:2021-2039 | Shipped |
| Moving panes | Move to a new tab, a new workspace, another tab, or another device (copies the work and the agent session). | GR:2082-2158 | macOS/Linux clients only |
| Command palette | Double-Shift or Cmd-Shift-P. fzf-style search over workspaces, agents, commands, and projects, ranked by how often and how recently you use them. Live pane preview. Filter by agent status. Resume Palette. | GR:4275-4339 | Shipped |
| Go to Symbol and Go to File | Heuristic symbol index over the checkout (Rust, Swift, Go, Python, TS, Java, and more). Opens in your editor or in a read-only code tab. | GR:2778-2837 | Shipped (local checkouts only) |
| Code tabs | Read-only syntax-coloured file view with an outline. | GR:2799-2804 | Shipped |
| Open in editor | Opens `$EDITOR` at the line in a split. Neovim is reused through `--listen`. | GR:2783-2822 | macOS/Linux. Windows opens a code tab instead (GR:2823-2825) |
| Back and Forward | Browser-like history of focused panes. Mouse side buttons work. | GR:1682-1696 | Shipped |
| Editor groups | Side-by-side groups (Cmd-\\), each with its own live connection. Drag tabs between groups. Layout is restored. | GR:2878-2938 | Shipped |
| Activity bar | Workspaces, Overview, Pull requests, Issues, Security, Automations, Usage, and mod apps. | GR:2360-2403, CLI:71 | Shipped |
| Overview ("what needs you") | Agents waiting on you, failed runs, finished agents to review, working agents. Snooze, Settle, Pin, and Reopen (resumes Claude, Codex, or opencode). | GR:2370-2387 | Shipped |
| Devices Overview | Per-device agent activity heatmap for the last 2 hours, plus load, disk, and uptime. | GR:2839-2876 | Shipped |
| Sidebar search | Finds devices, worktrees, and branches. | GR:1132-1136 | Shipped |
| Agent icons | Brand icons for roughly 25 agents (Claude Code, Codex, Gemini, Cursor, Copilot, Amp, Kiro, Grok, and more). | GR:1440-1448 | Shipped |
| Notification mutes | Mute a workspace or a tab: no toast, no sound, no badge. | GR:1272-1303 | Shipped |
| Zen mode | Clears the window's chrome, with optional full screen, a centred column, and a font size (`[zen]`). | `src/config/zen.rs`, GR:2363-2364 | Shipped (only lightly documented) |
| Keep-awake ("caffeine") | A coffee cup in the status bar keeps the display and machine awake. Also `herdr-gpui caffeine on --for 30m`. | GR:1189-1195, CLI:178 | Shipped (macOS, Windows, Linux) |
| Network quality | Shows latency and loss in the status bar (one TCP connect every 15 s). | GR:1227-1237 | Opt-in (`show_network_quality`) |

---

## Desktop · 4. Agents, chats, and parallel dispatch

| Feature | Description | Source | Status |
| --- | --- | --- | --- |
| ACP chats (`herdr-acp`) | Runs Claude, Codex, Copilot, Gemini, OpenCode, or Pi through the Agent Client Protocol inside a Herdr pane. The app draws the chat natively over the pane: messages, reasoning, tool diffs, plan, permission requests, context meter, and cost. The TUI sees the same chat. | GR:3338-3419, `crates/herdr-acp/README.md` | **Opt-in** (`[features] acp_chats`, CLI:425-427, `features.ftl`). The binary ships in every package. Native view only for local and SSH hosts (GR:3416-3419, CLI:434-447) |
| Chat layouts | Timeline, Bubbles, Terminal, Document, Cards, plus iMessage and iA in `docs/rich-chat.md`. One-click launch buttons for each agent. | GR:3359-3368, 3405-3413, `docs/rich-chat.md:22-25` | Opt-in (with ACP chats) |
| Rich chat content | Markdown, equations, Mermaid, inline images, live HTML pages in messages, and `herdr-ui` interactive cards (choice, slider, table, chart, and more). | `docs/rich-chat.md`, SKILL.md:26-45, GR:3389-3403 | Opt-in. Live HTML only on macOS/Windows (Linux opens the browser) |
| Render existing CLI sessions | Shows a running Codex or Claude CLI conversation as a chat without restarting it. | CLI:452-459, `docs/rich-chat.md` | Opt-in (`chat.render_existing`). Docs say "Linux Codex/Claude" |
| Company endpoints | `[chat.env]` passes variables to every chat, such as GHE `GH_HOST` or `ANTHROPIC_BASE_URL`. | GR:3370-3375 | Opt-in |
| Smart Dispatch | Ranks connected hosts by spare cores and load, then creates the worktree on the best one (shipping a Git bundle when needed). | GR:2282-2358 | macOS/Linux clients |
| Fan-out | Runs one prompt across several agents and lanes, optionally spread across hosts. Compare the results, keep one, and the rest are removed. Survives a crash (`fan-outs.json`). | GR:2325-2354, CLI:134 | macOS/Linux clients |
| Teleport | Moves a worktree to another host: commits, uncommitted changes, tabs, splits, running programs, and agent sessions (Claude Code, Codex, opencode, pi/omp, Copilot CLI, Letta). Teleport back. Can lend a GitHub token. | GR:2160-2263, R:44-47 | macOS/Linux clients. Local, SSH, and cloud hosts (Coder, Daytona, Railway named) |
| Agent checkpoints | Snapshots a worktree every time an agent starts or finishes a turn, in hidden Git refs (newest 50 per branch). Restore from the workspace menu. CLI: `checkpoint list\|create\|diff\|restore`. | EX:113-119, FTL `checkpoints.ftl`, CLI:127-130, `crates/herdr-vcs/src/checkpoint.rs` | Shipped, **on by default** (`agent_checkpoints = true`). Unavailable from Windows clients for SSH hosts. WSL works (GR:4723-4725, 4764-4767). Not described in GR |
| Annotate terminal text | Shift-select text, then "Annotate" sends a note to the pane's agent. | GR:1832-1847, 4245-4259 | Shipped |
| Managed agent skills | Offers to install the `herdr-gpui-browser` skill (plus `herdr-automations` and `herdr-mods`) into `~/.claude/skills` and `~/.agents/skills`, and onto SSH hosts. | GR:3300-3336, `skills/` | Shipped (asks first) |
| Agent sound policies | Per-agent Done and Request sounds, volume control, custom MP3s. | GR:1553-1663 | Shipped |

---

## Desktop · 5. Git review, commit, push, PRs, and the orchestrator

| Feature | Description | Source | Status |
| --- | --- | --- | --- |
| Review Changes tab | PR-style diff of the uncommitted or branch changes. Unified or side-by-side, file tree, word-level highlights, "Viewed" marks, search, and huge-diff handling. Click a line to leave a note, then "Send to agent". | GR:3081-3203, R:52-61 | Shipped (local daemon checkouts only, GR:3203) |
| Title bar Git menu | Working-tree summary, then **Commit...**, **Push**, **Create pull request**, Open PR, checks, comments, **Comment...**, **Merge...** | FTL `git-menu.ftl`, `git-commit.ftl`, `pr-actions.ftl`, `crates/herdr-vcs/src/lib.rs:1-4`, GR:3970-3974, 4031-4041 | Shipped (not described as its own GR section) |
| PR badges and readiness colours | Green, red, yellow, orange, purple, or gray by checks, reviews, and merge state. Refreshed every 90 s. | GR:3942-4041 | Shipped (needs a GitHub sign-in) |
| Orchestrator: Issues | Open GitHub issues, GitLab issues, and Beads (`.beads`) issues across all the window's repositories (up to 32). Filters, sorts, search. "Start agent" on any item. | GR:2405-2445, `docs/orchestrator.md` | Shipped |
| Orchestrator: Pull requests | A table grouped by what each PR waits on (your review, yours to fix, ready to merge, ...). Each PR page shows its conversation and lets you comment and merge. | GR:2439-2459 | Shipped (GitHub). GitLab merge requests not yet (GR:2491-2492). GitHub Enterprise "not supported yet" (GR:2513-2514) |
| Dispatch agent / Review with agent | Prompt, agent, model, and branch `agent/<n>-<slug>-<hash>`. Creates a worktree and starts the agent. "Review with agent" runs a read-only PR review. | GR:2461-2483 | Shipped |
| agent-launcher compatibility | Imports agent-launcher's database and prompt profiles. | GR:2485-2490, `docs/orchestrator.md:9-25` | Shipped |
| Security section | Private vulnerability reports (titles and metadata only) and Dependabot alerts across your github.com repositories, most severe first. | CLI:71, 150-151, FTL `security.ftl` | Shipped (needs GitHub App security permissions, GR:4064-4066). Not described in GR |
| GitHub sign-in | Device flow through the "Herdr GPUI" GitHub App, a GitHub CLI token, or a PAT. Extra accounts (for example EMU) and a separate account per device. Tokens stay on this computer. | GR:4003-4128 | Shipped |
| GitLab sign-in | OAuth device code or a pasted PAT. Reads tokens from `.envrc`, `glab`, and self-managed instances. | GR:2491-2513 | Shipped |
| Credentials store | macOS Keychain (signed builds), Linux Secret Service, Windows Credential Manager, or an opt-in private `0600` file. | GR:4069-4112, 4890-4912, AGENTS.md (`herdr-credentials`) | Shipped (plaintext file is opt-in on Linux, default on macOS dev builds) |

---

## Desktop · 6. Hosts and devices: local, SSH, WSL, cloud

| Feature | Description | Source | Status |
| --- | --- | --- | --- |
| Saved SSH hosts | Runs the Herdr remote bridge over noninteractive SSH. Shares one login per host (ControlMaster-style). Handles MFA hosts through your own ControlMaster. Names the reason for refusals. | GR:17-32, 184-273 | Shipped (Windows: no connection sharing and no terminal setup, GR:4686-4728) |
| All Devices view and device filter | Spaces and Agents across every host. Cmd-click to multi-select devices. | GR:206-231 | Shipped |
| Sessions per device | List, create, attach to, and delete named Herdr sessions on any device. | GR:219-273 | Shipped |
| Add Device dialog | Checks the host, then saves it, or offers a guided setup in a terminal workspace (ssh-copy-id, installing or updating Herdr). | GR:275-433 | Shipped (Windows asks you to run `herdr machine add` yourself) |
| Nearby device suggestions | Suggests hosts from `~/.ssh/config`, Tailscale peers, and Bonjour `_ssh._tcp`. Device marks for Coder and Tailscale. | GR:312-349 | Shipped |
| Per-device terminal theme | Colours the terminal by host so you know where you are typing. | GR:1039-1051 | Shipped |
| Server keybindings per device | Equivalent to `--remote-keybindings server`. | GR:977-981, 4432-4442 | Shipped |
| WSL distributions as devices | Herdr inside WSL, reached through `wsl.exe`, with load, usage, ports, checkpoints, and path-mapped drops. | GR:4730-4792 | Windows only |
| Forward port | One `ssh -N` master per forward. Opens the page in a browser tab. | GR:374-393, CLI:146-148 | macOS/Linux (not on Windows, GR:4718-4720) |
| Listening ports | `:3000` chips per workspace (`ss` / `lsof`). Click to open in a browser tab, tunnelled automatically for remote hosts. | GR:1197-1225 | Shipped (Windows does not scan the local machine) |
| CPU, memory, disk, uptime | Sparkline and meter in the status bar and per host in the sidebar. Read over SSH for remote hosts. | GR:1161-1173, R:84-87 | Shipped |
| GPU and VRAM | NVIDIA (`nvidia-smi`), AMD (sysfs), Apple (`ioreg`, no VRAM). Intel is left out. | GR:1175-1187 | Opt-in (`show_gpu_load`, `show_sidebar_gpu_load`) |
| Cloud devices (general) | Settings > Cloud devices creates machines as devices. Removal checks for unpushed work first. Per-provider setup scripts install Claude Code, Codex, Gemini CLI, and OpenCode. Agents can drive this with `herdr-gpui cloud create/exec/destroy/wait`. | GR:358-372, 735-773, CLI:153-273 | Shipped. **Unavailable on Windows** for every provider (GR:514, 556, 578, 622-623, 643, 686) |
| Azure | VMs (over SSH) and Container Apps sandboxes (data-plane exec), with generic credentials or the Azure CLI. | `crates/herdr-gpui/Cargo.toml` features, FTL `settings-cloud-azure.ftl`, CLI:231 | Shipped (default feature). **No GR section**, only mentioned in lists |
| Coder | OAuth2 PKCE sign-in, templates and presets, one-click workspaces. | GR:463-514 | Shipped |
| Boat | Sandboxes with an API key, auto-stop, recovery of a lost creation. | GR:516-557 | Experimental ("not yet exercised against a live Boat account") |
| Daytona | Sandboxes through Daytona's SSH gateway. | GR:559-580 | Experimental ("proof of concept") |
| Containers (Docker or Podman) | Isolated local containers per agent: resource limits, `no-new-privileges`, idle auto-stop, max 8. | GR:582-624 | Shipped |
| Fly Sprites | Sprites, with the app as the exec relay. | GR:626-644 | Experimental |
| OpenSandbox (self-hosted) | Self-hosted sandbox server, with the app as the exec relay. | GR:646-668 | Experimental |
| Railway free VMs | `ssh railway.new` temporary VMs, no account needed. | GR:670-686 | Experimental (GR heading) |
| Sandbox images | `.herdr/sandbox/Dockerfile` or a devcontainer image, built per project for containers and Daytona. | GR:688-733 | Shipped (Daytona cannot COPY or ADD files yet) |
| `herdr-gpui://` links, jump list, Explorer entry | "Open in Herdr UI" on folders, recent workspaces, and devices. | GR:5000-5034, R:256-269 | Windows only |

---

## Desktop · 7. Browser, previews, VS Code, px0

| Feature | Description | Source | Status |
| --- | --- | --- | --- |
| Browser tabs | Native web views beside the terminals (WebKit on macOS, WebView2 on Windows). http and https only, downloads refused, zoom 50-300%. | GR:2940-3001 | macOS and Windows. **Linux: no embedded pages**, opens the system browser (GR:3000-3001). Plan: `docs/linux-browser-plan.md` (planned) |
| Agents open pages | `herdr-gpui browser open URL\|file.html`. Local files open in a split beside the agent. SSH-host agents use a shim. | GR:3219-3285, CLI:98-104 | macOS/Windows (Linux: web URLs only, through the system browser) |
| Page annotations and picks | Pick elements, select text, or draw regions, then write notes and "Send to agent". Screenshots on macOS. Edit, resend, or withdraw notes. | GR:3003-3079, R:66-68 | macOS/Windows. Screenshots macOS only |
| Local pages, explainer kit, page results | A private `herdr-preview:` scheme. Kit CSS/JS with theme colours. `herdr.send()` for mockup "Pick" results. | GR:3205-3217, 3287-3298, 3066-3079 | macOS/Windows |
| `browser text` and `browser screenshot` | The page's text, and a PNG for agents. | CLI:103-104 | Screenshots macOS only |
| VS Code tab | Runs `code serve-web` itself or uses your own address. One VS Code tab per space. | GR:3421-3539 | macOS/Windows (no Linux). Local folders only |
| px0 tab | The px0 review workbench in a tab. | GR:3541-3581, CLI:111-118 | macOS/Windows, local workspaces only |

---

## Desktop · 8. Plan usage

| Feature | Description | Source | Status |
| --- | --- | --- | --- |
| Plan usage in the status bar | Shows signed-in AI services closest to their limits. Click one for a details panel. Read on the selected host (local, SSH, WSL). | R:80-82, `docs/usage-providers.md:1-40` | Shipped (`[usage] show = true`) |
| Usage activity-bar section | One card per signed-in agent for each host. | GR:2391-2393 | Shipped |
| Provider count | 87 providers in `crates/herdr-usage/src/registry.rs` (88 `.rs` files, including helpers). Ported from CodexBar. Includes Codex/ChatGPT (also through OpenCode), Claude, Cursor, Copilot, Gemini, Grok (prepaid balance), OpenRouter, Bedrock, Vertex AI, Azure OpenAI, Ollama, Windsurf, Zed, Warp, Kiro, Amp, Mistral, DeepSeek, Perplexity, and more. | `crates/herdr-usage/src/providers/`, `registry.rs` | Shipped. Each needs its own sign-in or key |
| Daemon plugin usage tokens | E.g. `herdr_agents_tracker` percentages in the header and sidebar. | GR:1145-1159, 1562-1592 | Shipped |

---

## Desktop · 9. Look and feel: themes, layouts, fonts, i18n

| Feature | Description | Source | Status |
| --- | --- | --- | --- |
| Themes | Hundreds of themes (built-ins plus every Ghostty theme), live hover preview, favourite stars, a light and dark pair that follows the OS, and "Follow Herdr" (18 upstream palettes). | R:89-114, GR:874-904, 1007-1013 | Shipped |
| High contrast and WCAG floor | App marks keep 3:1 contrast (4.5:1 in high contrast). Terminal colours are never altered. | GR:1313-1323 | Shipped |
| Window backdrop | Solid, Translucent with blur, or a Picture. Acrylic or Mica on Windows 11. | GR:1331-1361 | Shipped (Linux blur only on KDE-protocol Wayland) |
| Sidebar layouts | normal, compact, comfortable (each with a `-rounded` variant), superset, orca, minimal, devices. Default for new installs: `comfortable-rounded`. | GR:1363-1429, R:72-75 | Shipped |
| Status indicator sets | Dots, Solid, Outline, Tiles, Wave, Braille, Orca, plus an "Edge rail". | GR:1007-1036 | Shipped |
| Tab layouts | Classic, Curved, Pills, Underline, Cards, Outline, Compact. | GR:1778-1781 | Shipped |
| Fonts | Four font roles, a searchable installed-font chooser, sizes 8-48, weights. | GR:906-914, 1524-1528 | Shipped |
| Terminal padding | Per-side padding. | GR:1450-1467 | Shipped |
| Settings window | Searchable and grouped. Every change applies live. Each row is tagged as shared with Herdr or local to this app. | GR:863-941, R:89-95 | Shipped (Windows cannot write shared Herdr settings) |
| Keyboard shortcuts editor | Rebind any command. Profiles per platform. Herdr prefix chords work. | GR:3726-3786, 4340-4442 | Shipped |
| Linux client-side decorations and in-window menu bar | — | GR:1712-1722, 5053-5062 | Linux |
| i18n | Fluent catalogs, a `language` setting and command, RTL-ready, `en-XA` and `en-XB` pseudo-locales. **Only English (`en-US`) ships.** | `docs/i18n.md:5, 293`, EX:49-54, `crates/herdr-gpui/locales/` (only `en-US/`) | Infrastructure shipped. Translations: **none besides English** |

---

## Desktop · 10. Extensibility: mods, automations, CLI

| Feature | Description | Source | Status |
| --- | --- | --- | --- |
| Mods (Luau) | Sandboxed Luau folders in `<config>/herdr/gpui-mods/`. A mod adds a status bar item, a sidebar section, a right-panel section, palette commands, or an activity-bar app, and reloads live. Example mods ship. | `docs/mods.md:1-40`, CLI:303-323, `crates/herdr-mods`, `crates/herdr-luau` | Shipped (on by default, `[mods]` lists disabled ones, EX:751-756). `docs/mods-plan.md` covers further phases |
| Automations | `herdr-automate` daemon per host. Luau scripts on a schedule (`at`, `every`, `cron`) or run by hand. Steps: pull, run tests, start agents in worktrees, notify. Required caps (agents, tokens per day, wall time). **Runs only after a person approves the exact text.** | `docs/automations.md`, CLI:152, FTL `automations.ftl` | Shipped (Automations section on by default, `[sidebar] automations`). Binary bundled |
| CLI control socket (`herdr-gpui COMMAND`) | About 60 commands covering windows, tabs, the browser, code, review, notes, checkpoints, scripts, teleport, fan-out, chat, hosts, PRs, ports, usage, security, cloud, remote, mods, events, config, theme, font, language, caffeine, logs, and diagnostics. `--json` everywhere. Exit-code table. Destructive actions always confirm in the app. | `docs/cli.md` (all), CLI:43-53, 342-365 | Shipped (Unix socket, or a Windows named pipe restricted to your SID) |
| Event stream | `herdr-gpui events --follow`, `wait tab-closed` | CLI:325-340 | Shipped |

---

## Desktop · 11. Remote control from a phone

| Feature | Description | Source | Status |
| --- | --- | --- | --- |
| Remote control daemon (`herdr-remote-control`) | An HTTPS PWA for phones: lists workspaces, follows `herdr-acp` chats, answers agent questions, sends Web Push notifications. Uses its own CA. Reachable on the LAN and over Tailscale. Dual-approval QR pairing. Runs as a user service (LaunchAgent, systemd user unit, or Windows logon task) and keeps running after the app quits. | `crates/herdr-remote-control/README.md`, EX:533-559, CLI:279-301 | Shipped in all packages. **Opt-in** (`[remote] enabled = false` by default). Scope is chats. In the plan, terminal tabs were "Not available yet" (`docs/remote-control-plan.md:13`) |
| Pair other machines | Approving a phone also pairs it with the SSH and cloud machines this computer connects to. | `crates/herdr-remote-control/README.md` "Other machines" | Opt-in |
| Nearby / Bluetooth (`herdr-nearby`) | Bluetooth GATT helper for pairing an iPhone nearby with "Herdr Remote", plus presence (paired iPhones keep their alerts until you step away). Swift helper on macOS, BlueZ on Linux, WinRT on Windows. | `crates/herdr-nearby/src/main.rs:1-8`, FTL `remote-pair.ftl:32-35`, `settings-remote.ftl:24` | Shipped as a binary in all packages. Depends on the iOS app |
| Herdr Remote iOS app | Native SwiftUI app with Live Activities, iPhone and iPad, iOS 26. Has a TestFlight upload script. | `ios/README.md`, `docs/ios-app-plan.md` | **Not a desktop feature.** Distribution status unclear (see facts to confirm) |

---

## Desktop · 12. Notifications, badges, updater, diagnostics

| Feature | Description | Source | Status |
| --- | --- | --- | --- |
| In-app toasts | Host-labelled "needs attention" and "finished" cards. Click to jump to the pane. | GR:4530-4578, 1239-1270 | Shipped (delivery follows Herdr's setting, which defaults to off) |
| System notifications | macOS UNUserNotificationCenter, freedesktop on Linux, WinRT toasts on Windows. Click opens the pane. | GR:3583-3629 | Shipped (when `[ui.toast] delivery = "system"`) |
| Dock badge and taskbar badge | Count of agents that are done or blocked. Taskbar progress bars. | GR:3631-3665, 5002-5011 | macOS Dock and Windows taskbar. **No Linux badge** |
| In-app updater | Ed25519-signed manifests. Separate Download and Install approvals. Homebrew upgrades through `brew`. Linux packages through APT, DNF, or pacman via PolicyKit. Windows runs the signed installer. Daemon sessions survive updates. | GR:131-162, `docs/updating.md`, R:168-175, 207-218, 386-395 | Shipped. Windows only when signed. Nix, AppImage, Snap, and Flatpak are excluded |
| Sheprd repository migration | The updater accepts the successor repository `sheprd-labs/sheprd` and assets named `Sheprd-*`, `sheprd-*`, `Sheprd.app`. | `crates/herdr-updater/src/release/repository.rs:8-17`, `release/format.rs:9-21`, `install/bundle.rs:10-11`, commit f26a014f9 | Shipped in v20261102.x (transition prep) |
| Logs window | JSONL log with rotation. Window > Logs filters, copies, and exports. Nothing is uploaded. | GR:3667-3682 | Shipped |
| Crash reports | Panic reports on all platforms. Minidumps on Windows. `herdr-gpui diagnostics bundle PATH` produces a redacted bug report. Never uploaded. | GR:3684-3703, CLI:184 | Shipped (no native-signal reports on macOS/Linux) |
| Launch at login | — | GR:4936-4950 | **Windows only** (macOS/Linux "not yet") |
| Single instance | — | GR:4952-4958 | Windows |
| QA menu | Previews of toasts, badge, sounds, update UI, pseudo-locales, slow animations. | GR:164-182, `features.ftl` | Opt-in (`qa_menu`) |
| Performance | Measured release hover p95 went from about 51 ms to 12 ms, and scroll from 56 ms to 14 ms, on an M4 Max. | R:452-467, `crates/herdr-gpui/PERFORMANCE.md` | (claim with caveats: measures CPU scene construction only) |

---

## Desktop · 13. Platform matrix of notable gaps

| Capability | macOS | Linux | Windows |
| --- | --- | --- | --- |
| Embedded browser tabs, annotations, VS Code, px0 | yes | **no** (system browser) | yes |
| Page screenshots in notes | yes | no | no |
| Teleport, fan-out, smart dispatch to other hosts, pane moves to devices | yes | yes | **no** |
| Cloud devices (all 8 providers) | yes | yes | **no** |
| SSH hosts | yes | yes | yes (no ControlMaster, no terminal setup, no uploads, no port-forward menu) |
| WSL devices | — | — | yes |
| Dock / taskbar badge | Dock | **none** | taskbar overlay and progress |
| Launch at login | no | no | yes |
| Shared Herdr settings editing | yes | yes | read-only |
| Open file in terminal editor | yes | yes | opens a code tab instead |

Sources: GR:3000-3001, 3015-3020, 2165-2167, 2357-2358, 623, 4686-4728, 3650-3651, 4949-4950, 993-995, 2823-2825.

---

## Desktop · 14. Planned or plan-doc-only (do not market as shipped)

| Item | Source | Notes |
| --- | --- | --- |
| Browser tabs on Linux | `docs/linux-browser-plan.md` | Plan. `wry` is gated to macOS and Windows today |
| Native iOS app as the main remote surface (Bluetooth pairing, full chat) | `docs/ios-app-plan.md`, `docs/ios-spikes.md` | Plan, with code in `ios/` in progress. Not desktop |
| Remote-control terminal tabs from the phone | `docs/remote-control-plan.md:13` | Plan scope says "Not available yet". The iOS README now says it shows terminal tabs, so check |
| Further mod phases | `docs/mods-plan.md` | Plan. Mods v1 itself is shipped |
| Translations beyond English | `docs/i18n.md` | Planned (infrastructure only) |
| GitLab merge requests, GitHub Enterprise in the orchestrator | GR:2491-2492, 2513-2514 | "not yet" |
| Launch at login on macOS/Linux | GR:4949-4950 | "not yet" |
| macOS Touch Bar window-close crash fix | GR:3705-3707, `docs/macos-window-close.md` | Upstream fix not adopted |
| Windows local access issue #357 | `docs/windows-local-access.md` | Unresolved investigation |

---

## Desktop · 15. Facts needing confirmation

1. **Names still say Herdr or Herdr UI everywhere.** The product is called "Herdr UI" in the README, the site (`herdrui.dev`), the Start menu, the Windows install directory (`%LOCALAPPDATA%\Programs\Herdr UI`), the winget ID `Penso.HerdrUI`, and the publisher "Herdr UI". The macOS app is `Herdr.app` (cask name "Herdr", `open -a Herdr`). The binary and CLI are `herdr-gpui`, and the URL scheme is `herdr-gpui://`. The bundle ID is `so.pen.herdr-gpui`. The GitHub App is "Herdr GPUI". Credential service names are `dev.herdr.gpui.github`. The companions are `herdr-acp`, `herdr-remote-control`, `herdr-automate`, and `herdr-nearby`. The skills are `herdr-gpui-browser`, `herdr-automations`, and `herdr-mods`. Config and state paths are `~/.config/herdr/`, `config-gpui*.toml`, and `~/.local/state/herdr/gpui`. The phone app is "Herdr Remote". Which of these change under Sheprd needs confirming. `winget.py` says the ID and publisher "must never change".
2. **Download URLs after the transfer.** Today: `github.com/penso/herdr-gpui/releases`, tap `penso/tap` (`github.com/penso/homebrew-tap`), `herdrui.dev/install.sh`, `herdrui.dev/install.ps1`, and `herdrui.dev/llms.txt`. The updater already accepts `sheprd-labs/sheprd` with assets renamed `Sheprd-VERSION-...` (`crates/herdr-updater/src/release/repository.rs:17`, `format.rs:18-21`). The new site domain, tap name, and install-script URLs are unknown.
3. **Unaffiliated wording vs. bundling herdr-core.** The README says "Not affiliated with, endorsed by, or supported by Herdr". Yet releases bundle Herdr's `herdr-core` 0.1.13 binary, downloaded from `releases.herdr.dev`, with licenses in `scripts/release/herdr-core-licenses.txt`. Confirm the exact attribution and wording to use on the site.
4. **Which cloud providers are "experimental".** The GR headings mark Boat, Daytona, Sprites, OpenSandbox, and Railway as experimental. The Settings UI tags only OpenSandbox and Sprites (`settings_window/cloud_devices/{opensandbox,sprites}.rs`). Azure has no GR section at all, yet it is a default feature. GR:775-779 says the default features are "boat, coder, daytona, railway", but `crates/herdr-gpui/Cargo.toml` defaults to all eight (azure, boat, coder, container, daytona, opensandbox, railway, sprites).
5. **Teleport's cloud host list.** GR:2165-2167 names Coder, Daytona, and Railway only. Confirm whether Azure, Containers, Sprites, and OpenSandbox work as Teleport destinations (GR:587 says containers are reached by Teleport, Fan-out, and dispatch).
6. **Windows ARM64 herdr-core.** `scripts/release/herdr-core.json` has no `windows-aarch64` target, but the Windows ARM64 zip requires `herdr-core.exe`. Confirm what ships there (likely the x86_64 build under emulation).
7. **Windows signing and winget.** README says that until signing is set up, Windows builds "ship unsigned, are not submitted to winget, and do not update themselves" (R:381-386). Confirm whether current releases are signed before claiming auto-update or winget on Windows.
8. **ACP chats default.** The CLI docs say chats are opt-in (`acp_chats = true`). GR's Chats section reads as if always available. Confirm the default before marketing "native chat" as on by default.
9. **"Render existing sessions" platform.** CLI:454-455 says "Linux Codex/Claude conversations". Confirm whether macOS works.
10. **Remote control scope.** The plan says chats only and that terminal tabs are not available. `ios/README.md` says the iOS app opens "a chat or a terminal". Confirm what the shipped PWA does with terminals.
11. **iOS app availability.** `ios/` has TestFlight tooling, and the desktop pairing dialog references "Herdr Remote on an iPhone nearby". Is it on TestFlight or the App Store? If not, the nearby-pairing feature cannot be marketed yet.
12. **Usage provider count.** 87 registered in `registry.rs`. Round it ("85+ AI services") or verify the final count at publish time.
13. **Agent icon count.** GR:1440-1446 lists 20 brand marks and 4 lettermarks. Teleport session transfer covers 6 agents, and chat launch covers 6.
14. **Minimum OS for Windows** is not stated beyond "Windows 10 and 11" in the manifest (GR:4976-4979). Acrylic and Mica need Windows 11 22H2+.
15. **The Linux macOS-style Git permission note** (GR:83-110) and other troubleshooting are not marketing material, but signal known rough edges.
16. **Checkpoints, Security, the commit/push/PR-creation menu, Automations UI, Mods, Remote control, and Azure** have no dedicated section in GR. Their status above rests on code, FTL strings, `docs/cli.md`, and the example config. A product owner should confirm they are release-quality.

---

## Phone, tablet, watch and nearby

### Release timeline (from git)

| Piece | First commit | First tag containing it |
| --- | --- | --- |
| `crates/herdr-remote-control`, PWA, `ios/` Live Activity companion, `HerdrRemoteWidgets` | 38ea4248d `feat(remote): remote control from a phone (#775)` | v20261026.1 beta, then v20261028.1 stable |
| Native iOS app (`HerdrRemote` rewrite), Notifications, Share, Watch, Nearby, `crates/herdr-nearby`, `macos/herdr-nearby` | 3c749ceed `feat(ios): native Herdr Remote app, and one pairing for every machine (#905)`, 2026-10-31 | v20261102.1 beta, then v20261102.2 stable (desktop side) |
| Herdr 2 / herdr-core client (`crates/herdr-core-client`, pinned core `scripts/release/herdr-core.json` v0.1.13) | 259a88c95 (#907), 2026-10-30 | v20261102.1 / .2 |

### Is the iOS app on the App Store or TestFlight?

- **TestFlight: yes, it is uploaded there.** Not public on the App Store.
- **Workflow:** `.github/workflows/ios.yml` has a "TestFlight upload" job. The latest main runs succeeded:
  - run 41584827171, 2026-11-03: "Swift tests and simulator build" succeeded, and "TestFlight upload" succeeded.
  - runs 41584799423 and 41224237064 also succeeded.
  - The repo has a `testflight` environment.
- **Bundle and versioning:** bundle id `so.pen.herdr.remote`. The build number comes from App Store Connect (`ios/tools/next-build.py`).
- **Who can install it:** `docs/ios-app-plan.md` "TestFlight from CI" says: "Internal testers get each build at once; an external group is a manual step in App Store Connect."
  - The repo contains no public TestFlight link (`testflight.apple.com`) and no App Store link (`apps.apple.com`); `git grep` finds none.
  - So distribution today is internal TestFlight, or building from source with your own team (`ios/README.md` "Sign with your own team", `just ios-build`).
- **Platform:** `docs/updating.md:331` confirms iOS and watchOS use App Store/TestFlight identities.
- **Do not claim** "Download on the App Store" or a public TestFlight. Safe wording: "Native iPhone/iPad app (TestFlight beta; build from source today)".

### Feature table

| Feature | Source ref | Status | Notes |
| --- | --- | --- | --- |
| Remote-control daemon `herdr-remote-control` (axum/Tokio HTTPS user service; LaunchAgent, systemd user unit, Windows logon task) | `crates/herdr-remote-control/README.md`; `docs/remote-control-plan.md` "Architecture", "Lifecycle" | **shipped** (since v20261026.1; release.yml builds it on mac, Linux and Windows; static musl copies go to SSH and cloud hosts) | Keeps running after the desktop app quits |
| Web/phone UI served by the daemon (mobile-first server-rendered HTML; Tailwind, Alpine CSP, SSE; installable PWA): home, attention board, chat focus view, question sheet, composer with photos and `/` commands, new workspace or chat | `docs/remote-control-plan.md` "Mobile UI" | **shipped** | Android and any browser use this. It is the fallback for phones without the native app |
| Terminal tabs on the phone (pane.read every 300 ms, send_keys, key rows, split, zoom, close) | `docs/remote-control-plan.md` "Terminal tabs" | **shipped** (Phase 5 "built") | "Size for phone" is still planned |
| Pairing: dual approval (QR, a matching 6-digit code on both screens, Allow on the desktop) | `docs/remote-control-plan.md` "Pairing and authentication"; `ios/README.md` "Pair" | **shipped** | Device key in the Secure Enclave on iOS; challenge-signed sessions; revoke |
| Own CA, TLS without DNS, works by LAN IP and Tailscale; the app pins the CA fingerprint | `docs/remote-control-plan.md` "TLS without DNS"; `crates/herdr-remote-control/README.md` | **shipped** | Pairing prefers a Tailscale address. Default port 7443 |
| One phone, many computers (allowing a phone also enrolls it on the SSH and cloud machines Herdr UI connects to) | `crates/herdr-remote-control/README.md` "Other machines"; `docs/remote-control-plan.md` "Several computers" | **shipped** (#905) | `herdr-remote-control enroll` over SSH |
| Web Push notifications from the PWA | `docs/remote-control-plan.md` "Push notifications" | **shipped** | |
| Native iPhone + iPad app `HerdrRemote` (iOS 26, Swift 6): merged home across computers, board, chat with Markdown, composer (photos, camera, on-device dictation, `/`), session sheet, queue, new workspace or chat, native terminal with key rows and hardware keys | `ios/README.md`; `docs/ios-app-plan.md` "Phases" (1, 3, 4) | **shipped to TestFlight**, checked in the simulator only | Plan: "Still to check on a phone: dictation, the camera, paste, and Face ID". The terminal screen is SwiftUI text, not yet Core Text |
| iPad: split view, attention list beside the chat, terminal inspector (⌘⌥I), multiple windows, keyboard shortcuts | `ios/README.md` "iPad and Apple Watch"; plan Phase 8 | **TestFlight**, checked in the iPad simulator | |
| Live Activities and Dynamic Island (per running turn: step, elapsed time, cost; amber when a question waits; push-to-start through APNs) | `ios/HerdrRemoteWidgets/ChatLiveActivity.swift`; `ios/README.md` "What the daemon sends"; `docs/remote-control-plan.md` "iOS Live Activities" | **experimental** (in TestFlight builds) | Needs the user's own APNs `.p8` key in the daemon's state dir. Lock-screen answer buttons "need a device" (`docs/ios-spikes.md`) |
| Sealed APNs alerts (end-to-end sealed; Apple sees only "Herdr / New activity"), Notification Service Extension, answer and reply actions, badges, background clears | `ios/HerdrRemoteNotifications/NotificationService.swift`; `ios/README.md` "Sealed alerts"; plan Phase 5 | **experimental** | The extension and APNs delivery have not been verified on a device. Requires the user's APNs key |
| Widgets ("Needs You": home and lock screen), Controls (Control Center, Lock Screen, Action button), Siri/App Intents, Spotlight-indexed chats | `ios/HerdrRemoteWidgets/*`, `ios/HerdrRemote/Intents/`; plan Phase 7 | **experimental** (built; "Needs a device or hands") | |
| Share extension (send text, a link or up to 4 pictures to an existing chat) | `ios/HerdrRemoteShare/ShareViewController.swift` | **experimental** | Sending to a new chat from the share sheet is not built |
| Face ID app lock, and Face ID before "always" answers | `ios/README.md` "Around the system" | **experimental** | |
| Apple Watch companion (what needs you, what is running, answers by tap or dictation, relayed through the phone; holds no keys) | `ios/HerdrRemoteWatch/*`; `ios/README.md` "iPad and Apple Watch" | **experimental / early** | Plan Phase 8 lists the Watch last. The code exists, but the "Phases" status text does not record it as checked. Sample mode `-sample YES` |
| Nearby pairing over Bonjour (`_herdr-remote._tcp`, pairing=1), no camera, commitment-based SAS | `docs/ios-app-plan.md` "Nearby pairing, exactly"; `macos/herdr-nearby/Bonjour.swift` | **shipped** desktop side; iOS side in TestFlight | Ran end to end in the simulator |
| Nearby pairing over Bluetooth (GATT): macOS Swift helper `macos/herdr-nearby`; Linux (BlueZ via zbus) and Windows (WinRT) Rust helper `crates/herdr-nearby` | `docs/ios-app-plan.md` "Bluetooth on the desktop"; `docs/ios-spikes.md` | **experimental** (ships in the v20261102.x desktop bundles, but "Bluetooth waits for a device") | The helpers hold no secrets; the crypto runs in the daemon |
| Bluetooth presence after pairing (hold phone alerts while the phone is near and the desktop is active; address refresh; wake on return) | `docs/ios-app-plan.md` "Bluetooth after pairing", "Presence, exactly"; Phase 6 | **experimental**, off by default (`[remote] presence`) | L2CAP fallback is **planned-only** |
| Desktop side: Settings > Remote control, title-bar phone button, QR pairing dialog drawn in GPUI, `herdr-gpui remote status/pair/devices` CLI | `docs/remote-control-plan.md` "Herdr UI"; screenshots `remote-pairing-*.png` | **shipped** | |
| Android native app | `docs/remote-control-plan.md` (the PWA "stays for Android") | **planned-only / none** | Android uses the PWA and Web Push |
| Passkeys, "Size for phone" terminals, Android notification actions | `docs/remote-control-plan.md` "Phases" 3 and 5 | **planned-only** | |

**Platforms**
- **Desktop:** the remote-control daemon and helpers run on macOS, Linux and Windows.
- **Phone:**
  - Native app on iOS 26+ (iPhone and iPad) and watchOS.
  - Any modern mobile browser through the PWA, including Android.
- **Network:** LAN, or Tailscale for access from anywhere.

**Evidence that something was tested on a real device: none recorded.** The plan says repeatedly that device checks are pending: "device checks wait for a signed build", "Needs a device".


---

## Old site: claims now obsolete


1. **"Herdr's TUI is great. Here's what a window adds" and the whole TUI-versus-window framing.**
   - Herdr now ships its **own native desktop app** (herdr.dev, `herdrdev/herdr-app`).
   - So "a native window for Herdr" is no longer this project's unique position, and saying so implies the first-party product lacks a GUI.
2. **"for the Herdr daemon you already run" / "install Herdr first" / "never installs or upgrades it; it only starts an already-installed `herdr server`".**
   - This is stale since v20261102.x.
   - The app now also supports Herdr 2 (herdr-core) terminals, and bundles a pinned herdr-core (`scripts/release/herdr-core.json` v0.1.13), which it starts when no core runs (AGENTS.md "Protocol And Safety"; `crates/herdr-gpui/README.md` "Herdr 2 Terminals").
   - Installing Herdr first is therefore no longer strictly required.
3. **"Same daemon. Same sessions. Use both at once."** This is true only for Herdr 1 (`herdr server`) sessions. Herdr's own app and herdr-core do **not** share agents with Herdr TUI (herdr.dev FAQ), so be precise about which backend.
4. **The "herdr TUI" capability rows describe Herdr TUI only.** They ignore Herdr's new app, which has spaces/tabs, an agents sidebar, SSH machines in one sidebar, mods, and layouts. Comparisons against "Herdr" are now ambiguous and could read as disparaging the first-party app.
5. **No mention of mobile at all.** The remote control, PWA and iOS app are missing.
6. **Brand assets** (`herdr-ui-icon-badge.png`: a ram's head with `>-` and a red "1" badge; social preview "Herdr UI") all carry the old name.
7. **Requirement text:** "macOS 14.2+" and the GPUI client wording are fine. The "WSL daemons work too" claim is still valid.


---

## The external Herdr projects (for positioning and attribution)

| Item | Finding | Source |
| --- | --- | --- |
| Owner | GitHub org **herdrdev** ("open agent runtime", https://herdr.dev). Lead maintainer **ogulcancelik** (1,314 commits to herdr; also top on herdr-app), then JJLiebig. No company named. Contact hey@herdr.dev; X @herdrdev; Discord | `gh api orgs/herdrdev`; https://github.com/herdrdev/herdr; local `~/code/herdr/README.md` |
| License | **Apache-2.0** for herdr, herdr-app and herdr-core. Site: "Open source, Apache 2.0"; "The app, herdr-core and Herdr TUI are all open source" (free) | https://herdr.dev/ FAQ |
| **Herdr (the app), new and now the headline product** | "Made for your agents. Remade by them." "A native app for running coding agents." Desktop app on macOS (Apple silicon), Linux x86_64/arm64 and Windows x86_64. Spaces, tabs, agents sidebar (Working/Blocked/Done/Idle), SSH machines in one sidebar, layouts, mods, config in `~/.config/herdr-app`. Repo `herdrdev/herdr-app` (Rust, created 2026-10-16, about 1.7k stars). Install: `curl -fsSL https://releases.herdr.dev/app/install.sh \| sh`, `brew install --cask herdr-app`, Herdr.dmg, `irm https://releases.herdr.dev/app/install.ps1 \| iex` | https://herdr.dev/, https://github.com/herdrdev/herdr-app |
| **herdr-core (`hc`), what this repo calls "Herdr 2"** | "A small server on your machine" that runs terminals that stay alive. CLI: `hc run/ls/attach/wait/read/send`, `hc machine add`, `hc --skill`, `hc schema` (OpenRPC). Status comes via **OSC 7501 "Program Status Protocol"**; "draws nothing and guesses nothing". **The API is explicitly open to third-party clients** (examples given: phone app, editor panel, Stream Deck). Install `curl -fsSL https://releases.herdr.dev/core/install.sh \| sh`. Repo `herdrdev/herdr-core` | https://herdr.dev/core/, https://github.com/herdrdev/herdr-core |
| **Herdr TUI, the original (this repo's "Herdr 1")** | "Herdr TUI is the original Herdr: a terminal app with its own server." It is still supported, at v0.9.5 (https://herdr.dev/tui/). The app and herdr-core **do not share agents** with the TUI. Install `curl -fsSL https://herdr.dev/install.sh \| sh`, `brew install herdr`, `mise use -g herdr`. Repo `herdrdev/herdr` (about 47.7k stars). Tagline "the runtime your coding agents live on" | https://herdr.dev/tui/, https://herdr.dev/ FAQ, https://github.com/herdrdev/herdr |
| Mobile | herdr.dev mentions no first-party mobile app. The core page names a "phone app" only as an example of what third parties could build | https://herdr.dev/core/ |
| "Herdr Code" | No such product name found on herdr.dev or in search | web search; herdr.dev |
| Local checkout | `~/code/herdr` is herdrdev/herdr at dc6b6e5f (2026-10-12), the TUI only. It predates the app and core launch on the site, so it is stale for positioning | local |

### Positioning implications for Sheprd

- **Herdr is no longer TUI-only.** Herdr now has its own first-party native desktop app, built on herdr-core.
  - Drop "Herdr's TUI is great, here's what a window adds" and "a native window for Herdr".
  - Position Sheprd as an **independent, third-party client** that works with:
    - Herdr TUI's server (`herdr server`, generation 1)
    - herdr-core (Herdr 2; a pinned core is bundled)
  - Differentiate on what Sheprd itself adds:
    - Orchestrator and dispatch, fan-out, review and checkpoints, Teleport
    - Cloud devices (Coder, Daytona), VS Code and browser tabs
    - Phone remote control (PWA, plus the iOS app on TestFlight)
    - Automations and mods
- **Do not say "the same sessions as Herdr's app."** The first-party app's agents live in herdr-core, while Sheprd shows Herdr 1 sessions and its own herdr-core terminals. Whether Sheprd and herdr-app see each other's herdr-core terminals is not verified; check before claiming it.
- **Do not imply endorsement.** Avoid Herdr's logo, its "Made for your agents" phrasing, and the name "Herdr" as the product name. The repo already tracks the "unaffiliated" notice.

### Suggested attribution line

> Sheprd is an independent, open-source (Apache-2.0) client for [Herdr](https://herdr.dev/)'s terminal servers, `herdr server` and [herdr-core](https://herdr.dev/core/). It is not made, endorsed or supported by the Herdr project (herdrdev). Herdr is Apache-2.0 software by its maintainers.

Shorter, for the footer: "Works with Herdr and herdr-core. Not affiliated with or endorsed by the Herdr project."

### Sources
- https://herdr.dev/ (home and FAQ)
- https://herdr.dev/core/
- https://herdr.dev/tui/
- https://github.com/herdrdev (via `gh api`): herdr, herdr-app, herdr-core
- https://github.com/herdrdev/herdr-app (README)
- Third-party write-ups, now describing the old TUI-only Herdr: https://tecmint.com/herdr-run-ai-coding-agents-in-linux-terminal, https://www.linuxlinks.com/herdr-terminal-based-agent-multiplexer/, https://www.bitdoze.com/herdr-agent-multiplexer/
