//! Settings search: the catalog finds rows by title, description or page, and
//! the results page replaces the body until a result is picked.
use super::*;
use gpui::{px, size};

#[test]
fn catalog_search_matches_title_description_and_page() {
    let hits = catalog::search("indent");
    assert_eq!(hits.len(), 1);
    assert_eq!(catalog::CATALOG[hits[0]].id, "sidebar-indent");
    assert!(catalog::search("TOAST").iter().all(|i| {
        matches!(
            catalog::CATALOG[*i].section,
            Section::Notifications | Section::General
        )
    }));
    assert!(
        catalog::search("tabs")
            .iter()
            .any(|i| catalog::CATALOG[*i].section == Section::Tabs),
        "page label matches"
    );
    assert!(catalog::search("").is_empty());
    assert!(catalog::search("   ").is_empty());
    assert!(catalog::search("zzzz").is_empty());
}

#[test]
fn every_feature_flag_has_a_general_catalog_entry() {
    for feature in crate::config::Feature::ALL {
        assert!(
            catalog::CATALOG
                .iter()
                .any(|entry| entry.id == feature.setting_id() && entry.section == Section::General),
            "{}",
            feature.key()
        );
    }
}

#[test]
fn every_catalog_id_is_unique_and_belongs_to_its_section() {
    let mut seen = std::collections::HashSet::new();
    for entry in catalog::CATALOG {
        assert!(seen.insert(entry.id), "duplicate {}", entry.id);
        let prefix = match entry.section {
            Section::Theme => "theme-",
            Section::Sidebar => "sidebar-",
            Section::Indicators => "indicators-",
            Section::Fonts => "fonts-",
            Section::General => "general-",
            Section::Tabs => "tabs-",
            Section::Notifications => "notif-",
            Section::StatusBar => "status-",
            Section::Code => "code-",
            Section::Integrations => "integrations-",
            Section::Plugins => "plugins-",
            #[cfg(feature = "cloud")]
            Section::CloudDevices => "cloud-",
            Section::RemoteControl => "remote-",
            Section::Configuration => "config-",
        };
        assert!(entry.id.starts_with(prefix), "{}", entry.id);
    }
}

#[gpui::test]
fn every_catalog_id_is_a_rendered_row(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    cx.simulate_resize(size(px(960.), px(3000.)));
    for section in Section::ALL.iter().copied() {
        view.update(cx, |view, _| view.section = section);
        cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
        for entry in catalog::CATALOG.iter().filter(|e| e.section == section) {
            // A row the fixture's state does not render, such as the theme
            // side row outside following, has its own test.
            if !view.read_with(cx, |view, _| view.available(entry.availability)) {
                continue;
            }
            assert!(
                cx.debug_bounds(entry.id).is_some(),
                "{:?} page renders no {}",
                section,
                entry.id
            );
        }
    }
}

#[gpui::test]
fn typing_shows_results_and_clearing_returns_to_the_previous_page(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    cx.simulate_resize(size(px(960.), px(900.)));
    view.update(cx, |view, cx| {
        view.section = Section::Fonts;
        view.set_search_query("host colour", cx);
    });
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("settings-search-result-0").is_some());
    view.update(cx, |view, cx| view.set_search_query("zzzz", cx));
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("settings-search-empty").is_some());
    view.update(cx, |view, cx| view.set_search_query("host colour", cx));
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    let first = cx
        .debug_bounds("settings-search-result-0")
        .unwrap()
        .center();
    cx.simulate_click(first, Default::default());
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.section, Section::Sidebar);
        assert!(view.search_results.is_none());
    });
    view.update(cx, |view, cx| {
        view.section = Section::Fonts;
        view.set_search_query("indent", cx);
        view.set_search_query("", cx);
    });
    view.read_with(cx, |view, _| {
        assert_eq!(view.section, Section::Fonts);
        assert!(view.search_results.is_none());
    });
}

#[gpui::test]
fn picking_a_result_scrolls_its_row_into_view(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    cx.simulate_resize(size(px(960.), px(560.)));
    view.update(cx, |view, cx| view.set_search_query("host gap", cx));
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    let first = cx
        .debug_bounds("settings-search-result-0")
        .unwrap()
        .center();
    cx.simulate_click(first, Default::default());
    // The scroll waits for the frame that paints the page, then one more.
    for _ in 0..3 {
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            crate::sidebar::layout_tests::full_draw(window, cx).clear(cx);
        });
    }
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.section, Section::Sidebar);
        assert!(
            view.body_scroll.offset().y < px(0.),
            "scrolled down to the row"
        );
    });
}

#[gpui::test]
fn enter_picks_the_first_result_and_escape_clears(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    cx.simulate_resize(size(px(960.), px(900.)));
    let search = view.read_with(cx, |view, _| view.nav_search.clone());
    let focus = search.read_with(cx, |input, _| input.focus.clone());
    cx.update(|window, cx| focus.focus(window, cx));
    view.update(cx, |view, cx| view.set_search_query("tabs", cx));
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    cx.simulate_keystrokes("escape");
    view.read_with(cx, |view, _| assert!(view.search_results.is_none()));
    view.update(cx, |view, cx| view.set_search_query("confirm before", cx));
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    let focus = search.read_with(cx, |input, _| input.focus.clone());
    cx.update(|window, cx| focus.focus(window, cx));
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        assert_eq!(view.section, Section::Tabs);
        assert!(view.search_results.is_none());
    });
}

#[gpui::test]
fn search_offers_the_editing_row_only_while_following_a_pair(cx: &mut TestAppContext) {
    use crate::config::ThemeName;
    let (view, cx) = cx.add_window_view(skill_fixture);
    let side = |view: &SettingsWindow| {
        view.visible_results()
            .iter()
            .any(|index| catalog::CATALOG[*index].id == "theme-side")
    };
    view.update(cx, |view, cx| {
        assert!(!ThemeName::follows_system(view.drafted_theme()));
        view.set_search_query("editing", cx);
        assert!(
            view.search_results
                .iter()
                .flatten()
                .any(|index| catalog::CATALOG[*index].id == "theme-side"),
            "the catalog still knows the row"
        );
        assert!(!side(view), "a single theme renders no Editing row");
        view.config.theme = ThemeName::system("Catppuccin Latte", "Catppuccin Mocha");
        assert!(ThemeName::follows_system(view.drafted_theme()));
        assert!(side(view), "a following pair does");
    });
}
