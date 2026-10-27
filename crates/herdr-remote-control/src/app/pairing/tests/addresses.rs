use super::*;
use crate::app::Error;

const TAILSCALE: &str = "https://100.101.102.103:7443";
const IPV6: &str = "https://[fd7a:115c:a1e0::1]:7443";

fn choices(id: &str) -> Offer {
    let mut offer = offer(id);
    offer.addresses.extend([TAILSCALE.into(), IPV6.into()]);
    offer
}

fn shown(pairing: &Pairing) -> (&Offer, &Qr) {
    let Phase::Showing { offer, qr, .. } = pairing.phase() else {
        panic!("expected an offer");
    };
    (offer, qr)
}

#[test]
fn changing_address_preserves_token_deadline_and_in_flight_poll() {
    let now = Instant::now();
    let mut pairing = Pairing::new(true);
    step(&mut pairing, now, Ok(Response::Offer(choices("p1"))));
    let countdown = pairing.countdown(now);
    let poll = pairing.next(now + POLL).unwrap();
    pairing.select_address(TAILSCALE).unwrap();
    assert_eq!(pairing.countdown(now), countdown);
    assert!(pairing.next(now + POLL).is_none());
    let (offer, qr) = shown(&pairing);
    assert_eq!(offer.url, format!("{TAILSCALE}/pair#t=p1&fp=5F:2A"));
    assert_eq!(qr, &Qr::encode(&offer.url).unwrap());
    assert_eq!(offer.id, PairingId("p1".into()));
    pairing.receive(
        &poll,
        Ok(Response::Pairing(PairingState::Waiting {
            expires_in_ms: 20_000,
        })),
        now + POLL,
    );
    assert!(shown(&pairing).0.url.starts_with(TAILSCALE));
    let rotation = pairing.next(now + Duration::from_secs(61)).unwrap();
    assert_eq!(rotation.request, Request::Pair);
    pairing.receive(
        &rotation,
        Ok(Response::Offer(choices("p2"))),
        now + Duration::from_secs(61),
    );
    assert_eq!(
        shown(&pairing).0.url,
        format!("{TAILSCALE}/pair#t=p2&fp=5F:2A")
    );
}

#[test]
fn ipv6_and_cli_selection_encode_the_selected_origin() {
    let now = Instant::now();
    let mut pairing = Pairing::new(true).with_address(Some(IPV6.into()));
    step(&mut pairing, now, Ok(Response::Offer(choices("p1"))));
    assert_eq!(shown(&pairing).0.url, format!("{IPV6}/pair#t=p1&fp=5F:2A"));
    pairing.select_address("https://192.168.1.20:7443").unwrap();
    assert_eq!(shown(&pairing).0.url, offer("p1").url);
}

#[test]
fn unknown_or_non_origin_addresses_cannot_change_the_qr() {
    let now = Instant::now();
    let mut pairing = showing(now);
    for address in [
        "https://attacker.example",
        "http://192.168.1.20:7443",
        "https://192.168.1.20:7443/path",
    ] {
        let before = pairing.phase().clone();
        assert!(matches!(
            pairing.select_address(address),
            Err(Error::PairingAddress)
        ));
        assert_eq!(pairing.phase(), &before);
    }
    let mut malformed = offer("p1");
    malformed.addresses.push("https://host/path".into());
    assert!(matches!(
        pairing_url(&malformed, "https://host/path"),
        Err(Error::PairingAddress)
    ));
    malformed.url = ":invalid".into();
    let error = pairing_url(&malformed, &malformed.addresses[0]).unwrap_err();
    assert!(matches!(error, Error::PairingUrl(_)));
    assert!(std::error::Error::source(&error).is_some());
}

#[test]
fn selection_is_refused_after_claim_and_missing_cli_address_can_be_retried() {
    let now = Instant::now();
    let mut pairing = showing(now);
    step(
        &mut pairing,
        now + POLL,
        Ok(Response::Pairing(PairingState::Claimed {
            device_name: "phone".into(),
            code: "123456".into(),
            app: None,
        })),
    );
    let before = pairing.phase().clone();
    assert!(pairing.select_address(TAILSCALE).is_err());
    assert_eq!(pairing.phase(), &before);
    let mut pairing = Pairing::new(true).with_address(Some(TAILSCALE.into()));
    step(&mut pairing, now, Ok(Response::Offer(offer("p1"))));
    assert!(matches!(pairing.phase(), Phase::Failed { .. }));
    pairing.restart();
    step(&mut pairing, now, Ok(Response::Offer(offer("p2"))));
    assert_eq!(shown(&pairing).0.url, offer("p2").url);
}
