//! The control endpoint Herdr UI talks to: one request per connection,
//! answered from the daemon's state. A blocking thread accepts and each
//! connection gets its own, bounded by a deadline on the request.

use crate::{
    control::{self, ErrorCode, Request, Response, Status},
    web::{State, now_ms},
};
use herdr_chat_protocol::{read_frame, write_frame};
use herdr_client::private_endpoint::{Connection, Listener};
use std::{
    io::{BufReader, BufWriter, Write},
    path::Path,
    sync::Arc,
    thread,
    time::Duration,
};
use tokio::sync::watch;

/// How long a client may take to send its request.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
/// How long the check for a daemon already serving waits.
const PROBE_TIMEOUT: Duration = Duration::from_millis(500);

/// The version Herdr UI compares to restart an older daemon.
pub(crate) fn version() -> &'static str {
    option_env!("HERDR_RELEASE_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))
}

pub(super) fn serve(
    state_dir: &Path,
    state: Arc<State>,
    stop: watch::Sender<bool>,
) -> std::io::Result<()> {
    let address = control::address(state_dir);
    // A daemon already serving this state answers here; taking its socket
    // would leave it running where Herdr UI can no longer reach it.
    if Connection::connect(&address, PROBE_TIMEOUT).is_ok() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AddrInUse,
            "another herdr-remote-control serves this state directory",
        ));
    }
    control::release(state_dir);
    let listener = Listener::bind(&address)?;
    // Accepts for the life of the process; not joined.
    thread::Builder::new()
        .name("herdr-remote-control".into())
        .spawn(move || {
            loop {
                let Ok(connection) = listener.accept() else {
                    continue;
                };
                let (state, stop) = (state.clone(), stop.clone());
                let _ = thread::Builder::new()
                    .name("herdr-remote-control-client".into())
                    .spawn(move || {
                        let Ok((mut reader, writer, closer)) = connection.split() else {
                            return;
                        };
                        let _ = reader.set_timeout(Some(REQUEST_TIMEOUT));
                        if let Ok(Some(request)) =
                            read_frame::<_, Request>(&mut BufReader::new(reader))
                        {
                            let response = answer(&state, &stop, request);
                            let mut writer = BufWriter::new(writer);
                            let _ = write_frame(&mut writer, &response);
                            let _ = writer.flush();
                        }
                        closer.close();
                    });
            }
        })?;
    Ok(())
}

pub(crate) fn answer(state: &State, stop: &watch::Sender<bool>, request: Request) -> Response {
    let now = now_ms();
    let result = match request {
        Request::Status => return Response::Status(status(state)),
        Request::Pair => state
            .auth()
            .mint(now, &state.origins(), &state.fingerprint)
            .map(Response::Offer),
        Request::Pairing { id } => state.auth().pairing(&id, now).map(Response::Pairing),
        Request::Approve { id, allow, code } => state
            .auth()
            .answer(&id, allow, &code, now)
            .map(Response::Pairing),
        Request::CancelPairing { id } => state.auth().cancel(&id).map(|()| Response::Done),
        Request::Devices => {
            let connected = state.connected();
            return Response::Devices {
                devices: state
                    .auth()
                    .devices()
                    .list(|device| connected.contains_key(device)),
            };
        }
        Request::Revoke { device } => match state.auth().revoke(&device) {
            Ok(true) => Ok(Response::Done),
            Ok(false) => {
                return Response::Error {
                    code: ErrorCode::UnknownDevice,
                    message: "no such device".into(),
                };
            }
            Err(error) => Err(error),
        },
        Request::Shutdown => {
            stop.send_replace(true);
            return Response::Done;
        }
    };
    result.unwrap_or_else(|error| Response::Error {
        code: ErrorCode::from(&error),
        message: error.to_string(),
    })
}

fn status(state: &State) -> Status {
    let layout = state.hub.layout();
    Status {
        version: version().to_owned(),
        port: state.port,
        addresses: state.origins(),
        fingerprint: state.fingerprint.clone(),
        herdr: layout.connected,
        chats: layout.chats().count(),
        devices: state.auth().devices().len(),
        connected: state.connected().len(),
    }
}
