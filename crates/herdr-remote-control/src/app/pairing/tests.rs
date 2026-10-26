use super::*;
use crate::control::DeviceId;

fn offer(id: &str) -> Offer {
    Offer {
        id: PairingId(id.into()),
        url: format!("https://192.168.1.20:7443/pair#t={id}&fp=5F:2A"),
        fingerprint: "5F:2A".into(),
        expires_in_ms: 60_000,
        addresses: vec!["https://192.168.1.20:7443".into()],
    }
}

/// Sends what the machine asks for next and answers it with `response`.
fn step(
    pairing: &mut Pairing,
    now: Instant,
    response: Result<Response, control::Error>,
) -> Request {
    let ticket = pairing.next(now).unwrap();
    let request = ticket.request.clone();
    pairing.receive(&ticket, response, now);
    request
}

fn showing(now: Instant) -> Pairing {
    let mut pairing = Pairing::new(true);
    assert_eq!(
        step(&mut pairing, now, Ok(Response::Offer(offer("p1")))),
        Request::Pair
    );
    pairing
}

#[test]
fn off_asks_for_nothing() {
    let mut pairing = Pairing::new(false);
    assert_eq!(pairing.phase(), &Phase::Off);
    assert!(pairing.next(Instant::now()).is_none());
    pairing.restart();
    assert_eq!(pairing.phase(), &Phase::Off);
}

#[test]
fn an_offer_shows_a_qr_and_is_polled_every_second() {
    let now = Instant::now();
    let mut pairing = showing(now);
    let Phase::Showing { offer: shown, .. } = pairing.phase() else {
        panic!("{:?}", pairing.phase());
    };
    assert_eq!(shown.id, PairingId("p1".into()));
    assert!(pairing.cancels_on_close());
    // Polled at most once a second, and one request at a time.
    assert!(pairing.next(now + Duration::from_millis(500)).is_none());
    let ticket = pairing.next(now + POLL).unwrap();
    assert_eq!(
        ticket.request,
        Request::Pairing {
            id: PairingId("p1".into())
        }
    );
    assert!(pairing.next(now + POLL * 3).is_none(), "one in flight");
    pairing.receive(
        &ticket,
        Ok(Response::Pairing(PairingState::Waiting {
            expires_in_ms: 30_000,
        })),
        now + POLL,
    );
    // The daemon's remaining time replaces the dialog's own.
    let (left, share) = pairing.countdown(now + POLL).unwrap();
    assert_eq!(left, Duration::from_secs(30));
    assert!((share - 0.5).abs() < 1e-6);
}

#[test]
fn the_countdown_runs_out_and_a_fresh_qr_is_asked_for() {
    let now = Instant::now();
    let mut pairing = showing(now);
    let (left, share) = pairing.countdown(now).unwrap();
    assert_eq!(left, Duration::from_secs(60));
    assert_eq!(share, 1.);
    let later = now + Duration::from_secs(45);
    let (left, share) = pairing.countdown(later).unwrap();
    assert_eq!(left, Duration::from_secs(15));
    assert!((share - 0.25).abs() < 1e-6);
    let expired = now + Duration::from_secs(60);
    assert_eq!(pairing.countdown(expired).unwrap(), (Duration::ZERO, 0.));
    assert_eq!(
        step(&mut pairing, expired, Ok(Response::Offer(offer("p2")))),
        Request::Pair
    );
    assert_eq!(pairing.id(), Some(&PairingId("p2".into())));
    assert_eq!(
        pairing.countdown(expired).unwrap().0,
        Duration::from_secs(60)
    );
}

#[test]
fn an_expired_token_while_showing_is_replaced() {
    let now = Instant::now();
    let mut pairing = showing(now);
    step(
        &mut pairing,
        now + POLL,
        Ok(Response::Pairing(PairingState::Expired)),
    );
    assert_eq!(pairing.phase(), &Phase::Requesting);
    assert_eq!(pairing.next(now + POLL).unwrap().request, Request::Pair);
}

#[test]
fn a_claim_asks_the_user_and_allow_waits_for_approval() {
    let now = Instant::now();
    let mut pairing = showing(now);
    step(
        &mut pairing,
        now + POLL,
        Ok(Response::Pairing(PairingState::Claimed {
            device_name: "Fabien's iPhone".into(),
            code: "428913".into(),
            app: None,
        })),
    );
    let Phase::Claimed {
        device_name,
        code,
        answer: None,
        ..
    } = pairing.phase()
    else {
        panic!("{:?}", pairing.phase());
    };
    assert_eq!(device_name, "Fabien's iPhone");
    assert_eq!(spaced(code), "428 913");
    // No countdown while the user decides.
    assert!(pairing.countdown(now + POLL).is_none());
    pairing.answer(true);
    pairing.answer(false);
    let at = now + POLL + Duration::from_millis(10);
    // A daemon still saying Claimed leaves the answer sent and waiting.
    assert_eq!(
        step(
            &mut pairing,
            at,
            Ok(Response::Pairing(PairingState::Claimed {
                device_name: "Fabien's iPhone".into(),
                code: "428913".into(),
                app: None,
            })),
        ),
        Request::Approve {
            id: PairingId("p1".into()),
            allow: true,
            code: "428913".into(),
        }
    );
    assert!(matches!(
        pairing.phase(),
        Phase::Claimed {
            answer: Some(Answer {
                allow: true,
                sent: true
            }),
            ..
        }
    ));
    assert!(pairing.cancels_on_close());
    step(
        &mut pairing,
        at + POLL,
        Ok(Response::Pairing(PairingState::Approved {
            device: DeviceId("d1".into()),
        })),
    );
    assert_eq!(
        pairing.phase(),
        &Phase::Approved {
            device_name: "Fabien's iPhone".into()
        }
    );
    assert!(!pairing.cancels_on_close());
    assert!(pairing.next(at + POLL * 5).is_none());
}

#[test]
fn deny_ends_the_pairing() {
    let now = Instant::now();
    let mut pairing = showing(now);
    step(
        &mut pairing,
        now + POLL,
        Ok(Response::Pairing(PairingState::Claimed {
            device_name: "Pixel".into(),
            code: "000111".into(),
            app: None,
        })),
    );
    pairing.answer(false);
    assert_eq!(
        step(
            &mut pairing,
            now + POLL,
            Ok(Response::Pairing(PairingState::Denied))
        ),
        Request::Approve {
            id: PairingId("p1".into()),
            allow: false,
            code: "000111".into(),
        }
    );
    assert_eq!(pairing.phase(), &Phase::Denied);
    assert!(!pairing.cancels_on_close());
}

#[test]
fn a_claim_left_unanswered_expires_and_can_start_over() {
    let now = Instant::now();
    let mut pairing = showing(now);
    step(
        &mut pairing,
        now + POLL,
        Ok(Response::Pairing(PairingState::Claimed {
            device_name: "Pixel".into(),
            code: "000111".into(),
            app: None,
        })),
    );
    step(
        &mut pairing,
        now + POLL * 2,
        Err(control::Error::Daemon {
            code: ErrorCode::UnknownPairing,
            message: "gone".into(),
        }),
    );
    assert_eq!(
        pairing.phase(),
        &Phase::Expired {
            device_name: "Pixel".into()
        }
    );
    assert!(pairing.next(now + POLL * 3).is_none());
    pairing.restart();
    assert_eq!(pairing.next(now + POLL * 3).unwrap().request, Request::Pair);
}

#[test]
fn an_answer_to_a_request_sent_before_a_change_is_dropped() {
    let now = Instant::now();
    let mut pairing = showing(now);
    step(
        &mut pairing,
        now + POLL,
        Ok(Response::Pairing(PairingState::Claimed {
            device_name: "Pixel".into(),
            code: "000111".into(),
            app: None,
        })),
    );
    let stale = pairing.next(now + POLL * 2).unwrap();
    pairing.answer(true);
    pairing.receive(
        &stale,
        Ok(Response::Pairing(PairingState::Expired)),
        now + POLL * 2,
    );
    assert!(matches!(pairing.phase(), Phase::Claimed { .. }));
    assert!(matches!(
        pairing.next(now + POLL * 2).unwrap().request,
        Request::Approve { allow: true, .. }
    ));
}

#[test]
fn an_unreachable_daemon_fails_with_its_reason() {
    let mut pairing = Pairing::new(true);
    step(&mut pairing, Instant::now(), Err(control::Error::NoAnswer));
    let Phase::Failed { message } = pairing.phase() else {
        panic!("{:?}", pairing.phase());
    };
    assert!(message.contains("without answering"), "{message}");
    assert!(!pairing.cancels_on_close());
    pairing.restart();
    assert_eq!(pairing.phase(), &Phase::Requesting);
}

#[test]
fn allow_shows_the_approval_the_daemon_answers_with() {
    let now = Instant::now();
    let mut pairing = showing(now);
    step(
        &mut pairing,
        now + POLL,
        Ok(Response::Pairing(PairingState::Claimed {
            device_name: "Pixel".into(),
            code: "000111".into(),
            app: None,
        })),
    );
    pairing.answer(true);
    step(
        &mut pairing,
        now + POLL,
        Ok(Response::Pairing(PairingState::Approved {
            device: DeviceId("d1".into()),
        })),
    );
    assert_eq!(
        pairing.phase(),
        &Phase::Approved {
            device_name: "Pixel".into()
        }
    );
    // An approval answered with something else is a failure, not success.
    let mut pairing = showing(now);
    step(
        &mut pairing,
        now + POLL,
        Ok(Response::Pairing(PairingState::Claimed {
            device_name: "Pixel".into(),
            code: "000111".into(),
            app: None,
        })),
    );
    pairing.answer(true);
    step(&mut pairing, now + POLL, Ok(Response::Done));
    assert!(matches!(pairing.phase(), Phase::Failed { .. }));
}

#[test]
fn an_answer_of_the_wrong_kind_fails() {
    let mut pairing = Pairing::new(true);
    step(&mut pairing, Instant::now(), Ok(Response::Done));
    assert!(matches!(pairing.phase(), Phase::Failed { .. }));
}

#[test]
fn only_six_digit_codes_are_spaced() {
    assert_eq!(spaced("123456"), "123 456");
    assert_eq!(spaced("123 456"), "123 456");
    assert_eq!(spaced("12345"), "12345");
    assert_eq!(spaced("1234567"), "1234567");
}
