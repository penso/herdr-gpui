use super::*;
use std::ffi::OsString;

fn record(endpoint: &str) -> Record {
    Record {
        adapter: false,
        agent: Agent::Claude,
        cwd: PathBuf::from("/work/repo"),
        endpoint: endpoint.parse().unwrap(),
        pane: "w1:p3".to_owned(),
        session: "0c6f0c39-3c1e-4b7e-9a51-6d1b8e2d7f10".to_owned(),
        socket: PathBuf::from("/home/me/.config/herdr/herdr.sock"),
    }
}

const ONE: &str = "0123456789abcdef0123456789abcdef";
const TWO: &str = "fedcba9876543210fedcba9876543210";

#[test]
fn a_record_round_trips_through_its_file() {
    let dir = tempfile::tempdir().unwrap();
    let dir = dir.path().join("restore");
    let written = record(ONE);
    written.write(&dir).unwrap();
    let path = record_path(&dir, &written.endpoint);
    assert_eq!(Record::read(&path).unwrap(), written);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&dir), 0o700);
        assert_eq!(mode(&path), 0o600);
    }
    // No temporary file is left beside it.
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
}

#[test]
fn records_that_could_type_anything_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    for session in [
        "",
        "-rf",
        "a b",
        "a'b",
        "a;b",
        "a$b",
        "a\nb",
        &"x".repeat(201),
    ] {
        let mut bad = record(ONE);
        bad.session = session.to_owned();
        assert!(
            matches!(bad.write(dir.path()), Err(Error::Session)),
            "{session:?}"
        );
    }
    let mut bad = record(ONE);
    bad.pane = "w1\u{7}".to_owned();
    assert!(matches!(bad.validate(), Err(Error::Pane)));
    let mut bad = record(ONE);
    bad.cwd = PathBuf::from("relative");
    assert!(matches!(bad.validate(), Err(Error::Path)));
    assert!(session_id_is_plain("ses_01JX:abc@2+1.x-y"));
}

#[test]
fn a_record_of_another_version_or_agent_does_not_read() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(format!("{ONE}.json"));
    let mut stored = serde_json::to_value(Stored::from(&record(ONE))).unwrap();
    stored["version"] = 2.into();
    fs::write(&path, stored.to_string()).unwrap();
    assert!(matches!(Record::read(&path), Err(Error::Version(2))));
    stored["version"] = VERSION.into();
    stored["agent"] = "nope".into();
    fs::write(&path, stored.to_string()).unwrap();
    assert!(matches!(Record::read(&path), Err(Error::Agent)));
    fs::write(&path, vec![b' '; MAX_RECORD_SIZE as usize + 1]).unwrap();
    assert!(matches!(Record::read(&path), Err(Error::Size)));
    // The listing skips what does not read rather than failing.
    assert!(list(dir.path(), None).records.is_empty());
}

#[test]
fn listing_reads_records_with_their_seen_boot() {
    let dir = tempfile::tempdir().unwrap();
    record(ONE).write(dir.path()).unwrap();
    record(TWO).write(dir.path()).unwrap();
    let one: EndpointId = ONE.parse().unwrap();
    write_seen(dir.path(), &one, "boot-1").unwrap();
    // A stamp for a chat with no record is not written.
    let missing: EndpointId = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".parse().unwrap();
    write_seen(dir.path(), &missing, "boot-1").unwrap();
    assert!(!seen_path(dir.path(), &missing).exists());
    assert!(matches!(write_seen(dir.path(), &one, ""), Err(Error::Boot)));

    let mut listed = list(dir.path(), None).records;
    listed.sort_by(|a, b| a.0.endpoint.as_str().cmp(b.0.endpoint.as_str()));
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].1.as_deref(), Some("boot-1"));
    assert_eq!(listed[1].1, None);
}

#[test]
fn only_one_client_claims_a_record() {
    let dir = tempfile::tempdir().unwrap();
    record(ONE).write(dir.path()).unwrap();
    let id: EndpointId = ONE.parse().unwrap();
    assert!(claim(dir.path(), &id).unwrap());
    assert!(!claim(dir.path(), &id).unwrap());
    // A claimed record is no longer listed, so no other pass sees it.
    assert!(list(dir.path(), None).records.is_empty());
    unclaim(dir.path(), &id).unwrap();
    assert_eq!(list(dir.path(), None).records.len(), 1);

    write_seen(dir.path(), &id, "boot-1").unwrap();
    assert!(claim(dir.path(), &id).unwrap());
    remove(dir.path(), &id);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn the_restore_directory_sits_beside_the_sockets() {
    let env = |pairs: &'static [(&'static str, &'static str)]| {
        move |name: &str| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| OsString::from(value))
        }
    };
    assert_eq!(
        restore_dir_with(env(&[("XDG_STATE_HOME", "/state"), ("HOME", "/home/me")])),
        Some(PathBuf::from("/state/herdr/acp/restore"))
    );
    assert_eq!(
        restore_dir_with(env(&[("HOME", "/home/me")])),
        Some(PathBuf::from("/home/me/.local/state/herdr/acp/restore"))
    );
    assert_eq!(
        restore_dir_with(env(&[("USERPROFILE", "/Users/me")])),
        Some(PathBuf::from("/Users/me/.local/state/herdr/acp/restore"))
    );
    assert_eq!(restore_dir_with(env(&[])), None);
}

#[test]
fn pruning_drops_only_what_no_client_can_use() {
    use std::time::{Duration, SystemTime};
    let dir = tempfile::tempdir().unwrap();
    let dir = dir.path();
    let [seen, unseen, claimed, dangling] = [
        ONE,
        TWO,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    ]
    .map(|id| id.parse::<EndpointId>().unwrap());
    record(seen.as_str()).write(dir).unwrap();
    write_seen(dir, &seen, "boot-1").unwrap();
    record(unseen.as_str()).write(dir).unwrap();
    record(claimed.as_str()).write(dir).unwrap();
    assert!(claim(dir, &claimed).unwrap());
    fs::write(seen_path(dir, &dangling), "boot-1").unwrap();

    // Fresh files stay, apart from a stamp whose record is gone.
    prune(dir, SystemTime::now());
    assert!(!seen_path(dir, &dangling).exists());
    assert!(record_path(dir, &unseen).exists());
    assert!(claimed_path(dir, &claimed).exists());

    // A week on, the unseen record goes, and the stale claim is given back;
    // a record some client saw live stays however old it is.
    prune(
        dir,
        SystemTime::now() + UNSEEN_AGE + Duration::from_secs(60),
    );
    assert!(!record_path(dir, &unseen).exists());
    assert!(!claimed_path(dir, &claimed).exists());
    assert!(record_path(dir, &claimed).exists());
    assert!(record_path(dir, &seen).exists());
    assert!(seen_path(dir, &seen).exists());
}

#[test]
fn a_stale_claim_is_given_back_but_a_fresh_one_is_kept() {
    use std::time::{Duration, SystemTime};
    let dir = tempfile::tempdir().unwrap();
    let dir = dir.path();
    let id: EndpointId = ONE.parse().unwrap();
    record(ONE).write(dir).unwrap();
    // The record is old; the claim is dated when it is made.
    let old = SystemTime::now() - CLAIM_STALE * 10;
    fs::File::options()
        .write(true)
        .open(record_path(dir, &id))
        .unwrap()
        .set_modified(old)
        .unwrap();
    assert!(claim(dir, &id).unwrap());
    prune(dir, SystemTime::now());
    assert!(claimed_path(dir, &id).exists(), "a claim in use is kept");

    // A client that exited or crashed holding it: given back for a later
    // pass, never deleted.
    prune(
        dir,
        SystemTime::now() + CLAIM_STALE + Duration::from_secs(1),
    );
    assert!(!claimed_path(dir, &id).exists());
    assert_eq!(list(dir, None).records.len(), 1);

    // A claim beside a record of the same chat is a leftover; it goes.
    assert!(claim(dir, &id).unwrap());
    record(ONE).write(dir).unwrap();
    prune(
        dir,
        SystemTime::now() + CLAIM_STALE + Duration::from_secs(1),
    );
    assert!(!claimed_path(dir, &id).exists());
    assert!(record_path(dir, &id).exists());
}

#[test]
fn listing_reads_on_round_the_directory_so_no_record_stays_hidden() {
    let dir = tempfile::tempdir().unwrap();
    let total = MAX_RECORDS + 3;
    for n in 0..total {
        record(&format!("{n:032x}")).write(dir.path()).unwrap();
    }
    let mut reached = std::collections::HashSet::new();
    let mut after: Option<EndpointId> = None;
    for _ in 0..2 {
        let listing = list(dir.path(), after.as_ref());
        assert_eq!(listing.records.len(), MAX_RECORDS);
        assert!(!listing.complete());
        reached.extend(
            listing
                .records
                .into_iter()
                .map(|(record, _)| record.endpoint),
        );
        after = listing.next;
    }
    assert_eq!(reached.len(), total, "two calls reach every record");

    // A directory within the limit is read whole, every time.
    let small = tempfile::tempdir().unwrap();
    record(ONE).write(small.path()).unwrap();
    let listing = list(small.path(), Some(&TWO.parse().unwrap()));
    assert!(listing.complete());
    assert_eq!(listing.records.len(), 1);
}

#[test]
fn concurrent_stamps_of_one_chat_never_leave_a_partial_file() {
    let dir = tempfile::tempdir().unwrap();
    record(ONE).write(dir.path()).unwrap();
    let id: EndpointId = ONE.parse().unwrap();
    let boots: Vec<String> = (0..8)
        .map(|n| format!("boot-{n}-{}", "x".repeat(64)))
        .collect();
    std::thread::scope(|scope| {
        for boot in &boots {
            let (dir, id, boots) = (dir.path(), &id, &boots);
            scope.spawn(move || {
                for _ in 0..50 {
                    write_seen(dir, id, boot).unwrap();
                    let read = read_seen(dir, id).unwrap();
                    assert!(boots.contains(&read), "{read:?}");
                }
            });
        }
    });
    // No temporary file is left behind.
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
}
