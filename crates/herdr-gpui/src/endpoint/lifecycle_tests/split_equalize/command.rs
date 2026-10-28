use super::*;

fn run(view: &gpui::Entity<HerdrWindow>, cx: &mut gpui::VisualTestContext) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.command(Command::EqualizeSplits, window, cx);
        });
    });
}

#[gpui::test]
fn the_command_evens_every_split_in_the_tab(cx: &mut gpui::TestAppContext) {
    let (view, mut server, cx) = with_borders(cx, "ssh:even-command");
    run(&view, cx);
    assert!(cx.update(|_, cx| view.read(cx).live.drag_request.is_some()));
    assert_even(&evened(&view, &mut server, cx));
    // Nothing is left held, so the next border press starts fresh.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| view.flush_split(cx));
        assert!(view.read(cx).split_drag.is_none());
    });
}

#[gpui::test]
fn the_command_leaves_a_zoomed_tab_alone(cx: &mut gpui::TestAppContext) {
    let (view, _server, cx) = with_borders(cx, "ssh:even-zoomed");
    cx.update(|_, cx| view.update(cx, |view, _| set_zoomed(view, true)));
    run(&view, cx);
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert!(view.split_drag.is_none());
        assert!(view.live.drag_request.is_none());
    });
}

#[gpui::test]
fn the_command_leaves_a_held_border_drag_alone(cx: &mut gpui::TestAppContext) {
    let (view, _server, cx) = with_borders(cx, "ssh:even-held");
    cx.update(|_, cx| {
        view.update(cx, |view, cx| assert!(down(view, 40.5, 5., 1, cx)));
    });
    run(&view, cx);
    // Evening would have sent its first ratio at once; the held drag sends
    // nothing until the pointer moves.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            assert!(view.split_drag.is_some());
            assert!(view.live.drag_request.is_none());
            release(view, 40.5, 5., cx);
        });
    });
}

#[gpui::test]
fn running_it_again_mid_way_does_not_restart_the_queue(cx: &mut gpui::TestAppContext) {
    let (view, mut server, cx) = with_borders(cx, "ssh:even-twice");
    run(&view, cx);
    run(&view, cx);
    // One ratio per split, not a second round on top of the first.
    assert_even(&evened(&view, &mut server, cx));
}
