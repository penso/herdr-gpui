# macOS dictation clipboard timing (#761)

## Status

**Unresolved.** Investigation against `d4a488314ebb5b9c60655e1273410d7e56a7e38a`,
which includes merged PRs [#758] and [#818]. The issue has no comments, and a
search for open PRs mentioning `761`, `dictation`, or `pasteboard` found none
on 2026-10-26. This checkout is on Linux; native macOS acquisition timing and
the original crash have not been reproduced here. No production clipboard
behavior is changed by this investigation.

## What the code establishes

1. `window/input.rs::paste` calls `paste_or_hold`. On macOS,
   `window/pending_input.rs::replay_paste` calls `paste_native_clipboard`.
2. `window/images.rs::start_image_preparation` reserves input FIFO order,
   then invokes the acquisition closure on the background executor.
   `window/clipboard/macos.rs::read` launches a new copy of the executable.
   Only that helper calls `read_pasteboard` on its main thread.
3. `window/clipboard.rs::read_pasteboard` samples `changeCount` immediately
   before and after acquisition. Restoration **before** the first sample is
   invisible to this check: restored text is accepted, or an empty clipboard
   returns no item. Restoration during acquisition can produce `Changed`.
4. Current main also holds paste *intent*, not clipboard contents, during a
   projection gap or behind an earlier preparation. Those pastes acquire on
   replay. Fixing only helper startup would leave this additional delay.
5. #818 removes sleeps between chunks while draining large images. It does
   not move acquisition earlier. #758 explicitly leaves the timing issue open;
   its reported helper timings are measurements from that PR, not this host.

These establish the opportunity for the reported race, not its frequency with
Handy or the latency of any individual native operation.

## Deterministic characterization

`window/clipboard/pasteboard_tests/timing.rs` uses a uniquely named private
AppKit pasteboard. It writes a Unicode transcript, materializes a control copy,
restores old text (then, separately, an empty clipboard), and only then invokes
the production native reader. No sleep or scheduling assumption is needed.
It checks that the reader accepts the restored state without `Changed`, while
the already materialized control retains the transcript.

This is a characterization of the low-level reader's revision boundary, **not
a passing regression test for a fix to #761**. It does not exercise helper
startup, native key delivery, or a delayed-data provider. Run on macOS:

```sh
flock /tmp/opencode/herdr-issue-batch-build.lock env CARGO_BUILD_JOBS=2 \
  cargo test --locked -p herdr-gpui --bin herdr-gpui \
  restoration_before_acquisition_is_not_a_mid_read_revision_change
```

`flock` is the shared build-serialization tool used for this issue batch; it
must be installed to use that command on macOS. The test does not touch the
general clipboard or a daemon.

The headless native-paste tests do not prove acquisition timing:
`paste_native_clipboard` uses `cx.read_from_clipboard()` synchronously under
`cfg(test)`, moving an already owned item into the worker. Those tests establish
FIFO/target behavior, but bypass the late native acquisition under investigation.

## Why the suggested change is not implemented

Reading files/text synchronously would reintroduce AppKit clipboard acquisition
into the GUI process. #758 isolated this path after a native fault during a
dictation paste; it did not establish that main-thread text reads cannot fault.
`TEXT_LIMIT` bounds the additional Rust allocation **after** AppKit returns,
not the provider's execution time or AppKit's allocation. Apple's
[pasteboard concepts][apple] document on-demand representations, communication
with the pasteboard server, and communication exceptions. `stringForType` is
not evidence of a bounded, failure-isolated read.

There is no keystroke-time snapshot passed to the existing helper. The reviewed
APIs/documentation do not establish a way to retain such a snapshot without
native acquisition in the GUI. A warm helper or a dedicated worker could reduce
delay but would not, by itself, guarantee the contents from key handling.
Capturing a revision and rejecting a late read could avoid pasting old text but
would still lose the dictated text. None of these is a demonstrated complete
fix satisfying the requested constraints.

## Evidence needed to finish

- On macOS, establish and test an acquisition design that preserves crash
  isolation and keeps native provider waits off the UI thread. Include a
  delayed/failing text provider, not just eagerly published strings.
- Measure key handling, acquisition start/end, and completion separately,
  without logging clipboard contents. Cover cold startup and executor/process
  pressure against a synthetic restoration deadline of roughly 150 ms.
- Exercise normal ready input, a projection gap, and an earlier pending
  preparation. Verify exact transcript bytes followed by typing and Enter;
  cancellation/target changes must still refuse stale deliveries.
- Retain Finder files-before-text priority, local and remote image handling,
  PNG/TIFF (including #818's large TIFF), bounded output, and kill/reap behavior.
- Repeat actual Handy dictation with its default restoration behavior. A
  receipt-based paste mode is a workaround, not validation of an app fix.

This is an in-client design/native-validation blocker, not evidence that the
external Herdr daemon needs changing. No running app, clipboard, configuration,
or daemon was changed during this investigation.

## Local verification (Linux)

- `just format`, `just check-file-size`, `just check-sorted`, and
  `git diff --check` passed.
- `flock /tmp/opencode/herdr-issue-batch-build.lock env CARGO_BUILD_JOBS=2 cargo test --locked -p herdr-gpui --bin herdr-gpui clipboard`
  passed: 28 tests, zero failures. This excludes the new macOS-only test.
- `just ci`, under the same lock and job limit, passed format, file-size,
  sorting, and both workspace Clippy configurations (all features and no
  default features). It failed in the default workspace tests at
  `herdr-automate::herdr::tests::pane_details_parse`: launching its temporary
  `fake-herdr` returned OS error 26, `ExecutableFileBusy` / `Text file busy`.
  That crate is unchanged. The remaining default tests, all-feature tests,
  and later Python gates did not complete; the CI gate did not pass.
- `just lint-windows`, under the same lock and job limit, failed building
  `ring` because `x86_64-w64-mingw32-gcc` is not installed.
- `just lint-linux`, under the same lock and job limit, stopped because Docker
  is not installed. The host Linux Clippy runs above did pass.
- No macOS compilation, private-pasteboard test, or native GUI QA was run.

[#758]: https://github.com/penso/herdr-gpui/pull/758
[#818]: https://github.com/penso/herdr-gpui/pull/818
[apple]: https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/PasteboardGuide106/Articles/pbConcepts.html
