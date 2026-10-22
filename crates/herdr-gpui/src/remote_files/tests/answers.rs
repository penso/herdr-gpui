//! Answers to a copy's questions that arrive after the copy moved on.

use super::*;
use std::time::Duration;

const PATH: &str = "/home/me/notes.md";

/// One poll of the window, with a watch run due, and its work finished.
fn poll(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) {
    view.update_in(cx, |view, window, cx| {
        view.remote_files.last_watch = None;
        view.poll_remote_files(window, cx);
    });
    cx.run_until_parked();
}

#[gpui::test]
fn saving_the_synced_bytes_back_closes_the_open_upload_question(cx: &mut TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = window(&host, cx);
    view.update(cx, |view, cx| {
        view.open_remote_file(remote(PATH), None, None, None, cx)
    });
    cx.run_until_parked();
    let key = ("me@devbox".to_owned(), PATH.to_owned());
    let (local, synced) = view.read_with(cx, |view, _| {
        let copy = &view.remote_files.copies[&key];
        (copy.local.clone(), copy.synced_meta)
    });
    std::fs::write(&local, b"# Notes, edited\n").unwrap();
    // Two runs settle the save; the next poll opens its question.
    for _ in 0..3 {
        poll(&view, cx);
    }
    let page = |cx: &mut VisualTestContext| view.read_with(cx, |view, _| view.menu.page);
    assert_eq!(page(cx), Some(crate::menu::Page::RemoteUpload));

    // The user saves the synced bytes back while the question is open.
    std::fs::write(&local, NOTES).unwrap();
    let file = std::fs::File::options().write(true).open(&local).unwrap();
    file.set_modified(synced.modified + Duration::from_secs(5))
        .unwrap();
    drop(file);
    for _ in 0..2 {
        poll(&view, cx);
    }
    view.read_with(cx, |view, _| {
        assert_eq!(view.remote_files.copies[&key].state, SyncState::Synced);
        assert_eq!(view.shown_remote_file_prompt(), None);
    });
    assert_eq!(page(cx), None, "the question about the edits closed");
    assert!(host.received().is_empty(), "{:?}", host.received());
}

#[gpui::test]
fn overwrite_after_the_host_s_bytes_were_saved_back_pushes_nothing(cx: &mut TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = window(&host, cx);
    view.update(cx, |view, cx| {
        view.open_remote_file(remote(PATH), None, None, None, cx)
    });
    cx.run_until_parked();
    let key = ("me@devbox".to_owned(), PATH.to_owned());
    view.update(cx, |view, cx| {
        let files = &mut view.remote_files;
        let copy = files.copies.get_mut(&key).unwrap();
        // The host refused a save's upload: its file changed since the sync.
        copy.state = SyncState::Conflict {
            sha256: Sha256::digest(b"# Notes, edited\n").into(),
        };
        // Then the user saves the bytes the copy was synced with back.
        std::fs::write(&copy.local, NOTES).unwrap();
        let file = std::fs::File::options()
            .write(true)
            .open(&copy.local)
            .unwrap();
        file.set_modified(copy.synced_meta.modified + Duration::from_secs(5))
            .unwrap();
        drop(file);
        // Two watch runs settle that save.
        for _ in 0..2 {
            let seen = watch::scan(files.probes());
            files.apply_scan(seen);
        }
        // The conflict question, still open, is answered after that.
        view.resolve_remote_conflict(key.clone(), Resolution::Overwrite, cx);
    });
    cx.run_until_parked();
    assert!(host.received().is_empty(), "{:?}", host.received());
    view.read_with(cx, |view, _| {
        assert_eq!(view.remote_files.copies[&key].state, SyncState::Synced);
    });
}
