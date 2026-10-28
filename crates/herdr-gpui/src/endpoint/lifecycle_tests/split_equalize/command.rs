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
