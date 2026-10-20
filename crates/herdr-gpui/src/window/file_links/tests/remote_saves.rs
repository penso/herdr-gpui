//! Saves to the copy of a file an SSH pane printed, sent back to a fake
//! host. Watch runs are started by hand, with the once-a-second gate
//! rewound, so nothing here waits on a clock.

use super::remote::{TARGET, on_host};
use super::*;
use crate::{
    menu::Page,
    remote_files::{
        SyncState,
        tests::{FakeHost, NOTES, Received, conflict, cut_off, notes},
    },
    window::flash::Tone,
};
use herdr_client::Expect;
use sha2::{Digest, Sha256};

const PATH: &str = "/home/me/notes.md";
const EDITED: &[u8] = b"# Notes, edited\n";

fn sha(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

/// A host that refuses a push for a reason other than a conflict.
fn refused() -> herdr_client::Result<()> {
    Err(herdr_client::Error::RemoteFile(
        herdr_client::RemoteFileError::Missing,
    ))
}

/// A window that fetched `PATH` from `host` with a click on it.
fn fetched<'a>(
    host: &FakeHost,
    cx: &'a mut gpui::TestAppContext,
) -> (Entity<HerdrWindow>, &'a mut VisualTestContext) {
    let (view, cx) = on_host(host, notes, &format!("open {PATH}"), "/home/me", cx);
    click_path(&view, cx);
    assert_eq!(host.asked().len(), 1);
    (view, cx)
}

fn click_path(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) {
    let path_cell = at(view, cx, 7);
    modifier_click(cx, path_cell, Modifiers::secondary_key());
}

fn local(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> PathBuf {
    view.read_with(cx, |view, _| {
        view.remote_files.copy(TARGET, PATH).unwrap().local.clone()
    })
}

fn state(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> SyncState {
    view.read_with(cx, |view, _| {
        view.remote_files.copy(TARGET, PATH).unwrap().state
    })
}

fn page(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> Option<Page> {
    view.read_with(cx, |view, _| view.menu.page)
}

fn flash(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> (Tone, String) {
    view.read_with(cx, |view, _| {
        let (flash, _) = view.flash.as_ref().unwrap();
        (flash.tone, flash.text.to_string())
    })
}

/// One poll of the window, with a watch run due, and its work finished.
fn poll(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) {
    view.update_in(cx, |view, window, cx| {
        view.remote_files.rewind_watch();
        view.poll_remote_files(window, cx);
    });
    cx.run_until_parked();
}

/// Saves `bytes` to the copy and polls until a save that settled would
/// have opened its question: two runs to settle it, one more to open it.
fn save(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, bytes: &[u8]) {
    std::fs::write(local(view, cx), bytes).unwrap();
    for _ in 0..3 {
        poll(view, cx);
    }
}

fn press(cx: &mut VisualTestContext, id: &'static str) {
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    let button = cx.debug_bounds(id).unwrap().center();
    cx.simulate_click(button, Modifiers::default());
    cx.run_until_parked();
}

fn pushed(path: &str, expect: Expect, bytes: &[u8]) -> Received {
    Received {
        path: path.into(),
        expect,
        like: None,
        bytes: bytes.to_vec(),
    }
}

/// A kept-both push to `path`, which takes the access of the host's `PATH`.
fn pushed_beside(path: &str, bytes: &[u8]) -> Received {
    Received {
        like: Some(PATH.into()),
        ..pushed(path, Expect::Absent, bytes)
    }
}

#[gpui::test]
fn a_remote_save_asks_and_upload_pushes_it_once(cx: &mut gpui::TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = fetched(&host, cx);
    save(&view, cx, EDITED);
    assert_eq!(page(&view, cx), Some(Page::RemoteUpload));
    assert!(host.received().is_empty(), "nothing goes unasked");
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("remote-upload-path").is_some());

    press(cx, "remote-upload-yes");
    assert_eq!(
        host.received(),
        vec![pushed(PATH, Expect::Sha256(sha(NOTES)), EDITED)]
    );
    assert_eq!(page(&view, cx), None);
    assert_eq!(state(&view, cx), SyncState::Synced);
    assert_eq!(
        flash(&view, cx),
        (Tone::Success, "Uploaded notes.md to me@devbox".into())
    );
    view.read_with(cx, |view, _| {
        let copy = view.remote_files.copy(TARGET, PATH).unwrap();
        assert_eq!(copy.synced_sha256, sha(EDITED));
    });
    for _ in 0..3 {
        poll(&view, cx);
    }
    assert_eq!(host.received().len(), 1, "the pushed save is synced");
    assert_eq!(page(&view, cx), None);
}

#[gpui::test]
fn after_upload_future_saves_a_remote_save_pushes_without_asking(cx: &mut gpui::TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = fetched(&host, cx);
    save(&view, cx, EDITED);
    press(cx, "remote-upload-always");
    assert_eq!(host.received().len(), 1);

    let again: &[u8] = b"# Notes, edited twice\n";
    save(&view, cx, again);
    assert_eq!(page(&view, cx), None, "no question");
    assert_eq!(
        host.received(),
        vec![
            pushed(PATH, Expect::Sha256(sha(NOTES)), EDITED),
            pushed(PATH, Expect::Sha256(sha(EDITED)), again),
        ]
    );
    assert_eq!(state(&view, cx), SyncState::Synced);
}

#[gpui::test]
fn not_now_keeps_quiet_until_the_remote_copy_changes_again(cx: &mut gpui::TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = fetched(&host, cx);
    save(&view, cx, EDITED);
    cx.simulate_keystrokes("escape");
    assert_eq!(page(&view, cx), None);
    assert_eq!(
        state(&view, cx),
        SyncState::Declined {
            sha256: sha(EDITED)
        }
    );

    // The same bytes saved again ask nothing.
    let copy = local(&view, cx);
    let modified = std::fs::metadata(&copy).unwrap().modified().unwrap();
    std::fs::File::options()
        .write(true)
        .open(&copy)
        .unwrap()
        .set_modified(modified + std::time::Duration::from_secs(5))
        .unwrap();
    for _ in 0..3 {
        poll(&view, cx);
    }
    assert_eq!(page(&view, cx), None);

    save(&view, cx, b"# Notes, edited again\n");
    assert_eq!(page(&view, cx), Some(Page::RemoteUpload));
    press(cx, "remote-upload-no");
    assert!(host.received().is_empty());
}

/// A window whose upload of a save met a host file that changed.
fn conflicted<'a>(
    host: &FakeHost,
    cx: &'a mut gpui::TestAppContext,
) -> (Entity<HerdrWindow>, &'a mut VisualTestContext) {
    let (view, cx) = fetched(host, cx);
    host.answer_pushes(&[conflict]);
    save(&view, cx, EDITED);
    press(cx, "remote-upload-yes");
    assert_eq!(
        state(&view, cx),
        SyncState::Conflict {
            sha256: sha(EDITED)
        }
    );
    poll(&view, cx);
    assert_eq!(page(&view, cx), Some(Page::RemoteConflict));
    (view, cx)
}

#[gpui::test]
fn overwrite_anyway_pushes_the_remote_copy_over_whatever_is_there(cx: &mut gpui::TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = conflicted(&host, cx);
    press(cx, "remote-conflict-overwrite");
    assert_eq!(
        host.received(),
        vec![
            pushed(PATH, Expect::Sha256(sha(NOTES)), EDITED),
            pushed(PATH, Expect::Any, EDITED),
        ]
    );
    assert_eq!(state(&view, cx), SyncState::Synced);
    assert_eq!(page(&view, cx), None);
}

#[gpui::test]
fn re_fetch_replaces_the_remote_copy_with_the_host_s_file(cx: &mut gpui::TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = conflicted(&host, cx);
    let theirs: &[u8] = b"# Notes, changed on the host\n";
    host.change_file(theirs);
    press(cx, "remote-conflict-refetch");
    assert_eq!(host.asked().len(), 2);
    assert_eq!(std::fs::read(local(&view, cx)).unwrap(), theirs);
    assert_eq!(state(&view, cx), SyncState::Synced);
    assert_eq!(
        flash(&view, cx),
        (
            Tone::Success,
            "Your copy of notes.md now holds the version on me@devbox".into()
        )
    );
    assert_eq!(host.received().len(), 1, "nothing more is pushed");
    // The new bytes are the synced ones: no save is seen.
    for _ in 0..3 {
        poll(&view, cx);
    }
    assert_eq!(page(&view, cx), None);
}

#[gpui::test]
fn keep_both_pushes_the_remote_copy_beside_under_a_free_name(cx: &mut gpui::TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = conflicted(&host, cx);
    host.answer_pushes(&[conflict]);
    press(cx, "remote-conflict-keep-both");
    assert_eq!(
        host.received()[1..],
        [
            pushed_beside("/home/me/notes.conflict.md", EDITED),
            pushed_beside("/home/me/notes.conflict-2.md", EDITED),
        ]
    );
    assert_eq!(
        flash(&view, cx),
        (
            Tone::Success,
            "Uploaded your copy to me@devbox as notes.conflict-2.md".into()
        )
    );
    // The copy still tracks the host's original path.
    assert!(view.read_with(cx, |view, _| view.remote_files.copy(TARGET, PATH).is_some()));
    assert_eq!(page(&view, cx), None);
}

#[gpui::test]
fn a_remote_upload_question_waits_for_the_open_page(cx: &mut gpui::TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = fetched(&host, cx);
    view.update_in(cx, |view, window, cx| view.show_install_modal(window, cx));
    save(&view, cx, EDITED);
    assert_eq!(page(&view, cx), Some(Page::Install), "the page is kept");
    assert!(matches!(state(&view, cx), SyncState::Asking { .. }));

    cx.simulate_keystrokes("escape");
    assert_eq!(page(&view, cx), None);
    poll(&view, cx);
    assert_eq!(page(&view, cx), Some(Page::RemoteUpload));
    assert!(host.received().is_empty());
}

#[gpui::test]
fn a_click_on_a_remote_path_with_unsent_edits_reopens_the_copy(cx: &mut gpui::TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = fetched(&host, cx);
    host.answer_pushes(&[refused]);
    save(&view, cx, EDITED);
    cx.simulate_keystrokes("enter");
    assert_eq!(host.received().len(), 1);
    assert_eq!(
        flash(&view, cx),
        (
            Tone::Warning,
            "Could not upload the file: the file is not on the host".into()
        )
    );
    assert_eq!(
        state(&view, cx),
        SyncState::Edited {
            sha256: sha(EDITED)
        }
    );

    click_path(&view, cx);
    assert_eq!(host.asked().len(), 1, "not fetched again");
    assert!(cx.opened_url().unwrap().ends_with("/notes.md"));
    assert_eq!(std::fs::read(local(&view, cx)).unwrap(), EDITED);
}

#[gpui::test]
fn a_cut_off_remote_upload_warns_and_the_next_save_asks_again(cx: &mut gpui::TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = fetched(&host, cx);
    host.answer_pushes(&[cut_off]);
    save(&view, cx, EDITED);
    press(cx, "remote-upload-yes");
    assert_eq!(
        flash(&view, cx),
        (
            Tone::Warning,
            "Could not upload the file: the upload was cut off; the host's file is unchanged"
                .into()
        )
    );
    assert_eq!(
        state(&view, cx),
        SyncState::Edited {
            sha256: sha(EDITED)
        }
    );
    save(&view, cx, b"# Notes, edited once more\n");
    assert_eq!(page(&view, cx), Some(Page::RemoteUpload));
}
