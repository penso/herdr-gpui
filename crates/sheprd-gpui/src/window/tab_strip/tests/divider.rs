use {super::*, crate::controls::Command};

/// A split window over two terminal tabs.
fn split(cx: &mut TestAppContext) -> (Entity<SheprdWindow>, &mut VisualTestContext) {
    let (view, cx) = shared_tabs(cx, &["a", "b"], "tab_bar_position = 'top'");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.command(Command::SplitEditor, window, cx)
        })
    });
    draw(cx);
    (view, cx)
}

/// Presses at `from`, drags 100 pixels to the left, and lets go.
fn drag_left(cx: &mut VisualTestContext, from: Point<Pixels>) {
    let to = from - point(px(100.), px(0.));
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::none());
    cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    draw(cx);
}

fn close(a: Pixels, b: Pixels) -> bool {
    (a - b).abs() <= px(0.5)
}

#[test]
fn a_grip_is_centered_between_two_groups_the_app_draws() {
    let [left, right] = [0, 1].map(|index| Reach::of(&[false, false], index));
    assert_eq!((left.left, left.right), (0., 2.));
    assert_eq!((right.left, right.right), (2., 0.));
}

#[test]
fn a_grip_beside_one_native_page_lies_over_the_other_group() {
    let [left, right] = [0, 1].map(|index| Reach::of(&[false, true], index));
    assert_eq!((left.left, left.right), (0., 4.));
    assert_eq!(right, Reach::default());
    let [left, right] = [0, 1].map(|index| Reach::of(&[true, false], index));
    assert_eq!(left, Reach::default());
    assert_eq!((right.left, right.right), (4., 0.));
    // Each divider of a group between two pages keeps its whole width.
    let middle = Reach::of(&[true, false, true], 1);
    assert_eq!((middle.left, middle.right), (4., 4.));
}

#[test]
fn no_grip_lies_over_a_native_page_or_a_group_that_is_not_there() {
    for index in 0..3 {
        assert_eq!(Reach::of(&[true, true], index), Reach::default());
        assert_eq!(Reach::of(&[], index), Reach::default());
    }
    assert_eq!(Reach::of(&[false], 0), Reach::default());
}

/// The groups touch the line, and the grip is five pixels wide around it.
#[gpui::test]
fn two_groups_touch_the_line_between_them(cx: &mut TestAppContext) {
    let (_, cx) = split(cx);
    let line = cx.debug_bounds("group-divider-0").unwrap();
    let before = cx.debug_bounds("group-divider-0-left").unwrap();
    let after = cx.debug_bounds("group-divider-0-right").unwrap();
    let left = cx.debug_bounds("group").unwrap();
    assert!(close(line.size.width, px(1.)), "{line:?}");
    assert!(close(left.right(), line.left()), "{left:?} {line:?}");
    assert!(close(before.right(), line.left()), "{before:?} {line:?}");
    assert!(close(after.left(), line.right()), "{after:?} {line:?}");
    assert!(close(after.right() - before.left(), px(5.)));
    // The grip runs the whole height of the groups.
    for part in [before, after] {
        assert!(close(part.top(), line.top()) && close(part.bottom(), line.bottom()));
    }
    // Only a divider has a grip: the row's outer edges have none.
    assert!(cx.debug_bounds("group-divider-1-left").is_none());
}

#[gpui::test]
fn each_part_of_the_grip_drags_the_divider(cx: &mut TestAppContext) {
    for part in [
        "group-divider-0-left",
        "group-divider-0",
        "group-divider-0-right",
    ] {
        let (_, cx) = split(cx);
        let before = cx.debug_bounds("group").unwrap();
        let from = cx.debug_bounds(part).unwrap().center();
        drag_left(cx, from);
        let after = cx.debug_bounds("group").unwrap();
        assert!(
            after.size.width < before.size.width - px(50.),
            "{part}: {before:?} {after:?}"
        );
    }
}

/// On Windows a page draws above the grip, so the grip moves off it.
#[cfg(windows)]
#[gpui::test]
fn the_grip_keeps_off_a_page_and_keeps_its_width(cx: &mut TestAppContext) {
    let (view, cx) = split(cx);
    let right = view.read_with(cx, |view, _| view.group_slots()[1].id);
    cx.update(|window, cx| view.update(cx, |view, cx| view.open_browser_tab_in(right, window, cx)));
    draw(cx);
    let line = cx.debug_bounds("group-divider-0").unwrap();
    let grip = cx.debug_bounds("group-divider-0-left").unwrap();
    assert!(cx.debug_bounds("group-divider-0-right").is_none());
    assert!(close(grip.right(), line.left()), "{grip:?} {line:?}");
    assert!(close(line.right() - grip.left(), px(5.)), "{grip:?}");
    let before = cx.debug_bounds("group").unwrap();
    drag_left(cx, grip.center());
    let after = cx.debug_bounds("group").unwrap();
    assert!(after.size.width < before.size.width - px(50.));
}

/// On macOS a page lies beneath the window's drawing, so the grip stays
/// centered on its line over the page, and the part over the page drags.
#[cfg(target_os = "macos")]
#[gpui::test]
fn the_grip_lies_over_a_page_centered_on_its_line(cx: &mut TestAppContext) {
    let (view, cx) = split(cx);
    let right = view.read_with(cx, |view, _| view.group_slots()[1].id);
    cx.update(|window, cx| view.update(cx, |view, cx| view.open_browser_tab_in(right, window, cx)));
    draw(cx);
    let line = cx.debug_bounds("group-divider-0").unwrap();
    let before = cx.debug_bounds("group-divider-0-left").unwrap();
    let after = cx.debug_bounds("group-divider-0-right").unwrap();
    assert!(close(before.right(), line.left()), "{before:?} {line:?}");
    assert!(close(after.left(), line.right()), "{after:?} {line:?}");
    assert!(close(after.right() - before.left(), px(5.)));
    let group = cx.debug_bounds("group").unwrap();
    drag_left(cx, after.center());
    let dragged = cx.debug_bounds("group").unwrap();
    assert!(dragged.size.width < group.size.width - px(50.));
}
