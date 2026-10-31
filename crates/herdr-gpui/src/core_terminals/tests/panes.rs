use crate::core_terminals::panes::{Axis, Direction, Error, MAX_PANES, Node, Panes, Share};
use herdr_backend::TerminalId;

fn id(name: &str) -> TerminalId {
    TerminalId::try_from(name.to_owned()).unwrap()
}

/// t1 on the left; t2 over t3 on the right.
fn three() -> Panes {
    let mut panes = Panes::single(id("t1"));
    panes.split(&id("t1"), Axis::Row, id("t2")).unwrap();
    panes.split(&id("t2"), Axis::Column, id("t3")).unwrap();
    panes
}

#[test]
fn split_puts_the_new_pane_after_its_parent() {
    let panes = three();
    assert_eq!(panes.terminals(), [&id("t1"), &id("t2"), &id("t3")]);
    let rects = panes.rects();
    assert_eq!(rects[0].1.width, 0.5);
    assert_eq!(rects[1].1.height, 0.5);
    assert_eq!(rects[2].1.y, 0.5);
}

#[test]
fn split_refuses_duplicates_missing_panes_and_more_than_the_limit() {
    let mut panes = three();
    assert_eq!(
        panes.split(&id("t1"), Axis::Row, id("t2")),
        Err(Error::Duplicate(id("t2")))
    );
    assert_eq!(
        panes.split(&id("t9"), Axis::Row, id("t4")),
        Err(Error::Missing(id("t9")))
    );
    for index in panes.len()..MAX_PANES {
        panes
            .split(&id("t1"), Axis::Row, id(&format!("n{index}")))
            .unwrap();
    }
    assert_eq!(panes.len(), MAX_PANES);
    assert_eq!(
        panes.split(&id("t1"), Axis::Row, id("over")),
        Err(Error::Full)
    );
}

#[test]
fn remove_lets_the_sibling_take_the_split() {
    let mut panes = three();
    assert!(panes.remove(&id("t2")));
    assert_eq!(panes.terminals(), [&id("t1"), &id("t3")]);
    assert_eq!(panes.rects()[1].1.height, 1.);
    assert!(!panes.remove(&id("t9")));
    assert!(panes.remove(&id("t1")));
    // The last pane stays.
    assert!(!panes.remove(&id("t3")));
    assert_eq!(panes.terminals(), [&id("t3")]);
}

#[test]
fn neighbors_follow_the_layout() {
    let panes = three();
    assert_eq!(panes.neighbor(&id("t1"), Direction::Right), Some(id("t2")));
    assert_eq!(panes.neighbor(&id("t3"), Direction::Left), Some(id("t1")));
    assert_eq!(panes.neighbor(&id("t2"), Direction::Down), Some(id("t3")));
    assert_eq!(panes.neighbor(&id("t3"), Direction::Up), Some(id("t2")));
    assert_eq!(panes.neighbor(&id("t1"), Direction::Left), None);
    assert_eq!(panes.neighbor(&id("t2"), Direction::Up), None);
}

#[test]
fn a_neighbor_must_touch_the_edge() {
    // t1 | t2 | t3: t3 is right of t1 but does not touch it.
    let mut panes = Panes::single(id("t1"));
    panes.split(&id("t1"), Axis::Row, id("t2")).unwrap();
    panes.split(&id("t2"), Axis::Row, id("t3")).unwrap();
    assert_eq!(panes.neighbor(&id("t1"), Direction::Right), Some(id("t2")));
    assert_eq!(panes.neighbor(&id("t2"), Direction::Right), Some(id("t3")));
}

#[test]
fn resize_moves_the_nearest_split_on_the_axis_within_bounds() {
    let mut panes = three();
    assert!(panes.resize(&id("t3"), Direction::Down));
    assert!((panes.rects()[2].1.y - 0.55).abs() < 1e-6);
    // t3 has no row split of its own; the outer one moves.
    assert!(panes.resize(&id("t3"), Direction::Left));
    assert!((panes.rects()[0].1.width - 0.45).abs() < 1e-6);
    for _ in 0..20 {
        panes.resize(&id("t1"), Direction::Left);
    }
    assert!(!panes.resize(&id("t1"), Direction::Left));
    assert!((panes.rects()[0].1.width - 0.1).abs() < 1e-6);
    let mut single = Panes::single(id("t1"));
    assert!(!single.resize(&id("t1"), Direction::Right));
}

#[test]
fn set_share_targets_the_split_at_a_path() {
    let mut panes = three();
    assert!(panes.set_share(&[true], Share::at(0.25)));
    assert!(!panes.set_share(&[true], Share::at(0.25)));
    assert!((panes.rects()[1].1.height - 0.25).abs() < 1e-6);
    // Paths past a leaf name no split.
    assert!(!panes.set_share(&[false], Share::at(0.3)));
    // Shares stay where both sides are usable.
    assert_eq!(Share::at(0.), Share::from(100));
    assert_eq!(Share::at(2.), Share::from(900));
}

#[test]
fn saved_panes_read_back_including_a_single_saved_id() {
    let panes = three();
    let saved = serde_json::to_string(&panes).unwrap();
    assert_eq!(serde_json::from_str::<Panes>(&saved).unwrap(), panes);
    let single: Panes = serde_json::from_str("\"t7\"").unwrap();
    assert_eq!(single, Panes::single(id("t7")));
}

#[test]
fn saved_panes_with_a_terminal_twice_are_refused() {
    let twice = Node::Split {
        axis: Axis::Row,
        share: Share::default(),
        first: Box::new(Node::Leaf(id("t1"))),
        second: Box::new(Node::Leaf(id("t1"))),
    };
    let saved = serde_json::to_string(&twice).unwrap();
    assert!(serde_json::from_str::<Panes>(&saved).is_err());
    assert_eq!(Panes::try_from(twice), Err(Error::Duplicate(id("t1"))));
}

#[test]
fn a_tab_saved_before_splits_reads_back_as_one_pane() {
    use crate::browser::Location;
    let saved = r#"{"kind":"core_terminal","terminal":"t4"}"#;
    let location: Location = serde_json::from_str(saved).unwrap();
    assert_eq!(
        location,
        Location::CoreTerminal {
            panes: Panes::single(id("t4")),
            observe: false,
        }
    );
    let again = serde_json::to_string(&location).unwrap();
    assert_eq!(serde_json::from_str::<Location>(&again).unwrap(), location);
}

#[test]
fn every_tab_layout_draws_split_panes() {
    use crate::{config::TabLayoutMode, tab_layout::layout_for};
    for &mode in TabLayoutMode::ALL {
        let chrome = layout_for(mode).panes();
        assert!(chrome.gutter() >= 1., "{mode:?} needs a gutter to drag");
        assert!(chrome.veil() > 0, "{mode:?} must mark the panes not in use");
        let (top, bottom) = chrome.corners();
        assert!(top >= 0. && bottom >= 0.);
    }
}

#[test]
fn a_mirror_tab_saves_that_it_only_watches() {
    use crate::browser::Location;
    let mirror = Location::CoreTerminal {
        panes: Panes::single(id("t4")),
        observe: true,
    };
    let saved = serde_json::to_string(&mirror).unwrap();
    assert!(saved.contains("\"observe\":true"), "{saved}");
    assert_eq!(serde_json::from_str::<Location>(&saved).unwrap(), mirror);
    // A tab that types saves no flag at all.
    let typing = Location::CoreTerminal {
        panes: Panes::single(id("t4")),
        observe: false,
    };
    assert!(!serde_json::to_string(&typing).unwrap().contains("observe"));
}
