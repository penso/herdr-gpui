use super::*;
use herdr_client::scrollback::TextPoint;

fn commands() -> Vec<ClientShellCommand> {
    [
        ClientShellCommandAction::PluginAction,
        ClientShellCommandAction::Shell,
        ClientShellCommandAction::Pane,
        ClientShellCommandAction::Popup,
    ]
    .into_iter()
    .map(|action| ClientShellCommand {
        command_id: format!("{action:?}"),
        action,
        description: None,
        binding_label: String::new(),
        binding_labels: Vec::new(),
    })
    .collect()
}

fn selection(pane_id: &str) -> SelectionReadParams {
    SelectionReadParams {
        pane_id: pane_id.into(),
        anchor: TextPoint { row: 40, col: 3 },
        cursor: TextPoint { row: 38, col: 0 },
        content_revision: Some(8),
    }
}

/// Only a plugin action is sent the selection, as Herdr reads it for no
/// other kind; shell, pane, and popup commands keep their exact parameters.
#[test]
fn only_plugin_actions_are_sent_the_selection() {
    let mut snapshot = snapshot();
    snapshot.commands = commands();
    let pane = snapshot.focused_pane_id.clone().unwrap();
    let target = Target::capture(&snapshot, Some(selection(&pane)));
    for command in &snapshot.commands {
        let params = target
            .invocation(&snapshot, &command.command_id, command.action)
            .unwrap();
        let expected = (command.action == ClientShellCommandAction::PluginAction).then(|| {
            json!({
                "pane_id": pane,
                "anchor": {"row": 40, "col": 3},
                "cursor": {"row": 38, "col": 0},
                "content_revision": 8,
            })
        });
        assert_eq!(params.get("selection").cloned(), expected, "{command:?}");
    }
}

/// Herdr refuses a selection from a pane other than the target, so one that
/// does not match is left out rather than sent to be refused.
#[test]
fn a_selection_from_another_pane_is_not_sent() {
    let mut snapshot = snapshot();
    snapshot.commands = commands();
    let target = Target::capture(&snapshot, Some(selection("elsewhere")));
    let params = target
        .invocation(
            &snapshot,
            "PluginAction",
            ClientShellCommandAction::PluginAction,
        )
        .unwrap();
    assert_eq!(params.get("selection"), None);
    snapshot.focused_pane_id = None;
    let paneless = Target::capture(&snapshot, Some(selection("elsewhere")));
    let params = paneless
        .invocation(
            &snapshot,
            "PluginAction",
            ClientShellCommandAction::PluginAction,
        )
        .unwrap();
    assert_eq!(
        params,
        json!({"command_id": "PluginAction", "workspace_id": "w1", "tab_id": "w1:t1"})
    );
}
