#![allow(clippy::unwrap_used)]
use super::*;
use Shown::*;

fn page(id: u64) -> Pick {
    Pick::Page(TabId::test(id))
}

fn herdr(id: &str) -> Pick {
    Pick::Herdr(id.into())
}

/// Every group's connection focusing the same tab, as before a split
/// connects the others.
fn shown(layout: &Layout, focused: &str) -> Vec<Shown> {
    each(layout, &[focused; 8])
}

/// Group `i`'s connection focusing `focused[i]`.
fn each(layout: &Layout, focused: &[&str]) -> Vec<Shown> {
    let slots: Vec<Slot> = layout.slots().collect();
    let focus = |group: GroupId| {
        slots
            .iter()
            .position(|slot| slot.id == group)
            .map(|index| focused[index].to_owned())
    };
    slots
        .iter()
        .map(|slot| layout.shown(slot.id, focus))
        .collect()
}

fn layout() -> (Layout, GroupIds, GroupId) {
    let mut ids = GroupIds::default();
    let first = ids.next();
    (Layout::new(first), ids, first)
}

/// The tabs a workspace has when it splits.
fn tabs() -> Vec<Pick> {
    vec![herdr("t1"), herdr("t2"), page(1)]
}

/// Which of `picks` each group lists, left to right.
fn listed(layout: &Layout, picks: &[Pick]) -> Vec<Vec<Pick>> {
    layout
        .slots()
        .map(|slot| {
            picks
                .iter()
                .filter(|pick| layout.lists(slot.id, pick))
                .cloned()
                .collect()
        })
        .collect()
}

#[test]
fn an_unsplit_layout_follows_the_terminal_until_a_page_is_picked() {
    let (mut layout, _, a) = layout();
    assert_eq!(shown(&layout, "t1"), [Terminal]);
    assert_eq!(layout.shown(a, |_| None), Terminal);
    layout.choose(a, page(1));
    assert_eq!(shown(&layout, "t1"), [Page(TabId::test(1))]);
    layout.choose(a, herdr("t1"));
    assert_eq!(shown(&layout, "t1"), [Terminal]);
    // Picking a Herdr tab its connection does not focus waits for it.
    layout.choose(a, herdr("t2"));
    assert_eq!(shown(&layout, "t1"), [Elsewhere("t2".into())]);
    assert_eq!(shown(&layout, "t2"), [Terminal]);
    // Alone, a group lists every tab.
    assert_eq!(listed(&layout, &tabs()), [tabs()]);
}

#[test]
fn a_split_opens_without_tabs_and_moves_none() {
    let (mut layout, mut ids, a) = layout();
    let b = ids.next();
    assert!(layout.split(a, b, Some("t1"), tabs()));
    assert_eq!(layout.len(), 2);
    assert_eq!(layout.active(), b);
    assert_eq!(layout.share(a), 0.5);
    assert_eq!(layout.share(b), 0.5);
    // The source keeps every tab and the one it showed; the new group shows
    // nothing and needs no connection.
    assert_eq!(listed(&layout, &tabs()), [tabs(), vec![]]);
    assert_eq!(shown(&layout, "t1"), [Terminal, Empty]);
    assert_eq!(layout.terminal_holders(), [(a, "t1".to_owned())]);
    // Using the source again changes nothing about who owns what.
    assert!(layout.activate(a));
    assert!(!layout.activate(a));
    assert_eq!(listed(&layout, &tabs()), [tabs(), vec![]]);
    assert!(!layout.split(ids.next(), ids.next(), Some("t1"), tabs()));
}

#[test]
fn a_tab_belongs_to_exactly_one_group() {
    let (mut layout, mut ids, a) = layout();
    let b = ids.next();
    layout.split(a, b, Some("t1"), tabs());
    // A tab no group owns yet goes to the group in use, and stays there.
    assert_eq!(listed(&layout, &[herdr("t9")]), [vec![], vec![herdr("t9")]]);
    layout.adopt([herdr("t9")]);
    layout.activate(a);
    assert_eq!(listed(&layout, &[herdr("t9")]), [vec![], vec![herdr("t9")]]);
    // Shown in another group, a tab moves there.
    layout.choose(b, page(1));
    assert_eq!(
        listed(&layout, &tabs()),
        [vec![herdr("t1"), herdr("t2")], vec![page(1)]]
    );
    assert_eq!(shown(&layout, "t1"), [Terminal, Page(TabId::test(1))]);
    // Taking the tab a group shows leaves that group showing nothing until
    // it is given another of its own.
    layout.choose(b, herdr("t1"));
    assert_eq!(layout.pick(a, None), None);
    assert_eq!(layout.pick(b, None), Some(herdr("t1")));
    assert_eq!(
        listed(&layout, &tabs()),
        [vec![herdr("t2")], vec![herdr("t1"), page(1)]]
    );
    layout.show(a, herdr("t2"));
    assert_eq!(layout.active(), b);
    assert_eq!(layout.pick(a, None), Some(herdr("t2")));
}

#[test]
fn groups_show_different_herdr_tabs_through_their_own_connections() {
    let (mut layout, mut ids, a) = layout();
    let (b, c) = (ids.next(), ids.next());
    assert!(layout.split(a, b, Some("t1"), tabs()));
    assert!(layout.split(b, c, Some("t1"), tabs()));
    layout.choose(b, herdr("t2"));
    layout.choose(c, page(7));
    assert_eq!(
        each(&layout, &["t1", "t2", "t1"]),
        [Terminal, Terminal, Page(TabId::test(7))]
    );
    // A connection still on its way to the tab stands in for it.
    assert_eq!(
        each(&layout, &["t1", "t1", "t1"]),
        [Terminal, Elsewhere("t2".into()), Page(TabId::test(7))]
    );
    // Only groups showing a Herdr tab need a connection.
    assert_eq!(
        layout.terminal_holders(),
        [(a, "t1".to_owned()), (b, "t2".to_owned())]
    );
}

#[test]
fn closing_groups_hands_on_width_use_and_tabs() {
    let (mut layout, mut ids, a) = layout();
    let (b, c) = (ids.next(), ids.next());
    layout.split(a, b, Some("t1"), tabs());
    layout.split(b, c, Some("t1"), tabs());
    layout.choose(b, herdr("t2"));
    layout.choose(c, page(1));
    assert_eq!(layout.share(b), 0.25);
    // The group on the left takes the room and the tabs.
    assert!(layout.close(c));
    assert_eq!(layout.active(), b);
    assert_eq!(layout.share(b), 0.5);
    assert_eq!(
        listed(&layout, &tabs()),
        [vec![herdr("t1")], vec![herdr("t2"), page(1)]]
    );
    assert_eq!(layout.pick(b, None), Some(herdr("t2")));
    // The first group's go to the one on its right.
    assert!(layout.close(a));
    assert_eq!(layout.share(b), 1.);
    assert_eq!(listed(&layout, &tabs()), [tabs()]);
    assert!(!layout.close(b));
    assert_eq!(layout.len(), 1);
}

#[test]
fn dividers_drag_between_neighbours_only() {
    let (mut layout, mut ids, a) = layout();
    let (b, c) = (ids.next(), ids.next());
    layout.split(a, b, Some("t1"), tabs());
    layout.split(b, c, Some("t1"), tabs());
    // Shares 0.5, 0.25, 0.25: the second divider sits at 0.75.
    assert!(layout.drag(1, 600., 1000.));
    assert!((layout.share(b) - 0.1).abs() < 1e-6);
    assert!((layout.share(c) - 0.4).abs() < 1e-6);
    assert_eq!(layout.share(a), 0.5);
    assert!(layout.drag(0, 0., 1000.));
    assert_eq!(layout.share(a), MIN_SHARE);
    assert!(!layout.drag(2, 10., 1000.));
    assert!(!layout.drag(0, f32::NAN, 1000.));
    assert!(!layout.drag(0, 10., 0.));
}

#[test]
fn a_connections_focus_moves_only_its_own_group() {
    let (mut layout, mut ids, a) = layout();
    let b = ids.next();
    // Alone, a group follows the terminal.
    layout.focus_moved(a, "t9");
    assert_eq!(layout.pick(a, Some("t2")), Some(herdr("t2")));
    layout.split(a, b, Some("t1"), tabs());
    layout.choose(b, herdr("t2"));
    layout.focus_moved(a, "t3");
    assert_eq!(layout.pick(a, None), Some(herdr("t3")));
    assert_eq!(layout.pick(b, None), Some(herdr("t2")));
    // The tab its connection moved to is its own, and it stays unused.
    assert!(layout.lists(a, &herdr("t3")));
    assert_eq!(layout.active(), b);
    layout.choose(a, page(1));
    layout.focus_moved(a, "t4");
    assert_eq!(layout.pick(a, None), Some(herdr("t4")));
    // A group without tabs has no connection to follow.
    let c = ids.next();
    layout.split(b, c, None, tabs());
    layout.focus_moved(c, "t5");
    assert_eq!(layout.pick(c, None), None);
}

#[test]
fn a_closed_tab_leaves_the_group_that_showed_it() {
    let (mut layout, mut ids, a) = layout();
    let b = ids.next();
    layout.split(a, b, Some("t1"), tabs());
    layout.choose(b, page(1));
    // The group shows nothing until it is given another tab; the other
    // group is untouched.
    assert_eq!(layout.forget(&page(1)), Some(b));
    assert_eq!(layout.forget(&page(1)), None);
    assert_eq!(shown(&layout, "t1"), [Terminal, Empty]);
    assert!(layout.tabs().all(|pick| *pick != page(1)));
    // A tab no group showed is only forgotten.
    assert_eq!(layout.forget(&herdr("t2")), None);
    // Alone, a group whose tab closed follows the terminal again.
    layout.close(a);
    layout.choose(b, page(3));
    assert_eq!(layout.forget(&page(3)), Some(b));
    assert_eq!(layout.pick(b, Some("t5")), Some(herdr("t5")));
}

#[test]
fn layouts_round_trip_through_their_saved_form() {
    let (mut split, mut ids, a) = layout();
    assert_eq!(split.saved(), None);
    let (b, c) = (ids.next(), ids.next());
    split.split(a, b, Some("t1"), tabs());
    split.split(b, c, Some("t1"), tabs());
    split.choose(a, herdr("t2"));
    split.choose(c, page(4));
    split.activate(b);
    let saved = split.saved().unwrap();
    assert!(saved.valid());
    let json = serde_json::to_string(&saved).unwrap();
    assert!(json.contains(r#"{"kind":"herdr","id":"t2"}"#), "{json}");
    assert!(!json.contains("hidden"), "{json}");
    let restored = Layout::restore(&serde_json::from_str(&json).unwrap(), &mut ids);
    assert_eq!(restored.saved(), Some(saved));
    assert_eq!(restored.len(), 3);
    // Fresh IDs, and the group in use is the second one again.
    let slots: Vec<Slot> = restored.slots().collect();
    assert!(slots.iter().all(|slot| ![a, b, c].contains(&slot.id)));
    assert_eq!(restored.active(), slots[1].id);
    // Each group has its own tabs back, and shows its own.
    let all = [herdr("t1"), herdr("t2"), page(1), page(4)];
    assert_eq!(listed(&restored, &all), listed(&split, &all));
    assert_eq!(
        each(&restored, &["t2", "t1", "t1"]),
        [Terminal, Empty, Page(TabId::test(4))]
    );
    // A lone group on a page is worth keeping too, and names no tabs.
    let (mut lone, _, a) = layout();
    lone.choose(a, page(1));
    let json = serde_json::to_string(&lone.saved().unwrap()).unwrap();
    assert!(!json.contains("tabs"), "{json}");
}

#[test]
fn saved_layouts_the_app_could_not_have_written_are_refused() {
    let group = |pick: Option<Pick>, share: f32| SavedGroup {
        pick,
        share,
        tabs: Vec::new(),
        hidden: Vec::new(),
    };
    let valid = SavedLayout {
        groups: vec![group(None, 0.5), group(Some(herdr("t1")), 0.5)],
        active: 1,
    };
    assert!(valid.valid());
    let crowded = SavedGroup {
        tabs: vec![herdr("t1"); MAX_SAVED_TABS + 1],
        ..group(None, 1.)
    };
    let unnamed = SavedGroup {
        tabs: vec![herdr("")],
        ..group(None, 1.)
    };
    for invalid in [
        SavedLayout {
            groups: vec![],
            active: 0,
        },
        SavedLayout {
            active: 2,
            ..valid.clone()
        },
        SavedLayout {
            groups: vec![group(None, f32::NAN)],
            active: 0,
        },
        SavedLayout {
            groups: vec![group(None, 0.)],
            active: 0,
        },
        SavedLayout {
            groups: vec![group(Some(herdr("")), 1.)],
            active: 0,
        },
        SavedLayout {
            groups: vec![group(Some(herdr(&"x".repeat(300))), 1.)],
            active: 0,
        },
        SavedLayout {
            groups: vec![group(None, 1.); MAX_SAVED_GROUPS + 1],
            active: 0,
        },
        SavedLayout {
            groups: vec![crowded],
            active: 0,
        },
        SavedLayout {
            groups: vec![unnamed],
            active: 0,
        },
    ] {
        assert!(!invalid.valid(), "{invalid:?}");
    }
}

/// Development builds saved whether a VS Code panel showed; such layouts
/// still restore, and nothing of the panel is saved again.
#[test]
fn layouts_saved_with_the_old_vs_code_panel_still_restore() {
    let (lone, mut ids, _) = layout();
    assert_eq!(lone.saved(), None);
    let old: SavedLayout =
        serde_json::from_str(r#"{"groups":[{"pick":null,"share":1}],"active":0,"code":true}"#)
            .unwrap();
    assert!(old.valid());
    let restored = Layout::restore(&old, &mut ids);
    assert_eq!(restored.saved(), None);
    assert!(!serde_json::to_string(&old).unwrap().contains("code"));
}

/// Before a tab belonged to one group, every group listed every tab except
/// the ones closed in it, and such layouts are still on disk.
#[test]
fn layouts_saved_with_tabs_closed_per_group_give_each_tab_one_group() {
    let (_, mut ids, _) = layout();
    let old: SavedLayout = serde_json::from_str(
        r#"{"groups":[
            {"pick":{"kind":"herdr","id":"t2"},"share":0.5,
             "hidden":[{"kind":"page","id":6},{"kind":"page","id":8}]},
            {"pick":{"kind":"herdr","id":"t5"},"share":0.5,
             "hidden":[{"kind":"herdr","id":"t2"},{"kind":"page","id":7},{"kind":"page","id":8}]}
        ],"active":0}"#,
    )
    .unwrap();
    assert!(old.valid());
    let restored = Layout::restore(&old, &mut ids);
    // Each tab goes to the group that still listed it; one closed in every
    // group goes to the group in use, where it was still listed.
    let all = [herdr("t2"), herdr("t5"), page(6), page(7), page(8)];
    assert_eq!(
        listed(&restored, &all),
        [
            vec![herdr("t2"), page(7), page(8)],
            vec![herdr("t5"), page(6)]
        ]
    );
    assert_eq!(each(&restored, &["t2", "t5"]), [Terminal, Terminal]);
    let json = serde_json::to_string(&restored.saved().unwrap()).unwrap();
    assert!(!json.contains("hidden"), "{json}");

    // Two groups showing one tab: the group in use keeps it.
    let shared: SavedLayout = serde_json::from_str(
        r#"{"groups":[
            {"pick":{"kind":"herdr","id":"t1"},"share":0.5},
            {"pick":{"kind":"herdr","id":"t1"},"share":0.5}
        ],"active":1}"#,
    )
    .unwrap();
    let restored = Layout::restore(&shared, &mut ids);
    assert_eq!(
        listed(&restored, &[herdr("t1")]),
        [vec![], vec![herdr("t1")]]
    );
    assert_eq!(each(&restored, &["t1", "t1"]), [Empty, Terminal]);
}

/// Which group takes a tab that several groups of an old layout had kept.
#[test]
fn old_layouts_prefer_a_group_showing_a_tab_then_the_group_in_use() {
    let (_, mut ids, _) = layout();
    let restore = |json: &str, ids: &mut GroupIds| {
        let old: SavedLayout = serde_json::from_str(json).unwrap();
        assert!(old.valid());
        Layout::restore(&old, ids)
    };
    // Closed in the first group only and shown by none: of the two groups
    // that kept it, the group in use takes it, not the first one.
    let listed_twice = restore(
        r#"{"groups":[
            {"pick":{"kind":"herdr","id":"t1"},"share":0.4,"hidden":[{"kind":"page","id":9}]},
            {"pick":{"kind":"herdr","id":"t2"},"share":0.3},
            {"pick":{"kind":"herdr","id":"t3"},"share":0.3}
        ],"active":2}"#,
        &mut ids,
    );
    assert_eq!(
        listed(&listed_twice, &[page(9)]),
        [vec![], vec![], vec![page(9)]]
    );
    // A group showing the tab keeps it, though the group in use listed it
    // too: no group loses the tab it shows.
    let shown_elsewhere = restore(
        r#"{"groups":[
            {"pick":{"kind":"herdr","id":"t1"},"share":0.4,"hidden":[{"kind":"page","id":9}]},
            {"pick":{"kind":"page","id":9},"share":0.3},
            {"pick":{"kind":"herdr","id":"t3"},"share":0.3}
        ],"active":2}"#,
        &mut ids,
    );
    assert_eq!(
        listed(&shown_elsewhere, &[page(9)]),
        [vec![], vec![page(9)], vec![]]
    );
    assert_eq!(
        each(&shown_elsewhere, &["t1", "t1", "t3"]),
        [Terminal, Page(TabId::test(9)), Terminal]
    );
}

#[test]
fn selectors_keep_their_names_in_the_first_group() {
    let (_, mut ids, _) = layout();
    let mut slot = |index| Slot {
        id: ids.next(),
        index,
    };
    assert_eq!(slot(0).selector("tab-t0"), "tab-t0");
    assert_eq!(slot(2).selector("tab-t0"), "g2-tab-t0");
}
