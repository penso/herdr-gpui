//! The WebSocket under a provider's exec stream, for the relays that turn one
//! into the standard streams Herdr's bridge, installer, and Teleport's
//! scripts use (see `sprites::relay` and `opensandbox::relay`): TLS with the
//! bundled web roots, or plain TCP to a loopback or `ws` endpoint, bounded
//! messages, and a reader that feeds this process's stdin to the relay.

use std::{
    io::{self, Read, Write},
    net::{TcpStream, ToSocketAddrs},
    sync::{
        Arc,
        mpsc::{self, Receiver},
    },
    time::Duration,
};
use tungstenite::{
    WebSocket, client::IntoClientRequest, http::HeaderValue, protocol::WebSocketConfig,
};
use url::Url;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);
const CHUNK: usize = 32 * 1024;
const MESSAGE_LIMIT: usize = 4 * 1024 * 1024;
const QUEUED_CHUNKS: usize = 64;
/// How long a read waits before the relay forwards pending input.
pub const POLL: Duration = Duration::from_millis(20);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("The exec stream's address is not valid.")]
    Url,
    #[error("Could not reach the exec stream.")]
    Connect(#[source] io::Error),
    #[error("TLS setup for the exec stream failed.")]
    Tls(#[source] rustls::Error),
    #[error("The exec stream failed.")]
    Stream(#[source] tungstenite::Error),
    #[error("Invalid exec stream header.")]
    Header(#[source] tungstenite::http::header::InvalidHeaderValue),
    #[error("Invalid exec stream header name.")]
    HeaderName(#[source] tungstenite::http::header::InvalidHeaderName),
    #[error("The exec stream closed before the command finished.")]
    Closed,
}

/// The socket under the exec stream: TLS for `wss`, plain TCP for `ws`.
pub enum Stream {
    Plain(TcpStream),
    Tls(Box<rustls::StreamOwned<rustls::ClientConnection, TcpStream>>),
}

impl Stream {
    pub fn tcp(&self) -> &TcpStream {
        match self {
            Self::Plain(tcp) => tcp,
            Self::Tls(tls) => &tls.sock,
        }
    }
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Plain(tcp) => tcp.read(buf),
            Self::Tls(tls) => tls.read(buf),
        }
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::Plain(tcp) => tcp.write(buf),
            Self::Tls(tls) => tls.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Plain(tcp) => tcp.flush(),
            Self::Tls(tls) => tls.flush(),
        }
    }
}

/// A header to send with the handshake. A sensitive one, such as a bearer
/// token, is kept out of debug output.
pub struct Header<'a> {
    pub name: &'a str,
    pub value: &'a [u8],
    pub sensitive: bool,
}

/// Open the WebSocket at `url` with `headers`, its reads timing out after
/// [`POLL`] so input keeps flowing while the command is quiet.
pub fn open(url: &Url, headers: &[Header]) -> Result<WebSocket<Stream>, Error> {
    // Headers may carry credentials; unencrypted only on this computer.
    let encrypted = match url.scheme() {
        "wss" => true,
        "ws" if loopback(url) => false,
        _ => return Err(Error::Url),
    };
    let host = url.host_str().ok_or(Error::Url)?.to_owned();
    let port = url.port_or_known_default().ok_or(Error::Url)?;
    let mut request = url.as_str().into_client_request().map_err(Error::Stream)?;
    for header in headers {
        let mut value = HeaderValue::from_bytes(header.value).map_err(Error::Header)?;
        value.set_sensitive(header.sensitive);
        let name = tungstenite::http::HeaderName::from_bytes(header.name.as_bytes())
            .map_err(Error::HeaderName)?;
        request.headers_mut().insert(name, value);
    }
    let address = (host.as_str(), port)
        .to_socket_addrs()
        .map_err(Error::Connect)?
        .next()
        .ok_or(Error::Url)?;
    let tcp = TcpStream::connect_timeout(&address, CONNECT_TIMEOUT).map_err(Error::Connect)?;
    tcp.set_read_timeout(Some(HANDSHAKE_TIMEOUT))
        .map_err(Error::Connect)?;
    tcp.set_write_timeout(Some(HANDSHAKE_TIMEOUT))
        .map_err(Error::Connect)?;
    tcp.set_nodelay(true).map_err(Error::Connect)?;
    let stream = if encrypted {
        let roots = rustls::RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        };
        let tls = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .map_err(Error::Tls)?
        .with_root_certificates(roots)
        .with_no_client_auth();
        let name = rustls::pki_types::ServerName::try_from(host).map_err(|_| Error::Url)?;
        let connection = rustls::ClientConnection::new(Arc::new(tls), name).map_err(Error::Tls)?;
        Stream::Tls(Box::new(rustls::StreamOwned::new(connection, tcp)))
    } else {
        Stream::Plain(tcp)
    };
    let config = WebSocketConfig::default()
        .max_message_size(Some(MESSAGE_LIMIT))
        .max_frame_size(Some(MESSAGE_LIMIT));
    let (socket, _) = tungstenite::client::client_with_config(request, stream, Some(config))
        .map_err(|error| match error {
            tungstenite::HandshakeError::Failure(error) => Error::Stream(error),
            tungstenite::HandshakeError::Interrupted(_) => Error::Closed,
        })?;
    socket
        .get_ref()
        .tcp()
        .set_read_timeout(Some(POLL))
        .map_err(Error::Connect)?;
    Ok(socket)
}

/// Read `source` on its own thread, in chunks, then `None` at its end. The
/// channel is bounded, so a slow remote end holds the reader back.
pub fn input(mut source: impl Read + Send + 'static) -> Receiver<Option<Vec<u8>>> {
    let (sender, receiver) = mpsc::sync_channel(QUEUED_CHUNKS);
    let spawned = std::thread::Builder::new()
        .name("herdr-relay-stdin".into())
        .spawn(move || {
            let mut chunk = vec![0; CHUNK];
            loop {
                match source.read(&mut chunk) {
                    Ok(0) | Err(_) => {
                        let _ = sender.send(None);
                        return;
                    }
                    Ok(read) => {
                        if sender.send(Some(chunk[..read].to_vec())).is_err() {
                            return;
                        }
                    }
                }
            }
        });
    if let Err(error) = spawned {
        tracing::warn!(%error, "could not start a relay's input reader");
    }
    receiver
}

fn loopback(url: &Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    }
}

/// A read or write that timed out with nothing to do, not a failure.
pub fn pending(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
    )
}
