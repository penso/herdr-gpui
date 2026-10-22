use super::*;
use crate::window::chrome_focus::Region;

/// The warning from #756, too long for the card's one row.
const LONG: &str =
    "config-gpui.local.toml: layout.sidebar_gap is deprecated; use terminal.padding_x";

fn show_gui_warning(
    view: &gpui::Entity<crate::HerdrWindow>,
    text: &str,
    cx: &mut VisualTestContext,
) {
    view.update(cx, |view, cx| {
        view.gui_config_diagnostic.sync(Some(text));
        cx.notify();
    });
    // The first frame measures the card; one it disagrees with draws again.
    draw(cx);
    draw(cx);
}

fn expanded(view: &gpui::Entity<crate::HerdrWindow>, cx: &mut VisualTestContext) -> bool {
    view.read_with(cx, |view, _| view.gui_config_diagnostic.expanded())
}

fn click(selector: &'static str, cx: &mut VisualTestContext) {
    let bounds = cx.debug_bounds(selector).unwrap();
    cx.simulate_click(bounds.center(), Default::default());
    draw(cx);
    draw(cx);
}

#[gpui::test]
fn a_message_that_fits_draws_no_chevron_and_ignores_clicks(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.simulate_resize(size(px(1000.), px(600.)));
    show_gui_warning(&view, "config-gpui.local.toml: ignoring unknown keys a", cx);
    assert!(cx.debug_bounds("gui-config-diagnostic-fold").is_none());
    let card = cx.debug_bounds("gui-config-diagnostic").unwrap();
    // The dismiss button still ends the card, as it did before.
    let dismiss = cx.debug_bounds("gui-config-diagnostic-dismiss").unwrap();
    assert_eq!(dismiss.right(), card.right() - px(11.));
    click("gui-config-diagnostic-line-0", cx);
    assert!(!expanded(&view, cx));
    assert_eq!(cx.debug_bounds("gui-config-diagnostic").unwrap(), card);
}

#[gpui::test]
fn a_cut_short_line_opens_in_full_and_closes_again(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.simulate_resize(size(px(1000.), px(600.)));
    show_gui_warning(&view, LONG, cx);
    let closed = cx.debug_bounds("gui-config-diagnostic").unwrap();
    let row = cx.debug_bounds("gui-config-diagnostic-line-0").unwrap();
    let chevron = cx.debug_bounds("gui-config-diagnostic-fold").unwrap();
    let dismiss = cx.debug_bounds("gui-config-diagnostic-dismiss").unwrap();
    // The chevron sits just before the dismiss button, the same size.
    assert_eq!(chevron.size, dismiss.size);
    assert_eq!(chevron.top(), dismiss.top());
    assert_eq!(chevron.right() + px(4.), dismiss.left());
    assert!(row.right() <= chevron.left());

    click("gui-config-diagnostic-fold", cx);
    assert!(expanded(&view, cx));
    let open = cx.debug_bounds("gui-config-diagnostic").unwrap();
    let wrapped = cx.debug_bounds("gui-config-diagnostic-line-0").unwrap();
    // The line wraps onto more rows in the same column; the card grows down
    // from where it stood and keeps its chevron to close it.
    assert!(
        wrapped.size.height > row.size.height,
        "{wrapped:?} vs {row:?}"
    );
    assert_eq!(wrapped.left(), row.left());
    assert_eq!(open.top(), closed.top());
    assert_eq!(open.right(), closed.right());
    assert!(open.size.height > closed.size.height);
    assert!(cx.debug_bounds("gui-config-diagnostic-fold").is_some());

    // A click on the card's text closes it again.
    click("gui-config-diagnostic-line-0", cx);
    assert!(!expanded(&view, cx));
    assert_eq!(cx.debug_bounds("gui-config-diagnostic").unwrap(), closed);
}

#[gpui::test]
fn dismissing_an_open_card_does_not_toggle_it_first(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.simulate_resize(size(px(1000.), px(600.)));
    show_gui_warning(&view, LONG, cx);
    click("gui-config-diagnostic-fold", cx);
    click("gui-config-diagnostic-dismiss", cx);
    assert!(cx.debug_bounds("gui-config-diagnostic").is_none());
    // New text arrives closed.
    show_gui_warning(&view, &format!("{LONG}, again"), cx);
    assert!(!expanded(&view, cx));
}

#[gpui::test]
fn an_open_card_scrolls_its_lines_past_a_few_rows(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.simulate_resize(size(px(1000.), px(800.)));
    let text = [LONG; 4].map(|line| line.repeat(3)).join("\n");
    show_gui_warning(&view, &text, cx);
    click("gui-config-diagnostic-fold", cx);
    assert!(expanded(&view, cx));
    let lines = cx.debug_bounds("gui-config-diagnostic-lines").unwrap();
    assert!(lines.size.height <= px(240.), "{lines:?}");
    let card = cx.debug_bounds("gui-config-diagnostic").unwrap();
    assert!(card.bottom() <= px(800.));
}

#[gpui::test]
fn an_open_card_stays_inside_a_narrow_window(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.simulate_resize(size(px(320.), px(600.)));
    show_gui_warning(&view, LONG, cx);
    click("gui-config-diagnostic-fold", cx);
    assert!(expanded(&view, cx));
    for part in [
        "gui-config-diagnostic",
        "gui-config-diagnostic-line-0",
        "gui-config-diagnostic-fold",
        "gui-config-diagnostic-dismiss",
    ] {
        let bounds = cx.debug_bounds(part).unwrap();
        assert!(
            bounds.left() >= px(0.) && bounds.right() <= px(320.),
            "{part}: {bounds:?}"
        );
    }
}

#[gpui::test]
fn the_endpoint_card_opens_only_for_its_own_connection(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.simulate_resize(size(px(1000.), px(600.)));
    set_diagnostic(&view, 0, Some(&LONG.replace("-gpui.local", "")), cx);
    draw(cx);
    click("config-diagnostic-fold", cx);
    view.read_with(cx, |view, _| {
        assert!(view.endpoints[0].config_diagnostic.expanded());
    });
    // A click drawn from a retired connection, or from text since replaced,
    // reaches no card.
    view.update(cx, |view, _| {
        let endpoint = &view.endpoints[0];
        let current = super::super::DrawnFrom {
            endpoint_id: endpoint.id.clone(),
            generation: endpoint.generation,
            inbox: endpoint.connection.inbox.clone(),
            lines: endpoint.config_diagnostic.visible().unwrap().clone(),
        };
        let retired = super::super::DrawnFrom {
            generation: current.generation + 1,
            ..current.clone()
        };
        assert!(retired.diagnostic(view).is_none());
        let replaced: Arc<[String]> = vec!["other".to_owned()].into();
        let diagnostic = current.diagnostic(view).unwrap();
        assert!(!diagnostic.toggle(&replaced));
        assert!(diagnostic.toggle(&current.lines));
        assert!(!diagnostic.expanded());
    });
}

#[gpui::test]
fn the_keyboard_reaches_the_chevron_and_enter_opens_the_card(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.simulate_resize(size(px(1000.), px(600.)));
    cx.update(|window, _| window.activate_window());
    view.read_with(cx, |view, _| assert!(!view.notices_shown()));
    show_gui_warning(&view, LONG, cx);
    view.read_with(cx, |view, _| assert!(view.notices_shown()));
    view.update(cx, |view, _| view.chrome_region = Some(Region::Notices));
    draw(cx);
    // The chevron is the region's first stop.
    cx.update(|window, cx| window.focus_next(cx));
    draw(cx);
    for key in ["enter", "space"] {
        let before = expanded(&view, cx);
        let keystroke = gpui::Keystroke::parse(key).unwrap();
        cx.simulate_event(gpui::KeyDownEvent {
            keystroke: keystroke.clone(),
            is_held: false,
            prefer_character_input: false,
        });
        cx.simulate_event(gpui::KeyUpEvent { keystroke });
        draw(cx);
        assert_ne!(expanded(&view, cx), before, "{key}");
    }
}
