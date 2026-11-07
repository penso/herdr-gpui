//! The tab menu's row that moves a terminal agent into a chat. No test here
//! reaches an idle agent: that would start the worker, which runs `herdr`.
use super::*;
use crate::{chat::Program, state::ConnectionStatus};
use sheprd_chat_protocol::TOKEN;
use sheprd_client::protocol::{AgentStatus, ClientShellAgent};

/// The fixture's agent in `w1:t1`, and an unfocused tab `w1:t2` with `agent`
/// in it, on a window that can reach its daemon's chats.
fn window<'a>(
    agent: &str,
    status: AgentStatus,
    cx: &'a mut TestAppContext,
) -> (Entity<SheprdWindow>, &'a mut VisualTestContext) {
    let (agent, mut snapshot) = (agent.to_owned(), fixture());
    let mut tab = snapshot.tabs[0].clone();
    tab.tab_id = "w1:t2".into();
    tab.focused = false;
    snapshot.tabs.push(tab);
    let mut pane = snapshot.panes[0].clone();
    pane.pane_id = "w1:p2".into();
    pane.tab_id = "w1:t2".into();
    pane.focused = false;
    snapshot.panes.push(pane);
    let mut other = snapshot.agents[0].clone();
    other.pane_id = "w1:p2".into();
    other.tab_id = "w1:t2".into();
    other.agent = Some(agent);
    other.agent_status = status;
    other.focused = false;
    snapshot.agents.push(other);
    cx.add_window_view(move |window, cx| {
        crate::bind_keys(cx);
        let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
        view.live.snapshot = Some(Arc::new(snapshot));
        view.live.status = ConnectionStatus::Connected;
        view.live.local_daemon_peer = true;
        view.endpoints[0].connection.target = sheprd_client::ConnectTarget::Local;
        view.config.features.acp_chats = true;
        view.chats.program = Program::Found("/opt/herdr-acp".into());
        view
    })
}

fn fixture() -> ClientShellSnapshot {
    serde_json::from_str(include_str!(
        "../../../../sheprd-protocol/tests/fixtures/endpoint-snapshot-v1.json"
    ))
    .unwrap()
}

/// The pane the menu of `tab` would move, once opened.
fn offered(view: &Entity<SheprdWindow>, tab: &str, cx: &mut VisualTestContext) -> Option<String> {
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.dismiss_menu(window, cx);
            v.open_tab_menu(tab, None, Point::default(), window, cx);
            let menu = v.menu.tab.as_ref().unwrap();
            assert_eq!(
                menu.agent_pane.is_some(),
                menu.actions().contains(&Action::MoveToChat)
            );
            menu.agent_pane.clone()
        })
    })
}

#[gpui::test]
fn the_row_names_the_right_clicked_tabs_pane_not_the_focused_one(cx: &mut TestAppContext) {
    let (view, cx) = window("codex", AgentStatus::Idle, cx);
    assert_eq!(offered(&view, "w1:t2", cx).as_deref(), Some("w1:p2"));
    assert_eq!(offered(&view, "w1:t1", cx).as_deref(), Some("w1:p1"));
}

#[gpui::test]
fn the_row_sits_above_mute_and_close(cx: &mut TestAppContext) {
    let (view, cx) = window("codex", AgentStatus::Idle, cx);
    offered(&view, "w1:t2", cx);
    view.read_with(cx, |v, _| {
        let actions = v.menu.tab.as_ref().unwrap().actions();
        let tail = &actions[actions.len() - 3..];
        assert_eq!(tail, [Action::MoveToChat, Action::Mute, Action::Close]);
    });
}

#[gpui::test]
fn a_tab_with_several_agents_moves_its_focused_one(cx: &mut TestAppContext) {
    let (view, cx) = window("codex", AgentStatus::Idle, cx);
    let share_tab = |focused: &str, v: &mut SheprdWindow| {
        let snapshot = Arc::make_mut(v.live.snapshot.as_mut().unwrap());
        for agent in &mut snapshot.agents {
            agent.tab_id = "w1:t1".into();
            agent.focused = agent.pane_id == focused;
        }
    };
    view.update(cx, |v, _| share_tab("w1:p2", v));
    assert_eq!(offered(&view, "w1:t1", cx).as_deref(), Some("w1:p2"));
    view.update(cx, |v, _| share_tab("none", v));
    assert_eq!(offered(&view, "w1:t1", cx).as_deref(), Some("w1:p1"));
}

/// The agent of `w1:t2`.
fn agent(view: &mut SheprdWindow) -> &mut ClientShellAgent {
    &mut Arc::make_mut(view.live.snapshot.as_mut().unwrap()).agents[1]
}

#[gpui::test]
fn no_row_for_what_waiting_would_not_fix(cx: &mut TestAppContext) {
    let (view, cx) = window("claude", AgentStatus::Idle, cx);
    assert!(offered(&view, "w1:t2", cx).is_some());

    view.update(cx, |v, _| agent(v).agent = Some("gemini".into()));
    assert_eq!(offered(&view, "w1:t2", cx), None, "Herdr has no session");
    view.update(cx, |v, _| agent(v).agent = None);
    assert_eq!(offered(&view, "w1:t2", cx), None, "no agent in the pane");
    view.update(cx, |v, _| {
        agent(v).agent = Some("claude".into());
        agent(v).tokens = vec![(TOKEN.to_owned(), "a".repeat(32))];
    });
    assert_eq!(offered(&view, "w1:t2", cx), None, "already a chat");
    view.update(cx, |v, _| agent(v).tokens.clear());
    assert!(offered(&view, "w1:t2", cx).is_some());

    view.update(cx, |v, _| v.config.features.acp_chats = false);
    assert_eq!(offered(&view, "w1:t2", cx), None, "chats are off");
    view.update(cx, |v, _| {
        v.config.features.acp_chats = true;
        v.live.local_daemon_peer = false;
    });
    assert_eq!(offered(&view, "w1:t2", cx), None, "no route to its chats");
}

#[gpui::test]
fn a_busy_agent_keeps_the_row_and_refuses_inside_the_menu(cx: &mut TestAppContext) {
    let (view, cx) = window("claude", AgentStatus::Working, cx);
    assert_eq!(offered(&view, "w1:t2", cx).as_deref(), Some("w1:p2"));
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.activate_tab_menu(Action::MoveToChat, window, cx);
            assert_eq!(v.menu.page, Some(Page::Tab), "the menu stays open");
            let error = v.menu.tab.as_ref().unwrap().error.clone().unwrap();
            assert!(error.contains("Claude"), "{error}");
            assert!(v.chats.converting.is_none(), "the agent was not touched");
        })
    });
}
