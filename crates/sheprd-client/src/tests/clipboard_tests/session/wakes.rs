use super::*;
use crate::{
    limits::QUIET_PING,
    tests::idle_wait::{idle_after, paced},
};

/// Far longer than any test runs, so only a wake or the socket ends a wait.
const NEVER: Duration = Duration::from_secs(3600);

fn ready_idle() -> (
    Client,
    Stream,
    thread::JoinHandle<Result<()>>,
    Arc<AtomicUsize>,
) {
    let (client, mut server, worker, waits) = test_client_paced(true, false, paced(NEVER, TIMEOUT));
    handshake(&mut server);
    event(&client);
    event(&client);
    (client, server, worker, waits)
}

/// Reserve a slot and return once the worker holds it and waits on it.
fn reserved(client: &Client, waits: &AtomicUsize) -> ClipboardImageUpload {
    let idle = idle_after(waits, 0);
    let upload = reserve(client);
    idle_after(waits, idle);
    upload
}

fn finishes(cancel: &ClipboardImageCancellation) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !cancel.is_finished() {
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(1));
    }
    true
}

#[test]
fn publication_wakes_a_worker_waiting_on_a_prepared_slot() {
    let (client, mut server, worker, waits) = ready_idle();
    let upload = reserved(&client, &waits);
    upload
        .complete_input(ClientPaneInputEvent::Paste("late.png".into()))
        .unwrap();
    server
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    assert!(matches!(
        receive(&mut server),
        ClientMessage::ClientShellPaneInput { .. }
    ));
    client.handle.disconnect();
    worker.join().unwrap().unwrap();
}

#[test]
fn dropping_or_cancelling_a_slot_wakes_the_worker_to_release_it() {
    for drop_upload in [true, false] {
        let (client, _server, worker, waits) = ready_idle();
        let upload = reserved(&client, &waits);
        let cancel = upload.cancellation_handle();
        if drop_upload {
            drop(upload);
        } else {
            cancel.cancel();
        }
        assert!(finishes(&cancel));
        client.handle.disconnect();
        worker.join().unwrap().unwrap();
    }
}

#[test]
fn a_backpressured_image_resumes_when_the_daemon_reads() {
    let (client, mut server, worker, _) = ready_idle();
    let upload = reserve(&client);
    upload
        .complete("png", vec![7; MAX_CLIPBOARD_IMAGE_PAYLOAD])
        .unwrap();
    // The image far exceeds the socket buffer, so the worker must wait for
    // room many times; with an hour-long idle cap, only writability ends it.
    server
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let message: ClientMessage = read_message(&mut server, MAX_CLIPBOARD_IMAGE_FRAME_SIZE).unwrap();
    assert!(matches!(message, ClientMessage::ClipboardImage { .. }));
    client.handle.disconnect();
    worker.join().unwrap().unwrap();
}

#[test]
fn a_liveness_request_deferred_by_an_image_is_sent_when_the_image_ends() {
    let (client, mut server, worker, _) = test_client_paced(false, true, paced(NEVER, TIMEOUT));
    ssh_handshake(&mut server);
    event(&client);
    event(&client);
    let quiet_since = Instant::now();
    reserve(&client)
        .complete("png", vec![7; MAX_FRAME_SIZE + 1])
        .unwrap();
    let mut prefix = [0; 4];
    server.read_exact(&mut prefix).unwrap();
    // The image is now part written, so the probe must wait for its end.
    client.handle.check_liveness();
    let mut payload = vec![0; u32::from_le_bytes(prefix) as usize];
    server.read_exact(&mut payload).unwrap();
    assert!(
        matches!(receive(&mut server), ClientMessage::EndpointControl { kind, .. } if kind == "endpoint.health.ping.v1")
    );
    // Sooner than the routine ping of a quiet link, which would also send one.
    assert!(
        quiet_since.elapsed() < QUIET_PING,
        "{:?}",
        quiet_since.elapsed()
    );
    client.handle.disconnect();
    worker.join().unwrap().unwrap();
}

#[test]
fn an_upload_kept_past_its_handle_does_not_keep_the_worker_waiting() {
    let (client, _server, worker, waits) = ready_idle();
    let upload = reserved(&client, &waits);
    // The upload keeps the wake channel open, so only an explicit wake from
    // the dropped handle ends the worker's wait.
    drop(client);
    let (done, finished) = bounded(1);
    thread::spawn(move || done.send(worker.join().unwrap()));
    finished
        .recv_timeout(Duration::from_secs(10))
        .expect("the worker kept waiting")
        .unwrap();
    drop(upload);
}

#[test]
fn a_stalled_image_sleeps_past_the_health_deadline_it_defers() {
    let liveness = Duration::from_secs(1);
    let pace = Pace {
        liveness,
        ..paced(NEVER, TIMEOUT)
    };
    let (client, mut server, worker, waits) = test_client_paced(false, true, pace);
    ssh_handshake(&mut server);
    event(&client);
    event(&client);
    // An unanswered probe puts the health deadline `liveness` away.
    client.handle.check_liveness();
    assert!(
        matches!(receive(&mut server), ClientMessage::EndpointControl { kind, .. } if kind == "endpoint.health.ping.v1")
    );
    let probed = Instant::now();
    // The peer reads only the image's prefix, so the frame stays part written
    // and the health check stays deferred well past that deadline.
    reserve(&client)
        .complete("png", vec![7; MAX_FRAME_SIZE + 1])
        .unwrap();
    let mut prefix = [0; 4];
    server.read_exact(&mut prefix).unwrap();
    assert!(probed.elapsed() < liveness, "the image started too late");
    thread::sleep((probed + liveness * 3 / 2).saturating_duration_since(Instant::now()));
    let before = waits.load(std::sync::atomic::Ordering::Acquire);
    thread::sleep(liveness / 2);
    let passes = waits.load(std::sync::atomic::Ordering::Acquire) - before;
    // A worker that kept waking for the deferred check would pass here about
    // every 10 ms; one waiting for room in the socket does not pass at all.
    assert!(passes <= 2, "{passes} wait entries and exits");
    client.handle.disconnect();
    worker.join().unwrap().unwrap();
}

#[test]
fn a_reservation_expiring_behind_an_inbound_frame_waits_for_the_frame() {
    use crate::{
        clipboard::{ImageLease, ImageSlot},
        handle::Command,
    };
    let expires_in = Duration::from_millis(500);
    // A reservation made `COMMAND_TIMEOUT - expires_in` ago, without
    // subtracting from an `Instant` that may be younger than that.
    let Some(reserved_at) = Instant::now().checked_sub(COMMAND_TIMEOUT - expires_in) else {
        return;
    };
    let (client, mut server, worker, waits) = ready_idle();
    let lease = Arc::new(ImageLease {
        busy: client.handle.inner.image_busy.clone(),
        claimed: AtomicBool::new(false),
        cancelled: Arc::new(AtomicBool::new(false)),
        finished: Arc::new(AtomicBool::new(false)),
        reserved_at,
        waker: client.handle.inner.commands.waker(),
    });
    let (_sender, receiver) = bounded::<Vec<u8>>(1);
    let idle = idle_after(&waits, 0);
    client
        .handle
        .inner
        .commands
        .try_send(Command {
            boot_id: "boot-v1".into(),
            bytes: Vec::new(),
            input_pane: None,
            written: Default::default(),
            request: None,
            image: Some(ImageSlot {
                receiver,
                lease,
                writer: Default::default(),
            }),
        })
        .unwrap_or_else(|_| panic!("empty queue"));
    // The worker holds the unpublished slot, then starts an inbound frame
    // that fences it before it expires.
    let idle = idle_after(&waits, idle);
    let bytes = encode_message(
        &ServerMessage::PaneSurface(baseline()),
        MAX_GRAPHICS_FRAME_SIZE,
    )
    .unwrap();
    server.write_all(&bytes[..6]).unwrap();
    idle_after(&waits, idle);
    thread::sleep(expires_in * 2);
    let before = waits.load(std::sync::atomic::Ordering::Acquire);
    thread::sleep(expires_in);
    let passes = waits.load(std::sync::atomic::Ordering::Acquire) - before;
    // The expiry cannot run behind the frame, so its deadline must not keep
    // ending the wait.
    assert!(passes <= 2, "{passes} wait entries and exits");
    // Once the frame ends, the slot expires.
    server.write_all(&bytes[6..]).unwrap();
    assert!(matches!(event(&client), ClientEvent::Surface(_)));
    assert!(matches!(
        event(&client),
        ClientEvent::CommandRejected {
            reason: Error::Clipboard(clipboard::Error::PreparationTimeout),
            ..
        }
    ));
    client.handle.disconnect();
    worker.join().unwrap().unwrap();
}
