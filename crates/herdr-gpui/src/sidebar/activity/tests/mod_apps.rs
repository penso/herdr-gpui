use super::*;
use crate::sidebar::layout_tests::{SidebarFixture, fixture_window, full_draw};
use gpui::{AppContext as _, Entity, Modifiers, VisualTestContext, px, size};
use herdr_mods::{Activity, Manifest, Needs, Node};

/// A window with the bar on and `apps` mod apps, `height` tall.
fn fixture(
    cx: &mut gpui::TestAppContext,
    apps: usize,
    height: f32,
) -> (Entity<HerdrWindow>, &mut VisualTestContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let view = cx.new(|cx| {
            let mut view = fixture_window(window, cx);
            view.config.sidebar_style.activity_bar = true;
            for index in 0..apps {
                let name = format!("app{index:02}");
                view.mods.insert_drawn(
                    Manifest {
                        name: name.clone(),
                        places: Vec::new(),
                        commands: Vec::new(),
                        activity: Some(Activity {
                            title: format!("App {index}"),
                            icon: "icon.svg".into(),
                        }),
                        panel: herdr_mods::Panel {
                            title: None,
                            order: 500,
                        },
                        needs: Needs::default(),
                    },
                    Some(Node::Spacer),
                );
            }
            view
        });
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    cx.simulate_resize(size(px(1000.), px(height)));
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    draw(cx);
    (view, cx)
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        full_draw(window, cx).clear(cx);
    });
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx.debug_bounds(selector).unwrap();
    cx.simulate_click(bounds.center(), Modifiers::default());
    draw(cx);
}

/// More app icons than a short window holds scroll, and Settings stays in
/// the bar under them.
#[gpui::test]
fn many_mod_apps_scroll_and_keep_settings_in_reach(cx: &mut gpui::TestAppContext) {
    let (_view, cx) = fixture(cx, 32, 400.);
    let bar = cx.debug_bounds("activity-bar").unwrap();
    let tabs = cx.debug_bounds("activity-tabs").unwrap();
    let settings = cx.debug_bounds("activity-settings").unwrap();
    assert!(settings.size.height > px(0.));
    assert!(
        settings.bottom() <= bar.bottom(),
        "{settings:?} past {bar:?}"
    );
    assert!(
        tabs.bottom() <= settings.top(),
        "{tabs:?} over {settings:?}"
    );
}

/// Picking an app's icon shows it; a section picked, or a command a mod
/// runs, gives the window back.
#[gpui::test]
fn a_mod_app_steps_aside_for_sections_and_commands(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx, 1, 700.);
    click(cx, "activity-mod-app00");
    assert!(cx.debug_bounds("mod-app-app00").is_some());
    assert!(view.read_with(cx, |view, _| view.mod_app() == Some("app00")));

    click(cx, "activity-overview");
    assert!(cx.debug_bounds("mod-app-app00").is_none());
    assert!(view.read_with(cx, |view, _| view.activity.app.is_none()));

    click(cx, "activity-mod-app00");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.run_mod_command(crate::controls::Command::ToggleStatusBar, window, cx);
        });
    });
    draw(cx);
    assert!(cx.debug_bounds("mod-app-app00").is_none());
    assert!(view.read_with(cx, |view, _| view.mod_app().is_none()));
}

/// Leaving an app over Workspaces gives the keyboard back to the terminal,
/// even for a command that moves no focus of its own.
#[gpui::test]
fn leaving_a_mod_app_gives_the_terminal_the_keyboard(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx, 1, 700.);
    click(cx, "activity-workspaces");
    click(cx, "activity-mod-app00");
    let focused =
        |cx: &mut VisualTestContext| cx.update(|window, cx| view.read(cx).focus.is_focused(window));
    assert!(!focused(cx), "the app holds the keyboard");
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            // Moves no focus of its own, unlike most window commands.
            view.run_mod_command(crate::controls::Command::EqualizeSplits, window, cx);
        });
    });
    draw(cx);
    assert!(focused(cx), "the terminal has the keyboard again");
}

/// An app whose view returned nil is done: it shows no loading text.
#[gpui::test]
fn an_app_that_draws_nothing_is_not_loading(cx: &mut gpui::TestAppContext) {
    let (view, cx) = fixture(cx, 1, 700.);
    view.update(cx, |view, _| {
        view.mods.insert_drawn(
            Manifest {
                name: "app00".into(),
                places: Vec::new(),
                commands: Vec::new(),
                activity: Some(Activity {
                    title: "App 0".into(),
                    icon: "icon.svg".into(),
                }),
                panel: herdr_mods::Panel {
                    title: None,
                    order: 500,
                },
                needs: Needs::default(),
            },
            None,
        );
    });
    click(cx, "activity-mod-app00");
    assert!(cx.debug_bounds("mod-app-app00").is_some());
    assert!(cx.debug_bounds("mod-app-loading").is_none());
}
