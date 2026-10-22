//! A conflict question closed without an answer, which a click on the
//! copy's path brings back.

use super::*;
use crate::menu::Page;

const PATH: &str = "/home/me/notes.md";
const EDITED: &[u8] = b"# Notes, edited\n";

fn key() -> Key {
    ("me@devbox".to_owned(), PATH.to_owned())
}

/// One poll of the window, and its work finished.
fn poll(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) {
    view.update_in(cx, |view, window, cx| view.poll_remote_files(window, cx));
    cx.run_until_parked();
}

/// A link click on `PATH`, and its work finished.
fn click(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) {
    view.update(cx, |view, cx| {
        view.open_remote_file(remote(PATH), None, None, None, cx)
    });
    cx.run_until_parked();
}

fn page(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> Option<Page> {
    view.read_with(cx, |view, _| view.menu.page)
}

/// The questions waiting for the window, by kind.
fn waiting(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> Vec<PromptKind> {
    view.read_with(cx, |view, _| {
        view.remote_files
            .prompts
            .iter()
            .map(|prompt| prompt.kind)
            .collect()
    })
}

fn set_state(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, state: SyncState) {
    view.update(cx, |view, _| {
        view.remote_files.copies.get_mut(&key()).unwrap().state = state;
    });
}

/// A window whose copy of `PATH` is in conflict, its question closed with
/// Escape.
fn dismissed<'a>(
    host: &FakeHost,
    cx: &'a mut TestAppContext,
) -> (Entity<HerdrWindow>, &'a mut VisualTestContext) {
    let (view, cx) = window(host, cx);
    click(&view, cx);
    set_state(
        &view,
        cx,
        SyncState::Conflict {
            sha256: Sha256::digest(EDITED).into(),
        },
    );
    view.update(cx, |view, _| {
        view.remote_files.enqueue(Prompt {
            key: key(),
            kind: PromptKind::Conflict,
        });
    });
    poll(&view, cx);
    assert_eq!(page(&view, cx), Some(Page::RemoteConflict));
    cx.simulate_keystrokes("escape");
    assert_eq!(page(&view, cx), None);
    poll(&view, cx);
    assert_eq!(page(&view, cx), None, "nothing brings it back by itself");
    assert_eq!(waiting(&view, cx), []);
    (view, cx)
}

#[gpui::test]
fn a_click_on_a_conflicted_copy_s_path_asks_its_closed_question_again(cx: &mut TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = dismissed(&host, cx);
    // Something else opened since, so the copy's own opening shows.
    cx.update(|_, cx| cx.open_url("about:blank"));

    click(&view, cx);
    assert_eq!(host.asked().len(), 1, "not fetched again");
    assert!(cx.opened_url().unwrap().ends_with("/notes.md"));
    assert_eq!(waiting(&view, cx), [PromptKind::Conflict]);
    poll(&view, cx);
    assert_eq!(page(&view, cx), Some(Page::RemoteConflict));
    assert_eq!(
        view.read_with(cx, |view, _| view.shown_remote_file_prompt()),
        Some(Prompt {
            key: key(),
            kind: PromptKind::Conflict,
        })
    );
}

#[gpui::test]
fn two_clicks_on_a_conflicted_copy_s_path_queue_its_question_once(cx: &mut TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = dismissed(&host, cx);
    click(&view, cx);
    click(&view, cx);
    assert_eq!(waiting(&view, cx), [PromptKind::Conflict]);

    // Nor does a click while the question is open queue it behind itself.
    poll(&view, cx);
    assert_eq!(page(&view, cx), Some(Page::RemoteConflict));
    click(&view, cx);
    assert_eq!(waiting(&view, cx), []);
}

#[gpui::test]
fn a_click_on_a_copy_not_in_conflict_queues_no_question(cx: &mut TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = window(&host, cx);
    click(&view, cx);
    // A synced copy is fetched again.
    click(&view, cx);
    assert_eq!(host.asked().len(), 2);
    assert_eq!(waiting(&view, cx), []);

    // One asking to be uploaded, whose question waits, is reopened as it is.
    set_state(
        &view,
        cx,
        SyncState::Asking {
            sha256: Sha256::digest(EDITED).into(),
        },
    );
    view.update(cx, |view, _| {
        view.remote_files.enqueue(Prompt {
            key: key(),
            kind: PromptKind::Upload,
        });
    });
    click(&view, cx);
    assert_eq!(host.asked().len(), 2, "not fetched again");
    assert_eq!(waiting(&view, cx), [PromptKind::Upload]);
}
