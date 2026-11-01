use super::*;

fn host(n: u8, target: &str, session: &str) -> SavedHost {
    SavedHost {
        id: format!("{n:032x}"),
        label: format!("machine-{n}"),
        target: target.into(),
        session: session.into(),
        enabled: true,
    }
}

/// A target near the longest a profile may have.
fn long_target(n: u8) -> String {
    format!("me@{}-{n}", "h".repeat(1000))
}

#[test]
fn another_session_on_a_saved_target_is_added() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("endpoints.json");
    let default = host(1, "box", "default");
    store_host(&path, &default).unwrap();
    let work = host(2, "box", "work");
    // The same target and session under another ID is the same profile.
    let again = host(3, "box", "default");
    assert_eq!(add_hosts(&path, &[work.clone(), again]).unwrap(), 1);
    assert_eq!(load_catalog(&path).unwrap().ssh, vec![default, work]);
}

#[test]
fn adding_stops_at_the_catalogs_byte_limit() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("endpoints.json");
    let hosts: Vec<SavedHost> = (1..=60)
        .map(|n| host(n, &long_target(n), "default"))
        .collect();
    let added = add_hosts(&path, &hosts).unwrap();
    assert!(added > 0 && added < hosts.len(), "{added}");
    let saved = load_catalog(&path).unwrap().ssh;
    assert_eq!(saved, hosts[..added]);
    assert!(fs::metadata(&path).unwrap().len() <= 65536);
}
