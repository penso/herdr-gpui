use super::*;

/// A group split off in zen mode is the one drawn, across the whole row
/// at once: its opening slide would lay it out wider than the window.
#[gpui::test]
fn a_group_split_off_in_zen_mode_fills_the_window_at_once(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    run(&view, Command::ToggleZenMode, cx);
    view.update(cx, |view, _| view.hold_group_motion());

    run(&view, Command::SplitEditor, cx);
    let (kept, opening) = view.read_with(cx, |view, _| {
        let kept = view.drawn_group_slots();
        let opening = view.group_opened(kept[0].id);
        (kept, opening)
    });
    assert_eq!(kept.len(), 1);
    assert!(opening.is_some_and(|k| k < 0.5), "{opening:?}");
    let group = Box::leak(kept[0].selector("group").into_boxed_str());
    let width = cx.update(|window, _| window.viewport_size().width);
    assert_eq!(cx.debug_bounds(group).unwrap().size.width, width);
}
