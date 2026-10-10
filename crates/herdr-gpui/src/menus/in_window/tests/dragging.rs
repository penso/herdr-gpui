use super::*;

/// Observe presses reaching the bar itself. The headless platform does
/// not implement compositor moves; button presses must be stopped before the
/// title-bar handler, while the empty background must allow that handler to run.
struct PressObserver {
    view: Entity<HerdrWindow>,
    presses: Rc<RefCell<Vec<MouseButton>>>,
}

impl Render for PressObserver {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let left = self.presses.clone();
        let right = self.presses.clone();
        self.view.update(cx, |view, cx| {
            view.render_application_bar(window, cx)
                .on_mouse_down(MouseButton::Left, move |event, _, _| {
                    left.borrow_mut().push(event.button)
                })
                .on_mouse_down(MouseButton::Right, move |event, _, _| {
                    right.borrow_mut().push(event.button)
                })
        })
    }
}

#[gpui::test]
fn background_presses_reach_window_chrome_but_menu_buttons_do_not(cx: &mut TestAppContext) {
    let presses = Rc::new(RefCell::new(Vec::new()));
    let observed = presses.clone();
    let (root, cx) = cx.add_window_view(|window, cx| PressObserver {
        view: cx.new(|cx| {
            let view = fixture_window(window, cx);
            view.focus.focus(window, cx);
            view
        }),
        presses: observed,
    });
    let view = root.read_with(cx, |root, _| root.view.clone());
    for (width, selector) in [
        (1200., "application-menu-Herdr"),
        (360., "application-menu-Menu"),
    ] {
        cx.simulate_resize(size(px(width), px(500.)));
        draw(cx);
        presses.borrow_mut().clear();
        let bar = cx.debug_bounds("application-menu-bar").unwrap();
        cx.simulate_click(
            point(bar.right() - px(12.), bar.center().y),
            Modifiers::none(),
        );
        assert_eq!(*presses.borrow(), [MouseButton::Left]);
        assert!(view.read_with(cx, |view, _| view.menu.page.is_none()));

        let button = cx.debug_bounds(selector).unwrap();
        cx.simulate_click(button.center(), Modifiers::none());
        draw(cx);
        assert_eq!(*presses.borrow(), [MouseButton::Left]);
        assert_eq!(
            view.read_with(cx, |view, _| view.menu.page),
            Some(Page::Application)
        );

        // The bar rendered while a menu is open must also isolate presses.
        let button = cx.debug_bounds(selector).unwrap();
        cx.simulate_event(MouseDownEvent {
            position: button.center(),
            button: MouseButton::Right,
            modifiers: Modifiers::none(),
            click_count: 1,
            first_mouse: false,
        });
        cx.simulate_event(MouseUpEvent {
            position: button.center(),
            button: MouseButton::Right,
            modifiers: Modifiers::none(),
            click_count: 1,
        });
        assert_eq!(*presses.borrow(), [MouseButton::Left]);
        assert_eq!(
            view.read_with(cx, |view, _| view.menu.page),
            Some(Page::Application)
        );
        cx.update(|window, cx| view.update(cx, |view, cx| view.dismiss_menu(window, cx)));
        draw(cx);
    }
}
