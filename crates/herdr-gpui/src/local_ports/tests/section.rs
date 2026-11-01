//! The Ports section and its confirmation against a headless window.
use super::*;
use crate::{
    HerdrWindow,
    menu::Page,
    sidebar::{
        ActivityPanel,
        layout_tests::{SidebarFixture, fixture_window, full_draw},
    },
};
use gpui::{AppContext as _, Entity, Modifiers, VisualTestContext, px, size};

fn fixture(
    cx: &mut gpui::TestAppContext,
    listing: Vec<Listener>,
) -> (Entity<HerdrWindow>, &mut VisualTestContext, Channels) {
    let (ports, updates, commands) = LocalPorts::fake();
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut view = fixture_window(window, cx);
            view.config.sidebar_style.activity_bar = true;
            view.local_ports = ports;
            view.local_ports.seed(listing);
            view
        });
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    cx.simulate_resize(size(px(1100.), px(700.)));
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.show_activity_panel(ActivityPanel::Ports, window, cx)
        });
        window.refresh();
        full_draw(window, cx).clear(cx);
    });
    (
        view,
        cx,
        Channels {
            commands,
            _updates: updates,
        },
    )
}

/// The fake worker's ends: its commands, and the answers it never sends,
/// kept open so the window does not take it for stopped.
struct Channels {
    commands: mpsc::Receiver<Command>,
    _updates: mpsc::SyncSender<Update>,
}

fn redraw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        full_draw(window, cx).clear(cx);
    });
}

#[gpui::test]
fn ports_list_each_listener_across_the_window(cx: &mut gpui::TestAppContext) {
    let mut paused = listener(8080, 4343, 20);
    paused.paused = true;
    let (_, cx, _channels) = fixture(cx, vec![listener(3000, 4242, 10), paused]);
    let section = cx.debug_bounds("local-ports-section").unwrap();
    assert_eq!(
        section.left(),
        cx.debug_bounds("activity-bar").unwrap().right()
    );
    assert!(cx.debug_bounds("local-ports-refresh").is_some() || !SUPPORTED);
    for selector in [
        "local-port-0",
        "local-port-0-open",
        "local-port-0-pause",
        "local-port-0-stop",
        "local-port-1",
        "local-port-1-resume",
        "local-port-1-stop",
    ] {
        let bounds = cx.debug_bounds(selector);
        assert!(bounds.is_some(), "{selector}");
        assert!(section.contains(&bounds.unwrap().origin), "{selector}");
    }
    assert!(cx.debug_bounds("local-port-1-pause").is_none());
    assert!(cx.debug_bounds("local-port-0-resume").is_none());
}

#[gpui::test]
fn nothing_listening_says_so(cx: &mut gpui::TestAppContext) {
    let (_, cx, _channels) = fixture(cx, Vec::new());
    assert!(cx.debug_bounds("local-ports-empty").is_some());
    assert!(cx.debug_bounds("local-port-0").is_none());
}

#[gpui::test]
fn stopping_asks_first_and_signals_only_once_confirmed(cx: &mut gpui::TestAppContext) {
    let (view, cx, channels) = fixture(cx, vec![listener(3000, 4242, 10)]);
    let stop = cx.debug_bounds("local-port-0-stop").unwrap().center();
    cx.simulate_click(stop, Modifiers::default());
    redraw(cx);
    assert!(view.read_with(cx, |v, _| v.menu.page == Some(Page::PortSignal)));
    assert!(cx.debug_bounds("port-signal-row-0").is_some());
    assert!(channels.commands.try_recv().is_err());
    let cancel = cx.debug_bounds("port-signal-cancel").unwrap().center();
    cx.simulate_click(cancel, Modifiers::default());
    redraw(cx);
    assert!(view.read_with(cx, |v, _| v.menu.page.is_none()));
    assert!(channels.commands.try_recv().is_err());

    cx.simulate_click(stop, Modifiers::default());
    redraw(cx);
    let confirm = cx.debug_bounds("port-signal-confirm").unwrap().center();
    cx.simulate_click(confirm, Modifiers::default());
    assert_eq!(
        channels.commands.try_recv().unwrap(),
        Command::Signal(
            PortAction::Stop,
            vec![Identity {
                pid: 4242,
                started: 10
            }]
        )
    );
    assert!(view.read_with(cx, |v, _| v.menu.page.is_none() && v.local_ports.acting()));
}

#[gpui::test]
fn resuming_needs_no_question(cx: &mut gpui::TestAppContext) {
    let mut paused = listener(3000, 4242, 10);
    paused.paused = true;
    let (view, cx, channels) = fixture(cx, vec![paused]);
    let resume = cx.debug_bounds("local-port-0-resume").unwrap().center();
    cx.simulate_click(resume, Modifiers::default());
    assert!(view.read_with(cx, |v, _| v.menu.page.is_none()));
    assert_eq!(
        channels.commands.try_recv().unwrap(),
        Command::Signal(
            PortAction::Resume,
            vec![Identity {
                pid: 4242,
                started: 10
            }]
        )
    );
}
