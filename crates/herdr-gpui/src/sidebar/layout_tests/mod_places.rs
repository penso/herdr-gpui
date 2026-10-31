use super::*;
use gpui::Entity;
use herdr_mods::{Container, Gap, Manifest, Needs, Node, Place, Size, Text, Tone, Weight};

fn text(text: &str) -> Node {
    Node::Text(Text {
        text: text.to_owned(),
        tone: Tone::Normal,
        size: Size::Normal,
        weight: Weight::Normal,
        truncate: false,
    })
}

fn manifest(name: &str, place: Place) -> Manifest {
    Manifest {
        name: name.to_owned(),
        places: vec![place],
        commands: Vec::new(),
        activity: None,
        panel: herdr_mods::Panel {
            title: None,
            order: 500,
        },
        needs: Needs::default(),
    }
}

/// A panel section titled `title` at `order`.
fn panel_mod(name: &str, title: &str, order: i32) -> Manifest {
    Manifest {
        panel: herdr_mods::Panel {
            title: Some(title.to_owned()),
            order,
        },
        ..manifest(name, Place::Panel)
    }
}

/// A window with panel sections at orders 300, 100, and 200, and one mod
/// whose view drew nothing; `hidden` hides the panel before it first shows.
fn panel_fixture(
    cx: &mut gpui::TestAppContext,
    hidden: bool,
) -> (Entity<HerdrWindow>, &mut gpui::VisualTestContext) {
    let (view, cx) = cx.add_window_view(move |window, cx| {
        let mut view = fixture_window(window, cx);
        view.mods.set_panel_hidden(hidden);
        for (name, order) in [("third", 300), ("first", 100), ("second", 200)] {
            view.mods.insert_drawn(
                panel_mod(name, name, order),
                Some(text(&format!("{name} body"))),
            );
        }
        view.mods
            .insert_drawn(panel_mod("empty", "Empty", 50), None);
        view.update_mod_panel();
        view
    });
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    (view, cx)
}

/// Sections sit in a panel on the window's right, in their order; a mod
/// that drew nothing has none.
#[gpui::test]
fn panel_sections_sit_beside_the_terminals_in_order(cx: &mut gpui::TestAppContext) {
    let (_view, cx) = panel_fixture(cx, false);
    let panel = cx.debug_bounds("mod-panel").unwrap();
    let window = cx.update(|window, _| window.viewport_size());
    assert!(panel.right() <= window.width + px(1.), "{panel:?}");
    assert!(panel.size.width >= px(220.), "{panel:?}");
    let top =
        |cx: &mut gpui::VisualTestContext, name: &'static str| cx.debug_bounds(name).unwrap().top();
    let (first, second, third) = (
        top(cx, "mod-panel-first"),
        top(cx, "mod-panel-second"),
        top(cx, "mod-panel-third"),
    );
    assert!(
        first < second && second < third,
        "{first:?} {second:?} {third:?}"
    );
    assert!(cx.debug_bounds("mod-panel-empty").is_none());
}

/// A section's header folds it to the header and back.
#[gpui::test]
fn a_panel_header_folds_its_section(cx: &mut gpui::TestAppContext) {
    let (_view, cx) = panel_fixture(cx, false);
    assert!(cx.debug_bounds("mod-panel-first-body").is_some());
    let header = cx.debug_bounds("mod-panel-first-header").unwrap();
    cx.simulate_click(header.center(), Modifiers::default());
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("mod-panel-first-body").is_none());
    assert!(cx.debug_bounds("mod-panel-first-header").is_some());
    assert!(cx.debug_bounds("mod-panel-second-body").is_some());
}

/// A hidden panel takes no room, and Toggle Mod Panel brings it back.
#[gpui::test]
fn toggle_mod_panel_hides_and_shows_it(cx: &mut gpui::TestAppContext) {
    let (view, cx) = panel_fixture(cx, true);
    assert!(cx.debug_bounds("mod-panel").is_none());
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.command(crate::controls::Command::ToggleModPanel, window, cx);
        });
    });
    assert!(view.read_with(cx, |view, _| !view.mods.panel_hidden()));
}

/// Many tall sidebar mods share one area of at most 40% of the sidebar, so
/// the workspace and agent lists keep the rest.
#[gpui::test]
fn sidebar_mods_never_take_the_lists_height(cx: &mut gpui::TestAppContext) {
    let (_view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        for index in 0..5 {
            let lines = (0..12).map(|line| text(&format!("mod {index} line {line}")));
            let tree = Node::Column(Container {
                gap: Gap::Small,
                align: herdr_mods::Align::Stretch,
                children: lines.collect(),
            });
            view.mods.insert_drawn(
                manifest(&format!("tall{index}"), Place::Sidebar),
                Some(tree),
            );
        }
        view
    });
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let sidebar = cx.debug_bounds("sidebar").unwrap();
    let mods = cx.debug_bounds("mod-sidebar").unwrap();
    assert!(
        mods.size.height <= sidebar.size.height * 0.4 + px(1.),
        "{mods:?} in {sidebar:?}"
    );
    let spaces = cx.debug_bounds("spaces-section").unwrap();
    assert!(spaces.size.height > px(80.), "{spaces:?}");
    // The footer under the mods still shows inside the sidebar.
    assert!(mods.bottom() <= sidebar.bottom());
}

/// Wide status mods share one area that gives way before the app's own
/// controls.
#[gpui::test]
fn status_mods_never_push_out_the_apps_controls(cx: &mut gpui::TestAppContext) {
    let (_view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        for index in 0..4 {
            let label = format!("a very wide status bar item number {index} with a long label");
            view.mods.insert_drawn(
                manifest(&format!("wide{index}"), Place::StatusBar),
                Some(text(&label)),
            );
        }
        view
    });
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let area = cx.debug_bounds("mod-status").unwrap();
    assert!(area.size.width <= px(360.), "{area:?}");
    let bar = cx.debug_bounds("connection-status").unwrap();
    let theme = cx.debug_bounds("status-theme").unwrap();
    assert!(theme.size.width > px(0.));
    assert!(theme.right() <= bar.right(), "{theme:?} past {bar:?}");
}

/// In a narrow window beside a wide sidebar the panel narrows, then steps
/// aside, so the terminals always keep room.
#[gpui::test]
fn the_panel_leaves_the_terminals_room(cx: &mut gpui::TestAppContext) {
    for (window_width, sidebar) in [(640., 360.), (900., 300.), (1400., 280.)] {
        let (view, cx) = panel_fixture(cx, false);
        view.update(cx, |view, _| view.sidebar_width = Some(sidebar));
        cx.simulate_resize(size(px(window_width), px(700.)));
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
        let Some(panel) = cx.debug_bounds("mod-panel") else {
            continue;
        };
        let side = cx.debug_bounds("sidebar").unwrap();
        let content = panel.left() - side.right();
        assert!(
            content >= px(239.),
            "{window_width}: {content:?} left beside {panel:?}"
        );
        assert!(panel.size.width >= px(160.), "{window_width}: {panel:?}");
    }
}

/// Focus Next Region reaches the panel, and Enter on a header folds it.
#[gpui::test]
fn a_panel_header_folds_from_the_keyboard(cx: &mut gpui::TestAppContext) {
    let (view, cx) = panel_fixture(cx, false);
    cx.update(|window, _| window.activate_window());
    for _ in 0..6 {
        let region = view.read_with(cx, |view, _| view.chrome_region);
        if region == Some(crate::window::chrome_focus::Region::ModPanel) {
            break;
        }
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.command(crate::controls::Command::FocusNextRegion, window, cx);
            });
        });
        for _ in 0..3 {
            cx.update(|window, cx| full_draw(window, cx).clear(cx));
            cx.run_until_parked();
        }
    }
    assert_eq!(
        view.read_with(cx, |view, _| view.chrome_region),
        Some(crate::window::chrome_focus::Region::ModPanel)
    );
    assert!(cx.debug_bounds("mod-panel-first-body").is_some());
    // The region's first stop takes focus on the next frame, which the
    // test window does not run; as the notices' test does, move it here.
    cx.update(|window, cx| window.focus_next(cx));
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    // The first header is the region's first stop; Enter and Space use it,
    // as with the app's own chrome controls.
    for (key, folded) in [("enter", true), ("space", false)] {
        let keystroke = gpui::Keystroke::parse(key).unwrap();
        cx.simulate_event(gpui::KeyDownEvent {
            keystroke: keystroke.clone(),
            is_held: false,
            prefer_character_input: false,
        });
        cx.simulate_event(gpui::KeyUpEvent { keystroke });
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
        assert_eq!(
            cx.debug_bounds("mod-panel-first-body").is_none(),
            folded,
            "{key}"
        );
    }
}
