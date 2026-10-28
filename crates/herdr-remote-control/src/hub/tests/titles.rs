use super::*;

#[test]
fn terminal_paste_markers_use_the_same_useful_label_on_phone_and_desktop() {
    let mut shell = snapshot(json!([]));
    shell.workspaces[0].label = "agent-357-windows-gui-never-connects".into();
    shell.tabs[0].label = "<pasted_content id=\"9d74\">".into();
    let projected = layout::project(&shell);
    assert_eq!(
        projected.workspaces[0].terminals[0].label,
        "agent-357-windows-gui-never-connects"
    );
    assert_eq!(
        herdr_client::text::tab_label(&shell, &shell.tabs[0]),
        projected.workspaces[0].terminals[0].label
    );
    shell.panes[0].label = Some("#357 Windows GUI never connects".into());
    assert_eq!(
        layout::project(&shell).workspaces[0].terminals[0].label,
        "#357 Windows GUI never connects"
    );
    shell.tabs[0].label = "My tab".into();
    assert_eq!(
        layout::project(&shell).workspaces[0].terminals[0].label,
        "My tab"
    );
}

#[test]
fn explicit_pane_names_win_over_chat_transport_titles() {
    let mut named = agent(
        "p1",
        "t1",
        json!([[
            herdr_chat_protocol::TOKEN,
            "0123456789abcdef0123456789abcdef"
        ]]),
    );
    named["title"] = json!("<pasted_content id=\"9d74\">");
    let mut shell = snapshot(json!([named]));
    shell.panes[0].label = Some("#357 Windows GUI never connects".into());
    assert_eq!(
        layout::project(&shell).workspaces[0].chats[0].title,
        "#357 Windows GUI never connects"
    );
}
