//! A live session follows the views the phone names, within bounds, and
//! ends with `bye` when its device loses access.

use super::*;
use wire::{ByeReason, View};

/// A layout of `count` terminal tabs, `p0` to `pN`, besides chat `p1`'s
/// workspace.
fn terminals(count: usize) -> Layout {
    let mut layout = layout(EndpointId::random());
    let template = layout.workspaces[0].terminals[0].clone();
    layout.workspaces[0].terminals = (0..count)
        .map(|index| {
            let mut terminal = template.clone();
            terminal.tab = format!("t-{index}");
            terminal.panes[0].pane = format!("term-{index}");
            terminal
        })
        .collect();
    layout
}

fn subscribe(view: View) -> ClientMessage {
    ClientMessage::Subscribe {
        view,
        since: None,
        size: None,
    }
}

#[tokio::test]
async fn home_is_sent_as_soon_as_it_is_subscribed() {
    let dir = tempfile::tempdir().unwrap();
    let mut session = Session::start(offline(dir.path(), layout(EndpointId::random())));
    session.send(subscribe(View::Home)).await;
    let ServerMessage::Home { home } = session.next().await else {
        panic!("not home");
    };
    assert_eq!(home.computer, "Studio");
    assert_eq!(home.workspaces[0].chats[0].pane, "p1");
}

#[tokio::test]
async fn a_view_that_does_not_exist_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let mut session = Session::start(offline(dir.path(), layout(EndpointId::random())));
    // `p1` is listed but not linked: the offline hub reaches no chat.
    for (view, expected) in [
        (View::Chat { pane: "p1".into() }, "no such chat"),
        (
            View::Chat {
                pane: "nope".into(),
            },
            "no such chat",
        ),
        (View::Term { pane: "p1".into() }, "no such terminal"),
    ] {
        session.send(subscribe(view)).await;
        assert_eq!(
            session.next().await,
            ServerMessage::Error {
                message: expected.into()
            }
        );
    }
}

#[tokio::test]
async fn a_socket_follows_at_most_eight_views_but_may_replace_one() {
    let dir = tempfile::tempdir().unwrap();
    let mut session = Session::start(offline(dir.path(), terminals(8)));
    session.send(subscribe(View::Home)).await;
    for index in 0..8 {
        session
            .send(subscribe(View::Term {
                pane: format!("term-{index}"),
            }))
            .await;
    }
    // Home and the refusal come from different tasks, in either order;
    // the terminals cannot be read, so they may reload meanwhile.
    let (mut refused, mut homes) = (0, 0);
    while refused == 0 || homes == 0 {
        match session.next().await {
            ServerMessage::Error { message } => {
                assert_eq!(message, "too many views on one connection");
                refused += 1;
            }
            ServerMessage::Home { .. } => homes += 1,
            ServerMessage::Reload { .. } => {}
            other => panic!("{other:?}"),
        }
    }
    assert_eq!((refused, homes), (1, 1));
    // Home again replaces the first, so it is not counted twice.
    session.send(subscribe(View::Home)).await;
    loop {
        match session.next().await {
            ServerMessage::Home { .. } => break,
            ServerMessage::Reload { .. } => {}
            other => panic!("{other:?}"),
        }
    }
}

#[tokio::test]
async fn revoking_the_device_ends_the_session_with_bye() {
    let dir = tempfile::tempdir().unwrap();
    let mut session = Session::start(offline(dir.path(), layout(EndpointId::random())));
    session.send(subscribe(View::Home)).await;
    assert!(matches!(session.next().await, ServerMessage::Home { .. }));
    assert!(session.state.auth().revoke(&session.device).unwrap());
    assert_eq!(
        session.next().await,
        ServerMessage::Bye {
            reason: ByeReason::Revoked
        }
    );
    // Nothing follows a bye.
    let after = tokio::time::timeout(Duration::from_secs(1), session.from_live.recv()).await;
    assert!(matches!(after, Ok(None)), "{after:?}");
}

#[tokio::test]
async fn a_terminal_that_cannot_be_read_reloads() {
    let dir = tempfile::tempdir().unwrap();
    let mut session = Session::start(offline(dir.path(), layout(EndpointId::random())));
    session
        .send(subscribe(View::Term { pane: "p2".into() }))
        .await;
    assert_eq!(
        session.next().await,
        ServerMessage::Reload {
            view: View::Term { pane: "p2".into() }
        }
    );
}
