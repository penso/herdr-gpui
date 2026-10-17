//! Saves to remote file copies: noticing them, the rules for uploading
//! them, and the push itself, without a window.

use super::*;
use crate::remote_files::{
    push::{Hashing, push_copy},
    sync::{Action, Event, Push, conflict_path, step},
    watch::{Debounce, Meta, Watch, debounce},
};
use std::time::{Duration, SystemTime};

const PATH: &str = r"C:\dev\notes.md";

/// Window-less copies of `PATH`, fetched from `host`.
fn fetched(host: &FakeHost) -> (RemoteFiles, Key, PathBuf) {
    let mut files = RemoteFiles::default();
    host.serve(&mut files, notes);
    let done = fetch_fresh(&files.io, files.owner, &remote(PATH)).unwrap();
    let local = done.copy.local.clone();
    let key = done.key.clone();
    files.record(done);
    (files, key, local)
}

/// One watch run, done in place.
fn run(files: &mut RemoteFiles) -> Vec<(Key, Action)> {
    let seen = watch::scan(files.probes());
    files.apply_scan(seen)
}

fn sha(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

#[test]
fn a_kept_copy_is_named_beside_the_host_s_file() {
    let windows = RemoteShell::PowerShell;
    let posix = RemoteShell::Posix;
    let cases = [
        (r"C:\dev\notes.md", windows, 1, r"C:\dev\notes.conflict.md"),
        (
            r"C:\dev\notes.md",
            windows,
            2,
            r"C:\dev\notes.conflict-2.md",
        ),
        (
            r"C:\dev\notes.md",
            windows,
            9,
            r"C:\dev\notes.conflict-9.md",
        ),
        (r"~\.bashrc", windows, 1, r"~\.bashrc.conflict"),
        ("/srv/Makefile", posix, 1, "/srv/Makefile.conflict"),
        (
            "/srv/archive.tar.gz",
            posix,
            1,
            "/srv/archive.tar.conflict.gz",
        ),
        (
            "/srv/archive.tar.gz",
            posix,
            3,
            "/srv/archive.tar.conflict-3.gz",
        ),
        ("~/.bashrc", posix, 1, "~/.bashrc.conflict"),
        // A backslash is part of a POSIX name, not a folder.
        (r"/srv/a\b.txt", posix, 1, r"/srv/a\b.conflict.txt"),
        (r"/srv/v1.0\notes", posix, 1, r"/srv/v1.conflict.0\notes"),
    ];
    for (path, shell, n, expected) in cases {
        assert_eq!(conflict_path(path, shell, n), expected, "{path} {n}");
    }
}

#[test]
fn each_event_moves_a_copy_by_the_upload_rules() {
    use SyncState::{Asking, Conflict, Declined, Edited, Refetching, Synced, Uploading};
    let (h, g) = ([1; 32], [2; 32]);
    let ask = Policy::Ask;
    let always = Policy::Always;
    #[rustfmt::skip]
    let cases = [
        (Synced, ask, Event::Saved(h), Asking { sha256: h }, ask, Action::Ask),
        (Synced, always, Event::Saved(h), Uploading { sha256: h }, always, Action::Push(Push::Expected)),
        (Asking { sha256: h }, ask, Event::Saved(g), Asking { sha256: g }, ask, Action::Nothing),
        (Asking { sha256: h }, ask, Event::Answered(Answer::Upload), Uploading { sha256: h }, ask, Action::Push(Push::Expected)),
        (Asking { sha256: h }, ask, Event::Answered(Answer::Always), Uploading { sha256: h }, always, Action::Push(Push::Expected)),
        (Asking { sha256: h }, ask, Event::Answered(Answer::NotNow), Declined { sha256: h }, ask, Action::Nothing),
        (Asking { sha256: h }, ask, Event::Reverted, Synced, ask, Action::Nothing),
        (Declined { sha256: h }, ask, Event::Saved(h), Declined { sha256: h }, ask, Action::Nothing),
        (Declined { sha256: h }, ask, Event::Saved(g), Asking { sha256: g }, ask, Action::Ask),
        (Declined { sha256: h }, ask, Event::Reverted, Synced, ask, Action::Nothing),
        (Uploading { sha256: h }, ask, Event::Saved(g), Uploading { sha256: h }, ask, Action::Nothing),
        (Uploading { sha256: h }, ask, Event::Uploaded, Synced, ask, Action::Nothing),
        (Uploading { sha256: h }, always, Event::Conflicted, Conflict { sha256: h }, always, Action::ShowConflict),
        (Uploading { sha256: h }, ask, Event::Failed, Edited { sha256: h }, ask, Action::Nothing),
        (Uploading { sha256: h }, ask, Event::KeptBoth, Declined { sha256: h }, ask, Action::Nothing),
        (Edited { sha256: h }, ask, Event::Saved(g), Asking { sha256: g }, ask, Action::Ask),
        (Edited { sha256: h }, always, Event::Saved(g), Uploading { sha256: g }, always, Action::Push(Push::Expected)),
        (Edited { sha256: h }, ask, Event::Reverted, Synced, ask, Action::Nothing),
        (Conflict { sha256: h }, ask, Event::Resolved(Resolution::Overwrite), Uploading { sha256: h }, ask, Action::Push(Push::Any)),
        (Conflict { sha256: h }, ask, Event::Resolved(Resolution::Refetch), Refetching { sha256: h }, ask, Action::Refetch),
        (Conflict { sha256: h }, ask, Event::Resolved(Resolution::KeepBoth), Uploading { sha256: h }, ask, Action::Push(Push::Beside(1))),
        (Conflict { sha256: h }, always, Event::Saved(g), Conflict { sha256: g }, always, Action::ShowConflict),
        (Conflict { sha256: h }, ask, Event::Reverted, Synced, ask, Action::Nothing),
        (Refetching { sha256: h }, ask, Event::Saved(g), Refetching { sha256: h }, ask, Action::Nothing),
        (Refetching { sha256: h }, ask, Event::RefetchFailed, Conflict { sha256: h }, ask, Action::ShowConflict),
        // Answers that arrive for a question no longer asked do nothing.
        (Synced, ask, Event::Answered(Answer::Upload), Synced, ask, Action::Nothing),
        (Uploading { sha256: h }, ask, Event::Answered(Answer::Always), Uploading { sha256: h }, ask, Action::Nothing),
        (Synced, ask, Event::Resolved(Resolution::Overwrite), Synced, ask, Action::Nothing),
    ];
    for (state, policy, event, next, next_policy, action) in cases {
        assert_eq!(
            step(state, policy, event),
            (next, next_policy, action),
            "{state:?} {policy:?} {event:?}"
        );
    }
}

#[test]
fn a_change_counts_once_two_runs_see_it() {
    let at = |secs| Meta {
        modified: SystemTime::UNIX_EPOCH + Duration::from_secs(secs),
        len: 8,
    };
    let synced = at(1);
    let fresh = Watch::default();
    assert_eq!(debounce(synced, &fresh, synced), Debounce::Quiet);
    assert_eq!(debounce(synced, &fresh, at(2)), Debounce::Pending(at(2)));
    let seen_once = Watch {
        pending: Some(at(2)),
        handled: None,
    };
    assert_eq!(
        debounce(synced, &seen_once, at(2)),
        Debounce::Settled(at(2))
    );
    // Still being written: a different change starts the count again.
    assert_eq!(
        debounce(synced, &seen_once, at(3)),
        Debounce::Pending(at(3))
    );
    let acted_on = Watch {
        pending: None,
        handled: Some(at(2)),
    };
    assert_eq!(debounce(synced, &acted_on, at(2)), Debounce::Quiet);
}

#[test]
fn a_save_is_a_remote_edit_on_the_second_run_that_sees_it() {
    let host = FakeHost::new();
    let (mut files, key, local) = fetched(&host);
    assert!(run(&mut files).is_empty(), "nothing changed");

    std::fs::write(&local, b"# Edited notes\n").unwrap();
    assert!(run(&mut files).is_empty(), "one run is not enough");
    assert_eq!(files.copies[&key].state, SyncState::Synced);
    assert_eq!(run(&mut files), vec![(key.clone(), Action::Ask)]);
    assert_eq!(
        files.copies[&key].state,
        SyncState::Asking {
            sha256: sha(b"# Edited notes\n")
        }
    );
    // The save already acted on stays quiet.
    assert!(run(&mut files).is_empty());
    assert!(run(&mut files).is_empty());
    assert!(host.received().is_empty(), "nothing is pushed unasked");
}

#[test]
fn a_touch_that_keeps_the_host_s_bytes_is_no_remote_edit() {
    let host = FakeHost::new();
    let (mut files, key, local) = fetched(&host);
    let synced = files.copies[&key].synced_meta;
    let file = std::fs::File::options().write(true).open(&local).unwrap();
    file.set_modified(synced.modified + Duration::from_secs(5))
        .unwrap();
    drop(file);

    assert!(run(&mut files).is_empty());
    assert_eq!(run(&mut files), vec![(key.clone(), Action::Nothing)]);
    let copy = &files.copies[&key];
    assert_eq!(copy.state, SyncState::Synced);
    assert_eq!(
        copy.synced_meta.modified,
        synced.modified + Duration::from_secs(5)
    );
    assert!(
        run(&mut files).is_empty(),
        "the touch is now the synced state"
    );
}

#[test]
fn a_copy_that_is_gone_is_skipped_until_it_returns() {
    let host = FakeHost::new();
    let (mut files, key, local) = fetched(&host);
    std::fs::write(&local, b"# Edited notes\n").unwrap();
    assert!(run(&mut files).is_empty());
    std::fs::remove_file(&local).unwrap();
    assert!(run(&mut files).is_empty());
    assert_eq!(files.copies[&key].state, SyncState::Synced);
    std::fs::write(&local, b"# Edited notes\n").unwrap();
    // Back again, the save counts once, when it has settled.
    let second = run(&mut files);
    let third = run(&mut files);
    assert_eq!([second, third].concat(), vec![(key, Action::Ask)]);
}

#[test]
fn a_remote_push_sends_the_copy_and_hashes_what_it_sent() {
    let host = FakeHost::new();
    let (files, _, local) = fetched(&host);
    let expect = Expect::Sha256(sha(NOTES));
    let pushed = push_copy(
        &files.io,
        &remote(PATH),
        &local,
        (expect, RemoteFileAccess::Keep),
        1024,
    )
    .unwrap();
    assert_eq!(pushed.sha256, sha(NOTES));
    assert_eq!(
        pushed.meta,
        files.copies.values().next().unwrap().synced_meta
    );
    assert_eq!(
        host.received(),
        vec![Received {
            path: PATH.into(),
            expect,
            like: None,
            bytes: NOTES.to_vec(),
        }]
    );
}

#[test]
fn a_remote_copy_over_the_bound_is_not_pushed() {
    let host = FakeHost::new();
    let (files, _, local) = fetched(&host);
    let limit = NOTES.len() as u64 - 1;
    let Err(error) = push_copy(
        &files.io,
        &remote(PATH),
        &local,
        (Expect::Any, RemoteFileAccess::Keep),
        limit,
    ) else {
        panic!("a copy over the bound was pushed");
    };
    assert!(
        matches!(error, Error::TooLarge { limit: l } if l == limit),
        "{error:?}"
    );
    assert!(host.received().is_empty());
    let error = Error::TooLarge {
        limit: MAX_REMOTE_FILE_BYTES,
    };
    assert_eq!(
        crate::Error::from(error).to_string(),
        "Not uploaded: the copy is larger than 64 MB"
    );
    // At the bound exactly, it goes.
    let at_bound = NOTES.len() as u64;
    push_copy(
        &files.io,
        &remote(PATH),
        &local,
        (Expect::Any, RemoteFileAccess::Keep),
        at_bound,
    )
    .unwrap();
}

#[test]
fn the_remote_hashing_reader_counts_and_hashes_what_it_reads() {
    let mut reader = Hashing::new(NOTES);
    let mut read = Vec::new();
    reader.read_to_end(&mut read).unwrap();
    assert_eq!(read, NOTES);
    assert_eq!(reader.finish(), (NOTES.len() as u64, sha(NOTES)));
}

/// I/O from `host` whose push first runs `meanwhile` on the copy at
/// `local`, as an editor saving during the upload would.
fn saving_during_push(host: &FakeHost, local: PathBuf, meanwhile: fn(&Path)) -> Io {
    let io = host.io(notes);
    let push = io.push.clone();
    Io {
        push: Arc::new(move |remote, expect, access, len, input| {
            meanwhile(&local);
            push(remote, expect, access, len, input)
        }),
        ..io
    }
}

#[test]
fn a_remote_copy_that_grows_during_its_push_sends_the_length_it_had() {
    let host = FakeHost::new();
    let (_, _, local) = fetched(&host);
    let io = saving_during_push(&host, local.clone(), |local| {
        let mut file = std::fs::File::options().append(true).open(local).unwrap();
        file.write_all(b"more").unwrap();
    });
    let pushed = push_copy(
        &io,
        &remote(PATH),
        &local,
        (Expect::Any, RemoteFileAccess::Keep),
        1024,
    )
    .unwrap();
    assert_eq!(host.received()[0].bytes, NOTES, "the stat'ed length only");
    assert_eq!(pushed.sha256, sha(NOTES), "the hash of the bytes sent");
    assert_eq!(pushed.meta.len, NOTES.len() as u64);
}

#[test]
fn a_remote_copy_that_shrinks_during_its_push_is_cut_off() {
    let host = FakeHost::new();
    let (_, _, local) = fetched(&host);
    let io = saving_during_push(&host, local.clone(), |local| {
        std::fs::write(local, b"#").unwrap();
    });
    let Err(error) = push_copy(
        &io,
        &remote(PATH),
        &local,
        (Expect::Any, RemoteFileAccess::Keep),
        1024,
    ) else {
        panic!("a cut-off push succeeded");
    };
    assert!(
        matches!(
            error,
            Error::Push(herdr_client::Error::RemoteFile(
                herdr_client::RemoteFileError::Incomplete
            ))
        ),
        "{error:?}"
    );
    assert_eq!(
        crate::Error::from(error).to_string(),
        "Could not upload the file: the upload was cut off; the host's file is unchanged"
    );
}

#[test]
fn a_remote_save_made_while_a_click_fetches_is_kept() {
    let host = FakeHost::new();
    let (mut files, key, local) = fetched(&host);
    let synced = files.copies[&key].synced_meta;
    let owner = files.owner;
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
    let outcome = std::thread::scope(|scope| {
        let fetching = scope.spawn(|| fetch_copy(&gated, owner, &remote(PATH), Some(synced)));
        on_start.recv().unwrap();
        std::fs::write(&local, b"# Saved meanwhile\n").unwrap();
        release.send(()).unwrap();
        fetching.join().unwrap()
    });
    assert!(matches!(outcome, Ok(Fetch::KeptEdits)));
    assert_eq!(std::fs::read(&local).unwrap(), b"# Saved meanwhile\n");
    assert_eq!(
        std::fs::read_dir(local.parent().unwrap()).unwrap().count(),
        1,
        "no .part left"
    );
    assert!(run(&mut files).is_empty());
    assert_eq!(run(&mut files), vec![(key, Action::Ask)]);

    // Unsaved since its sync, a copy takes the host's bytes.
    let host = FakeHost::new();
    let (files, key, local) = fetched(&host);
    host.change_file(b"# Newer\n");
    let synced = Some(files.copies[&key].synced_meta);
    let written = fetch_copy(&files.io, files.owner, &remote(PATH), synced).unwrap();
    assert!(matches!(written, Fetch::Written(_)));
    assert_eq!(std::fs::read(&local).unwrap(), b"# Newer\n");
}

#[test]
fn a_failed_remote_push_says_why() {
    let error = Error::Push(conflict().unwrap_err());
    assert!(error.is_conflict());
    assert_eq!(
        crate::Error::from(error).to_string(),
        "Could not upload the file: the host's file changed since it was fetched"
    );
    let missing = Error::Push(missing().unwrap_err());
    assert!(!missing.is_conflict());
}

#[test]
fn a_push_that_cannot_keep_the_file_s_access_says_nothing_changed() {
    let host = FakeHost::new();
    let (files, _, local) = fetched(&host);
    let io = Io {
        push: Arc::new(|_, _, _, _, _| {
            Err(herdr_client::Error::RemoteFile(
                herdr_client::RemoteFileError::Access,
            ))
        }),
        ..files.io.clone()
    };
    let rule = (Expect::Any, RemoteFileAccess::Keep);
    let Err(error) = push_copy(&io, &remote(PATH), &local, rule, 1024) else {
        panic!("a push that could not keep the access succeeded");
    };
    assert!(matches!(error, Error::Access), "{error:?}");
    assert!(!error.is_conflict());
    assert_eq!(
        crate::Error::from(error).to_string(),
        "Could not keep the file's access; nothing was changed."
    );
}

#[test]
fn the_remote_copies_are_listed_with_their_state() {
    let host = FakeHost::new();
    let (mut files, key, local) = fetched(&host);
    files.copies.get_mut(&key).unwrap().policy = Policy::Always;
    let listed: Vec<_> = files.list().collect();
    assert_eq!(
        listed,
        vec![Listed {
            target: "me@devbox",
            remote_path: PATH,
            local: &local,
            state: SyncState::Synced,
            policy: Policy::Always,
        }]
    );
}

#[test]
fn a_remote_copy_with_edits_is_reopened_rather_than_fetched() {
    let host = FakeHost::new();
    let (mut files, key, _) = fetched(&host);
    assert!(files.fetch_again(&remote(PATH)));
    let h = sha(b"edit");
    for state in [
        SyncState::Edited { sha256: h },
        SyncState::Asking { sha256: h },
        SyncState::Uploading { sha256: h },
        SyncState::Conflict { sha256: h },
        SyncState::Declined { sha256: h },
    ] {
        files.copies.get_mut(&key).unwrap().state = state;
        assert!(!files.fetch_again(&remote(PATH)), "{state:?}");
    }
    assert!(files.fetch_again(&remote("/elsewhere")), "no copy yet");
}
