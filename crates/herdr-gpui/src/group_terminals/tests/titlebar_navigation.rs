use super::*;
use crate::endpoint::Step;

/// Two connected groups: the left shows A; the active right shows B, reached
/// by going Back along A → B → C. Activating the left would erase C.
fn split_history<'a>(
    cx: &'a mut TestAppContext,
    collapsed: Option<&str>,
) -> (
    Entity<HerdrWindow>,
    &'a mut VisualTestContext,
    MockPeer,
    MockPeer,
) {
    let (view, cx) = window(cx);
    cx.simulate_resize(size(px(1600.), px(600.)));
    draw(cx);
    let active_peer = MockPeer::advertising(&["pane.focus"]);
    let parked_peer = MockPeer::advertising(&["pane.focus"]);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            if let Some(mode) = collapsed {
                view.settings.shared = Some(
                    crate::herdr_settings::Settings::parse_text(&format!(
                        "[ui]\nsidebar_collapsed_mode = '{mode}'"
                    ))
                    .unwrap(),
                );
                view.toggle_sidebar();
            }
            view.command(Command::SplitEditor, window, cx);
            let slots = view.group_slots();
            let (left, right) = (slots[0].id, slots[1].id);
            let layout = view.ensure_layout().unwrap();
            layout.choose(left, Pick::Herdr("t1".into()));
            layout.activate(right);

            let fixture = fixture_snapshot();
            let mut shown = (**view.live.snapshot.as_ref().unwrap()).clone();
            shown.boot_id = fixture.boot_id;
            shown.panes = ["a", "b", "c"]
                .into_iter()
                .map(|id| {
                    let mut pane = fixture.panes[0].clone();
                    pane.pane_id = id.into();
                    pane.workspace_id = "w0".into();
                    pane.tab_id = if id == "a" { "t1" } else { "t0" }.into();
                    pane
                })
                .collect();
            let endpoint = &mut view.endpoints[0];
            for pane in ["a", "b", "c"] {
                shown.focused_pane_id = Some(pane.into());
                endpoint.history.observe(Some(&shown), false);
            }
            endpoint.history.begin(1);
            shown.focused_pane_id = Some("b".into());
            endpoint.history.observe(Some(&shown), false);
            view.live.surface = Some(surface(&shown, "b"));
            view.live.snapshot = Some(Arc::new(shown.clone()));
            let mut connection = ConnectionBridge::new(endpoint.connection.target.clone());
            connection.handle = Some(active_peer.client.handle.clone());
            endpoint.trade_connection(&mut connection, &mut view.live.clone());

            shown.focused_tab_id = Some("t1".into());
            shown.focused_pane_id = Some("a".into());
            let key = view.browser_key().unwrap();
            let target = view.endpoints[0].connection.target.clone();
            view.browser
                .terminals
                .parked
                .push(parked(left, key, target, &parked_peer, shown));
            assert_eq!(view.primary_group(), Some(right));
            cx.notify();
        })
    });
    draw(cx);
    // Painting sets the requested geometry; model the daemon having supplied
    // that size before the click so navigation is ready to queue.
    view.update(cx, |view, _| {
        let frame = &mut Arc::make_mut(view.live.surface.as_mut().unwrap()).frame;
        frame.width = view.options.surface_size.cols;
        frame.height = view.options.surface_size.rows;
        view.endpoints[0].live = view.live.clone();
        assert!(view.input_ready());
    });
    (view, cx, active_peer, parked_peer)
}

fn navigation_click(cx: &mut TestAppContext, step: Step) {
    for collapsed in [None, Some("compact"), Some("hidden")] {
        let (view, cx, mut peer, _parked_peer) = split_history(cx, collapsed);
        let active = view.read_with(cx, |view, _| view.active_group());
        let (selector, destination) = match step {
            Step::Back => ("titlebar-back", "a"),
            Step::Forward => ("titlebar-forward", "c"),
        };
        let button = cx.debug_bounds(selector).unwrap();
        cx.simulate_click(button.center(), Modifiers::default());
        view.read_with(cx, |view, _| {
            assert_eq!(view.active_group(), active, "{step:?}, {collapsed:?}");
            assert_eq!(view.primary_group(), active);
            assert_eq!(
                view.live
                    .snapshot
                    .as_ref()
                    .unwrap()
                    .focused_pane_id
                    .as_deref(),
                Some("b")
            );
        });
        let request = peer.request();
        assert_eq!(request["method"], "pane.focus");
        assert_eq!(request["params"]["pane_id"], destination);

        // Land the requested step, then return to B. Both sides of the
        // original trail must still be available, including C after Back.
        view.update(cx, |view, _| {
            let mut shown = (**view.live.snapshot.as_ref().unwrap()).clone();
            let history = &mut view.endpoints[0].history;
            shown.focused_pane_id = Some(destination.into());
            history.observe(Some(&shown), false);
            let opposite = match step {
                Step::Back => Step::Forward,
                Step::Forward => Step::Back,
            };
            let (index, pane) = history.peek(opposite, &shown).unwrap();
            assert_eq!(pane, "b");
            history.begin(index);
            shown.focused_pane_id = Some("b".into());
            history.observe(Some(&shown), false);
            assert_eq!(history.peek(Step::Back, &shown), Some((0, "a")));
            assert_eq!(history.peek(Step::Forward, &shown), Some((2, "c")));
        });
    }
}

#[gpui::test]
fn titlebar_back_keeps_the_active_group_and_forward_history(cx: &mut TestAppContext) {
    navigation_click(cx, Step::Back);
}

#[gpui::test]
fn titlebar_forward_keeps_the_active_group_and_destination(cx: &mut TestAppContext) {
    navigation_click(cx, Step::Forward);
}

#[gpui::test]
fn disabled_navigation_does_not_activate_a_group_but_its_terminal_does(cx: &mut TestAppContext) {
    let (view, cx, _peer, _parked_peer) = split_history(cx, None);
    view.update(cx, |view, cx| {
        let endpoint = &mut view.endpoints[0];
        endpoint.history = Default::default();
        endpoint.sync_live();
        assert!(!view.can_travel(Step::Back));
        assert!(!view.can_travel(Step::Forward));
        cx.notify();
    });
    draw(cx);
    let active = view.read_with(cx, |view, _| view.active_group());
    for selector in ["titlebar-back", "titlebar-forward"] {
        let button = cx.debug_bounds(selector).unwrap();
        cx.simulate_click(button.center(), Modifiers::default());
        view.read_with(cx, |view, _| {
            assert_eq!(view.active_group(), active);
            assert_eq!(view.primary_group(), active);
            assert!(!view.can_travel(Step::Back));
            assert!(!view.can_travel(Step::Forward));
        });
    }
    let terminal = cx.debug_bounds("parked-terminal").unwrap();
    cx.simulate_click(terminal.center(), Modifiers::default());
    view.read_with(cx, |view, _| {
        assert_ne!(view.active_group(), active);
        assert_eq!(view.primary_group(), view.active_group());
        assert_eq!(view.focused_herdr_tab(), Some("t1"));
    });
}
