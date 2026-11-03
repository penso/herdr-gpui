use super::*;
use crate::window::chrome_focus::Region;

const LONG: &str =
    "config-gpui.local.toml: layout.sidebar_gap is deprecated; use terminal.padding_x";

fn press(key: &str, cx: &mut VisualTestContext) {
    let keystroke = gpui::Keystroke::parse(key).unwrap();
    cx.simulate_event(gpui::KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(gpui::KeyUpEvent { keystroke });
    draw(cx);
}

fn open(fold: &'static str, cx: &mut VisualTestContext) {
    // The first frame measures the card; one it disagrees with draws again.
    draw(cx);
    draw(cx);
    let bounds = cx.debug_bounds(fold).unwrap();
    cx.simulate_click(bounds.center(), Default::default());
    draw(cx);
    draw(cx);
}

#[gpui::test]
fn open_cards_in_a_short_window_stay_on_screen(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    let height = px(400.);
    cx.simulate_resize(size(px(1000.), height));
    let text = [LONG; 4].map(|line| line.repeat(3)).join("\n");
    view.update(cx, |view, cx| {
        view.gui_config_diagnostic.sync(Some(&text));
        cx.notify();
    });
    set_diagnostic(&view, 0, Some(&text.replace("-gpui.local", "")), cx);
    open("gui-config-diagnostic-fold", cx);
    open("config-diagnostic-fold", cx);
    view.read_with(cx, |view, _| {
        assert!(view.gui_config_diagnostic.expanded());
        assert!(view.endpoints[0].config_diagnostic.expanded());
    });
    let first = cx.debug_bounds("gui-config-diagnostic").unwrap();
    let second = cx.debug_bounds("config-diagnostic").unwrap();
    // The second card starts below the first and both end inside the window,
    // with the second's message and controls still on screen.
    assert!(second.top() >= first.bottom(), "{first:?} {second:?}");
    assert!(second.bottom() <= height, "{second:?}");
    for part in ["config-diagnostic-lines", "config-diagnostic-dismiss"] {
        let bounds = cx.debug_bounds(part).unwrap();
        assert!(
            bounds.top() >= second.top() && bounds.bottom() <= second.bottom(),
            "{part}: {bounds:?} in {second:?}"
        );
        assert!(bounds.size.height > px(0.), "{part}: {bounds:?}");
    }
}

#[gpui::test]
fn closed_cards_keep_their_height_beside_an_open_one(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.simulate_resize(size(px(1000.), px(400.)));
    view.update(cx, |view, cx| {
        view.gui_config_diagnostic.sync(Some(LONG));
        cx.notify();
    });
    set_diagnostic(&view, 0, Some(&[LONG; 8].join("\n")), cx);
    draw(cx);
    draw(cx);
    let closed = cx.debug_bounds("gui-config-diagnostic").unwrap();
    open("config-diagnostic-fold", cx);
    assert_eq!(cx.debug_bounds("gui-config-diagnostic").unwrap(), closed);
}

#[gpui::test]
fn dismissing_the_last_card_by_keyboard_returns_to_the_terminal(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.simulate_resize(size(px(1000.), px(600.)));
    cx.update(|window, _| window.activate_window());
    view.update(cx, |view, cx| {
        view.gui_config_diagnostic
            .sync(Some("config-gpui.local.toml: ignoring unknown keys a"));
        cx.notify();
    });
    draw(cx);
    draw(cx);
    view.update(cx, |view, _| view.chrome_region = Some(Region::Notices));
    draw(cx);
    // With no chevron, the dismiss button is the region's only stop.
    cx.update(|window, cx| window.focus_next(cx));
    draw(cx);
    press("enter", cx);
    assert!(cx.debug_bounds("gui-config-diagnostic").is_none());
    let (region, terminal) = cx.update(|window, cx| {
        let view = view.read(cx);
        (view.chrome_region, view.focus.is_focused(window))
    });
    assert_eq!(region, None);
    assert!(terminal);
}
