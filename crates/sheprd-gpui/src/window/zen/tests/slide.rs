use super::*;

/// Toggles zen mode with the chrome's slide held at its start.
fn toggle_held(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.zen.hold_slides();
            view.command(Command::ToggleZenMode, window, cx);
        })
    });
    draw(cx);
}

fn sliding(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) -> bool {
    view.read_with(cx, |view, _| view.chrome_sliding())
}

fn finish(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) {
    view.update(cx, |view, _| view.zen.finish_slide());
    draw(cx);
}

/// Entering is leaving played backwards: the window keeps its layout, the
/// terminal its size, while the sidebar and status bar slide out of their
/// own room. Only then does zen mode's layout take over, with one resize.
#[gpui::test]
fn entering_slides_the_chrome_out_before_the_layout_changes(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    let terminal = cx.debug_bounds("terminal").unwrap();
    let sidebar = cx.debug_bounds("sidebar").unwrap();
    let bar = cx.debug_bounds("connection-status").unwrap();

    toggle_held(&view, cx);
    assert!(sliding(&view, cx));
    view.read_with(cx, |view, _| {
        assert!(view.zen_active());
        assert!(!view.zen_layout());
    });
    assert_eq!(cx.debug_bounds("terminal").unwrap(), terminal);
    assert_eq!(cx.debug_bounds("sidebar").unwrap(), sidebar);
    assert_eq!(cx.debug_bounds("connection-status").unwrap(), bar);
    assert!(header_drawn(cx));
    assert!(drawn(cx, "tab-t0"));

    finish(&view, cx);
    assert!(!sliding(&view, cx));
    assert!(view.read_with(cx, |view, _| view.zen_layout()));
    assert!(!drawn(cx, "sidebar"));
    assert!(!drawn(cx, "connection-status"));
    assert!(!header_drawn(cx));
    let zen = cx.debug_bounds("terminal").unwrap();
    assert!(zen.size.width > terminal.size.width, "{zen:?}");
}

/// `[zen] font_size` waits for the chrome to have gone too, as it resizes
/// the terminal; leaving before then has nothing to put back.
#[gpui::test]
fn the_zen_font_waits_for_the_slide_out(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    view.update(cx, |view, _| view.config.zen.font_size = Some(24.));
    let size = |cx: &mut VisualTestContext| view.read_with(cx, |view, _| view.config.terminal.size);
    let before = size(cx);

    toggle_held(&view, cx);
    assert_eq!(size(cx), before);
    finish(&view, cx);
    assert_eq!(size(cx), 24.);
    run(&view, Command::ToggleZenMode, cx);
    assert_eq!(size(cx), before);

    toggle_held(&view, cx);
    run(&view, Command::ToggleZenMode, cx);
    assert_eq!(size(cx), before);
    assert!(!view.read_with(cx, |view, _| view.zen_active()));
}

/// Leaving, the sidebar and status bar take their room at once, so the
/// terminal resizes once, and slide in from beyond the edges.
#[gpui::test]
fn leaving_slides_the_chrome_back_into_room_taken_at_once(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    let terminal = cx.debug_bounds("terminal").unwrap();
    let sidebar = cx.debug_bounds("sidebar").unwrap();
    let bar = cx.debug_bounds("connection-status").unwrap();
    run(&view, Command::ToggleZenMode, cx);

    toggle_held(&view, cx);
    assert!(sliding(&view, cx));
    // The header comes back at once; only the sidebar and bar slide.
    assert_eq!(cx.debug_bounds("terminal").unwrap(), terminal);
    let entering = cx.debug_bounds("sidebar").unwrap();
    assert_eq!(entering.right(), sidebar.left());
    let entering_bar = cx.debug_bounds("connection-status").unwrap();
    assert_eq!(entering_bar.top(), bar.bottom());
}

#[gpui::test]
fn tests_draw_the_settled_window_without_sliding(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    run(&view, Command::ToggleZenMode, cx);
    assert!(!sliding(&view, cx));
    run(&view, Command::ToggleZenMode, cx);
    assert!(!sliding(&view, cx));
}
