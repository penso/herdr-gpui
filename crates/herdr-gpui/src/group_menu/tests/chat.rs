use super::*;
use crate::chat::Program;
use herdr_chat_protocol::Agent;

fn chats(actions: &[Action]) -> Vec<Agent> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Chat(agent) => Some(*agent),
            _ => None,
        })
        .collect()
}

/// The window's daemon as this user's own, local one.
fn local(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) {
    view.update(cx, |view, _| {
        view.config.features.acp_chats = true;
        view.endpoints[0].connection.target = herdr_client::ConnectTarget::Local;
        view.live.local_daemon_peer = true;
        view.live.status = crate::state::ConnectionStatus::Connected;
    });
}

/// Records the host's installed agents as `herdr-acp agents` answered.
fn installed(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, agents: &[Agent]) {
    view.update(cx, |view, _| {
        let id = view.endpoints[0].id.clone();
        view.chats.agents.record(&id, agents.to_vec());
    });
}

#[gpui::test]
fn the_menu_offers_a_chat_per_installed_agent(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    local(&view, cx);
    let group = groups(&view, cx)[0];
    view.update(cx, |view, _| view.chats.program = Program::Missing);
    installed(&view, cx, Agent::ALL);
    assert!(chats(&actions(&view, cx, group)).is_empty());
    view.update(cx, |view, _| {
        view.chats.program = Program::Found("/opt/herdr-acp".into());
    });
    installed(&view, cx, &[Agent::Copilot, Agent::Opencode]);
    assert_eq!(
        chats(&actions(&view, cx, group)),
        [Agent::Copilot, Agent::Opencode]
    );
    installed(&view, cx, Agent::ALL);
    let listed = actions(&view, cx, group);
    assert_eq!(chats(&listed), Agent::ALL);
    let first = listed
        .iter()
        .position(|action| matches!(action, Action::Chat(_)))
        .unwrap();
    // After the tabs to open, under a heading of their own.
    assert!(listed[..first].iter().all(|action| action.section() == 0));
    let row = &listed[first];
    assert_eq!(row.label(), Agent::ALL[0].display_name());
    assert_eq!(row.heading().as_deref(), Some("New Chat"));
    assert_eq!(
        row.icon(),
        Some(crate::icons::AgentIcon::from_identity(Some(Agent::ALL[0].name())).path())
    );
    assert_eq!(child_index(&listed, first), first + 1);
}

#[gpui::test]
fn an_ssh_host_offers_the_agents_it_has_once_it_answered(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    let group = groups(&view, cx)[0];
    view.update(cx, |view, _| {
        view.config.features.acp_chats = true;
        view.chats.program = Program::Missing;
        view.endpoints[0].connection.target = herdr_client::ConnectTarget::Ssh {
            target: "host".into(),
            session: "default".into(),
        };
        view.live.status = crate::state::ConnectionStatus::Connected;
    });
    assert!(
        chats(&actions(&view, cx, group)).is_empty(),
        "not asked yet"
    );
    installed(&view, cx, &[Agent::Claude]);
    assert_eq!(chats(&actions(&view, cx, group)), [Agent::Claude]);
}

#[test]
fn each_headed_section_adds_one_child_before_its_rows() {
    let actions = [
        Action::NewBrowserTab,
        Action::Chat(Agent::Claude),
        Action::Chat(Agent::Codex),
        Action::Port {
            number: 3000,
            process: "node".into(),
            link: Link::Page(crate::browser::WebUrl::try_from("http://localhost:3000/").unwrap()),
        },
        Action::Split,
    ];
    assert_eq!(child_index(&actions, 0), 0);
    assert_eq!(child_index(&actions, 1), 2);
    assert_eq!(child_index(&actions, 2), 3);
    assert_eq!(child_index(&actions, 3), 5);
    assert_eq!(child_index(&actions, 4), 6);
}
