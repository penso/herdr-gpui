use super::*;

fn daemon(_: &Path, request: &Request) -> Result<Response, control::Error> {
    Ok(match request {
        Request::Pair => Response::Offer(offer()),
        Request::Pairing { .. } => Response::Pairing(PairingState::Waiting {
            expires_in_ms: 60_000,
        }),
        _ => Response::Done,
    })
}

#[gpui::test]
fn choosing_an_address_updates_the_qr_and_the_dialog_still_closes(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, true, daemon);
    open(&view, cx);
    cx.run_until_parked();
    view.update(cx, |view, cx| view.set_other_addresses(true, cx));
    draw(cx);
    assert!(cx.debug_bounds("remote-pair-older-daemon").is_some());
    let address = cx.debug_bounds("remote-pair-address-1").unwrap().center();
    cx.simulate_click(address, Modifiers::default());
    let Phase::Showing {
        offer: selected,
        qr,
        ..
    } = phase(&view, cx)
    else {
        panic!("not showing");
    };
    assert_eq!(
        selected.url,
        "https://100.64.0.3:7443/pair#t=TOKEN&fp=5F:2A"
    );
    assert_eq!(*qr, Qr::encode(&selected.url).unwrap());
    draw(cx);
    let close = cx.debug_bounds("remote-pair-x").unwrap().center();
    cx.simulate_click(close, Modifiers::default());
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert!(view.menu.remote_pairing.is_none()));
}

#[gpui::test]
fn a_cli_address_is_used_when_the_offer_arrives(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, true, daemon);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            assert!(view.open_remote_pairing_at(
                Some("https://100.64.0.3:7443".into()),
                window,
                cx
            ));
        })
    });
    cx.run_until_parked();
    let Phase::Showing { offer, .. } = phase(&view, cx) else {
        panic!("not showing");
    };
    assert!(offer.url.starts_with("https://100.64.0.3:7443/pair#"));
}

#[gpui::test]
fn many_addresses_scroll_without_squeezing_the_rows(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, true, daemon);
    open(&view, cx);
    cx.run_until_parked();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let pairing = &mut view.menu.remote_pairing.as_mut().unwrap().pairing;
            pairing.restart();
            let now = Instant::now();
            let ticket = pairing.next(now).unwrap();
            let mut many = offer();
            many.addresses
                .extend((1..20).map(|i| format!("https://192.168.1.{i}:7443")));
            pairing.receive(&ticket, Ok(Response::Offer(many)), now);
            window.resize(size(px(640.), px(700.)));
            cx.notify();
        })
    });
    view.update(cx, |view, cx| view.set_other_addresses(true, cx));
    draw(cx);
    let list = cx.debug_bounds("remote-pair-addresses").unwrap();
    assert!(list.size.height <= px(132.));
    let row = cx.debug_bounds("remote-pair-address-1").unwrap();
    assert!(row.size.height >= px(20.), "{row:?}");
    assert!(row.size.width <= list.size.width);
}
