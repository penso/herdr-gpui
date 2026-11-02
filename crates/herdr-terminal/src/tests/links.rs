use super::*;

const LINK: &[u8] = b"\x1b]8;;https://a.example\x1b\\ab\x1b]8;;\x1b\\ \x1b]8;id=x;https://b.example\x07c\x1b]8;;\x07 \x1b]8;;https://a.example\x07d\x1b]8;;\x07";

#[test]
fn hyperlinks_are_listed_once_and_indexed() {
    let frame = fed(12, 1, LINK).frame();
    assert_eq!(frame.hyperlinks, ["https://a.example", "https://b.example"]);
    assert_eq!(cell(&frame, 0, 0).hyperlink, Some(0));
    assert_eq!(cell(&frame, 1, 0).hyperlink, Some(0));
    assert_eq!(cell(&frame, 2, 0).hyperlink, None);
    assert_eq!(cell(&frame, 3, 0).hyperlink, Some(1));
    assert_eq!(cell(&frame, 5, 0).hyperlink, Some(0));
    frame.validate().unwrap();
}

#[test]
fn rows_reuse_the_full_frames_links() {
    let mut emulator = fed(12, 2, LINK);
    assert!(matches!(emulator.take_damage(), Damage::Full(_)));
    emulator.feed(b"\r\n\x1b]8;;https://b.example\x07z\x1b]8;;\x07");
    let Damage::Rows { rows, .. } = emulator.take_damage() else {
        panic!("a known link stays row damage");
    };
    let linked = rows.iter().find(|row| row.y == 1).unwrap();
    assert_eq!(linked.cells[0].hyperlink, Some(1));
}

#[test]
fn a_new_link_makes_the_damage_full() {
    let mut emulator = fed(12, 2, b"plain");
    assert!(matches!(emulator.take_damage(), Damage::Full(_)));
    emulator.feed(b"\r\n\x1b]8;;https://new.example\x07n\x1b]8;;\x07");
    let Damage::Full(frame) = emulator.take_damage() else {
        panic!("expected full damage");
    };
    assert_eq!(frame.hyperlinks, ["https://new.example"]);
    assert_eq!(cell(&frame, 0, 1).hyperlink, Some(0));
}
