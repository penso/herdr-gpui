use super::*;
use crate::config::{StatusIcons, preferences::Preference};
use crate::settings_window::controls::preferences::PreferenceIo;
use std::sync::{Arc, Mutex};

type Edits = Arc<Mutex<Vec<Preference>>>;

fn indicators_page(
    cx: &mut TestAppContext,
) -> (Entity<SettingsWindow>, &mut VisualTestContext, Edits) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    let edits = Edits::default();
    let captured = edits.clone();
    view.update(cx, |view, cx| {
        view.controls.preference_io = Some(PreferenceIo {
            write: Arc::new(move |edit| {
                captured.lock().unwrap().push(edit);
                Ok(())
            }),
            load: Arc::new(skill_load),
        });
        view.section = Section::Indicators;
        cx.notify();
    });
    cx.simulate_resize(size(px(1100.), px(1400.)));
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    (view, cx, edits)
}

fn card(set: StatusIcons) -> &'static str {
    Box::leak(format!("indicator-set-{}", set.as_str()).into_boxed_str())
}

/// The page lists only this client's sets, each previewed with the sidebar's
/// own marks, so every set's working mark moves here as it does there.
#[gpui::test]
fn every_set_has_a_card_whose_working_mark_moves(cx: &mut TestAppContext) {
    let (_, cx, _) = indicators_page(cx);
    let sets = cx.debug_bounds("indicators-sets").unwrap();
    for &set in StatusIcons::ALL {
        let bounds = cx
            .debug_bounds(card(set))
            .unwrap_or_else(|| panic!("{set:?}"));
        assert!(bounds.right() <= sets.right(), "{set:?} {bounds:?}");
    }
    for selector in [
        "status-orbit",
        "status-spin",
        "status-wave",
        "status-chase",
        "status-braille",
        "status-braille-fill",
        "status-orca-ring",
    ] {
        assert!(cx.debug_bounds(selector).is_some(), "no {selector} preview");
    }
    // Herdr's own Dots/Symbols style is the terminal client's, not offered here.
    assert!(cx.debug_bounds("notif-indicators-control-0").is_none());
}

/// Clicking a card saves its set, and clicking the set in use saves nothing.
#[gpui::test]
fn clicking_a_card_saves_its_set_once(cx: &mut TestAppContext) {
    let (view, cx, edits) = indicators_page(cx);
    let click = |set: StatusIcons, cx: &mut VisualTestContext| {
        let choice = cx.debug_bounds(card(set)).unwrap();
        cx.simulate_click(choice.center(), Default::default());
        cx.run_until_parked();
        std::mem::take(&mut *edits.lock().unwrap())
    };
    assert!(click(StatusIcons::Dots, cx).is_empty(), "the default saved");
    for &set in &StatusIcons::ALL[1..] {
        assert_eq!(click(set, cx), [Preference::StatusIcons(set)]);
        view.update(cx, |view, cx| {
            view.config.status_icons = set;
            cx.notify();
        });
        cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
        assert!(click(set, cx).is_empty(), "{set:?} saved again");
    }
}

/// The edge rail switch saves its flip; a pick made while the rail is on
/// writes the rail too, so a retired `rail` value cannot take it along.
#[gpui::test]
fn the_edge_rail_switch_stays_with_any_set(cx: &mut TestAppContext) {
    let (view, cx, edits) = indicators_page(cx);
    let toggle = cx.debug_bounds("indicators-edge-rail-toggle").unwrap();
    cx.simulate_click(toggle.center(), Default::default());
    cx.run_until_parked();
    assert_eq!(
        std::mem::take(&mut *edits.lock().unwrap()),
        [Preference::EdgeRail(true)]
    );
    view.update(cx, |view, cx| {
        view.config.edge_rail = true;
        view.config.status_icons = StatusIcons::Outline;
        cx.notify();
    });
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    let wave = cx.debug_bounds(card(StatusIcons::Wave)).unwrap();
    cx.simulate_click(wave.center(), Default::default());
    cx.run_until_parked();
    assert_eq!(
        std::mem::take(&mut *edits.lock().unwrap()),
        [
            Preference::EdgeRail(true),
            Preference::StatusIcons(StatusIcons::Wave)
        ]
    );
}

/// The Icons search result opens the page at the cards, which record their
/// bounds so search has somewhere to scroll.
#[gpui::test]
fn searching_for_the_icons_opens_the_cards(cx: &mut TestAppContext) {
    use crate::settings_window::{catalog, search::RowBounds};
    let (view, cx) = cx.add_window_view(skill_fixture);
    cx.simulate_resize(size(px(960.), px(560.)));
    view.update(cx, |view, cx| view.set_search_query("braille", cx));
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    let results = view.read_with(cx, |view, _| view.search_results.clone().unwrap());
    let index = results
        .iter()
        .position(|&entry| catalog::CATALOG[entry].id == "indicators-sets")
        .unwrap();
    let result: &'static str =
        Box::leak(format!("settings-search-result-{index}").into_boxed_str());
    let center = cx.debug_bounds(result).unwrap().center();
    cx.simulate_click(center, Default::default());
    for _ in 0..3 {
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            crate::sidebar::layout_tests::full_draw(window, cx).clear(cx);
        });
    }
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert_eq!(view.section, Section::Indicators));
    let tracked = cx.update(|_, cx| {
        cx.try_global::<RowBounds>()
            .is_some_and(|rows| rows.0.contains_key("indicators-sets"))
    });
    assert!(tracked, "the cards record no bounds for search");
}
