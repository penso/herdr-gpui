use super::*;

struct InsetWindow {
    view: Entity<HerdrWindow>,
    inset: Pixels,
}

impl Render for InsetWindow {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().p(self.inset).child(self.view.clone())
    }
}

#[gpui::test]
fn open_menu_bar_follows_resizes_and_content_insets(cx: &mut TestAppContext) {
    let (root, cx) = cx.add_window_view(|window, cx| InsetWindow {
        view: cx.new(|cx| {
            let view = fixture_window(window, cx);
            view.focus.focus(window, cx);
            view
        }),
        inset: px(0.),
    });
    let view = root.read_with(cx, |root, _| root.view.clone());
    cx.simulate_resize(size(px(1200.), px(700.)));
    draw(cx);
    let button = cx.debug_bounds("application-menu-File").unwrap();
    cx.simulate_click(button.center(), Modifiers::none());
    draw(cx);

    for (width, inset, heading) in [
        (360., 0., "application-menu-Menu"),
        (1200., 11., "application-menu-File"),
        (360., 11., "application-menu-Menu"),
        (1200., 0., "application-menu-File"),
    ] {
        root.update(cx, |root, cx| {
            root.inset = px(inset);
            cx.notify();
        });
        cx.simulate_resize(size(px(width), px(700.)));
        for _ in 0..5 {
            draw(cx);
            let bar = cx.debug_bounds("application-menu-bar").unwrap();
            assert_eq!(bar.origin, point(px(inset), px(inset)));
            assert_eq!(bar.size.width, px(width - 2. * inset));
            assert!(bar.contains(&cx.debug_bounds(heading).unwrap().center()));
            view.read_with(cx, |view, _| {
                assert_eq!(view.menu.page, Some(Page::Application));
                let measured = view.menu.application_bar.bar.get();
                assert_eq!(measured.origin, bar.origin);
                assert_eq!(measured.size.width, bar.size.width);
            });
        }
        let panel = cx.debug_bounds("application-menu-panel").unwrap();
        assert!(panel.left() >= px(inset) && panel.right() <= px(width - inset));
    }

    // The resized heading still receives clicks through the modal overlay.
    let button = cx.debug_bounds("application-menu-File").unwrap();
    cx.simulate_click(button.center(), Modifiers::none());
    assert!(view.read_with(cx, |view, _| view.menu.page.is_none()));
}
