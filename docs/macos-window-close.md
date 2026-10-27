# macOS window-close crash (#290)

Investigation as of 2026-10-26, against Herdr UI `d4a48831`.
**Unresolved in this checkout.** The dependency still contains the faulty
teardown. The upstream fix and its tests have been reviewed as source, but
neither the crash nor a fixed Herdr build has been run on macOS here.

## Evidence and root cause

[Issue #290](https://github.com/penso/herdr-gpui/issues/290) and all four comments
report a window-close crash on Intel and Apple Silicon Touch Bar Macs. The
Settings crash reaches AppKit's `_NSTouchBarFinderObservation invalidate`,
which tries to remove a `nextResponder` key-value observer that is no longer
registered. Main-window close has the same reported symptom; its crash stack
has not been independently compared. Cmd-Q reportedly exits normally.

The issue's AccessKit explanation was originally a source-only hypothesis.
There is now independent upstream evidence:

- [AccessKit #823](https://github.com/AccessKit/accesskit/issues/823) traces the
  view's runtime class under a debugger: the adapter installs its subclass,
  KVO subclasses that class, and dropping the adapter forcibly restores the
  original class while the observer is still registered. Removing that
  observer then raises `NSRangeException`. Its small reproducer installs a
  `nextResponder` observer directly, without requiring Touch Bar hardware.
- [AccessKit #835](https://github.com/AccessKit/accesskit/pull/835), merged as
  `451885c1850ec1800cea1d0660bf8ba62e0f410d` on October 23, fixes this defect and
  adds deterministic tests. Testers report success with Godot on a macOS VM
  with a Touch Bar simulator and with Skribisto on a user's Touch Bar Mac.
- [accesskit_macos 0.27.2](https://github.com/AccessKit/accesskit/releases/tag/accesskit_macos-v0.27.2),
  published October 25, contains that fix.

This establishes a dependency defect matching the reported exception, not a
local reproduction proving that it explains every Herdr close failure.

### What this checkout actually runs

| Component | Resolved version | Relevant source |
| --- | --- | --- |
| `gpui-pre` / `gpui-pre-macos` | `0.3.6` | `src/window.rs`: `Window::new`, `MacWindow::a11y_init`, `Drop for MacWindow` |
| `accesskit_macos` | `0.26.3` | `src/subclass.rs`: `SubclassingAdapter::for_window`, `Drop for SubclassingAdapter` |
| `accesskit` | `0.24.1` | GPUI's accessibility tree and handler types |

The current lockfile has **0.26.3**, not the 0.26.0 cited in the old issue
comment. Inspection of the published 0.26.3 source still finds the unconditional
`object_setClass(view, prev_class)` in `SubclassingAdapter::drop`. The downloaded
0.26.3 and GPUI 0.3.6 archives' SHA-256 hashes match `Cargo.lock`.

Herdr uses `gpui_platform::application()` in `crates/herdr-gpui/src/app.rs`.
GPUI installs `SubclassingAdapter::for_window` on the native content view.
`MacWindow::drop` explicitly drops that adapter **before** scheduling the
native window's `close()` and `autorelease()`. These are the same dependency
operations implicated by the upstream reproducer.

The application close paths converge on that backend:

- `crates/herdr-gpui/src/settings_window.rs`: `on_window_should_close` calls
  `should_close`; pending edits finish before `finish_close` removes the
  window. Keyboard close goes through `close` and the same save policy.
- `crates/herdr-gpui/src/titlebar.rs` and `window/commands.rs`: main-window
  close calls `window.remove_window()`.
- `crates/herdr-gpui/src/app.rs`: Quit calls `cx.quit()`; closing the last
  window also requests quit. A hidden window would still count as open.

The related merged Herdr PRs [#323](https://github.com/penso/herdr-gpui/pull/323)
(Settings save/reload) and [#516](https://github.com/penso/herdr-gpui/pull/516)
(Close Window command) do not repair this dependency teardown. Searches of
open and merged Herdr PRs found no fix for this signature.

## Adoption blocker

GPUI 0.3.6 requires `accesskit_macos = "0.26.0"` (the compatible 0.26 series)
and `accesskit = "0.24.0"`. The fixed macOS adapter 0.27.2 requires
`accesskit = "0.25.2"` and `accesskit_consumer = "0.39.2"`. It is not a
compatible lockfile-only update: GPUI passes AccessKit tree and handler types
directly across this boundary. Adding another version as a direct application
dependency would leave GPUI using the faulty one.

No fixed 0.26-series release was published at the time of investigation. The
smallest compatible dependency change would be a backport of AccessKit #835
to that series, with its tests, followed by a targeted lockfile update.
[Zed #65186](https://github.com/zed-industries/zed/pull/65186) instead replaces
GPUI's subclassing adapter with the regular adapter on `GPUIView`. That PR
is still open and is not part of the pinned GPUI release.

A backport also needs a native startup check: the fixed adapter rejects views
that were already KVO-observed before installation. GPUI 0.3.6's `Window::new`
calls `platform.open_window` before `a11y_init`, and the macOS constructor can
show/focus the window before returning. Source alone does not establish whether
AppKit has installed an observer at that point on an affected Mac.

An application-level hide/reuse workaround, as in
[navop #268](https://github.com/feigeCode/navop/pull/268), would change close,
cleanup, window counting, and quit semantics. Simply retaining the adapter
also restores the native view retain cycle documented in GPUI's drop code.
Neither substitutes for fixing the observer lifetime. Accessibility must
remain enabled, and windows and their resources must actually close.

## Regression tests and native validation

The deterministic upstream suite is
[`adapters/macos/tests/integration/subclassing_adapter.rs`](https://github.com/AccessKit/accesskit/blob/451885c1850ec1800cea1d0660bf8ba62e0f410d/adapters/macos/tests/integration/subclassing_adapter.rs).
It runs on the main thread, as required by AppKit. In a separate checkout of
that AccessKit revision, on macOS:

```sh
cargo test --locked -p accesskit_macos --test integration
```

In particular, `observers_can_be_removed_after_dropping` installs a real KVO
observer, drops the adapter, checks that its handlers were freed and KVO's
class remains, queries accessibility through the fallback methods, then
removes the observer. The other tests cover reattaching an adapter to the
observed view, ordinary unobserved teardown, accessibility activation, and
rejecting a view observed before adapter installation. Carry these tests with
any compatible backport; a test of a Rust-only close-state model cannot catch
the Objective-C failure.

Herdr already has headless close/save coverage, useful when adopting a fix:

```sh
cargo test --locked -p herdr-gpui settings_window::tests::window_lifecycle
cargo test --locked -p herdr-gpui settings_window::tests::quit_saves
cargo test --locked -p herdr-gpui window::tests::close_window
```

These use GPUI's test platform, not AppKit, and cannot establish that #290 is
fixed. The upstream tests above have not been run during this Linux investigation.

Before marking the issue fixed, use a separate macOS test instance with private
HOME/XDG paths and an explicit isolated daemon socket (the existing test
sandbox provides that isolation). Record the tested commit and resolved
dependency versions, hardware, macOS version, and whether Touch Bar services
are active. Run:

- [ ] The deterministic upstream suite against the compatible backport.
- [ ] Open Settings, close with the red button, reopen, and repeat; cover
  closing without edits and closing while preferences are saving.
- [ ] Open two main windows; close either with its red button and with
  **Window > Close Window**. The other window remains usable.
- [ ] Close the final window and separately use Cmd-Q with Settings open;
  both exit normally, preserve pending preferences, and leave the isolated
  daemon and its terminals running.
- [ ] Repeat with VoiceOver active at launch and enabled after launch; verify
  Settings navigation, terminal text/selection, and focus after reopening.
- [ ] Check repeated open/close cycles for retained native windows/resources,
  including with blur enabled and disabled.
- [ ] Check both Intel and Apple Silicon affected machines. Capture separate
  Settings and main-window reports if either still fails.

Inspect macOS's `~/Library/Logs/DiagnosticReports` and the app's unified log
after a failed run. This is a foreign Objective-C exception, so the absence of
a Herdr Rust panic report does not establish a clean exit. Preserve the
exception reason and relevant frames; redact user/device identifiers before
sharing reports.
