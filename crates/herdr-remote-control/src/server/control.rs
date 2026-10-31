//! The control endpoint Herdr UI talks to: one request per connection,
//! answered from the daemon's state. A blocking thread accepts and each
//! connection gets its own, bounded by a deadline on the request.

use crate::{
    control::{self, DeviceId, ErrorCode, Request, Response, Status},
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
        Request::Approve {
            id,
            allow,
            code,
            hosts,
        } => state
            .auth()
            .answer_with_hosts(&id, allow, &code, &hosts, now)
            .map(Response::Pairing),
        Request::CancelPairing { id } => state.auth().cancel(&id).map(|()| Response::Done),
        // Herdr UI relays these from a phone over Bluetooth; the code the
        // phone shows is what authenticates it, as over the network.
        Request::NearbyStart { public_key } => state
            .auth()
            .nearby_start(None, &public_key, &state.computer, &state.fingerprint, now)
            .map(Response::NearbyStarted),
        Request::NearbyReveal { session, nonce } => {
            let origins = state.origins();
            state
                .auth()
                .nearby_reveal(&session, &nonce, &state.fingerprint, &origins, now)
                .map(Response::NearbyRevealed)
        }
        Request::PresenceChallenge { hint } => presence_challenge(state, &hint, now),
        Request::PresenceProve { nonce, signature } => {
            // A daemon on the user's own certificate holds no CA key, so it
            // never gave out a nonce to answer.
            let fingerprint = crate::auth::nearby::fingerprint_bytes(&state.fingerprint)
                .filter(|_| state.authority.is_some());
            let proven = fingerprint
                .ok_or(crate::auth::Error::UnknownPresence)
                .and_then(|fingerprint| {
                    state
                        .auth()
                        .presence_prove(&nonce, &signature, &fingerprint, now)
                });
            proven.map(|device| Response::PresenceProven {
                addresses: sealed_addresses(state, &device),
                device,
            })
        }
        Request::PresenceSet { device, near } => {
            if state.auth().devices().get(&device).is_none() {
                return Response::Error {
                    code: ErrorCode::UnknownDevice,
                    message: "no such device".into(),
                };
            }
            state.near().set(&device, near, now);
            Ok(Response::Done)
        }
        Request::Devices => {
            let connected = state.connected().clone();
            return Response::Devices {
                devices: state
                    .auth()
                    .device_list(|device| connected.contains_key(device), now),
            };
        }
        Request::Hosts { hosts } => state.auth().set_hosts(hosts).map(|()| Response::Done),
        Request::HostPairings => Ok(Response::HostPairings {
            pairings: state.auth().take_host_pairings(now),
        }),
        Request::HostPaired {
            device,
            host,
            public_key,
            outcome,
        } => state
            .auth()
            .host_paired(&device, &host, public_key.as_deref(), outcome, now)
            .map(|()| Response::Done),
        Request::HostRevokes => Ok(Response::HostRevokes {
            revokes: state.auth().host_revokes(),
        }),
        Request::HostRevoked(revoke) => state.auth().host_revoked(&revoke).map(|()| Response::Done),
        Request::Enroll(enroll) => state.auth().enroll(&enroll, now).map(|device| {
            Response::Enrolled(control::hosts::Enrolled {
                device,
                computer: state.computer.clone(),
                fingerprint: state.fingerprint.clone(),
                origins: state.origins(),
            })
        }),
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

/// A nonce for the phone whose hint is `hint`, signed with this computer's
/// CA key, so the phone knows which computer asks before it signs.
fn presence_challenge(state: &State, hint: &str, now: u64) -> Result<Response, crate::auth::Error> {
    let Some(authority) = &state.authority else {
        return Err(crate::auth::Error::UnknownPresence);
    };
    let challenge = state.auth().presence_challenge(hint, now)?;
    let message = crate::auth::presence::challenge_message(&challenge.nonce, &challenge.device);
    let signature = authority
        .sign(&message)
        .ok_or(crate::auth::Error::UnknownPresence)?;
    Ok(Response::PresenceChallenge {
        certificate: crate::auth::encode(authority.ca_der()),
        nonce: crate::auth::encode(&challenge.nonce),
        signature: crate::auth::encode(&signature),
    })
}

/// What a proven phone gets: the origins it can reach this computer at,
/// sealed to the key it registered for alerts, so only it reads them.
fn sealed_addresses(state: &State, device: &DeviceId) -> Option<String> {
    #[derive(serde::Serialize)]
    struct Addresses {
        origins: Vec<String>,
    }
    let registration = crate::apns::Registration::from_stored(
        state
            .auth()
            .devices()
            .get(device)
            .and_then(|record| record.apns.as_ref()),
    );
    let key = registration.notify_key.as_ref()?.key().to_owned();
    let plaintext = serde_json::to_vec(&Addresses {
        origins: state.origins(),
    })
    .ok()?;
    let sealed =
        crate::apns::seal::seal(&key, &plaintext, crate::apns::seal::ADDRESSES_INFO).ok()?;
    Some(crate::auth::encode(&sealed))
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
        computer: state.computer.clone(),
        herdr: layout.connected,
        chats: layout.chats().count(),
        devices: state.auth().devices().len(),
        connected: state.connected().len(),
    }
}
