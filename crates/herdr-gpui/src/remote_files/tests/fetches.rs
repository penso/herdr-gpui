//! Fetches that overlap: clicks made while a fetch runs, and fetches of one
//! host file by windows that share the process's cache.

use super::*;

const PATH: &str = "/home/me/notes.md";

#[gpui::test]
fn a_click_while_its_file_is_fetched_starts_no_second_fetch(cx: &mut TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = window(&host, cx);
    view.update(cx, |view, cx| {
        view.open_remote_file(remote(PATH), None, cx);
        view.open_remote_file(remote(PATH), None, cx);
    });
    cx.run_until_parked();
    assert_eq!(host.asked().len(), 1);

    // Once that fetch is done, a click fetches again.
    view.update(cx, |view, cx| view.open_remote_file(remote(PATH), None, cx));
    cx.run_until_parked();
    assert_eq!(host.asked().len(), 2);
}

#[gpui::test]
fn a_failed_click_fetch_lets_the_next_click_fetch_again(cx: &mut TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = window(&host, cx);
    view.update(cx, |view, _| host.serve(&mut view.remote_files, missing));
    view.update(cx, |view, cx| view.open_remote_file(remote(PATH), None, cx));
    cx.run_until_parked();
    view.update(cx, |view, cx| view.open_remote_file(remote(PATH), None, cx));
    cx.run_until_parked();
    assert_eq!(host.asked().len(), 2);
}

#[test]
fn overlapping_fetches_of_one_copy_both_return_and_leave_it_whole() {
    let host = FakeHost::new();
    let owner = Owner::default();
    let (started, on_start) = std::sync::mpsc::channel();
    let (release, on_release) = std::sync::mpsc::channel::<()>();
    let on_release = Mutex::new(on_release);
    let io = host.io(notes);
    let fetch = io.fetch.clone();
    let gated = Io {
        fetch: Arc::new(move |remote, out| {
            started.send(()).unwrap();
            on_release.lock().unwrap().recv().unwrap();
            fetch(remote, out)
        }),
        ..io
    };
    let (first, second) = std::thread::scope(|scope| {
        let first = scope.spawn(|| fetch_fresh(&gated, owner, &remote(PATH)));
        let second = scope.spawn(|| fetch_fresh(&gated, owner, &remote(PATH)));
        // Both fetches are writing before either may finish.
        on_start.recv().unwrap();
        on_start.recv().unwrap();
        release.send(()).unwrap();
        release.send(()).unwrap();
        (first.join().unwrap(), second.join().unwrap())
    });
    let (first, second) = (first.unwrap(), second.unwrap());
    assert_eq!(first.copy.local, second.copy.local);
    assert_eq!(std::fs::read(&first.copy.local).unwrap(), NOTES);
    assert_eq!(
        std::fs::read_dir(first.copy.local.parent().unwrap())
            .unwrap()
            .count(),
        1,
        "no .part left"
    );
    assert_eq!(host.asked().len(), 2);
}

/// Fetches `PATH` into `files`, as a click in its window does, returning
/// where the copy is.
fn fetch_into(files: &mut RemoteFiles) -> PathBuf {
    let done = fetch_fresh(&files.io, files.owner, &remote(PATH)).unwrap();
    let local = done.copy.local.clone();
    files.record(done);
    local
}

#[test]
fn a_fetch_in_another_window_keeps_this_window_s_edits() {
    const EDITED: &[u8] = b"# Notes, edited\n";
    let host = FakeHost::new();
    let (mut first, mut second) = (RemoteFiles::default(), RemoteFiles::default());
    host.serve(&mut first, notes);
    host.serve(&mut second, notes);
    let local = fetch_into(&mut first);
    std::fs::write(&local, EDITED).unwrap();
    // Two watch runs settle the save, which then asks to be uploaded.
    for _ in 0..2 {
        let seen = watch::scan(first.probes());
        first.apply_scan(seen);
    }
    let key = ("me@devbox".to_owned(), PATH.to_owned());
    assert!(
        matches!(first.copies[&key].state, SyncState::Asking { .. }),
        "{:?}",
        first.copies[&key].state
    );

    let theirs = fetch_into(&mut second);
    assert_eq!(std::fs::read(&local).unwrap(), EDITED);
    assert_ne!(theirs, local);
    assert_eq!(std::fs::read(&theirs).unwrap(), NOTES);
}

#[test]
fn two_fetches_of_one_copy_write_apart() {
    let host = FakeHost::new();
    let local = host.cache.launch().join("notes.md");
    let first = cache::part_path(&local);
    let second = cache::part_path(&local);
    assert_ne!(first, second);
    assert_eq!(first.parent(), local.parent(), "beside the copy");
    assert_eq!(second.parent(), local.parent(), "beside the copy");

    let _written = cache::create_private_file(&first).unwrap();
    assert!(
        cache::create_private_file(&first).is_err(),
        "a part is never opened over another fetch's"
    );
}
