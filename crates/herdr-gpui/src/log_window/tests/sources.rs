//! The source picker and the reading of an automation daemon's log lines.
use super::*;
use crate::automations::{LogSource, LogSources, Machine};
use source::{Source, parse, record};

fn ssh_source() -> LogSource {
    let target = herdr_client::ConnectTarget::Ssh {
        target: "me@buildbox".into(),
        session: "default".into(),
    };
    LogSource {
        machine: Machine::Ssh("me@buildbox".into()),
        label: "buildbox".into(),
        host: crate::teleport::Host::new(&target).unwrap(),
    }
}

#[test]
fn app_shaped_lines_keep_their_level_and_fields() {
    let mut original = Record::fixture(Level::WARN, "run 12 halted");
    original.target = "herdr_automate::runner".into();
    original.fields.insert("run".into(), 12.into());
    let line = serde_json::to_string(&original).unwrap();
    assert_eq!(record(&line), original);
}

#[test]
fn tracing_json_lines_become_records() {
    let line = r#"{"timestamp":"2026-10-22T03:00:00Z","level":"ERROR","target":"herdr_automate::step","fields":{"message":"pull failed","step":1}}"#;
    let read = record(line);
    assert_eq!(read.level, Level::ERROR);
    assert_eq!(read.message, "pull failed");
    assert_eq!(read.target, "herdr_automate::step");
    assert_eq!(read.namespace, "herdr_automate");
    assert_eq!(read.timestamp, "[2026-10-22T03:00:00Z]");
    assert_eq!(read.fields.get("step"), Some(&serde_json::json!(1)));
}

#[test]
fn other_lines_show_as_plain_text_without_control_characters() {
    let records = parse("started\n\n\u{1b}[31mred\u{202e}text\n{\"level\":\"loud\"}\n");
    let messages: Vec<_> = records.iter().map(|r| r.message.as_str()).collect();
    assert_eq!(messages, ["started", "[31mredtext", "{\"level\":\"loud\"}"]);
    assert!(records.iter().all(|r| r.level == Level::INFO));
    // Filters apply to host lines as to the app's.
    let errors = filtered(records, "red", Level::TRACE);
    assert_eq!(errors.len(), 1);
}

#[gpui::test]
fn the_picker_lists_hosts_with_a_daemon_log(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(LogWindow::new);
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    // Nothing to pick until a window found a daemon.
    assert!(cx.debug_bounds("log-sources").is_none());
    cx.update(|_, cx| cx.set_global(LogSources(vec![ssh_source()], false)));
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    assert!(cx.debug_bounds("log-sources").is_some());
    assert!(cx.debug_bounds("log-source-0").is_some());
    assert!(cx.debug_bounds("log-source-1").is_some());
    assert_eq!(
        Source::Automations(ssh_source()).label(),
        fl!("log-window-source-automations", host = "buildbox")
    );
    view.read_with(cx, |view, _| {
        assert_eq!(view.source, Source::App);
        assert!(view.remote.is_none());
    });
    // Picking the shown source again changes nothing.
    view.update(cx, |view, cx| view.pick_source(Source::App, cx));
    view.read_with(cx, |view, _| assert!(view.remote.is_none()));
}

#[gpui::test]
fn a_host_log_is_not_read_while_automations_are_off(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(LogWindow::new);
    cx.update(|_, cx| cx.set_global(LogSources(vec![ssh_source()], true)));
    view.update(cx, |view, cx| {
        view.pick_source(Source::Automations(ssh_source()), cx);
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.status,
            fl!(
                "log-window-automations-off",
                source = fl!("log-window-source-automations", host = "buildbox")
            )
        );
    });
}
