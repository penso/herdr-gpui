# Native Terminal Performance

## Main Merge Integration

The merge of `5300d39` retains upstream's separate decoration pass, including
spaces and wide-character continuation cells. Current deterministic budgets are
1,921 decoration/cursor quads per dense paint and 1,922 with the popup. Cached
popup painting now uses the same `popup_origin` helper as input and IME geometry,
and retained acknowledgements use `ConnectionBridge`'s authoritative inbox.

The measurements and 1,677/1,678 decoration counts below describe `ca2929b`
before this merge, not the updated rendering workload. Re-run `just compare-perf`
on an active desktop to measure the merged code; both comparison modes now include
the upstream decoration fix.

## Retained Scenes And Verified Runs (2026-09-20)

This iteration changes only the GUI and its tests/tooling. The protocol, daemon,
client event ordering, socket scheduling, and 16 ms update polling are unchanged.

- Terminal pixels live in a persistent `TerminalView`, rendered through GPUI's
  `AnyView::cached`. A new surface allocation (including cursor/popup changes) or
  removal invalidates it. GPUI also checks bounds, content mask and text style.
- The uncached sibling canvas still reports geometry, registers input/IME and
  acknowledges the coherent presented snapshot/surface. Scene reuse must not
  suppress activity acknowledgements or resize handling.
- Contiguous, undecorated, non-space ASCII cells of one resolved style can share
  a paint call. Native glyph IDs, fonts, metrics, widths, indexes and positions
  must exactly match independent cell shaping. Actual-origin floating-point
  accumulation is checked before painting; there is no position tolerance.
- Other text, decorations, emoji and wide/combining cells retain the existing
  per-cell path. Background and cursor ordering is unchanged. Row-local staging
  prevents a single eligible pair from cloning shapes for the entire grid.
- Cell shaping remains bounded at 4,096 entries. A separate 512-entry run cache
  stores accepted layouts and rejected candidates, with at most 256 ASCII cells
  per key. Both caches invalidate on font changes. Overflow uses the fallback
  path; this is not a byte-precise bound on all native layout storage.
- Successful responses, incoherent surfaces, identical surface Arcs and repeated
  identical errors no longer dirty otherwise unchanged UI state. Snapshot updates,
  including same-revision activity changes, are not suppressed.

### Reproduce And Gate

Run on a quiet, logged-in macOS desktop. The native fixture activates its own
window for the acknowledgement check; do not switch applications during a run.

```sh
# Builds standalone release, then checks both forced and retained sidebar draws.
just test-perf

# Five interleaved reference/candidate pairs, alternating execution order.
just compare-perf

# Preserve raw samples, logs, binary hashes and checkout metadata.
python3 -B scripts/compare-terminal-performance.py --pairs 5 --output comparison.json
```

The comparator requires at least 20% median per-run p95 improvement for hover
and single-cell updates. It rejects >15% p50 or p95 regression for every category
with at least 12 samples per run, including full-screen and mixed-fallback output.
These are dedicated-desktop gates, not universal CI timing guarantees. Failed
runs are not discarded or silently retried. Tune thresholds explicitly if needed.

The default reference uses the **same executable**, disabling retained views and
run batching while retaining the previously shipped glyph/background caches.
It is a previous-algorithm control, not an actual historical executable.
`--baseline-binary` accepts another binary with the same extended JSON harness.
The script does not build or infer an executable's revision from the checkout.

Integration-test-only switches (presence enables each, even `=0`):

| Variable | Effect |
| --- | --- |
| `HERDR_PERF_RETAINED` | Do not force refresh during warm sidebar samples |
| `HERDR_PERF_NO_RETAIN` | Disable the terminal view cache for comparison |
| `HERDR_PERF_NO_BATCH` | Use the previous per-cell glyph painter |
| `HERDR_PERF_UNCACHED` | Older reference: disable cell caching, background spans and run batching |
| `HERDR_PERF_SAMPLES` | Emit raw JSON sample arrays and a final JSON PASS record |
| `HERDR_PERF_P95_MS` | Absolute sidebar p95 gate; `inf` disables only this timing gate |

`HERDR_PERF_RETAINED` selects the benchmark, not product behavior: ordinary builds
always enable retention and eligible run batching. The subprocess regression
test cleans inherited switches and checks an explicit PASS marker for both modes.

### Results

Apple M4 Max, macOS 27.0 (26A428), GPUI 0.2.2, Rust 1.96.1, Menlo 14 px. These
are standalone optimized builds, not Cargo test-support builds. The final
comparison used five process pairs. Each steady sidebar category has 60 samples
per process; each update category has 12. Numbers are medians of per-process
nearest-rank percentiles, not percentiles of pooled correlated frames.

| Workload | Previous Algorithm p50 / p95 (ms) | New p50 / p95 (ms) | p95 Change |
| --- | --- | --- | --- |
| Warm hover | 12.367 / 12.670 | 5.646 / 5.757 | -54.6% |
| Warm sidebar scroll | 12.380 / 13.575 | 5.599 / 8.537 | -37.1% |
| Cursor updates | 11.660 / 11.934 | 6.745 / 7.032 | -41.1% |
| Single-cell edits | 11.769 / 12.140 | 6.793 / 7.086 | -41.6% |
| Erasure | 11.901 / 12.231 | 6.558 / 6.785 | -44.5% |
| Streaming row scroll | 11.944 / 12.445 | 6.757 / 7.222 | -42.0% |
| Full-screen alternating colors | 16.873 / 23.359 | 16.906 / 23.503 | +0.6% |
| Decorated output with one ASCII pair | 23.785 / 24.404 | 23.798 / 24.352 | -0.2% |

All comparative gates passed. A separate build of the actual starting revision
`449d45b` measured forced hover/scroll p95 of 13.729/15.737 ms; the final candidate
measured 7.493/10.644 ms using the same original forced-sidebar fixture. Those are
individual historical runs, not paired medians. `449d45b`, rather than `7a26c2b`,
was HEAD when implementation began. Historical 2026-09-19 results remain below.

The dense fixture still submits 6,981 glyphs, 50 background spans and 1,677
decoration/cursor quads per forced paint. It now uses 686 verified multi-cell
runs, with zero warm cell, metric **or run** shaping. Retained sidebar draws make
zero terminal painter calls. GPUI still replays primitives and sorts its scene;
retention is not a retained GPU texture or a zero-cost draw.

### Coverage And Limits

Terminal update inputs are prepared outside timing. Timed work includes setting
the surface, invalidation, native scene construction and arena clearing, but not
daemon/transport work or generating/cloning test inputs. Updates never call
`refresh()` before their measured draw: doing so would conceal missed invalidation.
Untimed forced draws compare work counts, and native caches are checked against
fresh independent glyph layouts. Popup show/hide, clear/restore, root-only status
changes, warm native resize and `Working -> Done -> Idle` acknowledgement with an
unchanged retained surface are explicitly checked.

Unit tests cover no-op state updates, same-revision snapshots, cache capacity and
font reset, batching eligibility, exact layout mismatch rejection and row-local
fallback. Native subprocess tests run both modes with a timeout. No fixture
connects to a daemon. Existing live-daemon input tests remain opt-in and were not
run for this iteration.

These are CPU benchmarks, **not** presentation/frame-deadline or pixel-diff tests.
The window remains clipped on this display, as in the original workload. The
12-sample update p95 is effectively the maximum; do not interpret it as a precise
tail-latency estimate. Glyph/layout and count checks do not independently prove
framebuffer equivalence. Device-scale transitions and sustained socket throughput
are not exercised. Full-screen decorated/alternating-color output remains slower
than sparse transcript updates; row damage tracking, custom glyph submission,
decoration reordering, cache eviction and event-driven polling are deliberately
not introduced without further profiling and correctness coverage.

## Original Cached-Cell Baseline (2026-09-19)

The following results and implementation notes describe the previous iteration,
before retained views and verified ASCII-run batching. The forced-refresh fixture
and old uncached reference remain available for continuity; current benchmark
runs also execute the update and acknowledgement checks described above.

## Recipes

Run from the repository root on a logged-in macOS desktop. These fixtures do not
connect to, start, stop, or send input to a Herdr daemon.

```sh
cargo build --release -p herdr-gpui --features integration-test
target/release/herdr-gpui --performance-test

# Reference algorithm: per-cell shaping and per-cell backgrounds, same harness.
HERDR_PERF_UNCACHED=1 target/release/herdr-gpui --performance-test

# Debug timings, representative of an unoptimized development build.
cargo build -p herdr-gpui --features integration-test
target/debug/herdr-gpui --performance-test
HERDR_PERF_UNCACHED=1 HERDR_PERF_P95_MS=5000 target/debug/herdr-gpui --performance-test

# Explicit native regression test with subprocess timeout and exit-status checks.
cargo test -p herdr-gpui --features integration-test --test performance_gui -- --ignored --nocapture

# Pure/cache correctness tests and the existing native label-width check.
cargo test -p herdr-gpui --bin herdr-gpui --features integration-test
target/release/herdr-gpui --sidebar-test
cargo clippy -p herdr-gpui --all-targets --features integration-test -- -D warnings
```

`--performance-test` requires `integration-test` and macOS. It is mutually
exclusive with the other fixture modes and all connection options. No benchmark
library or external desktop automation service is needed. The native test is
ignored by default; do not enable it on headless CI.

`HERDR_PERF_P95_MS` sets the maximum warm hover/scroll p95 in milliseconds. Defaults
are intentionally loose: 250 ms in release and 1000 ms in debug. Set a calibrated
budget on a dedicated runner, e.g. `HERDR_PERF_P95_MS=30` for this machine's release
build. `HERDR_PERF_P95_MS=inf` disables only the wall-clock gate, not the deterministic
checks. `HERDR_PERF_UNCACHED` is enabled by presence, including a value of `0`; leave
it unset for regression checks. It deliberately bypasses optimized count budgets
and native cache comparisons to permit measuring the old algorithm.

Failures exit nonzero directly. GPUI/AppKit's normal `cx.quit()` can terminate with
status zero before `main` returns, so this daemon-free driver does not rely on it
for reporting success or failure. Closing the fixture early is also a failure.

## Measurement Contract

The fixture contains 160 x 50 cells, 40 workspaces and 40 agents. Transcript rows
include shell commands, Rust, diffs, test output and review text, repeated across
the grid to keep it dense. Styles cover RGB/indexed/default colors, bold, italic,
dim, reverse, hidden, underline and strikethrough. Wide CJK cells have skip cells;
combining graphemes and a visible beam cursor are included.

The adapter creates AppKit mouse-move and pixel-scroll events and invokes the
fixture's `GPUIView` native handlers on the main thread. This traverses GPUI's real
event dispatch and hit testing, not direct sidebar callbacks. It neither posts
global input nor requires Accessibility permission. Mouse coordinates and actual
scroll offsets of **both** lists are asserted. This small macOS adapter is needed
because GPUI 0.2.2's public `Window::dispatch_event` returns an inaccessible private
type. Its unsafe Objective-C bridge is restricted to the opt-in test feature.

Each timed sample includes event construction/delivery plus forced full native
`Window::draw` scene construction and arena clearing. Samples run across event-loop
turns, with an untimed 20 ms delay between them. Text goes through the native text
system, `ShapedLine::paint`, glyph rasterization/atlas lookup, and native scene
construction, not `NoopTextSystem`. Terminal paint errors fail the run.

- `first-content`: one initial dense-content draw, including fixture construction
  and the resize request. This is not application-launch latency.
- `cache-cold`: ten draws with the application terminal cache cleared before each
  sample. GPUI/CoreText and the glyph atlas may already be warm.
- Frames 11-20 are warmup, excluded from percentiles.
- `warm-hover`: 60 samples traversing workspace rows with an unchanged terminal.
- `warm-scroll`: 60 samples scrolling workspace and agent lists, with mouse motion.
- Native cached/fresh layout comparisons and changed-cell/popup checks run outside
  timing. All 327 cached layouts are compared, including native glyph IDs,
  positions, font IDs, metrics and decoration colors.

The recorded time does **not** include the separate `Window::present` scene
submission, GPU completion, vsync or display presentation. It is a CPU
event-to-scene-construction cost, not OS pointer-to-photon latency.
Full refresh is deliberate: it measures the reported expensive whole-scene redraw
even if a future idle-frame scheduler would skip some draws.

Use the standalone `cargo build` recipes for comparable performance numbers.
`cargo test` unifies GPUI's dev-dependency `test-support` feature into the native
executable. Its text backend is still native, but GPUI adds synchronous redraws
after event dispatch. The harness reports `paints` and validates per-paint budgets
so these additional draws cannot hide work. Their time is included, making the
Cargo test slower than the standalone binary. Rebuild after `cargo test` before
benchmarking `target/debug/herdr-gpui` again.

## Results

Measured on Apple M4 Max, macOS 27.0 (26A428), GPUI 0.2.2, Menlo 14 px, on
2026-09-19. The requested 1640 x 1100 window settled at 1640 x 1007 logical pixels
on this display; the first-content draw was 1200 x 781. The full grid is submitted,
but the terminal content mask clips its bottom rows on this display. Timings below
are standalone native builds, one terminal paint per timed event.

A baseline was run **before** modifying the painter: release warm-hover p50/p95/max
49.195/50.196/50.415 ms and warm-scroll 51.179/55.215/55.970 ms. It made 6,981 terminal
shape calls and 8,000 background quads every draw. The final harness adds cold-cache
sampling, correct indexed-color encoding, a cursor, both-list scrolling and more
complete counters. The same-binary reference/optimized comparison below uses that
final workload; the reference preserves the original cell algorithm, though the
single font-width measurement is now cached in both modes.

| Release Phase | Reference p50 / p95 / max (ms) | Optimized p50 / p95 / max (ms) |
| --- | --- | --- |
| First content, n=1 | 33.789 / 33.789 / 33.789 | 23.459 / 23.459 / 23.459 |
| Cache cold, n=10 | 48.966 / 51.711 / 51.711 | 11.804 / 12.988 / 12.988 |
| Warm hover, n=60 | 48.995 / 50.755 / 53.521 | 11.548 / 12.065 / 12.374 |
| Warm scroll, n=60 | 52.514 / 55.985 / 59.184 | 12.579 / 14.432 / 16.687 |

| Debug Phase | Reference p50 / p95 / max (ms) | Optimized p50 / p95 / max (ms) |
| --- | --- | --- |
| First content, n=1 | 191.716 / 191.716 / 191.716 | 96.544 / 96.544 / 96.544 |
| Cache cold, n=10 | 461.866 / 467.543 / 467.543 | 147.089 / 149.076 / 149.076 |
| Warm hover, n=60 | 464.894 / 469.803 / 473.115 | 146.685 / 147.877 / 148.432 |
| Warm scroll, n=60 | 474.810 / 509.026 / 515.519 | 154.524 / 172.107 / 175.671 |

Release p95 improved approximately 76% for hover and 74% for scrolling. Debug is
substantially better but still slow; this change does not make an unoptimized
whole-window paint a 60 Hz workload. Another optimized release run measured hover
p95 12.176 ms and scroll p95 14.259 ms; another had scroll max 20.529 ms. Treat
elapsed times as machine/load-specific, not universal guarantees. The final run
also passed an explicitly tightened `HERDR_PERF_P95_MS=30` gate.

| Terminal Work Per Warm Paint | Reference | Optimized |
| --- | ---: | ---: |
| Cell `shape_line` calls | 6,981 | 0 |
| Font-width `shape_line` calls | 0 | 0 |
| Background quads (`quads`) | 8,000 | 50 |
| Underline/strike/cursor quads (`decorations`) | 1,677 | 1,677 |
| Total terminal quads | 9,677 | 1,727 |
| Glyphs passed through text painting | 6,981 | 6,981 |
| Paint errors | 0 | 0 |

Optimized cold shaping is 327 unique symbol/style combinations, below a 400-call
budget. Every warm sample must have zero cell **and** metric shaping. Per-paint
quad, decoration and glyph counts are exact, independent of timing budgets.

## Implementation And Limits

`terminal_painter::TerminalPainter` is owned per window and retained by its canvas.
`cell_width` caches the Menlo metric; `paint_frame` caches shaped symbols keyed by
resolved foreground and bold/italic flags, with the full base `Font` as cache
configuration. Font size and cell height remain compile-time constants. A changed
base font clears both caches. Backgrounds are coalesced by resolved color within
each row, not across rows, and recomputed from current cells on every paint.

No terminal strings are concatenated: wide glyphs, combining marks and all other
symbols are painted at the original `column * cell_width`, `row * CELL_HEIGHT`
positions. Backgrounds still precede all glyphs; cell decorations and cursors keep
their old ordering/geometry. Popup centering and input handling are unchanged.
The sidebar's fixed label-width workaround is unchanged; its native four-size
regression check passed after this change.

The cache holds at most 4,096 symbol/style entries. Once full, additional entries
are shaped without insertion; existing entries remain valid until a font change
or window destruction. Highly diverse long-lived transcripts can therefore miss
the zero-reshaping property beyond that working set. There is no row cache or
entity-level scene caching, no daemon/protocol changes, and no new handling of
terminal graphics/hyperlinks. Native checks exercise changed foregrounds, centered
popup drawing, beam and underline cursors, and fresh/cached glyph equality; they
are not a screenshot pixel-diff test or exhaustive Unicode/font-fallback suite.
