use super::*;

#[test]
fn rows_carry_the_words_a_search_matches() {
    let layout = Layout {
        connected: true,
        boot: "b".into(),
        workspaces: vec![workspace(vec![row("p1")])],
    };
    let summaries = HashMap::new();
    let lists = Lists {
        computer: "MacBook Pro",
        layout: &layout,
        summaries: &summaries,
        now_ms: 0,
        indicators: Indicators::Dots,
    };
    let home = home(&lists).into_string();
    assert!(
        home.contains(r#"data-find="herdr-gpui main Remote plan claude""#),
        "{home}"
    );
    assert!(home.contains(r#"data-find="herdr-gpui main zsh"#), "{home}");
    assert!(home.contains("data-group"));
    let board = board(&lists).into_string();
    assert!(
        board.contains(r#"data-find="herdr-gpui main Remote plan claude""#),
        "{board}"
    );
}

#[test]
fn the_search_field_sits_outside_the_lists_updates_replace() {
    let search = search().into_string();
    assert!(search.contains(r#"id="search""#));
    assert!(!search.contains(r#"id="lists""#));
    assert!(no_match().into_string().contains("hidden"));
}
