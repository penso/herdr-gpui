use super::*;

/// A stream touching most of what the emulator reads: SGR, cursor moves,
/// wide and combining text, OSC title and link, modes, alt screen, scrolling.
const STREAM: &[u8] = "\x1b]0;title\x07\x1b[1;31mred\x1b[0m 界e\u{301}\r\n\
    \x1b]8;;https://example.com\x1b\\link\x1b]8;;\x1b\\\r\n\
    \x1b[38;2;10;20;30mrgb\x1b[48;5;100m idx\x1b[m\r\n\
    \x1b[?2004h\x1b[?1h\x1b[?1006h\x1b[?1002h\x1b[?9001h\
    \x1b[5 q\x1b[2;3Hmid\x1b[K\r\n1\r\n2\r\n3\r\n4\r\n5\
    \x1b[?1049hALT\x1b[?1049l\x1b[?2026hsync\x1b[?2026l\x07"
    .as_bytes();

fn state(emulator: &mut Emulator) -> (FrameData, Modes, Option<String>, Vec<Event>) {
    (
        emulator.frame(),
        emulator.modes(),
        emulator.title().map(str::to_owned),
        emulator.take_events(),
    )
}

#[test]
fn byte_at_a_time_matches_feeding_whole() {
    let mut whole = fed(20, 5, STREAM);
    let mut split = emulator(20, 5);
    for byte in STREAM {
        split.feed(std::slice::from_ref(byte));
    }
    let expected = state(&mut whole);
    assert_eq!(state(&mut split), expected);
    assert!(expected.1.win32_input && expected.1.bracketed_paste);
    assert_eq!(expected.2.as_deref(), Some("title"));
    assert_eq!(whole.scroll_metrics(), split.scroll_metrics());
}

#[test]
fn every_split_point_matches_feeding_whole() {
    let mut whole = fed(20, 5, STREAM);
    let expected = state(&mut whole);
    for at in 1..STREAM.len() {
        let mut split = emulator(20, 5);
        split.feed(&STREAM[..at]);
        split.feed(&STREAM[at..]);
        assert_eq!(state(&mut split), expected, "split at {at}");
    }
}

#[test]
fn win32_input_mode_survives_a_split_sequence() {
    let mut emulator = emulator(5, 1);
    for chunk in [&b"\x1b"[..], b"[", b"?", b"90", b"0", b"1", b"h"] {
        assert!(!emulator.modes().win32_input);
        emulator.feed(chunk);
    }
    assert!(emulator.modes().win32_input);
    emulator.feed(b"\x1b[?9");
    emulator.feed(b"001l");
    assert!(!emulator.modes().win32_input);
}

#[test]
fn win32_input_mode_reads_parameter_lists_and_resets() {
    let mut emulator = fed(5, 1, b"\x1b[?1;9001;2004h");
    assert!(emulator.modes().win32_input);
    assert!(emulator.modes().bracketed_paste);
    emulator.feed(b"\x1bc");
    assert!(!emulator.modes().win32_input);
    for ignored in [
        &b"\x1b[9001h"[..],
        b"\x1b[?90010h",
        b"\x1b[?9001$p",
        b"\x1b[>9001h",
    ] {
        assert!(!fed(5, 1, ignored).modes().win32_input, "{ignored:?}");
    }
    let mut cancelled = fed(5, 1, b"\x1b[?9001\x18h");
    assert!(!cancelled.modes().win32_input);
    cancelled.feed(b"\x1b]0;\x1b[?9001h\x07");
    assert!(cancelled.modes().win32_input, "ESC ends an OSC string");
}

#[test]
fn oversized_osc_strings_are_dropped_whole() {
    let mut huge = b"\x1b]0;".to_vec();
    huge.extend(std::iter::repeat_n(b'a', MAX_OSC_BYTES * 4));
    huge.extend_from_slice(b"\x07after");
    let mut whole = fed(10, 1, &huge);
    assert_eq!(whole.title(), None);
    assert!(
        whole.take_events().is_empty(),
        "no bell from the terminator"
    );
    assert_eq!(line(&whole.frame(), 0), "after");

    let mut split = emulator(10, 1);
    for chunk in huge.chunks(7) {
        split.feed(chunk);
    }
    assert_eq!(split.frame(), whole.frame());

    let mut st = fed(10, 1, &huge[..huge.len() - 6]);
    st.feed(b"\x1b\\ok");
    assert_eq!(line(&st.frame(), 0), "ok");
}

#[test]
fn garbage_never_panics() {
    let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
    let mut next = move || {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (seed >> 33) as u8
    };
    let mut emulator = emulator(13, 7);
    for round in 0..300 {
        let length = usize::from(next()) * 4;
        let mut bytes: Vec<u8> = (0..length).map(|_| next()).collect();
        // Bias toward escapes so the parser's states are exercised.
        for index in (0..bytes.len()).step_by(9) {
            bytes[index] = [0x1b, b'[', b']', b'?', b';', 0x07][usize::from(next()) % 6];
        }
        emulator.feed(&bytes);
        if round % 50 == 0 {
            emulator.resize(size(u16::from(next() % 40) + 1, u16::from(next() % 20) + 1));
        }
        emulator.scroll(Scroll::Lines(i32::from(next() % 5) - 2));
        let frame = emulator.frame();
        frame.validate().unwrap();
        let _ = emulator.take_damage();
        let _ = emulator.take_events();
    }
}
