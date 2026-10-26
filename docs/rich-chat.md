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
through **Timeline**, **Bubbles**, **Terminal**, **Document**, **Cards**, and
**iMessage**.
Select a layout directly with:

```sh
herdr-gpui config set chat.layout bubbles
```

In **Settings > General**, turn on **Full-width chat** to use the available
conversation area, or set **Chat width** with the slider or numeric field.
The default is 800 logical pixels; custom widths range from 320 to 4,000 and
shrink to fit narrower windows. The transcript and native composer use the
same width. The equivalent commands are:

```sh
herdr-gpui config set chat.max_width 0     # full width
herdr-gpui config set chat.max_width 1200  # custom width
herdr-gpui config unset chat.max_width    # restore the default
```

The Terminal layout still renders chat. The terminal button and
`herdr-gpui chat hide` show the agent's actual terminal;
`herdr-gpui chat show` restores the chat view. These commands change presentation
without starting or stopping the agent. See the [CLI guide](cli.md#existing-agent-rendering).

## Messages and controls

Write normal Markdown and TeX in user and agent messages. Rich chat supports
headings, lists, links, aligned tables with horizontal scrolling, Mermaid,
attached images, and explicitly loaded remote pictures. Conversation prose uses
the UI font; inline code and code fences use the terminal font. Code blocks
preserve blank lines, show tabs at four-column stops and scroll without wrapping.
**Copy code** preserves the parsed source's tabs, indentation and Unicode.

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

Selected code preserves original tabs; tables copy as tab-separated cells and
newline-separated rows. Formulas are atomic selections and copy their original
TeX. Collapsed card details and retained hidden source are omitted until opened;
copy buttons remain available for complete retained code or original records.
Image pixels, embedded webviews and widget controls are separate surfaces, not
selectable conversation text.

ACP chats retain their native composer, permissions, tools, plans, context
information and file attachments. Tool and thinking cards start collapsed and
expand to show retained text and diffs.

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
