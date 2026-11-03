//! Rows a project-grouping plugin writes into Herdr's config: a one-line
//! card whose name turns bold when the daemon's label carries the plugin's
//! invisible group-head mark, and a dimmed detail line from a custom token
//! that disappears when the plugin withdraws it.

use super::*;

/// Zero width space and braille blank: marks that render as nothing, or as
/// a blank cell, so only a rule can see them.
const HEAD: char = '\u{200B}';
const HOME: char = '\u{2800}';

fn bold(token: &ResolvedToken) -> bool {
    token.style.bold == Some(true)
}

#[test]
fn invisible_marks_in_daemon_labels_drive_rules_and_withdrawn_tokens_drop_rows() {
    let layout = agent_layout(&format!(
        r#"rows = [
            ["state_icon", {{ token = "agent", bold = false, dim = false, rules = [{{ contains = "{HEAD}", bold = true }}] }}, "state_text"],
            [{{ token = "$group_sub", dim = true }}],
        ]"#
    ));
    let mut snapshot = layout_tests::snapshot(1);
    let agent = &mut snapshot.agents[0];
    agent.display_agent = Some(format!("alpha{HEAD}"));
    agent.tokens = vec![("group_sub".into(), "review \u{b7} report".into())];
    let rows = agent_rows(&layout, &snapshot.agents[0], &snapshot, None).unwrap();
    assert_eq!(texts(&rows)[1], ["review \u{b7} report"]);
    assert!(bold(&rows[0][1]), "a group head is bold");
    assert_eq!(rows[1][0].style.dim, Some(true));

    // A member of the group is plain, and a snapshot without the token, as
    // after the plugin cleared it, leaves no detail line behind.
    let agent = &mut snapshot.agents[0];
    agent.display_agent = Some("alpha".into());
    agent.tokens.clear();
    let rows = agent_rows(&layout, &snapshot.agents[0], &snapshot, None).unwrap();
    assert_eq!(rows.len(), 1);
    assert!(!bold(&rows[0][1]));
}

#[test]
fn a_home_mark_in_a_workspace_label_makes_its_row_the_group_head() {
    let layout = space_layout(&format!(
        r#"rows = [["state_icon", {{ token = "workspace", rules = [{{ contains = "{HOME}", bold = true }}] }}, {{ token = "branch", dim = true }}, "git_status"]]"#
    ));
    let row = |label: &str| {
        space_rows(
            &layout,
            SpaceContext {
                label,
                branch: Some("main"),
                status: AgentStatus::Idle,
                ahead_behind: None,
                tokens: &[],
                indented: false,
            },
        )
        .remove(0)
    };
    let home = format!("Alpha{HOME}");
    let head = row(&home);
    assert!(bold(&head[1]));
    assert_eq!(
        head[1].kind.text().map(|text| text.as_ref()),
        Some(home.as_str())
    );
    assert!(!bold(&row("Alpha")[1]));
}
