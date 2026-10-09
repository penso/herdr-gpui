# Orchestrator

The Issues & PRs tab (`crates/herdr-gpui/src/orchestrator/`) lists a
repository's issues, pull requests, and the agent runs dispatched for them,
and dispatches new ones through the Herdr CLI. User-facing behavior is in the
crate README's "Issues & PRs" section; this page is for working on it.

## Shared state with agent-launcher

The tab reads and writes agent-launcher's per-repository SQLite database, so
both applications can run against one repository at once. agent-launcher's
`docs/shared-state.md` is the contract: the file's location, the schema
version (`PRAGMA user_version`, currently 1), key formats, JSON column shapes,
and who may write what. In short:

- The database is `<data_local_dir>/agent-launcher/repositories/<uuid v5 of
  the canonical git common dir>/state.sqlite3`. Repositories on SSH hosts are
  keyed by `ssh://<destination><git common dir>` and live on this machine.
- A run's row, events, and `herdr_sessions` row are written only by its
  `runs.owner` (`agent-launcher` or `herdr-gpui`). herdr-gpui never stops,
  messages, removes, or recovers agent-launcher's runs, and never takes its
  `runtime.lock`.
- Either application may replace a source's issues, but only with a complete
  listing. A capped listing, or one made while the schema is newer than this
  build knows, is shown and not written.
- A newer schema opens read-only, with a banner; nothing is downgraded.

`store.rs` implements the contract; its tests open a version-0 database as
agent-launcher wrote it and check that migration keeps its rows.

## Layout

| Module | Responsibility |
| --- | --- |
| `model.rs`, `location.rs`, `store.rs` | Records in the database's shapes, its path, and SQLite access. |
| `repo.rs` | Finding a checkout's repository, main checkout, Beads, and remote on its host. |
| `github.rs`, `beads.rs` | The sources: one full listing each. |
| `service.rs` | The worker thread per open view: sync, reload, and the action queue. |
| `actions.rs` | Dispatch, stop, send, remove, delete: each on its own thread. |
| `prompt.rs` | Prompt profiles, agent-launcher's built-in and review prompts, branch names. |
| `view/` | The view entity: inbox, preview, item pages, dialogs. Pure row logic is in `view/rows.rs`. |
| `tab.rs` | Hosting the view in a tab: opening, restoring, the tick, and events. |

The view is a GPUI entity that talks to its host only through `Event`s and
setters (`set_look`, `set_live`, `set_github`), so a separate window could host
it as well. Nothing in it blocks: the worker does every disk, network, and host
script call, and the UI reads the newest `Snapshot` from a coalescing mailbox.

Live agent status comes from each connected host's Herdr snapshot, matched to
runs by host, workspace, and pane; it is never written back to the database.

## Adding GitLab

`Provider::Gitlab` already exists, so agent-launcher's GitLab rows decode, and
`repo::parse_remote_url` recognizes GitLab remotes. To list them:

1. Add `gitlab.rs` returning a complete listing in agent-launcher's GitLab row
   format (its `crates/issues/src/gitlab.rs` is the reference for fields and
   `native_id`).
2. Handle `Provider::Gitlab` in `service::Worker::sync_source`, and mark it
   supported in `service::sources`.
3. Decide where its token comes from; herdr-gpui has no GitLab sign-in yet.

Merge requests would also need `PullRequest`-like metadata in the shared
schema, which is a contract change for both applications.
