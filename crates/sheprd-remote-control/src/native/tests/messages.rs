//! The phone's messages parse strictly: a view it does not name, a missing
//! pane, or another type is refused, never guessed.

use super::*;
use serde_json::json;
use wire::View;

fn parse(value: serde_json::Value) -> Option<ClientMessage> {
    serde_json::from_value(value).ok()
}

#[test]
fn every_view_subscribes_and_unsubscribes() {
    assert_eq!(
        parse(json!({"type": "subscribe", "view": "home"})),
        Some(ClientMessage::Subscribe {
            view: View::Home,
            since: None,
            size: None,
        })
    );
    assert_eq!(
        parse(json!({"type": "subscribe", "view": "term", "pane": "p2"})),
        Some(ClientMessage::Subscribe {
            view: View::Term { pane: "p2".into() },
            since: None,
            size: None,
        })
    );
    assert_eq!(
        parse(json!({"type": "unsubscribe", "view": "chat", "pane": "p1"})),
        Some(ClientMessage::Unsubscribe {
            view: View::Chat { pane: "p1".into() }
        })
    );
}

#[test]
fn a_chat_subscription_carries_the_cursor_the_phone_holds() {
    let epoch = uuid::Uuid::from_u128(1);
    let message = parse(json!({
        "type": "subscribe", "view": "chat", "pane": "p1",
        "since": {"epoch": epoch, "seq": 4},
    }));
    let Some(ClientMessage::Subscribe { view, since, .. }) = message else {
        panic!("{message:?}");
    };
    assert_eq!(view, View::Chat { pane: "p1".into() });
    assert_eq!(
        since.map(|cursor| (cursor.epoch, cursor.seq)),
        Some((epoch, 4))
    );
}

#[test]
fn malformed_messages_are_refused() {
    for bad in [
        json!({}),
        json!({"type": "subscribe"}),
        json!({"type": "subscribe", "view": "chat"}),
        json!({"type": "subscribe", "view": "files"}),
        json!({"type": "subscribe", "view": "term", "pane": 3}),
        json!({"type": "prompt", "text": "hi"}),
        json!({"type": "subscribe", "view": "chat", "pane": "p1", "since": "now"}),
        json!(["subscribe", "home"]),
    ] {
        assert_eq!(parse(bad.clone()), None, "{bad}");
    }
}

#[test]
fn reload_names_its_view_beside_its_type() {
    let reload = ServerMessage::Reload {
        view: View::Term { pane: "p2".into() },
    };
    assert_eq!(
        serde_json::to_value(&reload).unwrap(),
        json!({"type": "reload", "view": "term", "pane": "p2"})
    );
    let home = ServerMessage::Reload { view: View::Home };
    assert_eq!(
        serde_json::to_value(&home).unwrap(),
        json!({"type": "reload", "view": "home"})
    );
}
