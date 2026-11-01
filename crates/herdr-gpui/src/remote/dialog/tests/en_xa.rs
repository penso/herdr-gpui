//! The `en-XA` leak test (`crate::i18n::leak_tests`) for remote control's
//! pairing dialog: off, showing a code with its addresses, and a phone
//! asking to be allowed. Each runs on its own thread, as `under_en_xa`
//! switches that thread's locale.
use super::*;
use crate::i18n::leak_tests::{assert_translated, draw, uncut, under_en_xa};

static CLAIMED_XA: AtomicBool = AtomicBool::new(false);

fn daemon(_: &Path, request: &Request) -> Result<Response, control::Error> {
    Ok(match request {
        Request::Pair => Response::Offer(offer()),
        Request::Pairing { .. } if CLAIMED_XA.load(Ordering::SeqCst) => {
            Response::Pairing(PairingState::Claimed {
                device_name: "Pixel".into(),
                code: "428913".into(),
                app: Some("https://other.example:8443".into()),
            })
        }
        Request::Pairing { .. } => Response::Pairing(PairingState::Waiting {
            expires_in_ms: 60_000,
        }),
        _ => Response::Done,
    })
}

#[test]
fn pairing_off_shows_only_messages_under_en_xa() {
    under_en_xa("remote pairing off en-XA leak", |cx, recorder| {
        let (view, cx) = window(cx, false, daemon);
        draw(cx);
        recorder.take();
        open(&view, cx);
        draw(cx);
        assert_translated("pairing off", &uncut(recorder.take()), &[]);
    });
}

#[test]
fn pairing_shows_only_messages_under_en_xa() {
    under_en_xa("remote pairing en-XA leak", |cx, recorder| {
        let (view, cx) = window(cx, true, daemon);
        draw(cx);
        recorder.take();
        open(&view, cx);
        wait(cx);
        draw(cx);
        // The addresses a phone reaches this computer at are data.
        let data = ["192.168.1.20:7443".to_owned(), "100.64.0.3:7443".to_owned()];
        assert_translated("pairing code", &uncut(recorder.take()), &data);
        CLAIMED_XA.store(true, Ordering::SeqCst);
        wait(cx);
        draw(cx);
        assert_translated(
            "phone asking",
            &uncut(recorder.take()),
            &["428 913".to_owned()],
        );
    });
}
