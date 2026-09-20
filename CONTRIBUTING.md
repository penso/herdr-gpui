# Contributing

Thanks for looking at Herdr GPUI. This file covers the mechanics; the
engineering rules that actually govern changes live in
[AGENTS.md](AGENTS.md) — architecture boundaries, Rust conventions, and
verification expectations. Read it before a non-trivial change. It applies to
human and agent contributors alike.

## Before You Start

- **macOS only.** The client is built on GPUI and Metal. There is no Linux or
  Windows build, and adding one is not a small patch.
- **You need a running Herdr daemon.** This repository is a client of the
  daemon from [herdrdev/herdr](https://github.com/herdrdev/herdr); it never
  installs, starts, or upgrades one for you. Bugs in the daemon, its session
  model, or its socket handling belong upstream, not here.
- **Open an issue first for anything large.** The README's *Next Milestones*
  section is the roadmap. A PR that lands a milestone differently than planned
  is more likely to stall than a short issue discussing it.

## Setup

Install rustup and the Xcode command-line tools. The toolchain is pinned in
`rust-toolchain.toml` and GPUI is pinned to an exact version, so `rustup show`
is enough — do not upgrade either as a side effect of another change.

```sh
just run          # optimized build; use this for anything interactive
just run-debug    # unoptimized, notably slower with a dense terminal
```

## Verifying a Change

Run the workspace gates before opening a PR:

```sh
just format
just ci
```

`just ci` is exactly what CI runs: `cargo fmt --all -- --check`, a
`-D warnings` clippy pass over all targets and features, and the test suite
under both default and all features. CI additionally builds release binaries
for Apple Silicon and Intel.

Some tests are opt-in because they need resources CI does not have:

```sh
just test-live /absolute/path/to/herdr   # needs an explicit daemon binary
just test-gui  /absolute/path/to/herdr   # also needs an active desktop
just test-sidebar                        # native glyph regression
just test-perf                           # release-mode scene budget
```

Never point a live test at your personal daemon — they create and clean up
their own isolated one. For visual changes, check the real native window:
narrow layouts, long labels, focus, and clipping. Headless layout tests do not
prove native glyph or input correctness.

## Pull Requests

- Use conventional commit subjects: `feat`, `fix`, `refactor`, `test`, `docs`,
  `chore`. Explain the problem and the chosen approach in the body for
  non-trivial work.
- `main` requires a pull request and green CI. Review your own full diff
  against the base before marking it ready.
- State the validation you actually ran, and say plainly which native or
  manual checks you could not run. Do not describe a headless test as
  end-to-end verification.
- Keep user-facing docs in step with behavior changes.

## License

By contributing, you agree that your contributions are licensed under
Apache-2.0, matching [LICENSE](LICENSE).
