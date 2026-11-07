use super::*;

const HOST: &str = "ssh:0123456789abcdef";

fn on(session: &str) -> Daemon {
    Daemon {
        endpoint: HOST.into(),
        session: session.into(),
    }
}

#[test]
fn a_muted_device_silences_everything_it_shows_in_every_session() {
    let shown = snapshot("boot", ["one", "two"]);
    let mut mutes = Mutes::default();
    assert!(mutes.is_empty());
    assert!(mutes.set_host(HOST, true));
    assert!(!mutes.is_empty());
    for session in ["default", "other"] {
        let silenced = mutes.silenced(&on(session), &shown);
        assert!(!silenced.is_empty());
        assert!(silenced.host() && silenced.any_workspace());
        assert!(silenced.tab("w2:t1") && silenced.pane("w2:p1"));
        assert!(
            silenced.notice(None, None, None),
            "a notice naming no place"
        );
    }
    assert!(mutes.silenced(&local(), &shown).is_empty(), "other devices");
}

#[test]
fn a_device_mute_leaves_a_workspace_or_tab_mute_as_its_own() {
    let shown = snapshot("boot", ["one", "two"]);
    let daemon = on("default");
    let mut mutes = Mutes::default();
    mutes.set_host(HOST, true);
    let silenced = mutes.silenced(&daemon, &shown);
    assert!(!silenced.workspace("w1") && !silenced.own_tab("w1:t1"));

    assert!(mutes.set(&daemon, &shown, Scope::Tab, "w1:t1", true, at(MUTED_AT)));
    assert!(mutes.set_host(HOST, false));
    let silenced = mutes.silenced(&daemon, &shown);
    assert!(silenced.tab("w1:t1") && !silenced.tab("w1:t2"));
}

#[test]
fn only_real_changes_count() {
    let mut mutes = Mutes::default();
    assert!(!mutes.set_host(HOST, false));
    assert!(mutes.set_host(HOST, true));
    let revision = mutes.revision();
    assert!(!mutes.set_host(HOST, true));
    assert_eq!(mutes.revision(), revision);
    assert!(mutes.set_host(HOST, false));
    assert!(mutes.is_empty());
    assert!(mutes.revision() > revision);
}

#[test]
fn a_device_mute_outlives_its_daemon_and_anything_a_snapshot_shows() {
    let first = snapshot("first", ["one", "two"]);
    let daemon = on("default");
    let mut mutes = Mutes::default();
    mutes.set_host(HOST, true);
    let mut empty = snapshot("second", ["one", "two"]);
    empty.workspaces.clear();
    empty.tabs.clear();
    empty.panes.clear();
    let later = at(MUTED_AT + 10 * FORGET_AFTER_SECONDS);
    assert!(mutes.current(&daemon, &empty, later));
    assert!(!mutes.reconcile(&daemon, &empty, later));
    assert!(mutes.silenced(&daemon, &first).host());
}

#[test]
fn an_id_that_cannot_be_kept_is_refused() {
    let mut mutes = Mutes::default();
    assert!(!mutes.set_host("", true));
    assert!(!mutes.set_host("bad\nid", true));
    assert!(!mutes.set_host(&"x".repeat(MAX_FIELD_BYTES + 1), true));
    assert!(mutes.is_empty());
}

#[test]
fn the_oldest_device_goes_first_beyond_the_bound() {
    let mut mutes = Mutes::default();
    for index in 0..=MAX_HOSTS {
        assert!(mutes.set_host(&format!("ssh:{index}"), true));
    }
    assert!(!mutes.host("ssh:0"));
    assert!(mutes.host("ssh:1") && mutes.host(&format!("ssh:{MAX_HOSTS}")));
}

#[test]
fn saved_devices_load_back_and_an_older_file_has_none() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE);
    let mut mutes = Mutes::at(Some(path.clone()));
    mutes.set_host(HOST, true);
    mutes.finish();
    assert!(Mutes::at(Some(path.clone())).host(HOST));

    std::fs::write(&path, br#"{"mutes":[]}"#).unwrap();
    assert!(Mutes::at(Some(path.clone())).is_empty());

    std::fs::write(&path, br#"{"mutes":[],"hosts":[""]}"#).unwrap();
    assert!(matches!(
        parse(&std::fs::read(&path).unwrap()),
        Err(Error::Invalid)
    ));
    assert!(Mutes::at(Some(path)).is_empty());
}

#[test]
fn the_largest_set_of_devices_fits_in_the_file() {
    let saved = Saved {
        mutes: Vec::new(),
        hosts: (0..MAX_HOSTS)
            .map(|index| format!("{index:\"<MAX_FIELD_BYTES$}"))
            .collect(),
    };
    let bytes = serde_json::to_vec(&saved).unwrap();
    assert!(bytes.len() as u64 <= MAX_FILE_BYTES);
    assert_eq!(parse(&bytes).unwrap().hosts.len(), MAX_HOSTS);
}

#[test]
fn a_muted_device_stays_silent_without_a_snapshot() {
    let mut mutes = Mutes::default();
    mutes.set_host(HOST, true);
    assert!(mutes.silenced_offline(HOST).notice(None, None, None));
    assert!(mutes.silenced_offline("local").is_empty());
}
