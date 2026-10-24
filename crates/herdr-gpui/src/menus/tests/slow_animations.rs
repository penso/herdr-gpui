use super::*;

#[test]
fn the_qa_menu_switches_slow_animations() {
    let menus = menus(Layout::default(), TabLayoutMode::default());
    let qa = menus
        .iter()
        .find(|menu| menu.name.as_ref() == "QA")
        .unwrap();
    assert!(qa.items.iter().any(|item| matches!(item,
        MenuItem::Action { name, action, .. }
            if name.as_ref() == "Slow Animations"
                && action.partial_eq(&ToggleSlowAnimations)
    )));
}
