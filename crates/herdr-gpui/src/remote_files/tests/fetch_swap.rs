//! A repeat fetch that keeps edits, against saves made at each step of
//! putting the host's bytes in place, and the watch runs and conflict
//! refetches that must leave a fetching copy alone.

use super::*;

const PATH: &str = "/home/me/notes.md";
/// Longer than [`NOTES`] and [`NEWER`], so a save changes the copy's length
/// even when the clock has not moved on.
const EDITED: &[u8] = b"# Notes, saved while the fetch ran\n";
/// What the host holds by the time the copy is fetched again.
const NEWER: &[u8] = b"# Notes, newer\n";

/// Window-less files holding a synced copy of `PATH`, fetched from `host`,
/// whose later fetches serve [`NEWER`].
fn synced(host: &FakeHost) -> (RemoteFiles, Key, PathBuf) {
    let mut files = RemoteFiles::default();
    host.serve(&mut files, notes);
    let done = fetch_fresh(&files.io, files.owner, &remote(PATH)).unwrap();
    let (key, local) = (done.key.clone(), done.copy.local.clone());
    files.record(done);
    host.change_file(NEWER);
    (files, key, local)
}

/// Fetches `PATH` again over `key`'s copy, keeping edits made since it was
/// synced, with `at` run at each step of putting it in place.
fn refetch(
    files: &mut RemoteFiles,
    key: &Key,
    at: impl Fn(Step, &Path) + Send + Sync + 'static,
) -> Result<Fetch> {
    files.io.step = Arc::new(at);
    let synced = files.copies[key].synced_meta;
    fetch_copy(&files.io, files.owner, &remote(PATH), Some(synced), None)
}

/// How many files the folder holding `local` has.
fn entries(local: &Path) -> usize {
    std::fs::read_dir(local.parent().unwrap()).unwrap().count()
}

/// One watch run, done in place.
fn run(files: &mut RemoteFiles) -> Vec<(Key, sync::Action)> {
    let seen = watch::scan(files.probes());
    files.apply_scan(seen)
}

#[test]
fn an_in_place_save_after_the_check_is_kept() {
    let host = FakeHost::new();
    let (mut files, key, local) = synced(&host);
    let fetched = refetch(&mut files, &key, |step, local| {
        if step == Step::Checked {
            std::fs::write(local, EDITED).unwrap();
        }
    });
    assert!(matches!(fetched, Ok(Fetch::KeptEdits)));
    assert_eq!(std::fs::read(&local).unwrap(), EDITED);
    assert_eq!(entries(&local), 1, "no part or kept file left");
}

#[test]
fn a_temp_and_rename_save_after_the_move_aside_is_kept() {
    let host = FakeHost::new();
    let (mut files, key, local) = synced(&host);
    let fetched = refetch(&mut files, &key, |step, local| {
        if step == Step::MovedAside {
            let temp = local.with_file_name("notes.md.save");
            std::fs::write(&temp, EDITED).unwrap();
            std::fs::rename(&temp, local).unwrap();
        }
    });
    assert!(matches!(fetched, Ok(Fetch::KeptEdits)));
    assert_eq!(std::fs::read(&local).unwrap(), EDITED);
    assert_eq!(entries(&local), 1, "the synced copy moved aside is gone");
}

#[test]
fn a_save_through_a_handle_opened_before_the_move_aside_is_kept() {
    let host = FakeHost::new();
    let (mut files, key, local) = synced(&host);
    let handle = Mutex::new(None::<std::fs::File>);
    let fetched = refetch(&mut files, &key, move |step, local| match step {
        Step::Checked => {
            let file = std::fs::OpenOptions::new().write(true).open(local).unwrap();
            *handle.lock().unwrap() = Some(file);
        }
        Step::MovedAside => {
            let mut file = handle.lock().unwrap().take().unwrap();
            file.write_all(EDITED).unwrap();
        }
        Step::Placed => {}
    });
    assert!(matches!(fetched, Ok(Fetch::KeptEdits)));
    assert_eq!(std::fs::read(&local).unwrap(), EDITED);
    assert_eq!(entries(&local), 1);
}

#[test]
fn a_save_after_the_copy_is_placed_asks_to_be_uploaded() {
    let host = FakeHost::new();
    let (mut files, key, local) = synced(&host);
    let fetched = refetch(&mut files, &key, |step, local| {
        if step == Step::Placed {
            std::fs::write(local, EDITED).unwrap();
        }
    });
    let Ok(Fetch::Written(done)) = fetched else {
        panic!("the host's bytes were not written");
    };
    assert_eq!(
        done.copy.synced_meta.len,
        NEWER.len() as u64,
        "the synced metadata is the downloaded bytes', not the save's"
    );
    assert_eq!(entries(&local), 1);
    files.record(*done);

    // Two watch runs settle the save, which then asks to be uploaded.
    let mut actions = Vec::new();
    for _ in 0..2 {
        actions.extend(run(&mut files));
    }
    assert_eq!(actions, vec![(key.clone(), sync::Action::Ask)]);
    assert_eq!(
        files.copies[&key].state,
        SyncState::Asking {
            sha256: Sha256::digest(EDITED).into()
        }
    );
}

#[test]
fn a_copy_deleted_before_the_fetch_is_written() {
    let host = FakeHost::new();
    let (mut files, key, local) = synced(&host);
    std::fs::remove_file(&local).unwrap();
    let fetched = refetch(&mut files, &key, |_, _| {});
    assert!(matches!(fetched, Ok(Fetch::Written(_))));
    assert_eq!(std::fs::read(&local).unwrap(), NEWER);
    assert_eq!(entries(&local), 1);
}

#[cfg(windows)]
#[test]
fn a_copy_held_open_without_delete_sharing_is_left_as_it_was() {
    use std::os::windows::fs::OpenOptionsExt;
    /// `FILE_SHARE_READ | FILE_SHARE_WRITE`, without `FILE_SHARE_DELETE`.
    const SHARE_READ_WRITE: u32 = 0x1 | 0x2;
    let host = FakeHost::new();
    let (mut files, key, local) = synced(&host);
    let held = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(SHARE_READ_WRITE)
        .open(&local)
        .unwrap();
    let fetched = refetch(&mut files, &key, |_, _| {});
    assert!(
        matches!(&fetched, Err(Error::Cache { .. })),
        "{:?}",
        fetched.err()
    );
    drop(held);
    assert_eq!(std::fs::read(&local).unwrap(), NOTES);
    assert_eq!(entries(&local), 1, "no part left");
}

#[test]
fn a_copy_being_fetched_is_not_watched_until_its_fetch_ends() {
    let host = FakeHost::new();
    let (mut files, key, local) = synced(&host);
    std::fs::write(&local, EDITED).unwrap();
    // A run that began before the fetch reports after it started.
    let seen = watch::scan(files.probes());
    files.fetching.insert(key.clone());
    assert_eq!(files.apply_scan(seen), vec![]);
    assert_eq!(files.copies[&key].watch, watch::Watch::default());
    for _ in 0..2 {
        assert_eq!(run(&mut files), vec![]);
    }
    assert_eq!(files.copies[&key].state, SyncState::Synced);

    files.fetching.remove(&key);
    let mut actions = Vec::new();
    for _ in 0..2 {
        actions.extend(run(&mut files));
    }
    assert_eq!(actions, vec![(key, sync::Action::Ask)]);
}

#[gpui::test]
fn a_conflict_refetch_holds_its_copy_while_it_runs(cx: &mut TestAppContext) {
    let host = FakeHost::new();
    let (view, cx) = window(&host, cx);
    view.update(cx, |view, cx| {
        view.open_remote_file(remote(PATH), None, None, None, cx)
    });
    cx.run_until_parked();
    let key: Key = ("me@devbox".into(), PATH.into());
    let conflict = SyncState::Conflict {
        sha256: Sha256::digest(EDITED).into(),
    };
    view.update(cx, |view, _| {
        view.remote_files.copies.get_mut(&key).unwrap().state = conflict;
        view.remote_files.io = host.io(missing);
    });

    // A refetch that fails lets go of the copy.
    view.update(cx, |view, cx| {
        view.resolve_remote_conflict(key.clone(), Resolution::Refetch, cx);
        assert!(view.remote_files.fetching.contains(&key), "held");
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(!view.remote_files.fetching.contains(&key), "released");
        assert_eq!(view.remote_files.copies[&key].state, conflict);
    });

    // As does one that succeeds.
    view.update(cx, |view, cx| {
        view.remote_files.io = host.io(notes);
        view.resolve_remote_conflict(key.clone(), Resolution::Refetch, cx);
        assert!(view.remote_files.fetching.contains(&key), "held");
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(!view.remote_files.fetching.contains(&key), "released");
        assert_eq!(view.remote_files.copies[&key].state, SyncState::Synced);
    });
    assert_eq!(host.asked().len(), 3);
}
