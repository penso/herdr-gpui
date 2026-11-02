---
name: release
description: Publish a herdr-gpui release. Use when the user asks to cut, push, ship, or publish a release, or to approve or verify one that is already running. Covers `just release`, the protected environments the run stops at, and the local verification step that is not part of CI.
---

# Releasing herdr-gpui

The owner has given standing authorization to approve the release run's protected
environments when they ask for a release. Do not wait to be asked again per run.
Everything else on this page still applies: the preconditions are hard gates.
Rerun a job that fails on a transient outside error, such as Apple's stapler
reporting "The network connection was lost"; stop for anything that needs a fix.

Releases go out to real users through the in-app updater and Homebrew. Never
dispatch one that was not asked for.

## Choose the version? No.

You do not pick a version and you cannot. The workflow derives
`vYYYYMMDD.COUNTER` from today's UTC date and the tags already published. A
second release on the same day becomes `.2`, then `.3`. Do not invent a tag, do
not pass one, and do not create a tag locally.

## Dispatch

```sh
just release          # -> bash scripts/release/dispatch.sh
```

`dispatch.sh` refuses unless every one of these holds. Check them first so a
failure is not a surprise:

| Precondition | Command |
| --- | --- |
| Working tree clean **including untracked files** | `git status --porcelain --untracked-files=all` |
| On `main` | `git symbolic-ref --quiet --short HEAD` |
| `origin` is `penso/herdr-gpui` | `git remote get-url origin` |
| `gh` authenticated as `penso` | `gh api user --jq .login` |
| Local HEAD == `origin/main` on GitHub | compare `git rev-parse HEAD` with `gh api repos/penso/herdr-gpui/git/ref/heads/main --jq .object.sha` |
The release workflow runs no test suites of its own: its `ci-gate` job
requires a successful owner-triggered push or manual CI run on main with every
platform check and Linux optimized build completed successfully. A green run
with skipped checks does not qualify. CI builds macOS release binaries on pull
requests only, so the release's macOS builds get the macOS runners. `dispatch.sh` does not wait for CI: the
release builds while CI runs, and `ci-gate` holds `sign` and `windows-sign`
until CI passes, so a failed or cancelled CI run fails the release at `ci-gate`.
If a CI job fails on a known flake, rerun it (`gh run rerun <id> --failed`)
while `ci-gate` waits. `sign` also waits for the `nix` job, which calls
`nix.yml` to build the flake on both Linux architectures from the release
commit: its one cargo build of all five packages unifies features that CI's
builds keep apart, so it can fail where CI passed. Rerun it the same way on a
flake. For a documentation-only commit whose push CI skipped
the suites, run `gh workflow run ci.yml --repo penso/herdr-gpui --ref main`
before dispatching. Apple's notary and stapler calls retry ten times in
`sign-macos.sh`, so a dropped connection there rarely needs a rerun.

The script never fetches or checks out for you: if HEAD does not match, stop and
tell the owner rather than pulling on their behalf.

Untracked files block a release. If you have added files to the repo (a skill, a
scratch script), they must be committed or ignored before `just release` will run.

Show the owner what is about to ship before dispatching. `just
changelog-unreleased` is the same text users will read on the release page; the
raw log is the fallback when `git-cliff` is not installed locally:

```sh
just changelog-unreleased          # only the commits since the last tag
just changelog                     # the whole generated changelog
just changelog-release 20260920.3 out/   # exactly the two files CI publishes
git log --oneline "$(gh api repos/penso/herdr-gpui/releases/latest --jq .tag_name)"..HEAD
```

`just changelog*` needs `git-cliff` locally (`cargo install --locked --version
2.12.0 git-cliff`); CI installs the same pinned version.

Dispatch is long-running. Run it with `run_in_background` so a slow build does
not hit a foreground timeout.

## Beta, then promote

A beta is a full release published as a GitHub prerelease. Only installs with
`[updates] channel = "beta"` (Settings > General > Install beta releases) are
offered it; the stable channel, `/releases/latest` and Homebrew never see it.

```sh
just release-beta                 # dispatch.sh --beta: same run, prerelease, no homebrew job
just release-promote 20261010.1   # dispatch.sh --promote: promote.yml, flips it to stable
```

Same preconditions as `just release`, and the same rule: never dispatch either
unless the owner asked for it. "Ship a beta" means `release-beta`; "promote" means
`release-promote` with the exact beta version the owner names (list candidates with
`gh release list --repo penso/herdr-gpui --limit 10`). Promotion rebuilds nothing:
it refuses anything that is not a published prerelease newer than the latest
stable, then its `promote` job (environment **`release`**) clears the prerelease
flag and its `homebrew` job (environment **`homebrew`**) updates the tap. Approve
both gates as below. A failed beta is not deleted or edited; it is simply never
promoted, and the next release supersedes it.

Unpromoted betas fold into the next release's notes, so a stable release lists
everything since the previous stable one. `release.yml` and `promote.yml` share the
`manual-release` concurrency group: one waits for the other.

## The run stops for approvals

Jobs run roughly: `audit` (shown as "Release Workflow Security") and `validate`,
then `macos-build` (arm64 + x86_64, both on Apple Silicon), `linux` (aarch64 + x86_64),
`windows` (x86_64 and ARM64 MSVC), `metadata` and `changelog`, then `sign` and
`windows-sign` (Authenticode, one per Windows target), `windows-smoke` (installs
the signed installers), `attest`, `publish`, and finally `homebrew` and `winget`.
Until the `WINDOWS_SIGNING_SUBJECT` repository variable is set, `windows-sign`,
`windows-smoke` and `winget` are skipped and Windows ships unsigned.

`sign`, `windows-sign`, `attest` and `publish` use the **`release`** environment;
`homebrew` uses the **`homebrew`** environment and `winget` the **`winget`**
environment. All have required reviewers, so the run pauses at each and waits.
`sign` is additionally gated on the actor being `penso`. Betas skip `homebrew`
and `winget`; `just release-promote` runs both. `winget` only warns until the
first winget-pkgs version is submitted by hand (`scripts/release/README.md`).

Approve a pending gate:

```sh
run=<run-id>
gh api repos/penso/herdr-gpui/actions/runs/$run/pending_deployments \
  --jq '.[] | "\(.environment.name) id=\(.environment.id) can_approve=\(.current_user_can_approve)"'

gh api repos/penso/herdr-gpui/actions/runs/$run/pending_deployments -X POST --input - <<'JSON'
{"environment_ids":[<id>],"state":"approved","comment":"<why>"}
JSON
```

Use `--input` with real JSON. `-f "environment_ids[]=<id>"` sends the id as a
string and the API rejects it with `422 ... "<id>" is not an integer`.

A job sitting in `waiting` is not proof a gate is open — it is also the state
while a runner is being assigned. Check `pending_deployments` (empty means no
gate) and `.../approvals` for who approved what, and read those before telling
the owner anything about approval state.

## `gh run watch` dying is not a failed release

`dispatch.sh` ends by watching the run, and that watch is a local poll against
`api.github.com`. A network blip kills it with `error connecting to
api.github.com` and `just release` exits 1 **while the release keeps going
server-side**. Never report a failed release on the strength of that exit code.
Re-check the truth:

```sh
gh run view <run-id> --repo penso/herdr-gpui --json status,conclusion,jobs
```

Because the watcher owns the final summary, losing it also means nothing local
will print the published tag. Confirm with
`gh api repos/penso/herdr-gpui/releases/latest --jq .tag_name`. A beta is never
`latest`; find it with `gh release list --repo penso/herdr-gpui --limit 5`.

## The release notes write themselves

`changelog` generates the release body from git history with `git-cliff`
(`cliff.toml`) and `publish` passes it to `gh release create --notes-file`. Do
not hand-write notes, and do not edit a published body to add them: fix the
commit subjects instead, since those are the entries. The standing preamble
about GUI-only DMG, Linux packages, Windows installers and zips, and the required daemon lives in
`scripts/release/generate-changelog.sh`.

`CHANGELOG.md` is generated too, but it is **not** a release asset — the asset
set is fixed by `artifact-manifest.py` and each asset is checksummed, signed and
attested. Both files are uploaded as the run's `release-notes` workflow artifact.

## After it publishes

This is local and deliberately **not** a CI step.

Verify the published artifacts — the exact asset set, both checksums, Sigstore,
and GitHub provenance. Pin the commit with `--sha`; it is recommended:

```sh
just verify-release --version vYYYYMMDD.COUNTER --sha <release-commit-sha>
```

That completes a release. Do **not** run `just sign-release` as part of one:
the owner skips supplemental GPG signatures. The immutable release cannot carry
them (its asset set is exact), nothing publishes them elsewhere, and GitHub
provenance plus Sigstore already tie every binary to its workflow and commit.

Run it only when the owner asks. Then the output directory must not already
exist, the fingerprint must be the **full** signing subkey fingerprint (ask, or
read it from their GPG config; never guess), the owner must tap the YubiKey, and
nothing is uploaded: the signatures land in `<output-dir>/gpg/`.

## Before touching release machinery itself

If the change is to the scripts or workflow rather than a normal release:

```sh
just release-check    # bash -n, mocked release tests, security tests, actionlint, zizmor
```

It dispatches and publishes nothing. `just dmg $VERSION` builds a local signed
universal DMG and also publishes nothing.

## Never

- Dispatch a release the owner did not ask for.
- Invent, pass, or locally create a version tag.
- Report success from a queue state, or failure from a dead watcher.
- Run `just sign-release` unless the owner asked for it.
- Edit the vendored protocol crate, the workflow pins, or `NOTICE.md` as
  incidental release cleanup.
