use super::*;
use herdr_protocol::CursorState;

fn cursor(bytes: &[u8]) -> Option<CursorState> {
    fed(10, 4, bytes).frame().cursor
}

#[test]
fn cursor_follows_output_and_moves() {
    let state = cursor(b"abc").unwrap();
    assert_eq!((state.x, state.y, state.visible), (3, 0, true));
    let state = cursor(b"\x1b[3;5H").unwrap();
    assert_eq!((state.x, state.y), (4, 2));
}

#[test]
fn hidden_cursor_is_not_visible() {
    assert!(!cursor(b"\x1b[?25l").unwrap().visible);
    assert!(cursor(b"\x1b[?25l\x1b[?25h").unwrap().visible);
}

#[test]
fn cursor_shapes_are_decscusr_parameters() {
    assert_eq!(cursor(b"").unwrap().shape, 0);
    for shape in 1..=6u8 {
        let bytes = format!("\x1b[{shape} q");
        assert_eq!(cursor(bytes.as_bytes()).unwrap().shape, shape);
    }
    assert_eq!(cursor(b"\x1b[5 q\x1b[0 q").unwrap().shape, 0);
}

#[test]
fn cursor_scrolled_out_of_view_is_none() {
    let mut emulator = fed(10, 2, b"1\r\n2\r\n3\r\n4\r\n5");
    emulator.scroll(Scroll::Lines(3));
    assert_eq!(emulator.frame().cursor, None);
}
