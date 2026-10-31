use super::*;
use herdr_i18n::Pseudo;

/// Rules on `state_text` match the English status, so a sidebar config keeps
/// hiding and styling the same rows whatever language the app shows.
#[test]
fn state_rules_match_the_english_status_in_every_language() {
    crate::i18n::show(&Pseudo::Accented.locale());
    let mut snapshot = layout_tests::snapshot(1);
    snapshot.agents[0].agent_status = AgentStatus::Working;
    let agents = agent_layout(
        r#"rows = [[{ token = "state_text", rules = [{ equals = "working", hide = true }] }, "workspace"]]"#,
    );
    let rows = agent_rows(&agents, &snapshot.agents[0], &snapshot, None).unwrap();
    let spaces = space_layout(
        r##"rows = [[{ token = "state_text", rules = [{ equals = "blocked", fg = "#ff0000" }] }]]"##,
    );
    let space = space_rows(
        &spaces,
        SpaceContext {
            label: "herdr",
            branch: None,
            status: AgentStatus::Blocked,
            ahead_behind: None,
            tokens: &[],
            indented: false,
        },
    );
    crate::i18n::show(&herdr_i18n::Locale::FALLBACK);
    assert!(
        rows[0]
            .iter()
            .all(|token| !matches!(token.kind, TokenKind::Text(_, TextRole::Status))),
        "the hidden status still shows: {:?}",
        texts(&rows)
    );
    let shown = space[0][0].kind.text().unwrap();
    assert!(herdr_i18n::pseudo::is_marked(shown), "{shown}");
    assert_eq!(space[0][0].style.fg, Some(0xff0000));
}
