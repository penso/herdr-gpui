use crate::protocol::{Channel, Command, Event, MAX_LINE, Radio, read_line};
use std::io::Cursor;

#[test]
fn commands_read_as_the_macos_helper_reads_them() {
    assert_eq!(
        Command::parse(
            r#"{"type":"advertise","name":"Studio","fp":"0123456789ABCDEF","port":"7443"}"#
        ),
        Some(Command::Advertise {
            name: "Studio".into(),
            fingerprint: "0123456789ABCDEF".into()
        })
    );
    assert_eq!(
        Command::parse(r#"{"type":"advertise_presence","on":true}"#),
        Some(Command::Presence(true))
    );
    assert_eq!(Command::parse(r#"{"type":"stop"}"#), Some(Command::Stop));
    assert_eq!(
        Command::parse(r#"{"type":"presence_frame","central":"C","data":"aGk="}"#),
        Some(Command::Frame {
            central: "C".into(),
            channel: Channel::Presence,
            data: b"hi".to_vec()
        })
    );
    assert_eq!(
        Command::parse(r#"{"type":"frame","central":"C","data":"aGk="}"#),
        Some(Command::Frame {
            central: "C".into(),
            channel: Channel::Pair,
            data: b"hi".to_vec()
        })
    );
}

#[test]
fn malformed_unknown_and_oversized_lines_are_ignored() {
    assert_eq!(Command::parse("not json"), None);
    assert_eq!(Command::parse(r#"{"type":"later"}"#), None);
    assert_eq!(
        Command::parse(r#"{"type":"frame","central":"C","data":"%%%"}"#),
        None
    );
    let big = format!(
        r#"{{"type":"frame","central":"C","data":"{}"}}"#,
        "A".repeat(MAX_LINE)
    );
    assert_eq!(Command::parse(&big), None);
}

#[test]
fn events_write_the_macos_helpers_lines() {
    let line = |event: Event| serde_json::to_string(&event).unwrap();
    assert_eq!(
        line(Event::State {
            state: Radio::PoweredOn
        }),
        r#"{"type":"state","state":"powered_on"}"#
    );
    assert_eq!(
        line(Event::frame("C", Channel::Presence, b"hi")),
        r#"{"type":"presence_frame","central":"C","data":"aGk="}"#
    );
    assert_eq!(
        line(Event::frame("C", Channel::Pair, b"hi")),
        r#"{"type":"frame","central":"C","data":"aGk="}"#
    );
    assert_eq!(
        line(Event::Disconnected {
            central: "C".into()
        }),
        r#"{"type":"disconnected","central":"C"}"#
    );
    assert_eq!(
        line(Event::Idle { seconds: 4 }),
        r#"{"type":"idle","seconds":4}"#
    );
    assert_eq!(
        line(Event::error("x".repeat(300))).len(),
        r#"{"type":"error","message":""}"#.len() + 200
    );
}

#[test]
fn an_overlong_line_is_skipped_whole_and_reading_goes_on() {
    let input = format!("{}\n{}\n", "x".repeat(MAX_LINE + 10), r#"{"type":"stop"}"#);
    let mut reader = Cursor::new(input.into_bytes());
    let mut line = Vec::new();
    assert!(read_line(&mut reader, &mut line).unwrap());
    assert!(line.is_empty());
    assert!(read_line(&mut reader, &mut line).unwrap());
    assert_eq!(
        String::from_utf8_lossy(&line).trim_end(),
        r#"{"type":"stop"}"#
    );
    assert!(!read_line(&mut reader, &mut line).unwrap());
}
