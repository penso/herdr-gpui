# sheprd.org: positioning brief and ten website concepts

Exploration only. Nothing here replaces `site/` or is deployed. Open
`index.html` (the gallery) to compare the ten concepts; each lives in
`concepts/NN-name/` and shares `assets/` (paw mark, screenshots, `base.css`,
and the one feature dataset `features.js`). Static files only, no build step,
so the folder can be dropped onto Netlify as is.

The full, source-referenced feature inventory is in
[`FEATURE-INVENTORY.md`](FEATURE-INVENTORY.md).

## What changed since the old site

The old site (herdrui.dev, "Herdr UI") sold *a window for the Herdr TUI*.
That framing no longer holds:

- Herdr now ships its own native desktop app (herdr.dev, `herdrdev/herdr-app`)
  and a server, herdr-core. Saying "Herdr is terminal-only" or "here's what a
  window adds" would be false and would read as a dig at the first-party app.
- This client now runs both Herdr 1 sessions (`herdr server`) and Herdr 2
  (herdr-core) terminals, and bundles a pinned herdr-core, so "install Herdr
  first" is no longer required.
- It grew far beyond a window: orchestration, review, checkpoints, Teleport,
  cloud machines, browser annotations, plan usage, mods, automations, a CLI
  for agents, and phone remote control.

So Sheprd should be positioned **on its own merits, as a complete workbench
for coding agents**, with one honest line about which terminal servers it
uses.

## Audience

1. **Primary: developers who already run several coding agents** (Claude
   Code, Codex, Gemini, OpenCode, Copilot…) in parallel and are drowning in
   terminal tabs, worktrees and "is it done yet?".
2. **Multi-machine developers**: people with a beefy desktop, SSH boxes, cloud
   sandboxes or WSL, who want agents to run where the cores are.
3. **Tech leads** turning issues into agent work and reviewing the result.
4. **Contributors and the security-minded** who judge a tool by its
   architecture, release integrity and how it treats secrets.

## Positioning statement

> Sheprd is an open-source desktop app for running a team of coding agents.
> It keeps every agent, worktree, machine and pull request in one native
> window on macOS, Linux and Windows, then helps you review, checkpoint and
> ship what they produce, and follow them from your phone.

## Proposed hero copy (options)

- **"Run a team of coding agents from one native window."**
  Sub: Parallel worktrees, every agent CLI, local, SSH and cloud machines,
  review and checkpoints, open source.
- **"Every agent. Every worktree. Every machine. One window."**
- **"From issue to merged PR, with agents doing the typing."**
- **"Herd your agents."** (warm/brand-led concepts only; plays on the name)

Recommended default: the first one. It is concrete, true today, and leaves
"how" to the page.

## Strongest differentiators (all shipped unless noted)

1. **The whole loop in one app**: issues from GitHub, GitLab and Beads →
   dispatch an agent into a new worktree → review the diff with notes sent back
   to the agent → commit, push, open and merge the PR.
2. **Parallel across machines**: Smart Dispatch picks the least-loaded host;
   Fan-out runs one prompt across several agents; Teleport moves a live
   worktree and its agent sessions between machines (macOS/Linux clients).
3. **A safety net for agent work**: a Git checkpoint at every agent turn,
   safe close that refuses to drop unpushed work, automations that run only
   after you approve the exact script.
4. **An agent-aware browser**: agents open previews beside the terminal; you
   pin notes to elements and send them back (macOS/Windows).
5. **Your agents in your pocket**: phone remote control with dual-approval
   pairing and its own CA (opt-in, any phone browser); native iPhone/iPad app
   in TestFlight beta.
6. **Native and cross-platform**: Rust + GPUI on macOS, Linux and Windows,
   x86_64 and ARM64.
7. **Open and verifiable**: Apache-2.0, signed updates, Sigstore, SBOM,
   secrets in the OS keychain, logs and crash reports kept local.

Supporting depth: 26 Rust crates, ~380k lines of production Rust, 8,000+
tests (method in the inventory). Use as a footnote, never as the pitch.

## Core workflows to tell

1. **Morning triage**: Overview shows what needs you; usage bar shows plan
   headroom; Devices shows which machine is free.
2. **Issue → PR**: pick an issue, dispatch, watch, review, ship.
3. **Fan-out and compare**: one prompt, three agents, keep the best.
4. **Move the work**: teleport a worktree to the GPU box; port-forward its
   preview back.
5. **Away from the desk**: phone notification → answer the agent's question.
6. **Make it yours**: themes and layouts, a mod in Luau, a nightly automation.

## Install and CTAs

Updated after merging main's rename (#1021). The repository moved to
`sheprd-labs/sheprd`; `penso/herdr-gpui` redirects there.

| Platform | Command | State on 2026-11-03 |
| --- | --- | --- |
| macOS 14.2+ (universal) | `brew install penso/tap/herdr-gpui` | Works. Main keeps this tap and token for upgrades; no `sheprd-labs` tap exists |
| Linux x86_64/ARM64 (APT, DNF, pacman) | `curl -fsSL https://sheprd.org/install.sh \| sh` | Needs sheprd.org live; the domain is still parked |
| Windows x86_64/ARM64 | `irm https://sheprd.org/install.ps1 \| iex` | Needs sheprd.org live |
| Nix | `nix run github:sheprd-labs/sheprd` | From main |
| iPhone/iPad | TestFlight beta (internal), or build from source | No public link |
| Any phone | The remote-control web app served by your computer | Opt-in |

The rename is not in a release yet: `v20261103.1` still ships `Herdr-*` assets,
and the tap's cask still names `Herdr.app`. The CLI on main is `sheprd`, with
`herdr-gpui` kept as a compatibility entry point on Linux and Windows.

## Attribution line (every concept carries it)

> Sheprd is an independent, open-source client for Herdr's terminal servers
> (`herdr server` and herdr-core). It is not made, endorsed or supported by
> the Herdr project.

## Facts requiring confirmation before launch

1. **Homebrew**: main keeps `penso/tap/herdr-gpui`. If a `sheprd-labs` tap is
   planned, its name and migration need deciding; the mockups show main's command.
2. **Launch order**: sheprd.org must serve `install.sh` and `install.ps1`, and a
   Sheprd-named release must ship, before these pages can go live.
3. **Attribution wording** given that releases bundle Herdr's herdr-core.
4. **Experimental cloud providers**: README and Settings disagree (README:
   Boat, Daytona, Sprites, OpenSandbox, Railway; Settings tags only
   OpenSandbox and Sprites). Azure has no README section.
5. **ACP chats default**: opt-in today (`[features] acp_chats`); confirm before
   calling chat a headline feature.
6. **Phone terminal tabs**: the remote-control plan lists them as built
   (Phase 5) in one place and "not available yet" in another.
7. **iOS distribution**: TestFlight uploads succeed but no public link exists;
   Live Activities, widgets, Watch and Bluetooth are untested on devices.
8. **Windows signing**: auto-update and winget on Windows depend on it.
9. **Teleport destinations** beyond Coder, Daytona and Railway.
10. **Usage provider count** (87 registered today; site says "85+").

## The ten concepts

See the gallery for previews and the comparison table. Summaries, audiences
and tradeoffs:

| # | Name | Big idea | Audience | Tradeoff |
| --- | --- | --- | --- | --- |
| 01 | Field Guide | Editorial long-read with chapters and figure captions | Readers who decide before installing | Slow to scan; needs strong writing |
| 02 | Control Room | Dark ops console; the hero *is* a live agent board | Power users running many agents | Dense; can intimidate newcomers |
| 03 | The Loop | Swiss, clean; a six-step issue→PR rail you can step through | Tech leads, teams | Workflow-first hides breadth until later |
| 04 | Atlas | The homepage is the searchable feature directory | Evaluators comparing tools | Weak emotional pitch |
| 05 | Bento | Bright modular tiles, launch-day energy | Broad dev audience, social | Tiles can trivialize depth |
| 06 | Prompt | The site as a terminal session, features as commands | CLI natives, agent builders | Niche aesthetic; harder for skimmers |
| 07 | Everywhere | Desk, servers, pocket: devices and hosts first | Multi-machine and remote workers | Phone story leans on beta pieces |
| 08 | Spec Sheet | Brutalist engineering datasheet, architecture and trust | Contributors, security reviewers | Cold; numbers can read as bragging |
| 09 | The Flock | Warm illustrated "herd your agents" story with the paw | Newcomers, indie devs | Whimsy can undercut seriousness |
| 10 | Keynote | Cinematic scroll; one sticky window that changes per chapter | First-time visitors | Heavy on motion and screenshots |

Social-card direction is noted in each concept's `<head>` (`og:` tags) and in
the gallery: cream paw on orange for the brand-led concepts, a cropped real
screenshot with headline for the product-led ones. No card images are
generated yet.

## Direction after review

The shortlist was 01 Field Guide, 03 The Loop and 07 Everywhere, and
`concepts/11-the-guide/` combines the marked parts:

| Part | From |
| --- | --- |
| Style: serif, cream paper, chapters | 01 Field Guide |
| Tagged agent board | 02 Control Room |
| Six-step issue to PR rail | 03 The Loop |
| Tilted hero window | 05 Bento |
| Headline, machines, phone, platform list | 07 Everywhere |
| Feature table and directory, shared filters | 08 Spec Sheet and 10 Keynote |
| Get Sheprd ending | 10 Keynote |

04 Atlas becomes the documentation companion at `docs.sheprd.org` (future).

The phone in 07 and 11 is a hand-drawn illustration, not an iOS screenshot.
A real capture from the app's `-sample YES` mode should replace it before
launch.

## herdrui.dev rename page

`herdrui-rename/index.html` is a mockup of the notice for the old domain: one
self-contained file that says Herdr UI is now Sheprd, then opens the same path
on sheprd.org after five seconds. Visitors can stop it, it still moves on
without JavaScript, and away from herdrui.dev it stops at zero for review.

Before it replaces the `herdrui-site` Cloudflare Worker:

- sheprd.org must be live, or every visitor is sent to a parked domain.
- `/install.sh`, `/install.ps1` and `/llms.txt` must never return this page.
  Keep serving them, or answer with a permanent redirect, so `curl | sh` and
  `irm | iex` from old docs keep working.
- `site/README.md` plans a permanent redirect instead. This page trades that
  for a visible notice, so search engines get a weaker signal; it sets
  `noindex` and a canonical link to compensate.

## Launch order

1. Publish a Sheprd-named release. `site/install.sh` and `site/install.ps1` on
   main look for `Sheprd-*` assets, and the latest release, `v20261103.1`, only
   has `Herdr-*` assets, so they fail until then.
2. Promote The Guide into `site/`. Merging `design/` publishes nothing, because
   Netlify serves `site/`, and `site/tests/test_site.py` must keep passing.
3. In Netlify, connect `sheprd-labs/sheprd`, then remove `build.ignore` from
   `netlify.toml` and update `site/tests/test_netlify_pause.py`, which requires it.
4. Add `sheprd.org` and `www.sheprd.org` in Netlify and point the Namecheap DNS
   at it. The domain still resolves to Namecheap's parking page.
5. Replace herdrui.dev with the rename page last.

## Recommendation

Written after building and checking all ten:

1. **03 The Loop** as the homepage. The issue → worktree → agent → review →
   checkpoint → PR rail tells the whole product in one interaction, each step
   carries a real screenshot and its caveat, and the Swiss system scales as
   features grow.
2. **10 Keynote** if launch impact matters more than upkeep. Real screenshots
   carry it and the sticky window reads "native app", but it is the most
   expensive to keep current.
3. **04 Atlas** as the docs companion at `docs.sheprd.org` whichever homepage wins. It is the
   honest answer to "list everything we do": search, status and platform
   filters, and the platform matrix.

Worth stealing into the winner: 08 Spec Sheet's architecture and trust
sections (as a `/security` or `/architecture` page), 07 Everywhere's device
constellation for the machines section, and 02 Control Room's agent board as
an animated hero illustration.
