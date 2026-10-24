use super::*;

const DAY: u64 = 24 * 60 * 60;

fn pane(id: &str) -> Key {
    Key::Pane {
        endpoint: "local".into(),
        boot: "boot".into(),
        id: id.into(),
    }
}

#[test]
fn boost_grows_with_visits_and_stays_bounded() {
    let mut frecency = Frecency::default();
    assert_eq!(frecency.boost(&pane("a"), 0), 0);
    frecency.visit(pane("a"), 0);
    let once = frecency.boost(&pane("a"), 0);
    assert!(once > 0);
    frecency.visit(pane("a"), 0);
    assert!(frecency.boost(&pane("a"), 0) > once);
    for _ in 0..10_000 {
        frecency.visit(pane("a"), 0);
    }
    assert_eq!(frecency.boost(&pane("a"), 0), MAX_BOOST);
    assert_eq!(frecency.boost(&pane("b"), 0), 0);
}

#[test]
fn old_visits_decay_and_a_backwards_clock_does_not_inflate_them() {
    let mut frecency = Frecency::default();
    for _ in 0..4 {
        frecency.visit(pane("a"), 30 * DAY);
    }
    let fresh = frecency.boost(&pane("a"), 30 * DAY);
    assert!(frecency.boost(&pane("a"), 37 * DAY) < fresh);
    assert!(frecency.boost(&pane("a"), 400 * DAY) < frecency.boost(&pane("a"), 37 * DAY));
    // Before the visit, as after a clock change, it counts as made now.
    assert_eq!(frecency.boost(&pane("a"), DAY), fresh);
    // Recency beats an old habit: one visit today outranks four a year ago.
    frecency.visit(pane("b"), 400 * DAY);
    assert!(frecency.boost(&pane("b"), 400 * DAY) > frecency.boost(&pane("a"), 400 * DAY));
}

#[test]
fn the_least_frecent_key_goes_first_beyond_the_bound() {
    let mut frecency = Frecency::default();
    frecency.visit(pane("old"), 0);
    for index in 0..MAX_KEYS {
        frecency.visit(pane(&index.to_string()), 100 * DAY);
    }
    assert_eq!(frecency.visits.len(), MAX_KEYS);
    assert_eq!(frecency.boost(&pane("old"), 100 * DAY), 0);
    assert!(frecency.boost(&pane("0"), 100 * DAY) > 0);
}

#[test]
fn keys_too_large_or_empty_are_not_remembered() {
    let mut frecency = Frecency::default();
    frecency.visit(pane(&"x".repeat(MAX_FIELD_BYTES + 1)), 0);
    frecency.visit(pane(""), 0);
    frecency.visit(
        Key::Workspace {
            endpoint: "local".into(),
            root: "/repo".into(),
            branch: Some(String::new()),
        },
        0,
    );
    assert!(frecency.visits.is_empty());
}

#[test]
fn saved_history_loads_back_and_damaged_files_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE);
    let project = Key::Project {
        path: "/code/herdr".into(),
    };
    let workspace = Key::Workspace {
        endpoint: "local".into(),
        root: "/code/herdr".into(),
        branch: Some("main".into()),
    };
    let mut frecency = Frecency::at(Some(path.clone()));
    frecency.visit(project.clone(), DAY);
    frecency.visit(workspace.clone(), DAY);
    frecency.visit(workspace.clone(), DAY);
    frecency.writer.take().unwrap().finish();
    let reopened = Frecency::at(Some(path.clone()));
    assert_eq!(reopened.visits, frecency.visits);
    assert!(reopened.boost(&workspace, DAY) > reopened.boost(&project, DAY));
    drop(reopened);

    for damaged in [
        "not json".to_owned(),
        r#"{"visits":[{"key":{"kind":"project","path":"/p"},"score":-1,"at":0}]}"#.to_owned(),
        r#"{"visits":[{"key":{"kind":"pane","endpoint":"","boot":"b","id":"p"},"score":1,"at":0}]}"#
            .to_owned(),
    ] {
        std::fs::write(&path, damaged).unwrap();
        assert!(Frecency::at(Some(path.clone())).visits.is_empty());
    }
    assert!(matches!(
        parse(br#"{"visits":[{"key":{"kind":"project","path":"/p"},"score":-1,"at":0}]}"#),
        Err(crate::Error::Palette(
            crate::palette::Error::InvalidFrecency
        ))
    ));
}

#[test]
fn the_largest_history_the_store_accepts_reads_back() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE);
    let field = |index: usize| {
        let mut text = index.to_string();
        text.push_str(&"\"".repeat(MAX_FIELD_BYTES - text.len()));
        text
    };
    let mut frecency = Frecency::at(Some(path.clone()));
    // Filled directly and saved once; a visit saves the whole history.
    frecency.visits = (0..MAX_KEYS)
        .map(|index| Visit {
            key: Key::Workspace {
                endpoint: field(index),
                root: field(index),
                branch: Some(field(index)),
            },
            score: f64::MAX,
            at: u64::MAX,
        })
        .collect();
    assert!(frecency.visits.iter().all(Visit::valid));
    frecency.save();
    frecency.writer.take().unwrap().finish();
    assert_eq!(Frecency::at(Some(path)).visits.len(), MAX_KEYS);
}
