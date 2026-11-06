//! The live socket over a real connection: only a signed-in device
//! upgrades, it reads text frames of bounded size, and a binary or
//! oversized frame closes it.

use super::*;
use futures_lite::FutureExt;
use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{
    WebSocketStream, client_async,
    tungstenite::{self, Message as Frame, client::IntoClientRequest},
};

type Socket = WebSocketStream<TcpStream>;

/// Serves the router on a loopback port; its address.
async fn serve(state: Shared) -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, crate::web::router(state))
            .await
            .unwrap();
    });
    address
}

async fn connect(
    address: std::net::SocketAddr,
    token: Option<&str>,
) -> Result<Socket, tungstenite::Error> {
    let mut request = format!("ws://{address}/api/v2/live")
        .into_client_request()
        .unwrap();
    if let Some(token) = token {
        request
            .headers_mut()
            .insert("authorization", format!("Bearer {token}").parse().unwrap());
    }
    let stream = TcpStream::connect(address).await.unwrap();
    client_async(request, stream)
        .await
        .map(|(socket, _)| socket)
}

/// The next text frame as a server message, skipping pings.
async fn next(socket: &mut Socket) -> Option<ServerMessage> {
    let read = async {
        loop {
            match socket.next().await? {
                Ok(Frame::Text(text)) => return Some(serde_json::from_str(&text).unwrap()),
                Ok(Frame::Ping(_) | Frame::Pong(_)) => {}
                Ok(_) | Err(_) => return None,
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(5), read)
        .await
        .expect("no frame in time")
}

/// Whether the server ends the connection within five seconds.
async fn closes(socket: &mut Socket) -> bool {
    let closed = async {
        loop {
            match socket.next().await {
                None | Some(Err(_) | Ok(Frame::Close(_))) => return true,
                Some(Ok(_)) => {}
            }
        }
    };
    closed
        .or(async {
            tokio::time::sleep(Duration::from_secs(5)).await;
            false
        })
        .await
}

fn status(error: tungstenite::Error) -> u16 {
    match error {
        tungstenite::Error::Http(response) => response.status().as_u16(),
        other => panic!("{other}"),
    }
}

async fn send(socket: &mut Socket, text: &str) {
    socket.send(Frame::Text(text.into())).await.unwrap();
}

#[tokio::test]
async fn only_a_signed_in_device_upgrades() {
    let dir = tempfile::tempdir().unwrap();
    let state = offline(dir.path(), layout(EndpointId::random()));
    let address = serve(state).await;
    assert_eq!(status(connect(address, None).await.unwrap_err()), 401);
    assert_eq!(
        status(connect(address, Some("forged")).await.unwrap_err()),
        401
    );
}

#[tokio::test]
async fn a_subscription_answers_and_a_bad_message_is_answered_with_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let state = offline(dir.path(), layout(EndpointId::random()));
    let (_, token) = crate::web::tests::signed_in(&state, 9);
    let address = serve(state).await;
    let mut socket = connect(address, Some(&token)).await.unwrap();
    send(&mut socket, r#"{"type":"subscribe","view":"home"}"#).await;
    assert!(matches!(
        next(&mut socket).await,
        Some(ServerMessage::Home { .. })
    ));
    send(&mut socket, "not json").await;
    assert!(matches!(
        next(&mut socket).await,
        Some(ServerMessage::Error { .. })
    ));
}

#[tokio::test]
async fn a_binary_frame_closes_the_socket() {
    let dir = tempfile::tempdir().unwrap();
    let state = offline(dir.path(), layout(EndpointId::random()));
    let (_, token) = crate::web::tests::signed_in(&state, 9);
    let address = serve(state).await;
    let mut socket = connect(address, Some(&token)).await.unwrap();
    socket
        .send(Frame::Binary(vec![1, 2, 3].into()))
        .await
        .unwrap();
    assert!(closes(&mut socket).await);
}

#[tokio::test]
async fn an_oversized_message_closes_the_socket() {
    let dir = tempfile::tempdir().unwrap();
    let state = offline(dir.path(), layout(EndpointId::random()));
    let (_, token) = crate::web::tests::signed_in(&state, 9);
    let address = serve(state).await;
    let mut socket = connect(address, Some(&token)).await.unwrap();
    let padding = "x".repeat(MAX_INBOUND + 1);
    send(
        &mut socket,
        &format!(r#"{{"type":"subscribe","view":"home","pad":"{padding}"}}"#),
    )
    .await;
    assert!(closes(&mut socket).await);
}

#[tokio::test]
async fn a_device_with_too_many_streams_open_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let state = offline(dir.path(), layout(EndpointId::random()));
    let (device, token) = crate::web::tests::signed_in(&state, 9);
    state.connected().insert(device, MAX_STREAMS);
    let address = serve(state).await;
    assert_eq!(
        status(connect(address, Some(&token)).await.unwrap_err()),
        429
    );
}
