//! Scrolls go to Herdr as wheel scrolls at the terminal's middle, in
//! order, after any resize, merged while they wait.

use super::*;

/// An attach at `cols` by `rows` and the peer that took it, past the
/// handshake.
fn attached(cols: u16, rows: u16) -> (TerminalAttach, Stream) {
    let (connect, mut peers) = streams(1);
    let mut peer = peers.remove(0);
    let herdr = thread::spawn(move || {
        receive(&mut peer);
        send(&mut peer, welcome(TERMINAL_PROTOCOL_VERSION, None));
        receive(&mut peer);
        send(&mut peer, ServerMessage::WindowTitle { title: None });
        peer
    });
    let attach = TerminalAttach::open_with(connect, "term_1", cols, rows).unwrap();
    (attach, herdr.join().unwrap())
}

fn wheel(direction: AttachScrollDirection, lines: u16, column: u16, row: u16) -> ClientMessage {
    ClientMessage::AttachScroll {
        source: AttachScrollSource::Wheel,
        direction,
        lines,
        column: Some(column),
        row: Some(row),
        modifiers: 0,
    }
}

#[test]
fn a_scroll_is_a_wheel_at_the_middle_of_the_terminal() {
    let (attach, mut peer) = attached(46, 30);
    attach.scroll(AttachScrollDirection::Up, 3).unwrap();
    assert_eq!(
        receive(&mut peer),
        wheel(AttachScrollDirection::Up, 3, 23, 15)
    );
    attach.scroll(AttachScrollDirection::Down, 2).unwrap();
    assert_eq!(
        receive(&mut peer),
        wheel(AttachScrollDirection::Down, 2, 23, 15)
    );
    drop(attach);
    assert_eq!(receive(&mut peer), ClientMessage::Detach);
}

#[test]
fn a_scroll_after_a_resize_points_at_the_new_middle() {
    let (attach, mut peer) = attached(46, 30);
    attach.resize(80, 40).unwrap();
    attach.scroll(AttachScrollDirection::Up, 1).unwrap();
    assert!(matches!(
        receive(&mut peer),
        ClientMessage::Resize {
            cols: 80,
            rows: 40,
            ..
        }
    ));
    assert_eq!(
        receive(&mut peer),
        wheel(AttachScrollDirection::Up, 1, 40, 20)
    );
}

#[test]
fn waiting_scrolls_merge_one_way_and_keep_their_order() {
    let pending = Shared {
        pending: Mutex::new(Pending {
            size: (10, 4),
            resize: None,
            scrolls: VecDeque::new(),
        }),
        stop: AtomicBool::new(false),
        ended: AtomicBool::new(false),
    };
    let attach = TerminalAttach {
        shared: Arc::new(pending),
        worker: None,
    };
    attach.scroll(AttachScrollDirection::Up, 3).unwrap();
    attach.scroll(AttachScrollDirection::Up, 2).unwrap();
    attach.scroll(AttachScrollDirection::Down, 1).unwrap();
    attach.scroll(AttachScrollDirection::Up, 999).unwrap();
    attach.scroll(AttachScrollDirection::Up, 999).unwrap();
    assert_eq!(
        attach.shared.take(),
        vec![
            wheel(AttachScrollDirection::Up, 5, 5, 2),
            wheel(AttachScrollDirection::Down, 1, 5, 2),
            wheel(AttachScrollDirection::Up, MAX_SCROLL_LINES, 5, 2),
        ]
    );
    for index in 0..MAX_SCROLLS {
        let direction = if index % 2 == 0 {
            AttachScrollDirection::Up
        } else {
            AttachScrollDirection::Down
        };
        attach.scroll(direction, 1).unwrap();
    }
    assert!(matches!(
        attach.scroll(AttachScrollDirection::Up, 1),
        Err(Error::Busy)
    ));
    assert!(matches!(
        attach.scroll(AttachScrollDirection::Up, 0),
        Err(Error::Scroll)
    ));
}

#[test]
fn an_ended_attach_does_not_scroll() {
    let (attach, peer) = attached(46, 30);
    drop(peer);
    let due = Instant::now() + Duration::from_secs(3);
    while attach.is_attached() {
        assert!(Instant::now() < due, "the attach never noticed the close");
        thread::sleep(Duration::from_millis(5));
    }
    assert!(matches!(
        attach.scroll(AttachScrollDirection::Up, 1),
        Err(Error::Ended)
    ));
}
