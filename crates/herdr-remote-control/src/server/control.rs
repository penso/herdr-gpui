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
        Request::Pair => {
            let mut origins = state.origins();
            let details = super::addresses::current(&mut origins);
            state
                .auth()
                .mint(now, &origins, &state.fingerprint)
                .map(|mut offer| {
                    offer.address_details = details;
                    Response::Offer(offer)
                })
        }
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
        Request::Indicators { set } => {
            return match state.look.set(crate::web::Indicators::from_config(&set)) {
                Ok(()) => Response::Done,
                Err(error) => Response::Error {
                    code: ErrorCode::Other,
                    message: error.to_string(),
                },
            };
        }
        Request::Shutdown => {
            stop.send_replace(true);
            return Response::Done;
        }
        Request::PullRequests { snapshot } => {
            if !snapshot.valid() {
                return Response::Error {
                    code: ErrorCode::Unsupported,
                    message: "Invalid pull request presentation data".into(),
                };
            }
            let layout = state.hub.layout();
            if snapshot.boot_id != layout.boot {
                return Response::Error {
                    code: ErrorCode::Other,
                    message: "The daemon layout changed".into(),
                };
            }
            if state.pull_requests.borrow().as_ref() != &snapshot {
                state.pull_requests.send_replace(Arc::new(snapshot));
            }
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
    let mut addresses = state.origins();
    let address_details = super::addresses::current(&mut addresses);
    Status {
        boot_id: layout.boot.clone(),
        address_details,
        version: version().to_owned(),
        port: state.port,
        addresses,
        fingerprint: state.fingerprint.clone(),
        herdr: layout.connected,
        chats: layout.chats().count(),
        devices: state.auth().devices().len(),
        connected: state.connected().len(),
    }
}
