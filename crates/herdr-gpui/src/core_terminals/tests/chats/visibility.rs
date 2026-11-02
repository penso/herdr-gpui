use super::*;

#[test]
fn chat_visibility_follows_the_rendered_split_and_zoom() {
    let mut terminals = terminals();
    let tab = TabId::test(1);
    assert_eq!(terminals.visible_chat_panes(tab), [&id("t1"), &id("t2")]);
    assert!(terminals.visible_chat_panes(TabId::test(2)).is_empty());
    assert!(terminals.visible_chat_panes(TabId::test(9)).is_empty());

    let shown = terminals.tabs.get_mut(&tab).unwrap();
    shown.zoomed = Some(id("t2"));
    shown.zoom.set(true, crate::motion::now(), true);
    assert_eq!(terminals.visible_chat_panes(tab), [&id("t2")]);

    let shown = terminals.tabs.get_mut(&tab).unwrap();
    shown.zoom.set(false, crate::motion::now(), true);
    assert_eq!(terminals.visible_chat_panes(tab), [&id("t1"), &id("t2")]);
}
