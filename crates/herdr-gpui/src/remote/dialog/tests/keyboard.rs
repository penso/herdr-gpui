use super::*;

fn daemon(_: &Path, request: &Request) -> Result<Response, control::Error> {
    Ok(match request {
        Request::Pair => {
            let mut choices = offer();
            choices.addresses.extend(
                (1..20).map(|i| format!("https://[fd7a:115c:a1e0:abcd:1234:5678:90ab:{i}]:7443")),
            );
            Response::Offer(choices)
        }
        Request::Pairing { .. } => Response::Pairing(PairingState::Waiting {
            expires_in_ms: 60_000,
        }),
        _ => Response::Done,
    })
}

fn selected(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, index: usize) {
    let Phase::Showing { offer, qr, .. } = phase(view, cx) else {
        panic!("not showing");
    };
    assert_eq!(
        offer.url,
        format!("{}/pair#t=TOKEN&fp=5F:2A", offer.addresses[index])
    );
    assert_eq!(*qr, Qr::encode(&offer.url).unwrap());
    draw(cx);
    if index == 0 {
        assert!(cx.debug_bounds("remote-pair-address-0").is_some());
        return;
    }
    let list = cx.debug_bounds("remote-pair-addresses").unwrap();
    let row = view.read_with(cx, |view, _| {
        let scroll = &view.menu.remote_pairing.as_ref().unwrap().addresses_scroll;
        let row = scroll.bounds_for_item(index - 1).unwrap();
        Bounds::new(row.origin + scroll.offset(), row.size)
    });
    assert!(row.top() >= list.top(), "{row:?} above {list:?}");
    assert!(row.bottom() <= list.bottom(), "{row:?} below {list:?}");
}

#[gpui::test]
fn keyboard_selects_and_reveals_addresses_without_leaving_the_dialog(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, true, daemon);
    open(&view, cx);
    cx.run_until_parked();
    cx.update(|window, _| window.resize(size(px(640.), px(700.))));
    draw(cx);
    let Phase::Showing { offer, .. } = phase(&view, cx) else {
        panic!("not showing");
    };
    let last = offer.addresses.len() - 1;
    for (key, index) in [
        ("down", 1),
        ("up", 0),
        ("up", last),
        ("down", 0),
        ("end", last),
        ("home", 0),
    ] {
        cx.simulate_keystrokes(key);
        selected(&view, cx, index);
        cx.update(|window, cx| {
            let view = view.read(cx);
            assert_eq!(view.menu.page, Some(Page::RemoteControl));
            assert!(view.menu.focus.is_focused(window));
        });
    }
    // Mouse and keyboard use the same selection, rather than separate cursors.
    let row = cx.debug_bounds("remote-pair-address-1").unwrap().center();
    cx.simulate_click(row, Modifiers::default());
    cx.simulate_keystrokes("down");
    selected(&view, cx, 2);
    cx.simulate_keystrokes("ctrl-down");
    selected(&view, cx, 2);
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.menu.page.is_none());
        assert!(view.menu.remote_pairing.is_none());
    });
}

#[gpui::test]
fn address_keys_do_not_act_on_a_claimed_pairing(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, true, daemon);
    open(&view, cx);
    cx.run_until_parked();
    view.update(cx, |view, cx| {
        let pairing = &mut view.menu.remote_pairing.as_mut().unwrap().pairing;
        let now = Instant::now() + Duration::from_secs(2);
        let ticket = pairing.next(now).unwrap();
        pairing.receive(
            &ticket,
            Ok(Response::Pairing(PairingState::Claimed {
                device_name: "Phone".into(),
                code: "123456".into(),
                app: None,
            })),
            now,
        );
        cx.notify();
    });
    let before = phase(&view, cx);
    assert!(matches!(before, Phase::Claimed { answer: None, .. }));
    draw(cx);
    cx.simulate_keystrokes("up down home end enter");
    assert_eq!(phase(&view, cx), before);
}
