use super::*;
use crate::app::config::{Bind, Privacy};
use crate::control::{Error as ControlError, Status};
use std::{cell::RefCell, path::Path};

fn plan() -> Plan {
    Plan {
        program: "/Applications/Herdr UI.app/Contents/MacOS/herdr-remote-control".into(),
        state_dir: "/Users/me/.local/state/herdr/gpui/remote".into(),
        config: RemoteConfig {
            enabled: true,
            ..RemoteConfig::default()
        },
        herdr_socket: None,
        chat_dir: None,
    }
}

#[test]
fn the_daemon_follows_the_herdr_this_app_resolved() {
    let mut located = plan();
    located.herdr_socket = Some("/Users/me/cfg/herdr/herdr-client.sock".into());
    located.chat_dir = Some("/Users/me/st/herdr/acp".into());
    let args = located.args();
    let words = words(&args);
    assert_eq!(
        words[words.len() - 4..],
        [
            "--herdr-socket",
            "/Users/me/cfg/herdr/herdr-client.sock",
            "--chat-dir",
            "/Users/me/st/herdr/acp",
        ]
    );
    // The same words reach every service file.
    let unit = files::systemd_unit(&located).unwrap();
    assert!(
        unit.contains("\"--herdr-socket\" \"/Users/me/cfg/herdr/herdr-client.sock\""),
        "{unit}"
    );
}

fn words(args: &[OsString]) -> Vec<&str> {
    args.iter().map(|arg| arg.to_str().unwrap()).collect()
}

#[test]
fn the_daemon_runs_with_every_setting_as_an_argument() {
    assert_eq!(
        words(&plan().args()),
        [
            "serve",
            "--state-dir",
            "/Users/me/.local/state/herdr/gpui/remote",
            "--port",
            "7443",
            "--bind",
            "all",
            "--privacy",
            "generic",
            "--device-idle-days",
            "30",
        ]
    );
    let mut custom = plan();
    custom.config = RemoteConfig {
        bind: Bind::Address("10.0.0.2".parse().unwrap()),
        certificate: Certificate::Files {
            cert: "/c.pem".into(),
            key: "/k.pem".into(),
        },
        device_idle_days: 9,
        enabled: true,
        keep_running: false,
        notifications: Privacy::Excerpt,
        port: 9443,
    };
    assert_eq!(
        words(&custom.args()),
        [
            "serve",
            "--state-dir",
            "/Users/me/.local/state/herdr/gpui/remote",
            "--port",
            "9443",
            "--bind",
            "10.0.0.2",
            "--cert",
            "/c.pem",
            "--key",
            "/k.pem",
            "--privacy",
            "excerpt",
            "--device-idle-days",
            "9",
        ]
    );
}

#[test]
fn the_launch_agent_starts_at_login_and_stays_down_after_a_clean_exit() {
    let mut plan = plan();
    plan.config.certificate = Certificate::Files {
        cert: "/a&b/<cert>.pem".into(),
        key: "/k.pem".into(),
    };
    let text = files::launch_agent(&plan).unwrap();
    assert!(text.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"));
    assert!(text.contains("<key>Label</key>\n\t<string>so.pen.herdr-gpui.remote</string>"));
    assert!(text.contains(
        "\t\t<string>/Applications/Herdr UI.app/Contents/MacOS/herdr-remote-control</string>\n\
         \t\t<string>serve</string>\n"
    ));
    // Text is escaped, not taken as markup.
    assert!(text.contains("<string>/a&amp;b/&lt;cert&gt;.pem</string>"));
    assert!(text.contains("<key>RunAtLoad</key>\n\t<true/>"));
    assert!(text.contains("<key>SuccessfulExit</key>\n\t\t<false/>"));
    let log = plan.state_dir.join("daemon.log");
    assert!(text.contains(&format!("<string>{}</string>", log.display())));
    assert!(text.ends_with("</plist>\n"));
}

#[test]
fn the_systemd_unit_quotes_every_word_and_restarts_only_on_failure() {
    let mut plan = plan();
    plan.program = "/home/me/my apps/herdr-remote-control".into();
    plan.state_dir = "/home/me/100%/$HOME/\"x\"\\y".into();
    let text = files::systemd_unit(&plan).unwrap();
    assert!(text.contains(
        "ExecStart=\"/home/me/my apps/herdr-remote-control\" \"serve\" \"--state-dir\" \
         \"/home/me/100%%/$$HOME/\\\"x\\\"\\\\y\" \"--port\" \"7443\""
    ));
    assert!(text.contains("\nRestart=on-failure\n"));
    assert!(text.contains("\n[Install]\nWantedBy=default.target\n"));
}

#[test]
fn a_path_a_service_file_cannot_hold_is_refused() {
    let mut plan = plan();
    plan.state_dir = "/tmp/line\nbreak".into();
    for text in [
        files::launch_agent(&plan),
        files::systemd_unit(&plan),
        files::scheduled_task(&plan, "PC\\me"),
    ] {
        assert!(matches!(text, Err(Error::PathText { .. })));
    }
}

#[test]
fn windows_arguments_survive_the_command_line_round_trip() {
    for (word, quoted) in [
        ("serve", "serve"),
        ("", "\"\""),
        ("C:\\Users\\Me Too\\state", "\"C:\\Users\\Me Too\\state\""),
        ("C:\\trailing space\\", "\"C:\\trailing space\\\\\""),
        ("say \"hi\"", "\"say \\\"hi\\\"\""),
        ("a\\\"b", "\"a\\\\\\\"b\""),
    ] {
        assert_eq!(files::windows_word(word), quoted, "{word}");
    }
}

#[test]
fn the_scheduled_task_runs_at_sign_in_without_a_time_limit() {
    let mut plan = plan();
    plan.program = "C:\\Program Files\\Herdr UI\\herdr-remote-control.exe".into();
    plan.state_dir = "C:\\Users\\Me\\AppData\\Local\\herdr\\gpui\\remote".into();
    let text = files::scheduled_task(&plan, "PC\\Me & You").unwrap();
    assert!(text.starts_with("<?xml version=\"1.0\" encoding=\"UTF-16\"?>\r\n"));
    assert!(text.contains(
        "<LogonTrigger>\r\n      <Enabled>true</Enabled>\r\n      <UserId>PC\\Me &amp; You</UserId>"
    ));
    assert!(text.contains("<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>"));
    assert!(text.contains("<StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>"));
    assert!(text.contains("<RunLevel>LeastPrivilege</RunLevel>"));
    assert!(
        text.contains("<Command>C:\\Program Files\\Herdr UI\\herdr-remote-control.exe</Command>")
    );
    assert!(text.contains(
        "<Arguments>serve --state-dir C:\\Users\\Me\\AppData\\Local\\herdr\\gpui\\remote \
         --port 7443 --bind all --privacy generic --device-idle-days 30</Arguments>"
    ));
    let bytes = files::utf16("<T/>");
    assert_eq!(&bytes[..2], &[0xFF, 0xFE]);
    assert_eq!(&bytes[2..4], &[b'<', 0]);
    assert_eq!(bytes.len(), 2 + 4 * 2);
}

#[test]
fn each_manager_files_its_definition_where_it_looks() {
    let (home, config, state) = (
        Path::new("/home/me"),
        Path::new("/home/me/.config"),
        Path::new("/state/remote"),
    );
    assert_eq!(
        Manager::LaunchAgent { uid: 501 }.file(home, config, state),
        Path::new("/home/me/Library/LaunchAgents/so.pen.herdr-gpui.remote.plist")
    );
    assert_eq!(
        Manager::Systemd.file(home, config, state),
        Path::new("/home/me/.config/systemd/user/herdr-remote-control.service")
    );
    assert_eq!(
        Manager::ScheduledTask { user: "me".into() }.file(home, config, state),
        Path::new("/state/remote/task.xml")
    );
}

fn command(step: &Step) -> String {
    std::iter::once(step.program)
        .chain(step.args.iter().map(|arg| arg.to_str().unwrap()))
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn managers_load_start_and_unload_with_their_own_commands() {
    let file = Path::new("/f");
    let launchd = Manager::LaunchAgent { uid: 501 };
    let commands = |steps: Vec<Step>| steps.iter().map(command).collect::<Vec<_>>();
    assert_eq!(
        commands(launchd.load(file)),
        [
            "launchctl bootout gui/501/so.pen.herdr-gpui.remote",
            "launchctl bootstrap gui/501 /f"
        ]
    );
    assert_eq!(
        commands(launchd.start(file)),
        [
            "launchctl bootstrap gui/501 /f",
            "launchctl kickstart gui/501/so.pen.herdr-gpui.remote"
        ]
    );
    assert_eq!(
        commands(launchd.unload()),
        ["launchctl bootout gui/501/so.pen.herdr-gpui.remote"]
    );
    assert!(
        !launchd.load(file)[0].required,
        "nothing loaded yet is fine"
    );
    assert_eq!(
        commands(Manager::Systemd.load(file)),
        [
            "systemctl --user daemon-reload",
            "systemctl --user enable herdr-remote-control.service",
            "systemctl --user restart herdr-remote-control.service"
        ]
    );
    assert_eq!(
        commands(Manager::Systemd.unload()),
        ["systemctl --user disable --now herdr-remote-control.service"]
    );
    assert_eq!(
        commands(Manager::Systemd.unloaded()),
        ["systemctl --user daemon-reload"]
    );
    let task = Manager::ScheduledTask { user: "me".into() };
    assert_eq!(
        commands(task.load(file)),
        [
            "schtasks /End /TN Herdr UI Remote Control",
            "schtasks /Create /TN Herdr UI Remote Control /XML /f /F",
            "schtasks /Run /TN Herdr UI Remote Control"
        ]
    );
    assert_eq!(
        commands(task.unload()),
        [
            "schtasks /End /TN Herdr UI Remote Control",
            "schtasks /Delete /TN Herdr UI Remote Control /F"
        ]
    );
}

/// A host in a scratch directory, with the daemon "found".
fn host(dir: &Path) -> Host {
    Host {
        manager: Manager::Systemd,
        file: dir.join("units/herdr-remote-control.service"),
        state_dir: dir.join("state/remote"),
        program: Some("/bin/herdr-remote-control".into()),
        release: None,
    }
}

fn status(version: &str) -> Response {
    Response::Status(Status {
        version: version.into(),
        port: 7443,
        addresses: Vec::new(),
        fingerprint: String::new(),
        herdr: true,
        chats: 0,
        devices: 0,
        connected: 0,
    })
}

fn unreachable() -> Result<Response, ControlError> {
    Err(ControlError::NoAnswer)
}

#[test]
fn turning_it_on_writes_the_unit_and_loads_it_once() {
    let dir = tempfile::tempdir().unwrap();
    let host = host(dir.path());
    let ran = RefCell::new(Vec::new());
    let on = RemoteConfig {
        enabled: true,
        ..RemoteConfig::default()
    };
    let outcome = reconcile(
        &on,
        &host,
        |_| unreachable(),
        |step| {
            ran.borrow_mut().push(command(step));
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(outcome, Outcome::Installed);
    assert!(
        fs::read_to_string(&host.file)
            .unwrap()
            .contains("\"--port\" \"7443\"")
    );
    assert!(host.state_dir.is_dir());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&host.state_dir).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
    }
    assert_eq!(ran.borrow().len(), 3);
    ran.borrow_mut().clear();
    // Running as configured: nothing to do.
    let outcome = reconcile(
        &on,
        &host,
        |_| Ok(status("dev")),
        |step| {
            ran.borrow_mut().push(command(step));
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(outcome, Outcome::Unchanged);
    assert!(ran.borrow().is_empty());
    // Installed but stopped, as after Herdr UI quit: started, not rewritten.
    let outcome = reconcile(
        &on,
        &host,
        |_| unreachable(),
        |step| {
            ran.borrow_mut().push(command(step));
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(outcome, Outcome::Started);
    assert_eq!(
        *ran.borrow(),
        ["systemctl --user start herdr-remote-control.service"]
    );
}

#[test]
fn a_changed_setting_rewrites_and_restarts() {
    let dir = tempfile::tempdir().unwrap();
    let host = host(dir.path());
    let mut config = RemoteConfig {
        enabled: true,
        ..RemoteConfig::default()
    };
    reconcile(&config, &host, |_| unreachable(), |_| Ok(())).unwrap();
    config.port = 9000;
    let ran = RefCell::new(Vec::new());
    let outcome = reconcile(
        &config,
        &host,
        |_| Ok(status("dev")),
        |step| {
            ran.borrow_mut().push(command(step));
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(outcome, Outcome::Installed);
    assert!(fs::read_to_string(&host.file).unwrap().contains("\"9000\""));
    assert_eq!(
        ran.borrow().last().map(String::as_str),
        Some("systemctl --user restart herdr-remote-control.service")
    );
}

#[test]
fn turning_it_off_shuts_the_daemon_down_and_removes_the_service() {
    let dir = tempfile::tempdir().unwrap();
    let host = host(dir.path());
    let on = RemoteConfig {
        enabled: true,
        ..RemoteConfig::default()
    };
    reconcile(&on, &host, |_| unreachable(), |_| Ok(())).unwrap();
    let asked = RefCell::new(Vec::new());
    let ran = RefCell::new(Vec::new());
    let off = RemoteConfig::default();
    let outcome = reconcile(
        &off,
        &host,
        |request| {
            asked.borrow_mut().push(request.clone());
            Ok(Response::Done)
        },
        |step| {
            ran.borrow_mut().push(command(step));
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(outcome, Outcome::Removed);
    assert_eq!(*asked.borrow(), [Request::Shutdown]);
    assert!(!host.file.exists());
    assert_eq!(
        *ran.borrow(),
        [
            "systemctl --user disable --now herdr-remote-control.service",
            "systemctl --user daemon-reload"
        ]
    );
    // Already off and gone: nothing is asked or run.
    let outcome = reconcile(&off, &host, |_| panic!("asked"), |_| panic!("ran")).unwrap();
    assert_eq!(outcome, Outcome::Unchanged);
}

#[test]
fn without_the_daemon_nothing_is_installed() {
    let dir = tempfile::tempdir().unwrap();
    let mut host = host(dir.path());
    host.program = None;
    let on = RemoteConfig {
        enabled: true,
        ..RemoteConfig::default()
    };
    let error = reconcile(&on, &host, |_| unreachable(), |_| panic!("ran")).unwrap_err();
    assert!(matches!(error, Error::ProgramMissing));
    assert!(!host.file.exists());
}

#[test]
fn a_failed_load_is_reported_with_its_cause() {
    let dir = tempfile::tempdir().unwrap();
    let host = host(dir.path());
    let on = RemoteConfig {
        enabled: true,
        ..RemoteConfig::default()
    };
    let error = reconcile(
        &on,
        &host,
        |_| unreachable(),
        |_| {
            Err(Error::Spawn {
                program: "systemctl",
                source: io::Error::from(io::ErrorKind::NotFound),
            })
        },
    )
    .unwrap_err();
    let Error::Spawn { program, .. } = &error else {
        panic!("{error:?}");
    };
    assert_eq!(*program, "systemctl");
    assert!(std::error::Error::source(&error).is_some());
}

mod release;
