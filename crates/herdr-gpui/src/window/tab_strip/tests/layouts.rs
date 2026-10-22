use super::*;
use crate::{browser::Store, config::TabLayoutMode};

/// A window whose strip lists a working tab, a plain one, a zoomed one, and
/// a titled page, in `mode`.
fn strip(
    cx: &mut TestAppContext,
    mode: TabLayoutMode,
) -> (Entity<HerdrWindow>, &mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
        view.config.tab_layout = mode;
        let mut snapshot: herdr_client::protocol::ClientShellSnapshot = serde_json::from_str(
            include_str!("../../../../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"),
        )
        .unwrap();
        snapshot.tabs[0].label = "a title long enough to outgrow the narrowest tab".into();
        assert_eq!(snapshot.tabs[0].agent_status, AgentStatus::Working);
        for (id, status, zoomed) in [
            ("plain", AgentStatus::Unknown, false),
            ("zoomy", AgentStatus::Idle, true),
        ] {
            let mut tab = snapshot.tabs[0].clone();
            tab.tab_id = id.into();
            tab.label = format!("the {id} tab, labelled at length");
            tab.focused = false;
            tab.agent_status = status;
            tab.zoomed = zoomed;
            snapshot.tabs.push(tab);
        }
        view.live.snapshot = Some(Arc::new(snapshot));
        view
    });
    cx.simulate_resize(size(px(1600.), px(600.)));
    let page = cx.update(|_, cx| {
        let scope = crate::browser::scope(&view.read(cx).endpoints[0]);
        let page = Store::update(cx, |store| store.open(scope, "w1", None, None)).unwrap();
        Store::update(cx, |store| {
            store.visited(page, None, Some("Pull request #42"))
        });
        page
    });
    for _ in 0..2 {
        cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
        view.update(cx, |view, _| view.settle_tabs());
    }
    let _ = page;
    (view, cx)
}

#[gpui::test]
fn every_layout_draws_its_tabs_as_wide_as_it_measures_them(cx: &mut TestAppContext) {
    for &mode in TabLayoutMode::ALL {
        let (view, cx) = strip(cx, mode);
        let near = |a: Pixels, b: Pixels| (a - b).abs() <= px(0.5);
        let measured: Vec<(String, f32)> = cx.update(|window, cx| {
            let view = view.read(cx);
            let indicators = Indicators::new(
                None,
                crate::config::StatusIcons::default(),
                false,
                false,
                &view.theme,
            );
            let snapshot = view.live.snapshot.as_ref().unwrap();
            let mut widths: Vec<(String, f32)> = snapshot
                .tabs
                .iter()
                .filter(|tab| tab.workspace_id == "w1")
                .map(|tab| {
                    let shape = TabShape {
                        lead: herdr_mark(snapshot, tab).lead(),
                        zoomed: tab.zoomed,
                        selected: tab.focused,
                    };
                    let label = SharedString::from(tab.label.clone());
                    let width = view.tab_width(&label, shape, indicators, window);
                    (format!("tab-{}", tab.tab_id), width)
                })
                .collect();
            for (pick, label) in view.browser_tab_labels(cx) {
                let Pick::Page(id) = pick else { continue };
                let shape = TabShape {
                    lead: Lead::Icon,
                    zoomed: false,
                    selected: false,
                };
                let width = view.tab_width(&label, shape, indicators, window);
                widths.push((format!("browser-tab-{id}"), width));
            }
            widths
        });
        assert!(measured.len() >= 4, "{mode}: {measured:?}");
        for (selector, width) in measured {
            let selector: &'static str = Box::leak(selector.into_boxed_str());
            let tab = cx
                .debug_bounds(selector)
                .unwrap_or_else(|| panic!("{mode}: no {selector}"));
            assert!(
                near(tab.size.width, px(width)),
                "{mode}: {selector} drew {tab:?}, measured {width}"
            );
        }
        // The chosen tab's close button lies inside it, in every layout;
        // Compact draws none on the others.
        for id in ["w1:t1", "zoomy"] {
            let tab = cx.debug_bounds(Box::leak(format!("tab-{id}").into_boxed_str()));
            let close = cx.debug_bounds(Box::leak(format!("close-tab-{id}").into_boxed_str()));
            match (tab, close) {
                (Some(tab), Some(close)) => assert!(
                    close.left() >= tab.left() && close.right() <= tab.right(),
                    "{mode}: {close:?} in {tab:?}"
                ),
                (_, None) => assert!(
                    id != "w1:t1" && mode == TabLayoutMode::Compact,
                    "{mode}: {id} has no close button"
                ),
                (None, Some(_)) => panic!("{mode}: no tab {id}"),
            }
        }
        // Compact names only the chosen tab, so only it shows the zoom mark.
        assert_eq!(
            cx.debug_bounds("tab-zoom-zoomy").is_some(),
            mode != TabLayoutMode::Compact,
            "{mode}"
        );
        assert!(cx.debug_bounds("tab-zoom-plain").is_none(), "{mode}");
    }
}

#[test]
fn a_chosen_tab_reads_apart_from_the_rest_in_every_layout() {
    let theme = crate::config::Theme::default();
    for &mode in TabLayoutMode::ALL {
        let layout = crate::tab_layout::layout_for(mode);
        let other = layout.colors(TabState::default(), &theme);
        for active in [true, false] {
            let chosen = layout.colors(
                TabState {
                    selected: true,
                    active,
                },
                &theme,
            );
            assert_ne!(chosen, other, "{mode} active {active}");
        }
    }
}

#[gpui::test]
fn choosing_a_tab_layout_redraws_and_saves_it_once(cx: &mut TestAppContext) {
    let (view, cx) = strip(cx, TabLayoutMode::Classic);
    let saved = Arc::new(std::sync::Mutex::new(Vec::new()));
    for mode in [TabLayoutMode::Pills, TabLayoutMode::Pills] {
        let saved = saved.clone();
        view.update(cx, |view, cx| {
            view.set_tab_layout_with(
                mode,
                move |mode| {
                    saved.lock().unwrap().push(mode);
                    Ok(())
                },
                cx,
            );
        });
        cx.run_until_parked();
    }
    view.read_with(cx, |view, _| {
        assert_eq!(view.config.tab_layout, TabLayoutMode::Pills);
    });
    assert_eq!(*saved.lock().unwrap(), [TabLayoutMode::Pills]);
}

#[gpui::test]
fn pills_show_close_buttons_while_the_strip_takes_tab_focus(cx: &mut TestAppContext) {
    let (view, cx) = strip(cx, TabLayoutMode::Pills);
    // The pointer reveals a pill's close button over its status.
    assert!(cx.debug_bounds("tab-status-w1:t1").is_some());
    view.update(cx, |view, cx| {
        view.chrome_region = Some(crate::window::chrome_focus::Region::TitleBar);
        cx.notify();
    });
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    // The keyboard can land on it, so it shows in place of the status.
    assert!(cx.debug_bounds("tab-status-w1:t1").is_none());
    assert!(cx.debug_bounds("close-tab-w1:t1").is_some());
}

#[gpui::test]
fn a_tab_layout_read_from_the_config_file_moves_the_menu_check(cx: &mut TestAppContext) {
    let (view, cx) = strip(cx, TabLayoutMode::Classic);
    view.update(cx, |view, cx| {
        let mut config = view.config.clone();
        config.tab_layout = TabLayoutMode::Cards;
        let theme = view.theme.clone();
        view.load_gui_config_with(move || Ok((config, theme)), cx);
    });
    cx.run_until_parked();
    let checked: Vec<String> = cx.update(|_, cx| {
        let menus = cx.get_menus().unwrap();
        let view_menu = menus
            .iter()
            .find(|menu| menu.name.as_ref() == "View")
            .unwrap();
        view_menu
            .items
            .iter()
            .find_map(|item| match item {
                OwnedMenuItem::Submenu(menu) if menu.name.as_ref() == "Tab Layout" => Some(menu),
                _ => None,
            })
            .unwrap()
            .items
            .iter()
            .filter_map(|item| match item {
                OwnedMenuItem::Action {
                    name,
                    checked: true,
                    ..
                } => Some(name.clone()),
                _ => None,
            })
            .collect()
    });
    assert_eq!(checked, [TabLayoutMode::Cards.label()]);
}
