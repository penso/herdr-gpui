//! A busy section's icon pulses in the window's mark layer, so its frames
//! redraw the layer and never the cached sidebar the bar sits in, and its
//! count still sits on the icon's corner, over the pulse.
use crate::sidebar::layout_tests::{fixture_window, run_frames, snapshot};
use crate::state::ConnectionStatus;
use gpui::px;
use herdr_client::protocol::AgentStatus;
use std::{sync::Arc, time::Duration};

/// Overview busy with one agent working and counting one blocked agent.
#[gpui::test]
fn a_busy_icon_pulses_with_its_count_without_rebuilding_the_sidebar(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        view.config.sidebar_style.activity_bar = true;
        let mut shown = snapshot(2);
        shown.agents[0].agent_status = AgentStatus::Blocked;
        view.live.snapshot = Some(Arc::new(shown));
        // Overview counts only a connected host's agents.
        view.live.status = ConnectionStatus::Connected;
        view
    });
    // The window's tick counts the agents for the bar.
    view.update(cx, |view, cx| view.poll_overview(cx));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds("activity-overview-busy").is_some(),
        "the working agent makes Overview busy"
    );
    let bar = cx.debug_bounds("activity-bar").unwrap();
    let glyph = cx.debug_bounds("activity-overview-glyph").unwrap();
    // The blocked agent is counted on the busy icon.
    let bubble = cx.debug_bounds("activity-overview-bubble").unwrap();
    assert!(bubble.right() <= bar.right() - px(1.), "{bubble:?}");
    assert!(bubble.left() < glyph.right(), "{bubble:?}");
    assert!(bubble.top() < glyph.top(), "{bubble:?}");

    let (sidebar, layer) = cx.update(|_, cx| {
        let view = view.read(cx);
        (view.sidebar_view.clone(), view.mark_layer.clone())
    });
    let (renders, layer_renders) =
        cx.update(|_, cx| (sidebar.read(cx).renders, layer.read(cx).renders));
    run_frames(cx, 4, Duration::from_millis(150));
    cx.update(|_, cx| {
        assert_eq!(
            sidebar.read(cx).renders,
            renders,
            "the pulse's frames left the sidebar alone"
        );
        assert!(layer.read(cx).renders >= layer_renders + 4);
    });
    assert!(cx.debug_bounds("activity-overview-busy").is_some());
    assert!(cx.debug_bounds("activity-overview-bubble").is_some());
}
