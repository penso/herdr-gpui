use super::*;
use herdr_protocol::PaneSurfacePatchRow;

fn rows(damage: Damage) -> Vec<PaneSurfacePatchRow> {
    match damage {
        Damage::Rows { rows, .. } => rows,
        Damage::Full(_) => panic!("expected row damage"),
    }
}

fn settled(cols: u16, rows: u16, bytes: &[u8]) -> Emulator {
    let mut emulator = fed(cols, rows, bytes);
    assert!(matches!(emulator.take_damage(), Damage::Full(_)));
    emulator
}

#[test]
fn the_first_take_is_full() {
    let mut emulator = emulator(5, 2);
    let Damage::Full(frame) = emulator.take_damage() else {
        panic!("expected full damage");
    };
    assert_eq!(frame, emulator.frame());
}

#[test]
fn nothing_fed_means_no_rows() {
    let mut emulator = settled(5, 3, b"abc");
    let damage = emulator.take_damage();
    assert!(damage.is_empty());
    let Damage::Rows { cursor, .. } = damage else {
        panic!("expected row damage");
    };
    assert_eq!(cursor, emulator.frame().cursor);
}

#[test]
fn only_changed_rows_are_listed() {
    let mut emulator = settled(5, 4, b"aaa\r\nbbb\r\nccc\r\nddd\x1b[2;1H");
    emulator.feed(b"X");
    let rows = rows(emulator.take_damage());
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].y, 1);
    assert_eq!(rows[0].x, 0);
    assert_eq!(rows[0].cells.len(), 5);
    assert_eq!(rows[0].cells[0].symbol, "X");
}

#[test]
fn a_cursor_move_lists_the_rows_it_left_and_entered() {
    let mut emulator = settled(5, 4, b"\x1b[1;1H");
    emulator.feed(b"\x1b[3;2H");
    let Damage::Rows { rows, cursor } = emulator.take_damage() else {
        panic!("expected row damage");
    };
    let mut lines: Vec<u16> = rows.iter().map(|row| row.y).collect();
    lines.sort_unstable();
    assert_eq!(lines, [0, 2]);
    let cursor = cursor.unwrap();
    assert_eq!((cursor.x, cursor.y), (1, 2));
}

#[test]
fn patched_rows_rebuild_the_frame() {
    let mut emulator = fed(6, 3, b"one\r\ntwo\r\nthree");
    let Damage::Full(mut frame) = emulator.take_damage() else {
        panic!("expected full damage");
    };
    emulator.feed(b"\x1b[1;1Hxy\x1b[3;3Hz");
    let Damage::Rows { rows, cursor } = emulator.take_damage() else {
        panic!("expected row damage");
    };
    for row in rows {
        let start = usize::from(row.y) * usize::from(frame.width);
        frame.cells[start..start + row.cells.len()].clone_from_slice(&row.cells);
    }
    frame.cursor = cursor;
    assert_eq!(frame, emulator.frame());
}

#[test]
fn resize_is_full_damage() {
    let mut emulator = settled(5, 3, b"abc");
    emulator.resize(size(8, 2));
    let Damage::Full(frame) = emulator.take_damage() else {
        panic!("expected full damage");
    };
    assert_eq!((frame.width, frame.height), (8, 2));
    assert_eq!(line(&frame, 0), "abc");
}

#[test]
fn resizing_to_the_same_size_is_not_damage() {
    let mut emulator = settled(5, 3, b"abc");
    emulator.resize(size(5, 3));
    assert!(emulator.take_damage().is_empty());
}

#[test]
fn alternate_screen_switches_are_full_damage() {
    let mut emulator = settled(5, 3, b"abc");
    emulator.feed(b"\x1b[?1049h");
    assert!(matches!(emulator.take_damage(), Damage::Full(_)));
    assert!(emulator.modes().alt_screen);
    emulator.feed(b"\x1b[?1049l");
    let Damage::Full(frame) = emulator.take_damage() else {
        panic!("expected full damage");
    };
    assert_eq!(line(&frame, 0), "abc");
}

#[test]
fn clearing_the_screen_is_full_damage() {
    let mut emulator = settled(5, 3, b"abc");
    emulator.feed(b"\x1b[2J");
    assert!(matches!(emulator.take_damage(), Damage::Full(_)));
}

#[test]
fn scrolling_the_view_is_full_damage() {
    let mut emulator = settled(5, 2, b"1\r\n2\r\n3\r\n4");
    emulator.scroll(Scroll::Lines(1));
    assert!(matches!(emulator.take_damage(), Damage::Full(_)));
    emulator.scroll(Scroll::Lines(0));
    assert!(emulator.take_damage().is_empty());
}
