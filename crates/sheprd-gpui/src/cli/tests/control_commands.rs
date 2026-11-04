use super::*;
use control::{
    AppCommand, CheckpointCommand, CodeCommand, ConfigCommand, ControlCommand, FanOut, FileAt,
    FontSize, Invocation, NotesCommand, PortsCommand, Script, Size, Switch, UiCommand, WaitFor,
    WindowCommand,
};

fn parse(args: &[&str]) -> Result<Invocation, CliError> {
    LaunchOptions::parse(args.iter().copied()).map(|options| match options.mode {
        LaunchMode::Control(invocation) => invocation,
        mode => panic!("{args:?} parsed as {mode:?}"),
    })
}

fn command(args: &[&str]) -> ControlCommand {
    parse(args).unwrap().command
}

#[test]
fn global_options_apply_anywhere_after_the_command() {
    let invocation = parse(&["window", "--json", "list", "--window", "7"]).unwrap();
    assert_eq!(
        invocation,
        Invocation {
            command: ControlCommand::Window(WindowCommand::List),
            json: true,
            window: Some(7),
        }
    );
    // A bare family runs its listing.
    assert_eq!(
        command(&["tab"]),
        ControlCommand::Tab(control::TabCommand::List)
    );
    assert_eq!(
        parse(&["window", "--window"]).unwrap_err(),
        CliError::MissingValue("--window")
    );
}

#[test]
fn help_wins_over_every_other_argument() {
    for args in [&["ui", "--help"][..], &["notes", "set", "x", "-h"]] {
        assert!(
            matches!(command(args), ControlCommand::Help(Some(_))),
            "{args:?}"
        );
    }
}

#[test]
fn app_window_and_ui_commands() {
    assert_eq!(
        command(&["app", "quit"]),
        ControlCommand::App(AppCommand::Quit)
    );
    assert_eq!(
        command(&["window", "fullscreen", "off"]),
        ControlCommand::Window(WindowCommand::Fullscreen(Switch::Off))
    );
    assert_eq!(
        command(&["window", "size", "1200x800"]),
        ControlCommand::Window(WindowCommand::Size(Some(Size {
            width: 1200,
            height: 800
        })))
    );
    assert_eq!(
        parse(&["window", "size", "12x8"]).unwrap_err(),
        CliError::InvalidValue("WIDTHxHEIGHT")
    );
    assert_eq!(
        command(&["ui", "sidebar"]),
        ControlCommand::Ui(UiCommand::Sidebar(Switch::Toggle))
    );
    assert_eq!(
        command(&["ui", "font-size", "-"]),
        ControlCommand::Ui(UiCommand::FontSize(FontSize::Smaller))
    );
    assert_eq!(
        command(&["ui", "font-size", "14"]),
        ControlCommand::Ui(UiCommand::FontSize(FontSize::Points { points: 14 }))
    );
    assert_eq!(
        parse(&["ui", "font-size", "99"]).unwrap_err(),
        CliError::InvalidValue("POINTS (8-48)")
    );
    assert_eq!(
        parse(&["app", "explode"]).unwrap_err(),
        CliError::UnknownCommand {
            family: "app",
            command: "explode".into()
        }
    );
}

#[test]
fn files_take_an_optional_line() {
    assert_eq!(
        command(&["code", "open", "src/main.rs:42"]),
        ControlCommand::Code(CodeCommand::Open(FileAt {
            path: "src/main.rs".into(),
            line: Some(42)
        }))
    );
    for plain in ["src/main.rs", "a:b", "x:0", ":3"] {
        let ControlCommand::Code(CodeCommand::Edit(file)) = command(&["code", "edit", plain])
        else {
            panic!("{plain}");
        };
        assert_eq!(file.path, std::path::PathBuf::from(plain), "{plain}");
        assert_eq!(file.line, None, "{plain}");
    }
}

#[test]
fn work_commands() {
    assert_eq!(
        command(&["notes", "set", "waiting", "on", "review"]),
        ControlCommand::Notes(NotesCommand::Set("waiting on review".into()))
    );
    assert_eq!(
        command(&["checkpoint", "diff", "0001700000000000"]),
        ControlCommand::Checkpoint(CheckpointCommand::Diff {
            from: "0001700000000000".into(),
            to: None
        })
    );
    assert_eq!(
        parse(&["checkpoint", "restore", "../x"]).unwrap_err(),
        CliError::InvalidValue("CHECKPOINT")
    );
    assert_eq!(
        command(&["scripts", "setup"]),
        ControlCommand::Scripts {
            kind: Some(Script::Setup)
        }
    );
    assert_eq!(
        command(&[
            "fan-out", "--agent", "claude:2", "fix", "it", "--agent", "Codex"
        ]),
        ControlCommand::FanOut(FanOut {
            prompt: "fix it".into(),
            agents: vec![("claude".into(), 2), ("codex".into(), 1)],
        })
    );
    assert_eq!(
        parse(&["fan-out", "--agent", "claude:9", "x"]).unwrap_err(),
        CliError::InvalidValue("--agent KIND[:COUNT]")
    );
    assert_eq!(
        parse(&["teleport"]).unwrap_err(),
        CliError::MissingValue("--to")
    );
    assert_eq!(
        command(&["wait", "tab-closed", "4", "--timeout", "5"]),
        ControlCommand::Wait {
            what: WaitFor::TabClosed(4),
            timeout: 5
        }
    );
}

#[test]
fn host_and_settings_commands() {
    assert_eq!(
        command(&["ports", "forward", "3000"]),
        ControlCommand::Ports(PortsCommand::Forward(3000))
    );
    assert_eq!(
        parse(&["ports", "open", "0"]).unwrap_err(),
        CliError::InvalidValue("PORT")
    );
    assert_eq!(
        command(&["config", "set", "terminal.size", "14"]),
        ControlCommand::Config(ConfigCommand::Set {
            key: "terminal.size".into(),
            value: "14".into()
        })
    );
    assert_eq!(
        parse(&["config", "get", "a..b"]).unwrap_err(),
        CliError::InvalidValue("KEY")
    );
    assert_eq!(
        command(&["caffeine", "on", "--for", "30m"]),
        ControlCommand::Caffeine(control::CaffeineCommand::Set {
            switch: Switch::On,
            seconds: Some(1800)
        })
    );
    assert_eq!(
        command(&["caffeine"]),
        ControlCommand::Caffeine(control::CaffeineCommand::Status)
    );
    assert_eq!(
        parse(&["caffeine", "off", "--for", "1h"]).unwrap_err(),
        CliError::InvalidValue("--for with on or toggle")
    );
    assert_eq!(
        command(&[
            "events",
            "--kind",
            "tab-opened",
            "--kind",
            "feedback",
            "--wait",
            "5"
        ]),
        ControlCommand::Events {
            after: None,
            follow: false,
            wait: 5,
            kinds: vec![control::EventKind::TabOpened, control::EventKind::Feedback],
        }
    );
    assert_eq!(
        parse(&["events", "--kind", "nope"]).unwrap_err(),
        CliError::InvalidValue("--kind")
    );
    assert_eq!(
        parse(&["search", "symbol"]).unwrap_err(),
        CliError::MissingArgument {
            command: "search",
            what: "QUERY"
        }
    );
    assert_eq!(
        parse(&["logs", "extra"]).unwrap_err(),
        CliError::Unexpected {
            command: "logs",
            argument: "extra".into()
        }
    );
}

#[test]
fn the_orchestrator_repository_may_come_before_the_verb() {
    for args in [
        &["orchestrator", "--repo", "/src/app"][..],
        &["orchestrator", "open", "--repo", "/src/app"],
        &["orchestrator", "--repo", "/src/app", "open"],
    ] {
        assert_eq!(
            command(args),
            ControlCommand::Orchestrator(control::OrchestratorCommand::Open {
                repo: Some("/src/app".into())
            }),
            "{args:?}"
        );
    }
    assert_eq!(
        command(&["orchestrator", "--repo", "/src/app", "prs"]),
        ControlCommand::Orchestrator(control::OrchestratorCommand::Items {
            repo: Some("/src/app".into()),
            items: control::Items::PullRequests,
        })
    );
}

#[test]
fn an_events_cursor_names_its_app_run() {
    assert_eq!(
        command(&["events", "--after", "77:12"]),
        ControlCommand::Events {
            after: Some(control::Cursor {
                instance: Some(77),
                seq: 12
            }),
            follow: false,
            wait: 0,
            kinds: Vec::new(),
        }
    );
    let ControlCommand::Events { after, .. } = command(&["events", "--after", "12"]) else {
        panic!("not events");
    };
    assert_eq!(
        after.map(|cursor| cursor.to_string()).as_deref(),
        Some("12")
    );
    assert_eq!(
        parse(&["events", "--after", "x:1"]).unwrap_err(),
        CliError::InvalidValue("--after")
    );
}
