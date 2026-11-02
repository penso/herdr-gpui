use super::*;

fn numbered(lines: usize) -> Vec<u8> {
    (0..lines)
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\r\n")
        .into_bytes()
}

#[test]
fn history_is_bounded_by_the_scrollback_asked_for() {
    let mut emulator = Emulator::new(size(10, 3), 5);
    emulator.feed(&numbered(50));
    let metrics = emulator.scroll_metrics();
    assert_eq!(metrics.history, 5);
    assert_eq!((metrics.offset, metrics.rows), (0, 3));
    emulator.scroll(Scroll::Top);
    let frame = emulator.frame();
    assert_eq!(line(&frame, 0), "42");
    assert_eq!(emulator.scroll_metrics().offset, 5);
}

#[test]
fn scrollback_is_clamped() {
    let mut emulator = Emulator::new(size(MAX_SIDE, 1), usize::MAX);
    emulator.feed(&numbered(5_000));
    assert_eq!(
        emulator.scroll_metrics().history,
        MAX_SCROLLBACK_CELLS / usize::from(MAX_SIDE)
    );
    let mut narrow = Emulator::new(size(1, 1), usize::MAX);
    narrow.feed(&"\n".repeat(MAX_SCROLLBACK_LINES + 10).into_bytes());
    assert_eq!(narrow.scroll_metrics().history, MAX_SCROLLBACK_LINES);
}

#[test]
fn scrolling_moves_the_view_and_clamps() {
    let mut emulator = Emulator::new(size(10, 3), 100);
    emulator.feed(&numbered(10));
    assert_eq!(line(&emulator.frame(), 2), "9");
    emulator.scroll(Scroll::Lines(2));
    assert_eq!(emulator.scroll_metrics().offset, 2);
    assert_eq!(line(&emulator.frame(), 2), "7");
    emulator.scroll(Scroll::Lines(100));
    assert_eq!(emulator.scroll_metrics().offset, 7);
    emulator.scroll(Scroll::PageDown);
    assert_eq!(emulator.scroll_metrics().offset, 4);
    emulator.scroll(Scroll::Lines(-1));
    assert_eq!(emulator.scroll_metrics().offset, 3);
    emulator.scroll(Scroll::Bottom);
    assert_eq!(emulator.scroll_metrics().offset, 0);
    emulator.scroll(Scroll::PageUp);
    assert_eq!(emulator.scroll_metrics().offset, 3);
}

#[test]
fn output_while_scrolled_keeps_the_view_on_its_lines() {
    let mut emulator = Emulator::new(size(10, 3), 100);
    emulator.feed(&numbered(10));
    emulator.scroll(Scroll::Lines(2));
    emulator.feed(b"\r\nmore");
    assert_eq!(emulator.scroll_metrics().offset, 3);
    assert_eq!(line(&emulator.frame(), 2), "7");
}
