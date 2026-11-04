//! One probe: the time a TCP connection takes to open. The handshake's SYN
//! and SYN-ACK make one round trip, measured in process without spawning
//! `ping` or needing an ICMP socket, and treated like the user's own traffic
//! by networks that deprioritize ICMP.

use std::{
    io,
    net::{SocketAddr, TcpStream},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// The handshake's round trip.
    Reply(Duration),
    /// No connection: timed out, refused, or no route. The kind says which.
    Lost(io::ErrorKind),
}

impl Outcome {
    pub fn round_trip(self) -> Option<Duration> {
        match self {
            Self::Reply(round_trip) => Some(round_trip),
            Self::Lost(_) => None,
        }
    }
}

/// Opens and at once drops a connection to `target`. A SYN lost on the way
/// is resent after about a second, so a slow reply can include one
/// retransmission; `timeout` bounds the wait either way.
pub(super) fn run(target: SocketAddr, timeout: Duration) -> Outcome {
    let started = Instant::now();
    match TcpStream::connect_timeout(&target, timeout) {
        Ok(_) => Outcome::Reply(started.elapsed()),
        Err(error) => Outcome::Lost(error.kind()),
    }
}
