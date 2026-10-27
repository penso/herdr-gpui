use super::*;
use herdr_remote_control::control::{AddressDetails, AddressKind};

fn daemon(_: &Path, request: &Request) -> Result<Response, control::Error> {
    Ok(match request {
        Request::Pair => {
            let mut offer = offer();
            offer.url = format!("{}/pair#t=TOKEN&fp=5F:2A", offer.addresses[1]);
            offer.address_details.insert(
                offer.addresses[1].clone(),
                AddressDetails {
                    interfaces: vec!["utun12".into()],
                    kind: AddressKind::Tailscale,
                },
            );
            offer.address_details.insert(
                offer.addresses[0].clone(),
                AddressDetails {
                    interfaces: vec!["en0".into()],
                    kind: AddressKind::Local,
                },
            );
            Response::Offer(offer)
        }
        Request::Pairing { .. } => Response::Pairing(PairingState::Waiting {
            expires_in_ms: 60_000,
        }),
        _ => Response::Done,
    })
}

#[gpui::test]
fn recommended_route_stays_visible_and_other_routes_expand_without_changing_the_qr(
    cx: &mut TestAppContext,
) {
    let (view, cx) = window(cx, true, daemon);
    open(&view, cx);
    cx.run_until_parked();
    draw(cx);
    assert!(cx.debug_bounds("remote-pair-address-1").is_some());
    assert!(cx.debug_bounds("remote-pair-older-daemon").is_none());
    assert!(cx.debug_bounds("remote-pair-addresses").is_none());
    let before = phase(&view, cx);
    let expand = cx
        .debug_bounds("remote-pair-other-addresses")
        .unwrap()
        .center();
    cx.simulate_click(expand, Modifiers::default());
    draw(cx);
    assert_eq!(phase(&view, cx), before);
    let other = cx.debug_bounds("remote-pair-address-0").unwrap().center();
    cx.simulate_click(other, Modifiers::default());
    let selected = phase(&view, cx);
    let Phase::Showing { offer, qr, .. } = &selected else {
        panic!("not showing");
    };
    assert!(offer.url.starts_with("https://192.168.1.20:7443/"));
    assert_eq!(**qr, Qr::encode(&offer.url).unwrap());
    cx.simulate_keystrokes("left");
    draw(cx);
    assert!(cx.debug_bounds("remote-pair-addresses").is_none());
    assert_eq!(phase(&view, cx), selected);
    wait(cx);
    draw(cx);
    assert!(
        cx.debug_bounds("remote-pair-addresses").is_none(),
        "polling must not reopen the list"
    );
    cx.simulate_keystrokes("home");
    draw(cx);
    assert!(cx.debug_bounds("remote-pair-address-0").is_some());
}
