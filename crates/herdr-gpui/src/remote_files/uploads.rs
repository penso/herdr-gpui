//! The window's side of sending saves back: starting watch runs from its
//! poll tick, opening the questions they queue once no page is open, and
//! running the pushes and fetches the answers lead to.

use super::{
    Answer, Error, Fetch, Key, MAX_REMOTE_FILE_BYTES, Prompt, PromptKind, Resolution, fetch_copy,
    push::{Pushed, push_copy},
    sync::{self, Action, Event, Push},
    watch,
};
use crate::window::{Flash, HerdrWindow, RemotePath};
use gpui::{Context, Window};
use herdr_client::{Expect, RemoteFileAccess};
use std::time::{Duration, Instant};

/// The shortest time between two watch runs.
const WATCH_INTERVAL: Duration = Duration::from_secs(1);

/// The highest `n` a kept-both copy's name tries.
const LAST_CONFLICT_NAME: u8 = 9;

impl HerdrWindow {
    /// Opens a waiting question when no page is open, and starts a watch run
    /// when one is due.
    pub(crate) fn poll_remote_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_next_remote_prompt(window, cx);
        self.start_remote_watch(window, cx);
    }

    /// Closes the open question when its copy left the state it asks about,
    /// so no buttons stay up that would do nothing.
    fn close_stale_remote_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(shown) = self.shown_remote_file_prompt() else {
            return;
        };
        if self.remote_files.applies(&shown) {
            return;
        }
        self.close_remote_file_prompt(window, cx);
    }

    fn show_next_remote_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let shown = self.shown_remote_file_prompt();
        self.remote_files.requeue_unanswered(shown.as_ref());
        if self.menu.page.is_some() {
            return;
        }
        let Some(prompt) = self.remote_files.next_prompt() else {
            return;
        };
        if self.show_remote_file_prompt(&prompt, window, cx) {
            self.remote_files.prompts.pop_front();
        }
    }

    fn start_remote_watch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let files = &mut self.remote_files;
        let now = Instant::now();
        if files.watching
            || files.copies.is_empty()
            || files
                .last_watch
                .is_some_and(|last| now.duration_since(last) < WATCH_INTERVAL)
        {
            return;
        }
        files.last_watch = Some(now);
        files.watching = true;
        let probes = files.probes();
        let scan = cx
            .background_executor()
            .spawn(async move { watch::scan(probes) });
        cx.spawn_in(window, async move |this, cx| {
            let seen = scan.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.remote_files_scanned(seen, window, cx);
            });
        })
        .detach();
    }

    fn remote_files_scanned(
        &mut self,
        seen: Vec<watch::Seen>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.remote_files.watching = false;
        for (key, action) in self.remote_files.apply_scan(seen) {
            self.act_on_remote_file(key, action, cx);
        }
        // A save back to the synced bytes ends the question about the edits.
        self.close_stale_remote_prompt(window, cx);
    }

    /// Answers the upload question about `key`'s copy.
    pub(crate) fn answer_remote_upload(
        &mut self,
        key: Key,
        answer: Answer,
        cx: &mut Context<Self>,
    ) {
        let action = self.remote_files.handle(&key, Event::Answered(answer));
        self.act_on_remote_file(key, action, cx);
    }

    /// Answers the conflict question about `key`'s copy.
    pub(crate) fn resolve_remote_conflict(
        &mut self,
        key: Key,
        resolution: Resolution,
        cx: &mut Context<Self>,
    ) {
        let action = self.remote_files.handle(&key, Event::Resolved(resolution));
        self.act_on_remote_file(key, action, cx);
    }

    fn act_on_remote_file(&mut self, key: Key, action: Action, cx: &mut Context<Self>) {
        match action {
            Action::Nothing => {}
            Action::Ask => self.remote_files.enqueue(Prompt {
                key,
                kind: PromptKind::Upload,
            }),
            Action::ShowConflict => self.remote_files.enqueue(Prompt {
                key,
                kind: PromptKind::Conflict,
            }),
            Action::Push(push) => self.start_remote_push(key, push, cx),
            Action::Refetch => self.start_remote_refetch(key, cx),
        }
        cx.notify();
    }

    /// The host file `push` goes to for `key`'s copy, what it expects there,
    /// and the host file whose access a new file takes: a kept-both copy
    /// takes the original's, so it lets no one read it who could not read
    /// that.
    fn push_target(&self, key: &Key, push: Push) -> Option<(RemotePath, Expect, Option<String>)> {
        let copy = self.remote_files.copies.get(key)?;
        let (path, expect, like) = match push {
            Push::Expected => (key.1.clone(), Expect::Sha256(copy.synced_sha256), None),
            Push::Any => (key.1.clone(), Expect::Any, None),
            Push::Beside(n) => (
                sync::conflict_path(&key.1, copy.shell, n),
                Expect::Absent,
                Some(key.1.clone()),
            ),
        };
        let remote = RemotePath {
            target: key.0.clone(),
            shell: copy.shell,
            path,
            line: None,
        };
        Some((remote, expect, like))
    }

    fn start_remote_push(&mut self, key: Key, push: Push, cx: &mut Context<Self>) {
        let (Some((remote, expect, like)), Some(copy)) = (
            self.push_target(&key, push),
            self.remote_files.copies.get(&key),
        ) else {
            return;
        };
        let local = copy.local.clone();
        let io = self.remote_files.io.clone();
        let task = cx.background_executor().spawn(async move {
            let access = like
                .as_deref()
                .map_or(RemoteFileAccess::Keep, RemoteFileAccess::Like);
            let pushed = push_copy(
                &io,
                &remote,
                &local,
                (expect, access),
                MAX_REMOTE_FILE_BYTES,
            );
            (remote, pushed)
        });
        cx.spawn(async move |this, cx| {
            let (remote, pushed) = task.await;
            let _ = this.update(cx, |this, cx| {
                this.remote_file_pushed(key, push, &remote, pushed, cx);
            });
        })
        .detach();
    }

    fn remote_file_pushed(
        &mut self,
        key: Key,
        push: Push,
        remote: &RemotePath,
        pushed: super::Result<Pushed>,
        cx: &mut Context<Self>,
    ) {
        let error = match pushed {
            Ok(pushed) => return self.remote_push_done(key, push, remote, pushed, cx),
            Err(error) => error,
        };
        let event = match push {
            Push::Beside(n) if error.is_conflict() && n < LAST_CONFLICT_NAME => {
                return self.start_remote_push(key, Push::Beside(n + 1), cx);
            }
            Push::Beside(_) if error.is_conflict() => {
                let last = super::basename(&remote.path).to_owned();
                self.remote_push_failed(&Error::NamesTaken { last }, cx);
                Event::Conflicted
            }
            _ if error.is_conflict() => Event::Conflicted,
            _ => {
                self.remote_push_failed(&error, cx);
                Event::Failed
            }
        };
        let action = self.remote_files.handle(&key, event);
        self.act_on_remote_file(key, action, cx);
    }

    fn remote_push_done(
        &mut self,
        key: Key,
        push: Push,
        remote: &RemotePath,
        pushed: Pushed,
        cx: &mut Context<Self>,
    ) {
        let name = super::basename(&remote.path);
        let (event, text) = match push {
            Push::Beside(_) => (
                Event::KeptBoth,
                format!("Uploaded your copy to {} as {name}", remote.target),
            ),
            Push::Expected | Push::Any => {
                self.remote_files.synced(&key, pushed);
                (
                    Event::Uploaded,
                    format!("Uploaded {name} to {}", remote.target),
                )
            }
        };
        self.show_flash(Flash::success(text), cx);
        let action = self.remote_files.handle(&key, event);
        self.act_on_remote_file(key, action, cx);
    }

    fn remote_push_failed(&mut self, error: &Error, cx: &mut Context<Self>) {
        tracing::warn!(%error, "Remote file copy not uploaded");
        self.show_flash(Flash::warning(error.to_string()), cx);
    }

    /// Fetches the host's file over `key`'s copy, dropping its edits,
    /// without opening it again.
    fn start_remote_refetch(&mut self, key: Key, cx: &mut Context<Self>) {
        let Some(copy) = self.remote_files.copies.get(&key) else {
            return;
        };
        let remote = RemotePath {
            target: key.0.clone(),
            shell: copy.shell,
            path: key.1.clone(),
            line: None,
        };
        let io = self.remote_files.io.clone();
        let owner = self.remote_files.owner;
        let fetch = cx
            .background_executor()
            // The user chose to drop the copy's edits, so none are kept.
            .spawn(async move { fetch_copy(&io, owner, &remote, None) });
        cx.spawn(async move |this, cx| {
            let fetched = fetch.await;
            let _ = this.update(cx, |this, cx| this.remote_file_refetched(key, fetched, cx));
        })
        .detach();
    }

    fn remote_file_refetched(
        &mut self,
        key: Key,
        fetched: super::Result<Fetch>,
        cx: &mut Context<Self>,
    ) {
        let done = match fetched {
            Ok(Fetch::Written(done)) => *done,
            // A fetch that keeps no edits always writes the copy.
            Ok(Fetch::KeptEdits) => return self.remote_refetch_failed(key, cx),
            Err(error) => {
                tracing::warn!(%error, "Remote file not fetched again");
                self.show_flash(Flash::warning(error.to_string()), cx);
                return self.remote_refetch_failed(key, cx);
            }
        };
        let text = format!(
            "Your copy of {} now holds the version on {}",
            super::basename(&key.1),
            key.0
        );
        self.remote_files.record(done);
        self.show_flash(Flash::success(text), cx);
    }

    fn remote_refetch_failed(&mut self, key: Key, cx: &mut Context<Self>) {
        let action = self.remote_files.handle(&key, Event::RefetchFailed);
        self.act_on_remote_file(key, action, cx);
    }
}
