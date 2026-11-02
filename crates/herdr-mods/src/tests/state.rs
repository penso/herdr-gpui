use super::*;

#[test]
fn changes_are_per_topic() {
    let before = State::default();
    let after = State {
        focus: Some(Focus {
            host: "local".to_owned(),
            workspace: None,
            tab: None,
            name: None,
            path: None,
        }),
        ..State::default()
    };
    let changed = Topic::ALL
        .iter()
        .copied()
        .filter(|topic| before.changed(&after, *topic))
        .collect::<Vec<_>>();
    assert_eq!(changed, [Topic::Focus]);
}

#[test]
fn every_topic_reaches_the_view() {
    let watch = Topic::NAMES
        .iter()
        .map(|name| format!("\"{name}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let main = format!(
        "local ui = require(\"@herdr/ui\")\n\
         return herdr.mod {{ watch = {{ {watch} }}, view = function(_, app)\n\
           return ui.text(app.hosts[1].name .. app.workspaces[1].tabs[1].name\n\
             .. app.agents.idle[1].name .. app.focus.host .. app.usage[1].used\n\
             .. app.pull_request.number)\n\
         end }}"
    );
    let state = State {
        hosts: vec![Host {
            id: "h".to_owned(),
            name: "H".to_owned(),
            connected: true,
        }],
        workspaces: vec![Workspace {
            id: "w".to_owned(),
            host: "h".to_owned(),
            name: "W".to_owned(),
            focused: true,
            tabs: vec![Tab {
                id: "t".to_owned(),
                name: "T".to_owned(),
                focused: true,
            }],
        }],
        agents: vec![Agent {
            id: "p".to_owned(),
            host: "h".to_owned(),
            workspace: "w".to_owned(),
            name: "A".to_owned(),
            status: AgentStatus::Idle,
        }],
        focus: Some(Focus {
            host: "F".to_owned(),
            workspace: None,
            tab: None,
            name: None,
            path: None,
        }),
        usage: vec![Usage {
            provider: "claude".to_owned(),
            label: "Session".to_owned(),
            used: 0.5,
            resets_at: None,
        }],
        pull_request: Some(PullRequest {
            number: 7,
            title: "x".to_owned(),
            url: "https://example.com".to_owned(),
            state: "open".to_owned(),
            checks: None,
        }),
    };
    let node = load(&main)
        .unwrap()
        .view(&state, SystemTime::UNIX_EPOCH)
        .unwrap();
    assert_eq!(node, Some(text("HTAF0.57")));
}
