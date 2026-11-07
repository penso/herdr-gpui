use super::*;

#[test]
fn the_qa_menu_switches_each_pseudo_locale_next_to_slow_animations() {
    let qa = qa_menu();
    let position = |wanted: &dyn Fn(&MenuItem) -> bool| qa.items.iter().position(wanted);
    let slow = position(&|item| {
        matches!(item, MenuItem::Action { action, .. } if action.partial_eq(&ToggleSlowAnimations))
    })
    .unwrap();
    for (offset, pseudo) in Pseudo::ALL.into_iter().enumerate() {
        let item = &qa.items[slow + 1 + offset];
        assert!(
            matches!(item, MenuItem::Action { name, action, checked, .. }
                if name.contains(&pseudo.locale().to_string())
                    && action.partial_eq(&TogglePseudoLocale { pseudo })
                    && !checked),
            "{pseudo:?}"
        );
    }
}
