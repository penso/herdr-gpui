//! Writes the views with demo data as standalone pages, to look at them in
//! a browser: `HERDR_REMOTE_PREVIEW=DIR cargo test -p herdr-remote-control preview
//! -- --ignored`. The pages link `app.css`, copied beside them.

use super::*;
use herdr_chat_protocol::{
    Mode, Modes, PlanEntry, PlanStatus, Status, Tool, ToolContent, ToolKind, ToolStatus, Turn,
    TurnEnd, TurnSummary, Usage,
};

fn page(dir: &std::path::Path, name: &str, body: Markup) {
    let html = format!(
        "<!doctype html><html><head><meta charset=utf-8><meta name=viewport content=\"width=device-width, initial-scale=1, viewport-fit=cover\"><link rel=stylesheet href=app.css></head><body><div id=view class=\"px-3.5 min-h-dvh\">{}</div></body></html>",
        body.into_string()
    );
    std::fs::write(dir.join(name), html).unwrap();
}

fn demo_transcript() -> Transcript {
    let mut transcript = Transcript::new("Claude");
    transcript.status = Status::Working;
    transcript.turn = Some(Turn { started_ms: 0 });
    transcript.usage = Some(Usage {
        cost: None,
        size: 200_000,
        used: 82_000,
    });
    transcript.modes = Some(Modes {
        available: vec![
            Mode {
                description: None,
                id: "default".into(),
                name: "Default".into(),
            },
            Mode {
                description: None,
                id: "plan".into(),
                name: "Plan".into(),
            },
        ],
        current: "default".into(),
    });
    transcript.entries = vec![
        entry(1, EntryKind::User { text: "Add the remote control plan".into() }),
        entry(2, EntryKind::Agent { text: "Done.".into() }),
        entry(3, EntryKind::Turn(TurnSummary { duration_ms: 125_000, end: TurnEnd::Done, tokens: None })),
        entry(4, EntryKind::User { text: "Write the plan in docs/ and give me mockups".into() }),
        entry(5, EntryKind::Tool(Tool {
            call_id: "1".into(),
            content: vec![ToolContent::Diff { new: "axum = \"0.8\"\ntokio = \"1\"".into(), old: Some("rustls = \"0.23\"".into()), path: "Cargo.toml".into() }],
            kind: ToolKind::Edit,
            locations: vec![],
            status: ToolStatus::Completed,
            title: "Edit Cargo.toml".into(),
        })),
        entry(6, EntryKind::Tool(Tool {
            call_id: "2".into(),
            content: vec![],
            kind: ToolKind::Execute,
            locations: vec![],
            status: ToolStatus::InProgress,
            title: "just check-file-size".into(),
        })),
        entry(7, EntryKind::Agent { text: "The plan covers pairing with **dual approval**, the private CA for LAN and Tailscale, and push.\n\n- `herdr-remote-control` daemon\n- mobile web app".into() }),
    ];
    transcript.plan = vec![
        PlanEntry {
            content: "Write the plan".into(),
            priority: Default::default(),
            status: PlanStatus::Completed,
        },
        PlanEntry {
            content: "Mockups".into(),
            priority: Default::default(),
            status: PlanStatus::InProgress,
        },
        PlanEntry {
            content: "Open in a browser tab".into(),
            priority: Default::default(),
            status: PlanStatus::Pending,
        },
    ];
    transcript
}

#[test]
#[ignore = "writes preview pages; run by hand"]
fn write_preview_pages() {
    let Some(dir) = std::env::var_os("HERDR_REMOTE_PREVIEW") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(
        concat!(env!("CARGO_MANIFEST_DIR"), "/src/web/assets/app.css"),
        dir.join("app.css"),
    )
    .unwrap();
    let mut second = row("p2");
    second.title = "Split equalize tests".into();
    second.agent = Some("codex".into());
    let mut other = workspace(vec![row("p3")]);
    other.id = "w2".into();
    other.label = "moltis".into();
    other.branch = None;
    let layout = Layout {
        connected: true,
        boot: "b".into(),
        workspaces: vec![workspace(vec![row("p1"), second]), other],
    };
    let summaries = HashMap::from([
        (
            "p1".to_owned(),
            Summary {
                question: Some(question("Run just ci?")),
                questions: 1,
                ..Summary::default()
            },
        ),
        (
            "p2".to_owned(),
            Summary {
                status: Status::Working,
                turn_started_ms: Some(0),
                ..Summary::default()
            },
        ),
        (
            "p3".to_owned(),
            Summary {
                status: Status::Idle,
                ..Summary::default()
            },
        ),
    ]);
    let lists = Lists {
        computer: "MacBook Pro",
        layout: &layout,
        summaries: &summaries,
        now_ms: 190_000,
    };
    page(&dir, "home.html", home(&lists));
    page(&dir, "board.html", board(&lists));
    let view = View {
        transcript: demo_transcript(),
        connected: true,
    };
    page(
        &dir,
        "chat.html",
        chat(&layout.workspaces[0], &row("p1"), &view, 72_000),
    );
    let mut asking = demo_transcript();
    asking.permissions = vec![question("Run `just ci` in ~/code/herdr-gpui?")];
    let view = View {
        transcript: asking,
        connected: true,
    };
    page(
        &dir,
        "question.html",
        chat(&layout.workspaces[0], &row("p1"), &view, 72_000),
    );
    let folders = Folders {
        recent: vec!["/code/herdr-gpui".into(), "/code/moltis".into()],
        roots: vec![(
            "/code".into(),
            vec!["/code/herdr-site".into(), "/code/agent-launcher".into()],
        )],
    };
    page(&dir, "new.html", new_workspace(&folders));
    page(
        &dir,
        "settings.html",
        devices(
            "MacBook Pro",
            &[
                "https://10.0.0.19:7443".into(),
                "https://100.64.0.3:7443".into(),
            ],
            "5F:2A:01:C1:9E",
        ),
    );
}
