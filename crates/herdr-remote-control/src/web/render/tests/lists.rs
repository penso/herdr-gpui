use super::*;

fn lists_html(summaries: HashMap<String, Summary>, board_view: bool) -> String {
    let layout = Layout {
        connected: true,
        boot: "b".into(),
        workspaces: vec![workspace(vec![row("p1"), row("p2")])],
    };
    let lists = Lists {
        computer: "MacBook Pro",
        layout: &layout,
        summaries: &summaries,
        now_ms: 10_000,
    };
    if board_view {
        board(&lists)
    } else {
        home(&lists)
    }
    .into_string()
}

#[test]
fn home_lists_chats_and_terminals_under_the_computer() {
    let html = lists_html(HashMap::new(), false);
    assert!(html.contains("MacBook Pro"));
    assert!(html.contains("herdr-gpui"));
    assert!(html.contains(r#"href="/chat/p1""#));
    assert!(html.contains("zsh"));
    // A terminal opens on its focused pane.
    assert!(html.contains(r#"href="/term/"#), "{html}");
    assert!(!html.contains("Soon"));
}

#[test]
fn waiting_questions_lead_home_and_the_board() {
    let summaries = HashMap::from([(
        "p2".to_owned(),
        Summary {
            question: Some(question("Run just ci?")),
            questions: 2,
            ..Summary::default()
        },
    )]);
    let home = lists_html(summaries.clone(), false);
    let strip = home.find("strip-card").unwrap();
    assert!(home[strip..].contains("Run just ci?"));
    assert!(home.contains("2 waiting"));
    assert!(home.contains("Answer"));
    let board = lists_html(summaries, true);
    let needs = board.find("Needs you").unwrap();
    let idle = board.find("Idle").unwrap();
    assert!(needs < board.find("Run just ci?").unwrap());
    assert!(board.find("Run just ci?").unwrap() < idle);
}

#[test]
fn a_title_from_daemon_data_is_escaped() {
    let mut chat = row("p1");
    chat.title = "<img src=x onerror=alert(1)>".into();
    let layout = Layout {
        connected: true,
        boot: "b".into(),
        workspaces: vec![workspace(vec![chat])],
    };
    let summaries = HashMap::new();
    let html = home(&Lists {
        computer: "c",
        layout: &layout,
        summaries: &summaries,
        now_ms: 0,
    })
    .into_string();
    assert!(!html.contains("<img src=x"));
}
