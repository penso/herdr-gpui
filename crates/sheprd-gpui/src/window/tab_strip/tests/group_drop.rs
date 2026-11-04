use {
    super::*,
    crate::{
        browser::{GroupId, Pick},
        controls::Command,
    },
};

/// Four Herdr tabs in the left group of a split, and none in the right.
fn split(
    cx: &mut TestAppContext,
    supports_tab_move: bool,
) -> (
    Entity<SheprdWindow>,
    &mut VisualTestContext,
    GroupId,
    GroupId,
) {
    let (view, cx) = four_tabs(cx, supports_tab_move);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.command(Command::SplitEditor, window, cx)
        })
    });
    for _ in 0..2 {
        draw(cx);
    }
    let slots = view.read_with(cx, |view, _| view.group_slots());
    (view, cx, slots[0].id, slots[1].id)
}

fn tabs(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext, group: GroupId) -> Vec<Pick> {
    cx.update(|_, cx| view.read(cx).group_tabs(group, cx))
}

fn herdr(ids: &[&str]) -> Vec<Pick> {
    ids.iter().map(|id| Pick::Herdr((*id).into())).collect()
}

/// Carries the tab drawn as `tab` to the middle of the group drawn as
/// `group`, without letting go.
fn carry(cx: &mut VisualTestContext, tab: &'static str, group: &'static str) -> Point<Pixels> {
    let from = cx.debug_bounds(tab).unwrap().center();
    let to = cx.debug_bounds(group).unwrap().center();
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    for _ in 0..2 {
        cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::default());
        draw(cx);
    }
    to
}

fn drop_at(cx: &mut VisualTestContext, at: Point<Pixels>) {
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::default());
    draw(cx);
}

#[gpui::test]
fn a_tab_dropped_on_another_group_moves_there(cx: &mut TestAppContext) {
    let (view, cx, left, right) = split(cx, true);
    assert_eq!(tabs(&view, cx, left), herdr(&["a", "b", "c", "d"]));
    assert_eq!(tabs(&view, cx, right), []);

    // Carried over the other group, the tab shows where it would land and
    // takes no place in its own strip.
    let over = carry(cx, "tab-b", "g1-group");
    assert!(cx.debug_bounds("g1-tab-drop").is_some());
    assert!(cx.debug_bounds("tab-drop").is_none());
    view.read_with(cx, |view, _| {
        assert!(!view.tab_drag.as_ref().unwrap().has_target());
    });
    drop_at(cx, over);

    // It is listed in the group it was dropped on alone, which shows it and
    // is the group in use; the group it left keeps the tab it showed.
    assert!(cx.debug_bounds("g1-tab-drop").is_none());
    assert_eq!(tabs(&view, cx, left), herdr(&["a", "c", "d"]));
    assert_eq!(tabs(&view, cx, right), herdr(&["b"]));
    assert!(cx.debug_bounds("g1-tab-b").is_some());
    assert!(cx.debug_bounds("tab-b").is_none());
    view.read_with(cx, |view, _| {
        assert!(view.tab_drag.is_none());
        assert_eq!(view.active_group(), Some(right));
        assert_eq!(view.group_pick(right), Some(Pick::Herdr("b".into())));
        assert_eq!(view.group_pick(left), Some(Pick::Herdr("a".into())));
    });
}

#[gpui::test]
fn a_group_goes_to_its_next_tab_when_the_one_it_shows_is_carried_off(cx: &mut TestAppContext) {
    let (view, cx, left, right) = split(cx, true);
    let over = carry(cx, "tab-a", "g1-group");
    drop_at(cx, over);
    assert_eq!(tabs(&view, cx, right), herdr(&["a"]));
    view.read_with(cx, |view, _| {
        assert_eq!(view.group_pick(left), Some(Pick::Herdr("b".into())));
        assert_eq!(view.group_pick(right), Some(Pick::Herdr("a".into())));
    });
}

#[gpui::test]
fn a_group_closes_when_its_last_tab_is_carried_off(cx: &mut TestAppContext) {
    let (view, cx, left, right) = split(cx, true);
    let over = carry(cx, "tab-b", "g1-group");
    drop_at(cx, over);
    assert_eq!(tabs(&view, cx, right), herdr(&["b"]));
    // The only tab of the right group goes back to the left one.
    let over = carry(cx, "g1-tab-b", "group");
    assert!(cx.debug_bounds("tab-drop").is_some());
    drop_at(cx, over);
    assert_eq!(view.read_with(cx, |view, _| view.group_slots().len()), 1);
    assert_eq!(tabs(&view, cx, left), herdr(&["a", "b", "c", "d"]));
    view.read_with(cx, |view, _| {
        assert_eq!(view.group_pick(left), Some(Pick::Herdr("b".into())));
    });
}

#[gpui::test]
fn a_tab_the_daemon_cannot_reorder_still_moves_to_another_group(cx: &mut TestAppContext) {
    let (view, cx, left, right) = split(cx, false);
    // In its own strip it stays put.
    let b = cx.debug_bounds("tab-b").unwrap();
    cx.simulate_mouse_down(b.center(), MouseButton::Left, Modifiers::default());
    let along = b.center() + point(px(-150.), px(0.));
    for _ in 0..2 {
        cx.simulate_mouse_move(along, MouseButton::Left, Modifiers::default());
        draw(cx);
    }
    view.read_with(cx, |view, _| {
        assert!(!view.tab_drag.as_ref().unwrap().has_target());
    });
    drop_at(cx, along);
    assert_eq!(tabs(&view, cx, left), herdr(&["a", "b", "c", "d"]));
    // Dropped on the other group, it moves.
    let over = carry(cx, "tab-b", "g1-group");
    drop_at(cx, over);
    assert_eq!(tabs(&view, cx, left), herdr(&["a", "c", "d"]));
    assert_eq!(tabs(&view, cx, right), herdr(&["b"]));
}
