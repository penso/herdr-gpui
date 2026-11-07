//! The section list shows each section under its group, above a search box.
use super::*;
use gpui::{px, size};

// Debug selectors are `&'static str`; a handful of test names may leak.
fn leak(name: String) -> &'static str {
    Box::leak(name.into_boxed_str())
}

#[gpui::test]
fn navigation_lists_every_section_under_its_group(cx: &mut TestAppContext) {
    let main = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let weak = cx.update(|cx| main.update(cx, |_, _, cx| cx.weak_entity()).unwrap());
    let (_view, cx) = cx.add_window_view(|window, cx| {
        let view = SettingsWindow::new(weak, cx);
        window.focus(&view.focus, cx);
        view
    });
    cx.simulate_resize(size(px(1200.), px(900.)));
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    let search = cx.debug_bounds("settings-nav-search").unwrap();
    let mut last_bottom = search.bottom();
    for section in Section::ALL.iter().filter(|s| s.group().is_none()) {
        let row = cx
            .debug_bounds(leak(section.selector()))
            .unwrap_or_else(|| panic!("missing section {section:?}"));
        assert!(row.top() >= last_bottom, "{section:?} above every group");
        last_bottom = row.bottom();
    }
    for group in Group::ALL {
        let label = cx
            .debug_bounds(leak(format!("settings-group-{}", group.slug())))
            .unwrap_or_else(|| panic!("missing group {group:?}"));
        assert!(
            label.top() >= last_bottom,
            "{group:?} below the previous row"
        );
        last_bottom = label.bottom();
        for section in Section::ALL.iter().filter(|s| s.group() == Some(group)) {
            let row = cx
                .debug_bounds(leak(section.selector()))
                .unwrap_or_else(|| panic!("missing section {section:?}"));
            assert!(row.top() >= last_bottom, "{section:?} under {group:?}");
            last_bottom = row.bottom();
        }
    }
    assert!(
        cx.debug_bounds("settings-footer-save").is_none(),
        "no pending drafts, no Save now"
    );
}

#[test]
fn every_section_icon_loads_through_the_registry() {
    use gpui::AssetSource;
    for section in Section::ALL {
        let loaded = crate::icons::Icons.load(section.icon());
        assert!(
            matches!(loaded, Ok(Some(_))),
            "{section:?} icon {} is not in the registry",
            section.icon()
        );
    }
}
