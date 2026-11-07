use super::*;

#[test]
fn view_menu_lists_every_chat_layout_and_checks_the_current_one() {
    for &current in ChatLayoutMode::ALL {
        let mut config = Config::default();
        config.chat.layout = current;
        let menus = menus(&config);
        let view = menus
            .iter()
            .find(|menu| menu.name.as_ref() == "View")
            .unwrap();
        let tabs = view
            .items
            .iter()
            .find_map(|item| match item {
                MenuItem::Submenu(menu) if menu.name.as_ref() == "Agent Layout" => Some(menu),
                _ => None,
            })
            .unwrap();
        let actions: Vec<_> = tabs
            .items
            .iter()
            .filter_map(|item| match item {
                MenuItem::Action {
                    name,
                    action,
                    checked,
                    ..
                } => Some((name, action, *checked)),
                _ => None,
            })
            .collect();
        assert_eq!(actions.len(), ChatLayoutMode::ALL.len());
        for ((name, action, checked), &mode) in actions.into_iter().zip(ChatLayoutMode::ALL) {
            assert_eq!(name.as_ref(), mode.label().as_str());
            assert!(action.partial_eq(&SetChatLayout { mode }));
            assert_eq!(checked, mode == current, "{current:?}");
        }
    }
}

#[test]
fn a_new_chat_layout_rebuilds_the_menu_bar() {
    let old = Config::default();
    let mut new = old.clone();
    new.chat.layout = ChatLayoutMode::OpenCode;
    assert!(changed(&old, &new));
}
