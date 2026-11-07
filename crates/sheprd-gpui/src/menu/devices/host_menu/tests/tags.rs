//! Grouping a device from its menu.
use super::*;
use crate::device_tags::DeviceTags;
use gpui::Entity;

fn open(
    action: Action,
    cx: &mut TestAppContext,
) -> (Entity<crate::SheprdWindow>, &mut VisualTestContext) {
    cx.update(|cx| cx.set_global(DeviceTags::default()));
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let mut view = fixture_window(window, cx);
        add_host(&mut view);
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    view.update_in(cx, |view, window, cx| {
        view.open_host_menu(HOST, point(px(20.), px(20.)), window, cx);
        view.activate_host_menu(action, window, cx);
    });
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    (view, cx)
}

fn tags(cx: &mut VisualTestContext) -> Vec<String> {
    cx.update(|_, cx| DeviceTags::current(cx).of(HOST).to_vec())
}

#[gpui::test]
fn moving_to_a_starter_group_tags_the_device_and_closes(cx: &mut TestAppContext) {
    let (view, cx) = open(Action::Tags(super::super::tags::Mode::MoveTo), cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.menu.page),
        Some(Page::DeviceTags)
    );
    // Offered before any tag exists: personal, work, then No group.
    let work = cx.debug_bounds("device-tags-row-1").unwrap().center();
    assert!(cx.debug_bounds("device-tags-row-2").is_some());
    cx.simulate_click(work, Modifiers::default());
    assert_eq!(tags(cx), ["work"]);
    assert!(view.read_with(cx, |view, _| view.menu.page.is_none()));
}

#[gpui::test]
fn a_typed_name_makes_a_new_group(cx: &mut TestAppContext) {
    let (_, cx) = open(Action::Tags(super::super::tags::Mode::MoveTo), cx);
    cx.simulate_input("  client  acme ");
    cx.simulate_keystrokes("enter");
    assert_eq!(tags(cx), ["client acme"]);
}

#[gpui::test]
fn also_tagged_ticks_tags_and_stays_open(cx: &mut TestAppContext) {
    let (view, cx) = open(Action::Tags(super::super::tags::Mode::AlsoTagged), cx);
    cx.simulate_keystrokes("down enter");
    assert_eq!(tags(cx), ["personal"]);
    assert_eq!(
        view.read_with(cx, |view, _| view.menu.page),
        Some(Page::DeviceTags)
    );
    cx.simulate_keystrokes("enter");
    assert_eq!(tags(cx), Vec::<String>::new());
}
