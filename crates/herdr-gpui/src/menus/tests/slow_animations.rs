use super::*;

#[test]
fn the_qa_menu_switches_slow_animations() {
    let qa = qa_menu();
    assert!(qa.items.iter().any(|item| matches!(item,
        MenuItem::Action { name, action, .. }
            if name.as_ref() == "Slow Animations"
                && action.partial_eq(&ToggleSlowAnimations)
    )));
}
