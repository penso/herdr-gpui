use super::*;
use crate::control::protocol::app::AlertInfo;

fn alert(needs_attention: bool, title: &str) -> AlertInfo {
    AlertInfo {
        host: "Local".into(),
        needs_attention,
        title: title.into(),
        body: None,
        pane: Some("w1:p1".into()),
        at: 0,
    }
}

#[test]
fn alerts_read_one_to_a_line_saying_what_each_wants() {
    let lines = app::alert_lines(&[alert(true, "claude"), alert(false, "codex")]);
    assert_eq!(lines, "needs you  Local  claude\nfinished  Local  codex\n");
}

#[test]
fn no_alerts_say_so() {
    assert_eq!(app::alert_lines(&[]), "No alerts yet");
}
