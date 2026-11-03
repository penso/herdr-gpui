use super::*;
use crate::{MouseEncoding, MouseReporting};
use herdr_protocol::{ClientMouseButton, ClientMouseKind, ClientMousePosition};

fn mouse(kind: ClientMouseKind, column: u16, row: u16, modifiers: u8) -> ClientPaneInputEvent {
    ClientPaneInputEvent::Mouse {
        kind,
        position: ClientMousePosition::Cell { column, row },
        geometry: None,
        modifiers,
        lines: 1,
    }
}

fn reporting(mouse: MouseReporting, mouse_encoding: MouseEncoding) -> Modes {
    Modes {
        mouse,
        mouse_encoding,
        ..Modes::default()
    }
}

const LEFT_DOWN: ClientMouseKind = ClientMouseKind::Down(ClientMouseButton::Left);

#[test]
fn nothing_is_reported_with_reporting_off() {
    assert!(encode(&mouse(LEFT_DOWN, 1, 1, 0), &Modes::default()).is_empty());
    assert!(
        encode(
            &mouse(ClientMouseKind::ScrollUp, 1, 1, 0),
            &Modes::default()
        )
        .is_empty()
    );
}

#[test]
fn sgr_reports() {
    let modes = reporting(MouseReporting::Click, MouseEncoding::Sgr);
    assert_eq!(bytes(&mouse(LEFT_DOWN, 0, 0, 0), &modes), "\x1b[<0;1;1M");
    let up = ClientMouseKind::Up(ClientMouseButton::Right);
    assert_eq!(bytes(&mouse(up, 4, 9, 0), &modes), "\x1b[<2;5;10m");
    assert_eq!(
        bytes(&mouse(ClientMouseKind::ScrollDown, 2, 3, 0), &modes),
        "\x1b[<65;3;4M"
    );
    assert_eq!(
        bytes(&mouse(LEFT_DOWN, 0, 0, SHIFT | ALT | CONTROL), &modes),
        "\x1b[<28;1;1M"
    );
    assert_eq!(
        bytes(&mouse(LEFT_DOWN, 3000, 0, 0), &modes),
        "\x1b[<0;3001;1M"
    );
}

#[test]
fn reporting_level_filters_motion() {
    let drag = ClientMouseKind::Drag(ClientMouseButton::Left);
    let click = reporting(MouseReporting::Click, MouseEncoding::Sgr);
    assert!(encode(&mouse(drag, 1, 1, 0), &click).is_empty());
    let dragging = reporting(MouseReporting::Drag, MouseEncoding::Sgr);
    assert_eq!(bytes(&mouse(drag, 1, 1, 0), &dragging), "\x1b[<32;2;2M");
    assert!(encode(&mouse(ClientMouseKind::Moved, 1, 1, 0), &dragging).is_empty());
    let motion = reporting(MouseReporting::Motion, MouseEncoding::Sgr);
    assert_eq!(
        bytes(&mouse(ClientMouseKind::Moved, 1, 1, 0), &motion),
        "\x1b[<35;2;2M"
    );
}

#[test]
fn x10_reports_and_their_bounds() {
    let modes = reporting(MouseReporting::Click, MouseEncoding::X10);
    assert_eq!(
        encode(&mouse(LEFT_DOWN, 0, 0, 0), &modes),
        b"\x1b[M\x20\x21\x21"
    );
    let up = ClientMouseKind::Up(ClientMouseButton::Left);
    assert_eq!(encode(&mouse(up, 1, 2, 0), &modes), b"\x1b[M\x23\x22\x23");
    assert_eq!(
        encode(&mouse(LEFT_DOWN, 222, 0, 0), &modes),
        b"\x1b[M\x20\xff\x21"
    );
    assert!(encode(&mouse(LEFT_DOWN, 223, 0, 0), &modes).is_empty());
}

#[test]
fn utf8_reports_reach_further() {
    let modes = reporting(MouseReporting::Click, MouseEncoding::Utf8);
    let mut expected = b"\x1b[M\x20".to_vec();
    expected.extend("\u{12c}".as_bytes());
    expected.push(0x21);
    assert_eq!(encode(&mouse(LEFT_DOWN, 267, 0, 0), &modes), expected);
    assert!(encode(&mouse(LEFT_DOWN, 2015, 0, 0), &modes).is_empty());
}

#[test]
fn wheel_lines_repeat_reports() {
    let modes = reporting(MouseReporting::Click, MouseEncoding::Sgr);
    let input = ClientPaneInputEvent::Mouse {
        kind: ClientMouseKind::ScrollUp,
        position: ClientMousePosition::Pixels {
            x: 10,
            y: 10,
            column: 0,
            row: 0,
        },
        geometry: None,
        modifiers: 0,
        lines: 3,
    };
    assert_eq!(bytes(&input, &modes), "\x1b[<64;1;1M".repeat(3));
}

#[test]
fn wheel_on_the_alternate_screen_sends_arrows() {
    let modes = Modes {
        alt_screen: true,
        alternate_scroll: true,
        ..Modes::default()
    };
    assert_eq!(
        bytes(&mouse(ClientMouseKind::ScrollUp, 0, 0, 0), &modes),
        "\x1b[A"
    );
    let app = Modes {
        app_cursor: true,
        ..modes
    };
    assert_eq!(
        bytes(&mouse(ClientMouseKind::ScrollDown, 0, 0, 0), &app),
        "\x1bOB"
    );
    let primary = Modes {
        alt_screen: false,
        ..modes
    };
    assert!(encode(&mouse(ClientMouseKind::ScrollUp, 0, 0, 0), &primary).is_empty());
}
