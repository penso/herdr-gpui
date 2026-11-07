use super::*;
use crate::{
    frame::ImageWriter,
    limits::{IDLE_WAIT, QUIET_PING},
    session::wait_timeout,
};

const SOON: Duration = Duration::from_millis(100);
/// Far longer than any test runs: a wait that ends at all ended on a wake,
/// the socket, or a deadline, never on the idle cap.
const NEVER: Duration = Duration::from_secs(3600);
/// Generous bound for something that should happen promptly, so a loaded
/// runner cannot fail it, while a missed wake (`NEVER`) still would.
const BOUND: Duration = Duration::from_secs(10);

/// Block until the worker is inside a wait it entered after count `after`,
/// returning the count. Whatever the test does next reaches the worker only
/// through a wake or the socket, never its next pass over the queue.
#[cfg(unix)]
pub(super) fn idle_after(waits: &AtomicUsize, after: usize) -> usize {
    let deadline = Instant::now() + BOUND;
    loop {
        let count = waits.load(std::sync::atomic::Ordering::Acquire);
        if count > after && count % 2 == 1 {
            return count;
        }
        assert!(Instant::now() < deadline, "the worker never waited");
        thread::sleep(Duration::from_millis(1));
    }
}

pub(super) fn paced(idle_wait: Duration, partial_frame: Duration) -> Pace {
    Pace {
        idle_wait,
        partial_frame,
        ..Pace::default()
    }
}

fn join_within(worker: thread::JoinHandle<Result<()>>, bound: Duration) -> Result<()> {
    let (done, finished) = bounded(1);
    thread::spawn(move || done.send(worker.join().unwrap()));
    finished
        .recv_timeout(bound)
        .expect("the worker kept waiting")
}

#[test]
fn nothing_pending_waits_the_idle_cap() {
    let now = Instant::now();
    let session = ready_session();
    assert_eq!(session.deadline(), None);
    assert_eq!(FrameReader::new().deadline(), None);
    assert_eq!(ImageWriter::default().deadline(), None);
    assert_eq!(Pace::default().idle_wait, IDLE_WAIT);
    assert_eq!(wait_timeout(now, IDLE_WAIT, [None, None]), IDLE_WAIT);
}

#[test]
fn each_owner_reports_the_deadline_its_check_enforces() {
    let now = Instant::now() + COMMAND_TIMEOUT * 2;

    let mut session = Session::new(true, false);
    session.started = now;
    assert_eq!(session.deadline(), Some(now + TIMEOUT));

    let mut session = ready_session();
    session.pending = Some(Pending {
        id: "1".into(),
        method: Method::PaneFocus,
        bytes: Vec::new(),
        started: now,
    });
    assert_eq!(session.deadline(), Some(now + COMMAND_TIMEOUT));

    let mut health = Health::new(now);
    assert_eq!(health.deadline(), now + QUIET_PING);
    health.ping = Some((now, now + SOON));
    assert_eq!(health.deadline(), now + SOON);

    let mut reader = FrameReader::with_timeout(SOON);
    reader.started = Some(now);
    assert_eq!(reader.deadline(), Some(now + SOON));

    let writer = ImageWriter {
        offset: 0,
        started: Some(now),
    };
    assert_eq!(writer.deadline(), Some(now + COMMAND_TIMEOUT));
}

#[test]
fn the_wait_ends_at_the_earliest_deadline_and_never_exceeds_the_cap() {
    let now = Instant::now();
    assert_eq!(
        wait_timeout(
            now,
            IDLE_WAIT,
            [Some(now + SOON * 3), None, Some(now + SOON)]
        ),
        SOON
    );
    assert_eq!(
        wait_timeout(now, IDLE_WAIT, [Some(now + IDLE_WAIT * 2)]),
        IDLE_WAIT
    );
    assert_eq!(wait_timeout(now, SOON, [None]), SOON);
    assert_eq!(
        wait_timeout(now + SOON, IDLE_WAIT, [Some(now)]),
        Duration::ZERO
    );
}

#[cfg(unix)]
#[test]
fn an_idle_worker_stops_on_disconnect_and_on_drop_through_its_wake() {
    for drop_handle in [false, true] {
        let (client, mut server, worker, waits) =
            test_client_paced(true, false, paced(NEVER, TIMEOUT));
        handshake(&mut server);
        event(&client);
        event(&client);
        idle_after(&waits, 0);
        if drop_handle {
            drop(client);
        } else {
            client.handle.disconnect();
        }
        join_within(worker, BOUND).unwrap();
    }
}

#[cfg(unix)]
#[test]
fn an_idle_worker_sends_a_queued_command_through_its_wake() {
    let (client, mut server, worker, waits) = test_client_paced(true, false, paced(NEVER, TIMEOUT));
    handshake(&mut server);
    event(&client);
    event(&client);
    idle_after(&waits, 0);
    client.handle.set_focus("boot-v1", true).unwrap();
    server.set_read_timeout(Some(BOUND)).unwrap();
    let _ = receive(&mut server);
    client.handle.disconnect();
    join_within(worker, BOUND).unwrap();
}

#[cfg(unix)]
#[test]
fn a_frame_split_across_an_idle_wait_is_still_delivered_whole() {
    let (client, mut server, worker, waits) = test_client_paced(true, false, paced(NEVER, NEVER));
    handshake(&mut server);
    event(&client);
    event(&client);
    let surface = baseline();
    let bytes = encode_message(
        &ServerMessage::PaneSurface(surface.clone()),
        MAX_GRAPHICS_FRAME_SIZE,
    )
    .unwrap();
    let (head, tail) = bytes.split_at(6);
    let idle = idle_after(&waits, 0);
    server.write_all(head).unwrap();
    // The worker read the head and is waiting again, holding a partial frame.
    idle_after(&waits, idle);
    server.write_all(tail).unwrap();
    assert!(matches!(event(&client), ClientEvent::Surface(s) if *s == surface));
    client.handle.disconnect();
    join_within(worker, BOUND).unwrap();
}

#[test]
fn a_peer_that_stalls_mid_frame_is_timed_out_at_its_deadline() {
    let partial_frame = Duration::from_millis(200);
    let (client, mut server, worker, _) =
        test_client_paced(true, false, paced(NEVER, partial_frame));
    handshake(&mut server);
    event(&client);
    event(&client);
    let bytes = encode_message(
        &ServerMessage::PaneSurface(baseline()),
        MAX_GRAPHICS_FRAME_SIZE,
    )
    .unwrap();
    let stalled = Instant::now();
    server.write_all(&bytes[..8]).unwrap();
    // With an hour-long idle cap, only the frame's own deadline ends the wait.
    let result = join_within(worker, BOUND);
    assert!(
        matches!(result, Err(Error::PartialFrameTimeout)),
        "{result:?}"
    );
    assert!(stalled.elapsed() >= partial_frame);
    drop(server);
}

#[test]
fn bytes_of_a_slow_frame_answer_a_liveness_probe() {
    let liveness = Duration::from_millis(500);
    let pace = Pace {
        liveness,
        ..paced(NEVER, NEVER)
    };
    let (client, mut server, worker, _) = test_client_paced(false, true, pace);
    ssh_handshake(&mut server);
    event(&client);
    event(&client);
    let surface = baseline();
    let bytes = encode_message(
        &ServerMessage::PaneSurface(surface.clone()),
        MAX_GRAPHICS_FRAME_SIZE,
    )
    .unwrap();
    client.handle.check_liveness();
    assert!(
        matches!(receive(&mut server), ClientMessage::EndpointControl { kind, .. } if kind == "endpoint.health.ping.v1")
    );
    // The frame takes several probe deadlines to arrive, but never pauses
    // for one: the link is alive, only the frame is large and the link slow.
    let started = Instant::now();
    let mut sent = 0;
    while started.elapsed() < liveness * 3 && sent < bytes.len() - 1 {
        server.write_all(&bytes[sent..=sent]).unwrap();
        sent += 1;
        thread::sleep(liveness / 10);
    }
    assert!(started.elapsed() >= liveness * 3, "the frame is too short");
    server.write_all(&bytes[sent..]).unwrap();
    assert!(matches!(event(&client), ClientEvent::Surface(s) if *s == surface));
    client.handle.disconnect();
    join_within(worker, BOUND).unwrap();
}
