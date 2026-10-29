#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
use crate::config::Theme;
use core::prelude::v1::test;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

struct Harness {
    open: Option<Dropdown>,
    picked: Rc<Cell<Option<usize>>>,
    opens: Rc<Cell<u32>>,
    enabled: bool,
    keys: Rc<RefCell<Vec<Option<usize>>>>,
}

fn options() -> Vec<SharedString> {
    vec!["Top left".into(), "Top right".into(), "Bottom".into()]
}

impl Render for Harness {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::default();
        let picked = self.picked.clone();
        let opens = self.opens.clone();
        let keys = self.keys.clone();
        let routed = cx.entity().downgrade();
        let (opener, picker, dismisser) = (
            cx.entity().downgrade(),
            cx.entity().downgrade(),
            cx.entity().downgrade(),
        );
        div()
            .size_full()
            .relative()
            .p(px(40.))
            .on_key_down(move |event, _, cx| {
                let _ = routed.update(cx, |this, _| {
                    if let Some(open) = this.open.as_mut() {
                        keys.borrow_mut().push(open.key(event));
                    }
                });
            })
            .child(dropdown(
                &theme,
                "notif-position-control",
                "Top right",
                false,
                self.enabled,
                self.open
                    .as_ref()
                    .filter(|open| open.field == DropdownField::ToastPosition),
                move |window, cx| {
                    opens.set(opens.get() + 1);
                    let _ = opener.update(cx, |this, cx| {
                        this.open = Some(Dropdown::open(
                            DropdownField::ToastPosition,
                            options(),
                            1,
                            window,
                            cx,
                        ));
                        cx.notify();
                    });
                },
                move |index, _, cx| {
                    picked.set(Some(index));
                    let _ = picker.update(cx, |this, cx| {
                        this.open = None;
                        cx.notify();
                    });
                },
                move |_, cx| {
                    let _ = dismisser.update(cx, |this, cx| {
                        this.open = None;
                        cx.notify();
                    });
                },
            ))
    }
}

fn click_button(cx: &mut VisualTestContext) {
    let center = cx.debug_bounds("notif-position-control").unwrap().center();
    cx.simulate_click(center, Default::default());
}

fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

#[gpui::test]
fn dropdown_opens_picks_and_dismisses(cx: &mut TestAppContext) {
    let picked = Rc::new(Cell::new(None));
    let opens = Rc::new(Cell::new(0));
    let keys = Rc::new(RefCell::new(Vec::new()));
    let (view, cx) = cx.add_window_view(|_, _| Harness {
        open: None,
        picked: picked.clone(),
        opens: opens.clone(),
        enabled: true,
        keys: keys.clone(),
    });
    draw(cx);
    click_button(cx);
    draw(cx);
    let option = cx
        .debug_bounds("notif-position-control-option-2")
        .expect("list open");
    let anchor = cx.debug_bounds("notif-position-control").unwrap();
    assert!(option.top() >= anchor.bottom(), "list below the button");
    cx.simulate_click(option.center(), Default::default());
    draw(cx);
    assert_eq!(picked.get(), Some(2));
    view.read_with(cx, |view, _| assert!(view.open.is_none()));

    // Clicking far from the list closes it without picking.
    picked.set(None);
    click_button(cx);
    draw(cx);
    view.read_with(cx, |view, _| assert!(view.open.is_some()));
    cx.simulate_click(point(px(700.), px(500.)), Default::default());
    draw(cx);
    view.read_with(cx, |view, _| assert!(view.open.is_none()));
    assert_eq!(picked.get(), None);

    // A second click on the button closes the list and leaves it closed.
    opens.set(0);
    click_button(cx);
    draw(cx);
    assert_eq!(opens.get(), 1);
    click_button(cx);
    draw(cx);
    view.read_with(cx, |view, _| assert!(view.open.is_none()));
    assert_eq!(opens.get(), 1, "closing click must not reopen");
    assert!(cx.debug_bounds("notif-position-control-option-0").is_none());

    // A press on the open button that is released elsewhere leaves no stale
    // flag: the next real click on the closed button still opens.
    click_button(cx);
    draw(cx);
    let center = cx.debug_bounds("notif-position-control").unwrap().center();
    cx.simulate_mouse_down(center, MouseButton::Left, Default::default());
    cx.simulate_mouse_up(
        point(px(700.), px(500.)),
        MouseButton::Left,
        Default::default(),
    );
    draw(cx);
    view.read_with(cx, |view, _| assert!(view.open.is_none()));
    opens.set(0);
    click_button(cx);
    draw(cx);
    assert_eq!(
        opens.get(),
        1,
        "stale closing flag must not swallow the open"
    );
    view.read_with(cx, |view, _| assert!(view.open.is_some()));
    click_button(cx);
    draw(cx);

    // Keyboard focus and routing.
    click_button(cx);
    draw(cx);
    let focus = view.read_with(cx, |view, _| view.open.as_ref().unwrap().focus().clone());
    cx.update(|window, _| assert!(focus.is_focused(window), "list holds focus"));
    cx.simulate_keystrokes("down");
    view.read_with(cx, |view, _| {
        assert_eq!(view.open.as_ref().unwrap().selected, 2)
    });
    assert_eq!(*keys.borrow(), vec![None]);
    view.update(cx, |view, _| {
        let open = view.open.as_mut().unwrap();
        let key = |s: &str| KeyDownEvent {
            keystroke: Keystroke::parse(s).unwrap(),
            is_held: false,
            prefer_character_input: false,
        };
        assert_eq!(open.key(&key("down")), None);
        assert_eq!(open.selected, 0, "wraps");
        assert_eq!(open.key(&key("up")), None);
        assert_eq!(open.selected, 2);
        assert_eq!(open.key(&key("enter")), Some(2));
        assert_eq!(open.key(&key("escape")), Some(usize::MAX));
    });
    // Enter on an out-of-range selection picks nothing.
    view.update(cx, |view, _| {
        let open = view.open.as_mut().unwrap();
        open.selected = 9;
        let enter = KeyDownEvent {
            keystroke: Keystroke::parse("enter").unwrap(),
            is_held: false,
            prefer_character_input: false,
        };
        assert_eq!(open.key(&enter), None);
    });

    // A disabled dropdown never shows its list.
    view.update(cx, |view, _| view.enabled = false);
    draw(cx);
    assert!(cx.debug_bounds("notif-position-control-option-0").is_none());
}

#[test]
fn follow_herdr_value_finds_the_value_in_the_message() {
    let label = crate::i18n::fl!("settings-use-herdr", value = "Top left");
    let value = follow_herdr_value(&label).unwrap();
    assert_eq!(&label[value], "Top left");
    assert_eq!(follow_herdr_value("Top left"), None);
}
