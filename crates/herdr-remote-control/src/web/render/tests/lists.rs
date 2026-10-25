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
        indicators: Indicators::Dots,
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
        indicators: Indicators::Dots,
    })
    .into_string();
    assert!(!html.contains("<img src=x"));
}

#[test]
fn rows_show_the_daemons_status_dot_and_no_path() {
    let mut terminal = crate::hub::layout_terminal("t2", "zsh");
    terminal.cwd = Some("/srv/code/app/src".into());
    terminal.agent = Some((Some("codex".into()), AgentStatus::Blocked));
    let mut shell = crate::hub::layout_terminal("t3", "bash");
    shell.cwd = Some("/srv/code/app".into());
    let mut workspace = workspace(vec![row("p1")]);
    workspace.terminals = vec![terminal, shell];
    let layout = Layout {
        connected: true,
        boot: "b".into(),
        workspaces: vec![workspace],
    };
    let summaries = HashMap::new();
    let lists = Lists {
        computer: "MacBook Pro",
        layout: &layout,
        summaries: &summaries,
        now_ms: 0,
        indicators: Indicators::Dots,
    };
    let page = home(&lists).into_string();
    // The chat's dot is the daemon's status, not the chat's own.
    assert!(page.contains(r#"class="status-dot st-working""#), "{page}");
    assert!(page.contains(r#"class="status-dot st-blocked""#), "{page}");
    // One dot per agent; the plain shell has none.
    assert_eq!(page.matches("status-dot").count(), 2, "{page}");
    assert!(!page.contains(">/srv/code/app"), "{page}");
    // The old working ellipsis is gone (the header's menu keeps its own).
    assert!(!page.contains(r#"aria-label="Working">•••"#), "{page}");
}

#[test]
fn marks_follow_the_set_picked_in_herdr_ui() {
    use crate::web::render::status_mark;
    let dots = status_mark(AgentStatus::Working, Indicators::Dots).into_string();
    assert!(
        dots.contains("status-dot st-working") && !dots.contains("<svg"),
        "{dots}"
    );
    for set in [Indicators::Solid, Indicators::Outline, Indicators::Tiles] {
        let mark = status_mark(AgentStatus::Blocked, set).into_string();
        assert!(
            mark.contains("status-mark") && mark.contains("<svg"),
            "{mark}"
        );
        assert!(mark.contains(r#"aria-label="Blocked""#), "{mark}");
    }
    assert!(
        status_mark(AgentStatus::Done, Indicators::Tiles)
            .into_string()
            .contains("status-tile")
    );
    assert_eq!(Indicators::from_config("tiles"), Indicators::Tiles);
    assert_eq!(Indicators::from_config("rail"), Indicators::Outline);
    // Herdr UI's animated sets have no files to share.
    assert_eq!(Indicators::from_config("braille"), Indicators::Dots);
}
