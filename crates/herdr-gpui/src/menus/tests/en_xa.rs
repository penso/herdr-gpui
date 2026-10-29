//! Under `en-XA` every menu and item name is a translated message, QA
//! included: the menu bar is not drawn by the app on macOS, so the leak
//! test's text recorder cannot see it.
use super::*;
use crate::i18n::leak_tests::under_en_xa;

fn names(items: &[MenuItem], into: &mut Vec<String>) {
    for item in items {
        match item {
            MenuItem::Action { name, .. } => into.push(name.to_string()),
            MenuItem::Submenu(menu) => {
                into.push(menu.name.to_string());
                names(&menu.items, into);
            }
            _ => {}
        }
    }
}

#[test]
fn every_menu_bar_name_is_a_message_under_en_xa() {
    under_en_xa("menu bar en-XA", |_, _| {
        let mut config = Config::default();
        config.features.qa_menu = true;
        let mut shown = Vec::new();
        for menu in menus(&config) {
            shown.push(menu.name.to_string());
            names(&menu.items, &mut shown);
        }
        let leaks: Vec<_> = shown
            .iter()
            .filter(|name| !herdr_i18n::pseudo::is_marked(name))
            .collect();
        assert!(leaks.is_empty(), "{leaks:#?}");
    });
}
