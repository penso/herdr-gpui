# Rich chat

Herdr UI renders Markdown, equations, code and native interactive cards in ACP
chats. It can also show existing Codex and Claude CLI conversations above their
original terminal input, without restarting, resuming or replacing the agent.

## Enable and configure

Enable **ACP chats** in Settings > General, or set the following in the
[GUI configuration](../crates/herdr-gpui/README.md#configuration):

```toml
[features]
acp_chats = true

[chat]
layout = "bubbles"
render_existing = true
```

`render_existing` opts into discovery of running Codex and Claude sessions.
It is disabled by default. The chat header cycles
through **Timeline**, **Bubbles**, **Terminal**, **Document**, **Cards**,
**iMessage**, and **iA**. **iA** sets the chat like iA Writer: one column at
about 70 characters a line, your prompts and the agent's steps as quiet notes
hung from a gutter mark, the agent's prose in full ink, and earlier turns muted
until you point at them.
Select a layout directly with:

```sh
herdr-gpui config set chat.layout bubbles
```

In **Settings > General**, turn on **Full-width chat** to use the available
conversation area, or set **Chat width** with the slider or numeric field.
The default is 800 logical pixels; custom widths range from 320 to 4,000 and
shrink to fit narrower windows. The transcript and native composer use the
same width. At the default, the agent's prose keeps to about 70 characters a
line, the length that reads most easily, while code, tables, and pictures use
the whole column; a width you choose is followed as set. The equivalent
commands are:

```sh
herdr-gpui config set chat.max_width 0     # full width
herdr-gpui config set chat.max_width 1200  # custom width
herdr-gpui config unset chat.max_width    # restore the default
```

The Terminal layout still renders chat. The terminal button and
`herdr-gpui chat hide` show the agent's actual terminal;
`herdr-gpui chat show` restores the chat view. These commands change presentation
without starting or stopping the agent. See the [CLI guide](cli.md#existing-agent-rendering).

On first discovery or while waiting for the first history snapshot, a loading
view covers the conversation area while the original terminal editor remains
usable below it. **Use terminal** reveals the full original interface. Questions
and approvals keep that interface visible; failed or unmatched discovery falls
back to it. Local and remote discovery retain their before/after identity checks,
grouping the reads into two command streams instead of four.

## Sub-agents and background shells

When the agent hands a task to a sub-agent or leaves a command running in the
background, the summary above the composer says so below the activity strip,
such as `2 sub-agents · 1 background shell`. A background shell
outlives the turn that started it, so once the agent is idle the row keeps
saying what still runs (`1 background shell still running`) and clears when
everything has ended. Click it for a table of each one: what it is, how long it
has run or ran, and whether it is running, done, failed, or stopped.

A sub-agent's call is marked **Sub-agent** in the conversation, with the number
of steps it took; open it to list them. Its steps stay in its card rather than
in the conversation. A call that started a background shell says `running`
until the shell ends, even though the call itself is done. Which agents report
this, and how, is in the [`herdr-acp` README](../crates/herdr-acp/README.md#behaviour).

## Messages and controls

Write normal Markdown and TeX in user and agent messages. Rich chat supports
headings, lists, links, aligned tables with horizontal scrolling, Mermaid,
attached images, and explicitly loaded remote pictures. Conversation prose uses
the UI font; inline code and code fences use the terminal font. Code blocks
preserve blank lines, show tabs at four-column stops and scroll without wrapping.
**Copy code** preserves the parsed source's tabs, indentation and Unicode.

Conversation typography combines proportional prose with theme-colored headings,
emphasis, links and inline code. Quotes have a tinted panel and accent rail;
code keeps its monospace font. Colored user bubbles retain legible inherited
text instead of applying accents that could disappear against the bubble.

Inline and display equations accept dollar and bracket delimiters outside code.
Multiline display equations preserve TeX line breaks, including a standalone
`=`. Equations scale with prose and align with inline baselines. Failed display
previews show a notice and readable LaTeX; copying retains the formula source.

With rich chat focused, **Ctrl+-**, **Ctrl++** (or **Ctrl+=**) and **Ctrl+0**
change or reset text size on Linux and Windows, including while typing in the
original agent editor. macOS uses Cmd. The **View** menu provides the same font
size actions. Adjustments last for the window until reset or configuration reload.

Drag across conversation text to select across paragraphs, code, table cells and
messages. **Shift-click** extends the selection; double-click selects a word and
triple-click selects a paragraph or code line. Drag beyond the list edge to
scroll while selecting. With the conversation reader focused, **Ctrl+C** copies,
**Ctrl+A** selects all retained readable text, and **Esc** clears the selection
(macOS uses Cmd for copy/select-all). Click the composer or original CLI editor
to return typing there. Copying from the reader never sends an interrupt to the
agent. Dragging across a link selects its label; a plain click still opens it.

Scrolling up or selecting text keeps your reading position when new output
arrives. **Jump to latest** appears while the newest output is below the viewport,
centered above the input with theme-aware cyan text and a
downward arrow. Clicking it clears the text selection and resumes following
new output without changing the composer draft or moving keyboard focus.

Selected code preserves original tabs; tables copy as tab-separated cells and
newline-separated rows. Formulas are atomic selections and copy their original
TeX. Collapsed card details and retained hidden source are omitted until opened;
copy buttons remain available for complete retained code or original records.
Image pixels, embedded webviews and widget controls are separate surfaces, not
selectable conversation text.

ACP chats retain their native composer, permissions, tools, plans, context
information and file attachments. Tool and thinking cards start collapsed and
expand to show retained text and diffs.

## Streaming and tool activity

Every layout has an activity strip above the input. Its standard indicator uses
blue for running work, purple for thinking, amber for requests for input, green
for completed turns, and red for failures. Labels and icons accompany the colors.
Active work animates three dots; iMessage instead keeps its native typing cue,
with the shared metrics below it. Waiting, idle and disconnected states do not
animate. Animations use the app's motion settings; the three-dot indicator is
capped at 12 frames per second.

Elapsed time appears only with a reported turn start and a live working state.
Context usage is labeled separately from the previous turn's reported output
and thinking tokens. Unknown counts are omitted. Existing terminal sessions
use the daemon's activity state: saved thoughts or unfinished tool records
cannot make an idle agent appear busy. Their working phase remains **Working**
rather than guessing the current thinking/tool phase from historical entries. Codex
logs can supply turn clocks and completed counts; Claude's recorded duration
does not establish a successful outcome or a live clock.

Tool cards start folded, showing the title, state and a bounded file/location
summary. Click the chevron or header to reveal the output, code, images and copy
controls; **Collapse** hides those details again. Hidden details are excluded
from text selection. Explicit command/code arguments and fenced source receive syntax
highlighting where available. Known before/after text and recognized patches
show unified changes with red removal rows, green addition rows, and unchanged
context. The original `+`/`-` markers remain readable without color. Syntax
grammars follow supplied filenames, parsing old and new code independently.
File totals count those generated changes; unsupported or oversized payloads
retain their source without estimated totals.
Unknown previous contents are shown as written content without invented
deletions or new-file claims. **Copy diff** retains the displayed diff source;
**Copy code** retains decoded code and **Copy original** retains the original
payload. Malformed or unsupported edit formats stay readable as raw source.
Additional retained output remains available through its copy button.
Tool status labels use the same semantic colors as the activity strip.

The shared ACP reader renders message chunks and in-progress tool updates from
any conforming agent, including custom adapters. It does not choose streaming
behavior by provider name. Agent interfaces differ in what they report:

| Connection | Message text | Tool activity |
| --- | --- | --- |
| Direct Codex | Streams text and thinking chunks | Streams command output deltas into the running card |
| Direct Pi | Streams text and thinking chunks | Displays reported partial tool results as replacement snapshots; final output is authoritative |
| Direct Claude | Streams text and thinking chunks | Displays reported elapsed time and background-task progress; command output arrives with the tool result |
| Other/custom ACP adapters | Displays supplied message chunks | Displays supplied in-progress status/content updates; output cadence belongs to the adapter |
| Existing Codex/Claude terminal sessions | Refreshes saved log records | Projects recorded tool arguments, results and status; cannot show unsaved tokens |

The direct bridges share bounded streaming buffers. The first changed update is
shown immediately; later small updates share a fixed 50ms flush deadline.
Idle bridges do not poll. Expand a tool card to read its output; incoming updates
respect the expanded or folded choice. A completed result replaces queued
partial output, including an empty result, so stale text cannot reappear.
Oversized output is bounded and includes a visible truncation marker.

Claude's elapsed time and task summaries are reported progress, not a fabricated
stdout stream. Verified background tasks have their own cards and can keep
updating after the originating invocation returns. Task output-file paths are
never fetched. Generic ACP support likewise cannot produce data an adapter does
not send; the table describes the bridge contracts, not a guarantee that every
provider exposes incremental command output.

Any additional agent can use the standard ACP path, for example
`herdr-acp -- your-agent acp`. Arbitrary existing terminal programs need a
supported structured adapter or observer; the viewer does not guess an event
protocol by scraping terminal text. Tests with conforming fixtures verify the
shared contract, rather than claiming that every live provider was exercised.

Existing Codex and Claude terminal sessions refresh from their saved logs.
Known tools get descriptions from their structured arguments: commands, file
paths, search patterns, and agent task names replace opaque function names.
Expand a card to inspect its retained input and output. Unknown tools keep their
original names. Descriptions are deterministic; the viewer does not run tool
input or ask a model to summarize it. Arbitrary JavaScript remains a **Tool
script** card, with its source available when expanded; nested actions are not
guessed from code strings.

A returned tool invocation does not mean its background work finished. Explicit
command or script session IDs and completion results can update the matching
card; a spawned agent's eventual outcome is not inferred from a successful
spawn or wait call. Missing execution evidence stays pending, and reported
errors are shown as failures.

The log viewer cannot show tokens before the agent saves them. Observing an
existing terminal session does not resume it or silently take over its approval
requests. A new chat started through `herdr-acp` uses the agent's structured
streaming interface directly.

## Questions and interactive cards

Native chats support permission buttons and the single-choice questions exposed
by their agent bridge. Multi-question, multi-select and free-text prompts are
not universally supported. Attached CLI sessions answer live questions in their
original terminal input; the historical question cards below are read-only.

A closed `herdr-ui` fence can render native cards, metrics, choices, toggles,
sliders, inputs, tables, bar charts and prompt actions. See the
[catalog](../crates/herdr-gpui/src/chat/widgets/README.md) and
[example](../crates/herdr-gpui/src/chat/widgets/example.json). Editing a control
only changes local state. An explicit action with `mode: "draft"` adds the
selected values to the composer for review; `mode: "send"` uses the normal
prompt flow. Rendering or editing a native card cannot run code, submit a
prompt or answer a permission request. Existing CLI sessions and read-only mirrors cannot submit widget actions.

The catalog is application-specific, not an A2UI implementation or MCP Apps
host. Embedded HTML pages use the platform webview; see the
[Linux browser plan](linux-browser-plan.md). Rich chat does not provide hosted
ChatGPT services such as account history, voice/video chat or hosted tools.

## Existing CLI sessions

Discovery checks Herdr's session metadata and the foreground process identity.
The conversation appears in the same pane, with the original CLI editor and
status area below it. Keyboard input, IME, image paste, drafts, slash commands
and approvals remain owned by that CLI. Attaching does not inject or submit
text. Blocked questions and approvals automatically expand the original agent
input. Choose **Compact input** to reclaim the conversation area; the editor
still grows enough to keep its cursor visible. Automatic sizing returns to
compact after the answer; an explicit expanded choice stays expanded.
Drag the horizontal divider above **Agent input · same session** up or down
to choose its height; double-click the divider to restore automatic sizing.
The choice belongs to this pane and is bounded when the window shrinks. The
original editor's cursor remains visible even if it needs more rows than chosen.
Unusual full-screen dialogs may require the terminal view.
Leaving the agent returns to the ordinary terminal.

If another window owns a larger terminal grid, **Activate to fit agent input**
and **Fit input** let the normal window geometry update restore the editor and
status rows. An inactive window does not repeatedly resize another window's
terminal. Goal status stays in the original CLI interface; it is not inferred
from conversation text.

Observation requires a Linux agent host, its Herdr CLI metadata API, and a
matching `herdr-acp` helper. Local helpers are paired with the GUI; SSH observers
run on the agent's host through the existing encrypted connection. Remote helper
installation is explicit. Observation creates no listening port or persistent
service. Closing the GUI stops its observer children and leaves agents running.
Unsupported hosts or unverifiable identities retain the original terminal.
Explicit historical mirrors remain available through `chat.mirrors`, but are
ignored while `render_existing` is enabled so they cannot mask a changed or lost
live session.

Only a bounded log tail is projected: up to 8 MiB read, with at most 2,000
entries, 2 MiB of text and 8 MiB of image data retained. Older history remains
with the original agent. Model, thinking level and context usage appear only
when reported in the retained tail; missing values stay unknown. Activity comes
from Herdr's live snapshot. Historical permissions never become actionable
buttons. `herdr-gpui chat status --json` reports the input mode and session ID.

System activity becomes a compact notice only when transcript metadata verifies
its origin. Notice details remain inert, expandable and copyable; oversized
source includes a truncation marker. Ordinary user text that resembles a notice
stays literal.

Verified Codex question replies matched to their original questions appear as
read-only **Question answered** cards. Questions and answers support Markdown,
code and equations. **Copy answers** copies readable text; **Original record**
keeps the source separately copyable. These cards never replay approvals or
submit answers. Verified retained answers survive compaction without duplicating
unchanged answers. Unmatched or malformed wrappers stay literal, and compacted
replacement history is not replayed.

Embedded raster attachments use the transcript's image data and retain their
captions. The observer never fetches image paths or URLs from message text.
Local Markdown file links open only on an explicit click. Remote file paths stay
readable without opening a same-named local file.

## Implementation and bounds

Rich parsing lives in [`chat/rich`](../crates/herdr-gpui/src/chat/rich.rs), with
native layout in [`chat/view/rich.rs`](../crates/herdr-gpui/src/chat/view/rich.rs).
Math rasterization and syntax highlighting use the bounded visual worker queue;
discarded streamed revisions retain their worker slots until completion.
The lazy list prioritizes rich visuals in the rendered viewport, keeping at most
128 admitted formulas/code blocks per chat. Visible chat panes share at most two
running visual jobs per window; hidden, raw-terminal and covered panes start no
new jobs. Already running work keeps its slot until completion, even if its pane
is removed. Offscreen rich
entries can be evicted and reconstructed from retained message source when you
scroll back. Visible formulas can reclaim offscreen texture space within the
64 MiB math cache; a budget refusal retries only once space is available.
Unsupported formulas, oversized inputs and a viewport exceeding these limits
retain their readable source fallback.

Virtualization is at message-entry level, with a small overdraw margin. A visible
long message still lays out its blocks together. Initial retained-history parsing
is bounded and eager; unchanged bodies and text projections are reused. This is
separate from background syntax highlighting and formula rasterization.

- Message preparation is limited to 256 KiB and 2,000 lines. Rich fragments
  exceeding 4,096 words or 16,384 parser events use the bounded source renderer.
  Message copying retains the complete retained message.
- Code previews show at most 512 lines and 4,000 characters per line, with a
  visible truncation notice. **Copy code** retains the parsed block within the
  message preparation limit. Each rich fragment highlights at most 32 blocks of
  32 KiB each; larger blocks remain readable code.
- Formula input, nesting, dimensions and pixel allocations are bounded.
  Unsupported or oversized formulas remain readable source.
- Widget JSON is validated before presentation. Unknown types or keys, invalid
  bounds, unfinished fences and oversized cards remain source. Callbacks are
  bound to the pane, connection epoch, entry and exact current document.
- Observers bound JSONL tail reads and retained transcript memory, avoid loading
  transcript-linked files, and never replace the agent or overwrite its lifecycle
  metadata.

## Developer verification

Run the repository's normal `just ci` checks. Focused tests cover rich parsing,
render bounds, widget events and focus, stale callbacks, transcript parsing and
read-only protocol behavior. Use a temporary `HOME` and XDG directories when
configuration tests must be independent of installed desktop themes. Preserve
`CARGO_HOME` and `RUSTUP_HOME` when changing `HOME` to reuse the toolchain and
cache; system-wide packages remain visible.

For native visual checks on Linux, install Xvfb and ImageMagick, then run:

```sh
cargo build --locked -p herdr-gpui --features integration-test
python3 scripts/check-rich-chat-native.py
```

The [capture script](../scripts/check-rich-chat-native.py) uses its own X server
and temporary XDG directories, runs cases sequentially, and closes only its own
processes. It requires no daemon or browser. PNGs and logs go to
`target/rich-chat-native/`. The default cases cover math, code, diagrams, widgets,
conversation layouts, original-session input, attachments and Q&A across light,
dark, wide and narrow variants. Review the images for typography, clipping and
contrast. Use `--case`, `--layout`, `--theme`, `--size` and `--output` to select captures;
`--xvfb` and `--vulkan-library` select alternate test tools.

The selection probe additionally requires `libX11` and `libXtst`. It sends
native mouse/key events only to its private Xvfb fixture, checks exact selected
and clipboard text, and records screenshots plus `selection.json`:

```sh
python3 scripts/check-rich-chat-native.py --case selection --theme dark --size narrow
```

The tool disclosure probe clicks the actual chevron to expand, collapse and
expand a synthetic code edit, checking visible source and capturing each state.
It neither executes the displayed code nor connects to an agent:

```sh
python3 scripts/check-rich-chat-native.py --case tool-disclosure --theme dark --size narrow
python3 scripts/check-rich-chat-native.py --case tools-open --theme light --size narrow
python3 scripts/check-rich-chat-native.py --case loading --theme dark --size narrow
```

For example, `--layout imessage --case user --theme light --size narrow`
checks user-message contrast and wrapping in a narrow light-theme bubble.

To move a debug fixture binary to another machine, embed the RaTeX fonts:

```sh
cargo build --locked -p herdr-gpui --features integration-test \
  --config 'profile.dev.package."ratex-katex-fonts".debug-assertions=false' \
  --config 'profile.dev.package."rust-embed".debug-assertions=false'
```

Release builds embed these fonts automatically. Software-rendered captures
verify appearance, not hardware GPU performance. Native input, focus and IME
checks complement screenshots and headless geometry tests.

## Dependencies

TeX uses the MIT-licensed RaTeX engine crates pinned to 0.1.15. Their bundled
KaTeX fonts use the SIL Open Font License 1.1; release notices include the font
license and upstream provenance separately from the engine's MIT license.
Markdown and syntax highlighting reuse the project's pulldown-cmark and Syntect
dependencies.
The implementation extends the existing native ACP chat renderer.
