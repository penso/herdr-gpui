//! `ui agent-group` against a headless window.
use super::*;
use crate::sidebar::layout_tests::fixture_window;

fn groups(cx: &mut gpui::VisualTestContext) -> (Option<String>, Vec<String>) {
    let reply = cx.cx.update(|cx| ui(call(UiOp::AgentGroups), cx));
    let Response::App {
        reply: Reply::AgentGroups { current, groups },
    } = reply
    else {
        panic!("groups answered {reply:?}");
    };
    (current, groups)
}

#[gpui::test]
fn ui_agent_group_lists_picks_and_clears(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        view.config.sidebar_style.coordinator_suffix = Some("-coord".into());
        let snapshot = std::sync::Arc::make_mut(view.live.snapshot.as_mut().unwrap());
        snapshot.agents[0].name = Some("demo-coord".into());
        view
    });
    assert_eq!(groups(cx), (None, vec!["demo".to_owned()]));

    let pick = |name: Option<&str>, cx: &mut gpui::VisualTestContext| {
        cx.cx.update(|cx| {
            ui(
                call(UiOp::AgentGroup {
                    name: name.map(str::to_owned),
                }),
                cx,
            )
        })
    };
    let picked = pick(Some("demo"), cx);
    assert!(matches!(picked, Response::Done { .. }), "{picked:?}");
    assert_eq!(groups(cx).0.as_deref(), Some("demo"));

    // A name no coordinator goes by is refused, and the view stays put.
    assert_eq!(code(&pick(Some("nope"), cx)), Some(ErrorCode::NotFound));
    assert_eq!(groups(cx).0.as_deref(), Some("demo"));

    let cleared = pick(None, cx);
    assert!(matches!(cleared, Response::Done { .. }), "{cleared:?}");
    assert_eq!(view.read_with(cx, |view, _| view.agent_group.clone()), None);
}

#[gpui::test]
fn ui_agent_group_says_so_when_the_feature_is_off(cx: &mut gpui::TestAppContext) {
    let (_view, cx) = cx.add_window_view(fixture_window);
    for op in [UiOp::AgentGroups, UiOp::AgentGroup { name: None }] {
        let refused = cx.cx.update(|cx| ui(call(op), cx));
        assert_eq!(code(&refused), Some(ErrorCode::Unsupported));
    }
}
