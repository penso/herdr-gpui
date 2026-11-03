use super::*;
use crate::{
    browser::{GroupId, Pick, Shown},
    controls::Command,
};

fn groups(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) -> Vec<GroupId> {
    draw(cx);
    view.read_with(cx, |view, _| {
        view.group_slots().into_iter().map(|slot| slot.id).collect()
    })
}

fn shown(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) -> Vec<Shown> {
    let groups = groups(view, cx);
    cx.update(|_, cx| {
        groups
            .iter()
            .map(|group| view.read(cx).group_shown(*group, cx))
            .collect()
    })
}

fn run(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext, command: Command) {
    cx.update(|window, cx| view.update(cx, |view, cx| view.command(command, window, cx)));
    draw(cx);
}

/// Debug selectors are looked up as `'static`; tests name a few.
fn selector(name: String) -> &'static str {
    Box::leak(name.into_boxed_str())
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector}"));
    cx.simulate_click(bounds.center(), gpui::Modifiers::none());
    draw(cx);
}

fn herdr(tab: &str) -> Pick {
    Pick::Herdr(tab.into())
}

#[gpui::test]
fn splitting_only_splits(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    assert_eq!(shown(&view, cx), [Shown::Terminal]);
    click(cx, "split-editor");
    // The new group opens without tabs, and says so; every tab stays where
    // it was and nothing new opens.
    assert_eq!(shown(&view, cx), [Shown::Terminal, Shown::Empty]);
    assert!(
        cx.update(|_, cx| view.read(cx).browser_tab_ids(cx))
            .is_empty()
    );
    assert!(cx.debug_bounds("g1-stand-in").is_some());
    assert!(cx.debug_bounds("stand-in").is_none());
    assert!(cx.debug_bounds("tab-t0").is_some());
    assert!(cx.debug_bounds("g1-tab-t0").is_none());
    let [left, right] = groups(&view, cx)[..] else {
        panic!("two groups")
    };
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert!(!view.group_tabs(left, cx).is_empty());
        assert!(view.group_tabs(right, cx).is_empty());
    });
    // Split as often as wanted, from any group.
    click(cx, "g1-split-editor");
    run(&view, cx, Command::SplitEditor);
    assert_eq!(groups(&view, cx).len(), 4);
    for divider in 0..3 {
        let selector = selector(format!("group-divider-{divider}"));
        assert!(cx.debug_bounds(selector).is_some(), "{selector}");
    }
    // The terminal never left the first group.
    let terminal = cx.debug_bounds("terminal").unwrap();
    let first = cx.debug_bounds("group").unwrap();
    assert!(terminal.left() < first.right());
    assert_eq!(shown(&view, cx)[0], Shown::Terminal);
}

/// Needs a build that shows pages: elsewhere a new tab opens nothing.
#[cfg(any(target_os = "macos", windows))]
#[gpui::test]
fn a_page_and_the_terminal_sit_side_by_side(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    run(&view, cx, Command::SplitEditor);
    let [left, right] = groups(&view, cx)[..] else {
        panic!("two groups")
    };
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_browser_tab_in(right, window, cx);
            view.activate_group(left, window, cx);
        })
    });
    let page = crate::browser::TabId::test(0);
    assert_eq!(shown(&view, cx), [Shown::Terminal, Shown::Page(page)]);
    assert!(cx.debug_bounds("terminal").is_some());
    assert!(cx.debug_bounds("g1-browser").is_some());
    // Each group lists its own tabs only.
    for selector in ["tab-t0", "g1-browser-tab-0"] {
        assert!(cx.debug_bounds(selector).is_some(), "{selector}");
    }
    for selector in ["browser-tab-0", "g1-tab-t0"] {
        assert!(cx.debug_bounds(selector).is_none(), "{selector}");
    }
    // Closing the page closes the group it was the last tab of.
    cx.update(|window, cx| view.update(cx, |view, cx| view.activate_group(right, window, cx)));
    run(&view, cx, Command::CloseTab);
    assert!(
        cx.update(|_, cx| view.read(cx).browser_tab_ids(cx))
            .is_empty()
    );
    assert_eq!(shown(&view, cx), [Shown::Terminal]);
}

#[gpui::test]
fn picking_another_herdr_tab_focuses_it_in_the_daemon(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    run(&view, cx, Command::SplitEditor);
    let other = view.read_with(cx, |view, _| {
        let snapshot = view.live.snapshot.as_ref().unwrap();
        snapshot
            .tabs
            .iter()
            .find(|tab| tab.workspace_id == "w0" && tab.tab_id != "t0")
            .map(|tab| tab.tab_id.clone())
    });
    let Some(other) = other else {
        return;
    };
    click(cx, selector(format!("tab-{other}")));
    // Until the daemon focuses it, the group keeps drawing the
    // terminal's frame rather than flashing a stand-in.
    assert_eq!(shown(&view, cx)[0], Shown::Terminal);
    assert!(cx.debug_bounds("stand-in").is_none());
    assert!(cx.debug_bounds("terminal").is_some());
    // Once it does, the group shows it and the other keeps its tab.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            snapshot.focused_tab_id = Some(other.clone());
            view.terminal_focus_moved(&other, false, cx);
        })
    });
    assert_eq!(shown(&view, cx), [Shown::Terminal, Shown::Empty]);
}

/// The daemon opens `tab` in the focused workspace and focuses it on the
/// window's connection, as it answers a new tab.
fn open_herdr_tab(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext, tab: &str) {
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            let mut new = snapshot.tabs[0].clone();
            new.tab_id = tab.to_owned();
            new.workspace_id = snapshot.focused_workspace_id.clone().unwrap();
            snapshot.tabs.push(new);
            snapshot.focused_tab_id = Some(tab.to_owned());
            view.terminal_focus_moved(tab, false, cx);
        })
    });
    draw(cx);
}

fn tabs(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext, group: GroupId) -> Vec<Pick> {
    cx.update(|_, cx| view.read(cx).group_tabs(group, cx))
}

#[gpui::test]
fn a_new_tab_belongs_to_the_group_that_asked_for_it(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    run(&view, cx, Command::SplitEditor);
    let [left, right] = groups(&view, cx)[..] else {
        panic!("two groups")
    };
    let before = tabs(&view, cx, left);
    // The right group's "+" asks; the daemon answers on the window's
    // connection, which the right group then holds.
    click(cx, "g1-new-tab");
    open_herdr_tab(&view, cx, "new-right");
    assert_eq!(tabs(&view, cx, right), [herdr("new-right")]);
    assert_eq!(tabs(&view, cx, left), before);
    assert_eq!(shown(&view, cx)[1], Shown::Terminal);
    assert_eq!(
        view.read_with(cx, |view, _| view.active_group()),
        Some(right)
    );
    assert!(cx.debug_bounds("g1-tab-new-right").is_some());
    assert!(cx.debug_bounds("tab-new-right").is_none());

    // A tab nobody's "+" asked for, from a shortcut or an agent, goes to
    // the group in use.
    cx.update(|window, cx| view.update(cx, |view, cx| view.activate_group(left, window, cx)));
    open_herdr_tab(&view, cx, "new-left");
    assert!(tabs(&view, cx, left).contains(&herdr("new-left")));
    assert_eq!(tabs(&view, cx, right), [herdr("new-right")]);
}

#[gpui::test]
fn focus_moving_to_another_groups_tab_uses_that_group(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    run(&view, cx, Command::SplitEditor);
    let [left, right] = groups(&view, cx)[..] else {
        panic!("two groups")
    };
    // The right group owns "t1"; the left one is in use, showing "t0" on
    // the window's connection.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.ensure_layout().unwrap().show(right, herdr("t1"));
            view.activate_group(left, window, cx);
        })
    });
    assert_eq!(
        view.read_with(cx, |view, _| view.active_group()),
        Some(left)
    );
    let left_tabs = tabs(&view, cx, left);
    // The daemon focuses the right group's tab, as an agent or the next-tab
    // shortcut does: the tab stays where it is and its group is used.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            snapshot.focused_tab_id = Some("t1".into());
            view.terminal_focus_moved("t1", false, cx);
        })
    });
    draw(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.active_group()),
        Some(right)
    );
    assert_eq!(tabs(&view, cx, right), [herdr("t1")]);
    assert_eq!(tabs(&view, cx, left), left_tabs);
    assert_eq!(shown(&view, cx)[1], Shown::Terminal);
}

/// One window tick's worth of keeping groups in step with their tabs.
fn settle(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| view.update(cx, |view, cx| view.settle_groups(window, cx)));
    draw(cx);
}

/// The daemon closes `tab` and focuses `next` on the window's connection.
fn close_herdr_tab(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext, tab: &str, next: &str) {
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            snapshot.tabs.retain(|candidate| candidate.tab_id != tab);
            snapshot.focused_tab_id = Some(next.to_owned());
            view.terminal_focus_moved(next, false, cx);
        })
    });
    settle(view, cx);
}

#[gpui::test]
fn a_tab_opened_behind_the_window_stays_with_the_group_in_use(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    run(&view, cx, Command::SplitEditor);
    let [left, right] = groups(&view, cx)[..] else {
        panic!("two groups")
    };
    // An agent opens a tab without focusing it while the right group is in
    // use.
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            let mut new = snapshot.tabs[0].clone();
            new.tab_id = "quiet".into();
            new.workspace_id = "w0".into();
            snapshot.tabs.push(new);
        })
    });
    settle(&view, cx);
    assert_eq!(tabs(&view, cx, right), [herdr("quiet")]);
    // It gives the group something to show, and does not follow the group
    // in use around afterwards.
    assert_eq!(
        view.read_with(cx, |view, _| view.group_pick(right)),
        Some(herdr("quiet"))
    );
    cx.update(|window, cx| view.update(cx, |view, cx| view.activate_group(left, window, cx)));
    settle(&view, cx);
    assert_eq!(tabs(&view, cx, right), [herdr("quiet")]);
    assert!(!tabs(&view, cx, left).contains(&herdr("quiet")));
}

#[gpui::test]
fn a_group_whose_tab_the_daemon_closed_goes_to_another_of_its_own(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    // A third tab, so the left group has one to go to.
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            let mut spare = snapshot.tabs[0].clone();
            spare.tab_id = "spare".into();
            spare.workspace_id = "w0".into();
            snapshot.tabs.push(spare);
        })
    });
    draw(cx);
    run(&view, cx, Command::SplitEditor);
    let [left, right] = groups(&view, cx)[..] else {
        panic!("two groups")
    };
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.ensure_layout().unwrap().show(right, herdr("t1"));
            view.activate_group(left, window, cx);
        })
    });
    assert!(tabs(&view, cx, left).len() > 1);
    // The daemon closes the left group's tab and lands on the right
    // group's: the left group stays in use, on a tab of its own, and the
    // right one keeps what it showed.
    close_herdr_tab(&view, cx, "t0", "t1");
    let left_tabs = tabs(&view, cx, left);
    assert!(!left_tabs.contains(&herdr("t0")));
    view.read_with(cx, |view, _| {
        assert_eq!(view.active_group(), Some(left));
        assert_eq!(view.group_pick(left).as_ref(), left_tabs.first());
        assert_eq!(view.group_pick(right), Some(herdr("t1")));
    });
    assert_eq!(tabs(&view, cx, right), [herdr("t1")]);
}

#[gpui::test]
fn a_group_whose_last_tab_the_daemon_closed_closes(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    run(&view, cx, Command::SplitEditor);
    let [left, right] = groups(&view, cx)[..] else {
        panic!("two groups")
    };
    // Without tabs since the split, the right group stays open.
    settle(&view, cx);
    assert_eq!(groups(&view, cx), [left, right]);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.ensure_layout().unwrap().show(right, herdr("t1"));
            view.activate_group(left, window, cx);
        })
    });
    // Its only tab closes in Herdr, as after the close confirmation.
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            snapshot.tabs.retain(|tab| tab.tab_id != "t1");
        })
    });
    settle(&view, cx);
    assert_eq!(groups(&view, cx), [left]);
    assert_eq!(shown(&view, cx), [Shown::Terminal]);
}

/// The daemon focuses `tab`, which is already open, on the window's
/// connection, as an agent or a shortcut does.
fn focus_herdr_tab(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext, tab: &str) {
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            snapshot.focused_tab_id = Some(tab.to_owned());
            view.terminal_focus_moved(tab, false, cx);
        })
    });
    draw(cx);
}

#[gpui::test]
fn focus_moving_within_a_group_uses_it_only_when_the_group_in_use_shows_nothing(
    cx: &mut gpui::TestAppContext,
) {
    let (view, cx) = window(cx);
    // A third tab, so the left group keeps two after one moves away.
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            let mut spare = snapshot.tabs[0].clone();
            spare.tab_id = "spare".into();
            spare.workspace_id = "w0".into();
            snapshot.tabs.push(spare);
        })
    });
    draw(cx);
    run(&view, cx, Command::SplitEditor);
    let [left, right] = groups(&view, cx)[..] else {
        panic!("two groups")
    };
    // Since the split, the group in use has no tab. The left group, which
    // holds the connection, changes tab: it becomes the group in use, so
    // Close Tab acts on the tab the focus moved to rather than closing the
    // empty group.
    assert_eq!(
        view.read_with(cx, |view, _| view.active_group()),
        Some(right)
    );
    focus_herdr_tab(&view, cx, "spare");
    view.read_with(cx, |view, _| {
        assert_eq!(view.active_group(), Some(left));
        assert_eq!(view.group_pick(left), Some(herdr("spare")));
    });
    assert_eq!(groups(&view, cx), [left, right]);

    // With a tab of its own to show, the group in use is not taken away
    // when the left group changes tab again, as when an agent does it while
    // the user reads the other group.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.ensure_layout().unwrap().show(right, herdr("t1"));
            view.activate_group(right, window, cx);
        })
    });
    assert_eq!(
        view.read_with(cx, |view, _| view.active_group()),
        Some(right)
    );
    focus_herdr_tab(&view, cx, "t0");
    view.read_with(cx, |view, _| {
        assert_eq!(view.active_group(), Some(right));
        assert_eq!(view.group_pick(left), Some(herdr("t0")));
        assert_eq!(view.group_pick(right), Some(herdr("t1")));
    });
}

#[gpui::test]
fn switching_tabs_keeps_the_terminal_until_the_daemon_focuses(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    let other = view.read_with(cx, |view, _| {
        let snapshot = view.live.snapshot.as_ref().unwrap();
        snapshot
            .tabs
            .iter()
            .find(|tab| tab.workspace_id == "w0" && tab.tab_id != "t0")
            .map(|tab| tab.tab_id.clone())
    });
    let Some(other) = other else {
        return;
    };
    click(cx, selector(format!("tab-{other}")));
    assert_eq!(shown(&view, cx), [Shown::Terminal]);
    assert!(cx.debug_bounds("stand-in").is_none());
    assert!(cx.debug_bounds("terminal").is_some());
}

#[gpui::test]
fn a_saved_layout_comes_back_and_changes_are_recorded(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    let saved = r#"{"groups":[{"pick":{"kind":"herdr","id":"t0"},"share":0.3,"tabs":[{"kind":"herdr","id":"t0"}]},{"pick":{"kind":"herdr","id":"t1"},"share":0.7,"tabs":[{"kind":"herdr","id":"t1"}]}],"active":0}"#;
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            // As on a first show of the workspace since the restart.
            let key = view.browser_key().unwrap();
            view.browser.layouts.remove(&key);
            view.browser
                .saved
                .insert(key, serde_json::from_str(saved).unwrap());
        })
    });
    let shown = shown(&view, cx);
    assert_eq!(shown.len(), 2);
    // The group in use shows its tab; the other waits for its own
    // connection, which the fixture never gets.
    assert_eq!(shown[0], Shown::Terminal);
    assert_eq!(shown[1], Shown::Elsewhere("t1".into()));
    let (key, share) = view.read_with(cx, |view, _| {
        let slots = view.group_slots();
        (view.browser_key().unwrap(), view.group_share(slots[0].id))
    });
    assert!((share - 0.3).abs() < 1e-6);
    // The restored layout is saved as it is, then as it changes.
    cx.update(|_, cx| view.update(cx, |view, cx| view.save_group_layouts(cx)));
    let recorded = cx.update(|_, cx| crate::browser::Layouts::snapshot(cx));
    assert_eq!(
        serde_json::to_string(recorded.get(&key).unwrap()).unwrap(),
        saved
    );
    run(&view, cx, Command::SplitEditor);
    cx.update(|_, cx| view.update(cx, |view, cx| view.save_group_layouts(cx)));
    let recorded = cx.update(|_, cx| crate::browser::Layouts::snapshot(cx));
    let json = serde_json::to_string(recorded.get(&key).unwrap()).unwrap();
    assert_eq!(json.matches("\"share\"").count(), 3, "{json}");
    // Closing back to one group following the terminal forgets it.
    for group in groups(&view, cx).into_iter().skip(1) {
        cx.update(|window, cx| view.update(cx, |view, cx| view.close_group(group, window, cx)));
    }
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let group = view.group_slots()[0].id;
            let layout = view.ensure_layout().unwrap();
            layout.forget(&herdr("t0"));
            let _ = group;
            view.save_group_layouts(cx);
        })
    });
    let recorded = cx.update(|_, cx| crate::browser::Layouts::snapshot(cx));
    assert!(!recorded.contains_key(&key));
}

#[gpui::test]
fn a_split_opens_from_the_right_and_a_close_folds_away(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|_, cx| view.update(cx, |view, _| view.browser.group_motion.enable()));
    draw(cx);
    let whole = cx.debug_bounds("group").unwrap();
    let early = crate::motion::ENTER / 8;
    cx.update(|_, cx| view.update(cx, |view, _| view.browser.group_motion.freeze(early)));
    run(&view, cx, Command::SplitEditor);
    draw(cx);
    // Early on, the source still has most of the row, and the new group,
    // laid out at its settled width, is only partly uncovered from the
    // right: its content runs past the source's edge to the left.
    let source = cx.debug_bounds("group").unwrap();
    let opening = cx.debug_bounds("g1-group").unwrap();
    assert!(source.size.width > whole.size.width * 0.55, "{source:?}");
    assert!(opening.size.width > whole.size.width * 0.45, "{opening:?}");
    assert!(opening.left() < source.right(), "{opening:?} {source:?}");
    assert!(
        (opening.right() - whole.right()).abs() < gpui::px(2.),
        "{opening:?} {whole:?}"
    );
    view.read_with(cx, |view, _| {
        let group = view.group_slots()[1].id;
        let opened = view.group_opened(group).unwrap_or(1.);
        assert!(opened > 0. && opened < 1., "{opened}");
    });

    // Settled, then closed: the group folds away where it stood.
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.browser.group_motion = Default::default();
            view.browser.group_motion.enable();
        })
    });
    let [_, right] = groups(&view, cx)[..] else {
        panic!("two groups")
    };
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.browser.group_motion.freeze(std::time::Duration::ZERO)
        })
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.close_group(right, window, cx)));
    draw(cx);
    let folding = cx.debug_bounds("folding-group").unwrap();
    let left = cx.debug_bounds("group").unwrap();
    assert!(
        folding.left() >= left.right() - gpui::px(1.),
        "{folding:?} {left:?}"
    );
    assert!(folding.size.width > whole.size.width / 3., "{folding:?}");
    view.read_with(cx, |view, _| assert_eq!(view.folding_groups().len(), 1));
}

#[gpui::test]
fn empty_groups_and_dividers(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    run(&view, cx, Command::SplitEditor);
    let divider = cx.debug_bounds("group-divider-0").unwrap();
    let before = cx.debug_bounds("group").unwrap();
    let target = divider.center() - gpui::point(gpui::px(100.), gpui::px(0.));
    cx.simulate_mouse_down(
        divider.center(),
        gpui::MouseButton::Left,
        gpui::Modifiers::none(),
    );
    cx.simulate_mouse_move(
        target,
        Some(gpui::MouseButton::Left),
        gpui::Modifiers::none(),
    );
    cx.simulate_mouse_move(
        target,
        Some(gpui::MouseButton::Left),
        gpui::Modifiers::none(),
    );
    cx.simulate_mouse_up(target, gpui::MouseButton::Left, gpui::Modifiers::none());
    draw(cx);
    let after = cx.debug_bounds("group").unwrap();
    assert!(after.size.width < before.size.width - gpui::px(50.));
    // Closing a group hands its tab strip back to one group.
    let [_, right] = groups(&view, cx)[..] else {
        panic!("two groups")
    };
    cx.update(|window, cx| view.update(cx, |view, cx| view.close_group(right, window, cx)));
    assert_eq!(shown(&view, cx), [Shown::Terminal]);
    assert!(cx.debug_bounds("group-divider-0").is_none());
}
