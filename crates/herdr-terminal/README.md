# herdr-terminal

The terminal emulator behind herdr-core terminals. Herdr 1 sends rendered
cell grids; herdr-core sends each terminal's raw PTY output (its first paint
is VT bytes too), so the client emulates the terminal itself. This crate
wraps Zed's fork of `alacritty_terminal` and turns its grid into the same
`herdr_protocol::FrameData` the app's painter already draws.

It is pure and deterministic: no GPUI, I/O, threads, or clock. The same bytes
give the same frame however they are split across `Emulator::feed` calls.
It never answers terminal queries (herdr-core does), writes to the
clipboard, or opens hyperlinks.

## Use

- `Emulator::new(Size, scrollback_lines)`, then `feed(bytes)` as output
  arrives.
- `take_damage()` returns `Damage::Full(FrameData)` (first take, resize,
  alternate screen, clears, scrolling the view) or `Damage::Rows` with whole
  changed rows as `PaneSurfacePatchRow` plus the cursor. Rows only index
  hyperlinks the last full frame listed; a new link makes the take full.
- `frame()` is the viewport at any time; it does not reset damage.
- `modes()`, `title()`, `take_events()` (one coalesced title change and a
  bell count), `scroll()` and `scroll_metrics()`.
- `encode(&ClientPaneInputEvent)` (or `input::encode` with `Modes`) turns the
  app's semantic input into PTY bytes: xterm keys, the kitty keyboard
  protocol, Win32 input records (DECSET 9001), mouse reports, and bracketed
  paste.
- `sync_pending()` and `end_sync()` let the caller time out a synchronized
  update (DECSET 2026) with its own clock.

Everything is bounded: sizes (`Size` from herdr-backend, with `MAX_SIDE`
and `MAX_CELLS`), history
(`MAX_SCROLLBACK_LINES`, `MAX_SCROLLBACK_CELLS`), OSC strings
(`MAX_OSC_BYTES`, longer ones are dropped whole), and repeats
(`input::MAX_REPEAT`).

## Cell contract

Colors in `CellData::fg` and `bg` keep what the program chose, and the app's
theme resolves them:

| Top byte | Meaning |
| --- | --- |
| `0` | Low byte `1..=16` is palette index `value - 1` (black to bright white); `0` is the default foreground or background |
| `1` | `0x010000XX`: 256-color index `XX` |
| `2` | `0x02RRGGBB`: RGB |

Alacritty's dim colors become the matching normal color with `DIM`.

`CellData::modifier` bits are the crate's `BOLD` (1), `DIM` (1 << 1),
`ITALIC` (1 << 2), `UNDERLINE` (1 << 3, every underline style), `REVERSED`
(1 << 6), `HIDDEN` (1 << 7), and `STRIKETHROUGH` (1 << 8).

A wide character's second cell is `skip: true` with an empty symbol;
combining characters join their base cell's symbol. OSC 8 links are listed
once in `FrameData::hyperlinks` and cells carry their index.

`CursorState::shape` is a DECSCUSR parameter: 0 is the user's default style
(no program chose one), 1/2 block, 3/4 underline, 5/6 bar, odd values blink.
A hidden cursor is `visible: false`; one scrolled out of view is `None`.

Input modifier bits (`input::SHIFT` 1, `CONTROL` 2, `ALT` 4, `SUPER` 8,
`HYPER` 16, `META` 32) are the ones `ClientPaneInputEvent` carries.
