use super::*;

#[test]
fn view_menu_lists_every_tab_layout_and_checks_the_current_one() {
    for &current in TabLayoutMode::ALL {
        let menus = menus(&Config {
            tab_layout: current,
            ..Config::default()
        });
        let view = menus
            .iter()
            .find(|menu| menu.name.as_ref() == "View")
            .unwrap();
        let tabs = view
            .items
            .iter()
            .find_map(|item| match item {
                MenuItem::Submenu(menu) if menu.name.as_ref() == "Tab Layout" => Some(menu),
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
        assert_eq!(actions.len(), TabLayoutMode::ALL.len());
        for ((name, action, checked), &mode) in actions.into_iter().zip(TabLayoutMode::ALL) {
            assert_eq!(name.as_ref(), mode.label());
            assert!(action.partial_eq(&SetTabLayout { mode }));
            assert_eq!(checked, mode == current, "{current}");
        }
    }
}
