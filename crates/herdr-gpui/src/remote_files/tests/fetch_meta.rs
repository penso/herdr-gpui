//! The metadata a fetch records as its copy's synced state, against a save
//! made the moment the copy lands.

use super::*;

const PATH: &str = "/home/me/notes.md";
/// Longer than [`NOTES`], so the save changes the copy's length even when
/// the clock has not moved on.
const EDITED: &[u8] = b"# Notes, saved as the copy landed\n";

#[test]
fn a_save_as_the_fetched_copy_lands_asks_to_be_uploaded() {
    let host = FakeHost::new();
    let mut files = RemoteFiles::default();
    host.serve(&mut files, notes);
    files.io.renamed = Arc::new(|local| std::fs::write(local, EDITED).unwrap());
    let done = fetch_fresh(&files.io, files.owner, &remote(PATH)).unwrap();
    assert_eq!(
        done.copy.synced_meta.len,
        NOTES.len() as u64,
        "the synced metadata is the downloaded bytes', not the save's"
    );
    let key = done.key.clone();
    files.record(done);

    // Two watch runs settle the save, which then asks to be uploaded.
    let mut actions = Vec::new();
    for _ in 0..2 {
        let seen = watch::scan(files.probes());
        actions.extend(files.apply_scan(seen));
    }
    assert_eq!(actions, vec![(key.clone(), sync::Action::Ask)]);
    assert_eq!(
        files.copies[&key].state,
        SyncState::Asking {
            sha256: Sha256::digest(EDITED).into()
        }
    );
}
