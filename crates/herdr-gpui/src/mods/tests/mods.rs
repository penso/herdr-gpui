use super::*;

#[test]
fn mods_in_the_folder_start_and_stop_with_it() {
    let root = tempfile::tempdir().unwrap();
    write_mod(root.path(), "one", &counter("one"));
    write_mod(root.path(), "two", &counter("two"));
    fs::create_dir(root.path().join("not-a-mod")).unwrap();
    let mut mods = Mods::in_dir(root.path().to_owned());
    let config = Config::default();
    eventually(|| {
        mods.poll(&config);
        let drawn = mods
            .entries()
            .filter(|(_, entry)| entry.tree.is_some())
            .count();
        (drawn == 2).then_some(())
    });
    let names: Vec<_> = mods.entries().map(|(name, _)| name.to_owned()).collect();
    assert_eq!(names, ["one", "two"]);
    assert_eq!(
        mods.get("one").unwrap().places(),
        [herdr_mods::Place::StatusBar]
    );

    fs::remove_dir_all(root.path().join("two")).unwrap();
    eventually(|| {
        mods.poll(&config);
        (mods.entries().count() == 1).then_some(())
    });
}

#[test]
fn disabled_mods_and_turning_mods_off_stop_them() {
    let root = tempfile::tempdir().unwrap();
    write_mod(root.path(), "one", &counter("one"));
    write_mod(root.path(), "two", &counter("two"));
    let mut mods = Mods::in_dir(root.path().to_owned());
    let config = Config {
        disabled: vec!["two".into()],
        ..Config::default()
    };
    eventually(|| {
        mods.poll(&config);
        (mods.entries().count() == 1).then_some(())
    });
    assert!(mods.get("one").is_some());
    let off = Config {
        enabled: false,
        ..Config::default()
    };
    let polled = mods.poll(&off);
    assert!(polled.changed);
    assert_eq!(mods.entries().count(), 0);
}

#[test]
fn state_is_offered_only_when_it_changed_and_paced() {
    let root = tempfile::tempdir().unwrap();
    write_mod(root.path(), "one", &counter("one"));
    let mut mods = Mods::in_dir(root.path().to_owned());
    let start = Instant::now();
    assert!(!mods.wants_state(start), "no mod runs yet");
    eventually(|| {
        mods.poll(&Config::default());
        (mods.entries().count() == 1).then_some(())
    });
    assert!(mods.wants_state(start));
    mods.offer(State::default(), start);
    assert!(!mods.wants_state(start + Duration::from_millis(100)));
    assert!(mods.wants_state(start + STATE_GAP));
}

#[test]
fn turned_off_mods_leave_their_slot_to_the_next_by_name() {
    let root = tempfile::tempdir().unwrap();
    let names: Vec<String> = (0..=MAX_MODS).map(|index| format!("m{index:02}")).collect();
    for name in &names {
        let dir = root.path().join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("mod.toml"),
            format!("name = \"{name}\"\napi = 1\n"),
        )
        .unwrap();
    }
    // The scanner lists every folder by name, whatever the disk's order.
    let listed: Vec<String> = scan::list(root.path()).into_iter().collect();
    assert_eq!(listed, names);

    let mut mods = Mods::in_dir(root.path().to_owned());
    let config = Config {
        disabled: vec!["m00".into()],
        ..Config::default()
    };
    eventually(|| {
        mods.poll(&config);
        (mods.entries().count() == MAX_MODS).then_some(())
    });
    assert!(mods.get("m00").is_none());
    assert!(
        mods.get(&names[MAX_MODS]).is_some(),
        "the last mod took the freed slot"
    );
}

#[test]
fn sending_to_an_unknown_mod_is_refused() {
    let mods = Mods::in_dir(PathBuf::from("/nonexistent"));
    assert!(matches!(
        mods.send("ghost", Data::Nil),
        Err(Error::NotRunning(name)) if name == "ghost"
    ));
}
