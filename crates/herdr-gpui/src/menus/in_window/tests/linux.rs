use super::*;

struct InsetWindow(Entity<HerdrWindow>);

impl Render for InsetWindow {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().p(px(11.)).child(self.0.clone())
    }
}

#[gpui::test]
fn window_coordinates_do_not_drift_inside_client_decorations(cx: &mut TestAppContext) {
    let (root, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let view = fixture_window(window, cx);
            view.focus.focus(window, cx);
            view
        });
        InsetWindow(view)
    });
    let view = root.read_with(cx, |root, _| root.0.clone());
    cx.simulate_resize(size(px(360.), px(300.)));
    draw(cx);
    let original = cx.debug_bounds("application-menu-bar").unwrap();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_application_menu(Some(4), window, cx)
        })
    });
    for _ in 0..5 {
        draw(cx);
        assert_eq!(cx.debug_bounds("application-menu-bar").unwrap(), original);
        let panel = cx.debug_bounds("application-menu-panel").unwrap();
        assert!((panel.top() - original.bottom()).abs() <= px(1.));
        assert!(panel.left() >= px(11.) && panel.right() <= px(349.));
        assert!(panel.bottom() <= px(289.));
    }
}

#[gpui::test]
fn bar_uses_every_shared_heading_and_stays_with_sidebar_hidden(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.simulate_resize(size(px(1200.), px(700.)));
    view.update(cx, |view, cx| {
        view.sidebar_visible = false;
        cx.notify();
    });
    draw(cx);
    for selector in [
        "application-menu-Herdr",
        "application-menu-File",
        "application-menu-Edit",
        "application-menu-View",
        "application-menu-Terminal",
        "application-menu-Window",
        #[cfg(feature = "qa-menu")]
        "application-menu-QA",
    ] {
        assert!(cx.debug_bounds(selector).is_some());
    }
    let button = cx.debug_bounds("application-menu-File").unwrap();
    cx.simulate_click(button.center(), Modifiers::none());
    draw(cx);
    assert!(cx.debug_bounds("application-item-New Workspace").is_some());
    let button = cx.debug_bounds("application-menu-View").unwrap();
    cx.simulate_click(button.center(), Modifiers::none());
    draw(cx);
    assert!(cx.debug_bounds("application-item-Layout").is_some());
    cx.simulate_click(point(px(1190.), px(690.)), Modifiers::none());
    assert!(view.read_with(cx, |view, _| view.menu.page.is_none()));
    assert!(!view.read_with(cx, |view, _| view.sidebar_visible));
}

#[gpui::test]
fn compact_menu_and_long_lists_fit_a_narrow_short_window(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.simulate_resize(size(px(360.), px(300.)));
    draw(cx);
    let button = cx.debug_bounds("application-menu-Menu").unwrap();
    cx.simulate_click(button.center(), Modifiers::none());
    draw(cx);
    let terminal = cx.debug_bounds("application-item-Terminal").unwrap();
    cx.simulate_click(terminal.center(), Modifiers::none());
    draw(cx);
    cx.simulate_keystrokes("end");
    draw(cx);
    let panel = cx.debug_bounds("application-menu-panel").unwrap();
    let last = cx.debug_bounds("application-item-Reconnect").unwrap();
    assert!(panel.contains(&last.center()));
    assert!(panel.left() >= px(0.) && panel.right() <= px(360.));
    assert!(panel.top() >= px(0.) && panel.bottom() <= px(300.));
    cx.simulate_keystrokes("escape");
    assert!(view.read_with(cx, |view, _| view.menu.page.is_none()));
}

#[gpui::test]
fn f10_opens_and_dismisses_without_typing_into_the_terminal(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    cx.simulate_keystrokes("f10");
    draw(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.menu.page),
        Some(Page::Application)
    );
    cx.simulate_keystrokes("a b c");
    view.read_with(cx, |view, _| {
        assert!(view.marked.is_empty());
        assert!(view.local_error.is_none());
        assert_eq!(view.menu.page, Some(Page::Application));
    });
    cx.simulate_keystrokes("f10");
    cx.update(|window, cx| {
        assert!(view.read(cx).menu.page.is_none());
        assert!(view.read(cx).focus.is_focused(window));
    });
}
