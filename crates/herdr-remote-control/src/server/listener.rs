//! HTTPS connections for axum: a task accepts TCP connections and runs
//! each TLS handshake on its own task, with a deadline and a cap on
//! connections, so a slow or silent client never holds up the others.

use std::{
    io,
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::Duration,
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{OwnedSemaphorePermit, Semaphore, mpsc},
};
use tokio_rustls::{TlsAcceptor, server::TlsStream};

/// Connections open at once.
const MAX_CONNECTIONS: usize = 256;
/// How long a handshake may take.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
/// Finished handshakes waiting for axum.
const READY: usize = 64;

/// Binds `address:port`; every interface (`::`) also takes IPv4.
pub(super) fn bind(address: IpAddr, port: u16) -> io::Result<TcpListener> {
    let address = SocketAddr::new(address, port);
    let socket = socket2::Socket::new(
        socket2::Domain::for_address(address),
        socket2::Type::STREAM,
        Some(socket2::Protocol::TCP),
    )?;
    if address.is_ipv6() && address.ip().is_unspecified() {
        socket.set_only_v6(false)?;
    }
    socket.set_reuse_address(true)?;
    socket.set_nonblocking(true)?;
    socket.bind(&address.into())?;
    socket.listen(128)?;
    TcpListener::from_std(socket.into())
}

/// A TLS stream that keeps its connection's slot until it closes.
pub(super) struct Connection {
    stream: TlsStream<TcpStream>,
    _permit: OwnedSemaphorePermit,
}

pub(super) struct TlsListener {
    ready: mpsc::Receiver<(Connection, SocketAddr)>,
    local: io::Result<SocketAddr>,
}

impl TlsListener {
    pub(super) fn new(tcp: TcpListener, config: Arc<rustls::ServerConfig>) -> Self {
        let local = tcp.local_addr();
        let (sender, ready) = mpsc::channel(READY);
        let acceptor = TlsAcceptor::from(config);
        let slots = Arc::new(Semaphore::new(MAX_CONNECTIONS));
        tokio::spawn(async move {
            loop {
                let Ok(permit) = slots.clone().acquire_owned().await else {
                    return;
                };
                let Ok((tcp, peer)) = tcp.accept().await else {
                    continue;
                };
                let _ = tcp.set_nodelay(true);
                let (acceptor, sender) = (acceptor.clone(), sender.clone());
                tokio::spawn(async move {
                    let handshake =
                        tokio::time::timeout(HANDSHAKE_TIMEOUT, acceptor.accept(tcp)).await;
                    if let Ok(Ok(stream)) = handshake {
                        let connection = Connection {
                            stream,
                            _permit: permit,
                        };
                        let _ = sender.send((connection, peer)).await;
                    }
                });
            }
        });
        Self { ready, local }
    }
}

impl axum::extract::connect_info::Connected<axum::serve::IncomingStream<'_, TlsListener>>
    for crate::web::Peer
{
    fn connect_info(stream: axum::serve::IncomingStream<'_, TlsListener>) -> Self {
        Self(*stream.remote_addr())
    }
}

impl axum::serve::Listener for TlsListener {
    type Io = Connection;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            if let Some(ready) = self.ready.recv().await {
                return ready;
            }
            // The accept task ended; nothing more will come.
            std::future::pending::<()>().await;
        }
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        match &self.local {
            Ok(address) => Ok(*address),
            Err(error) => Err(io::Error::new(error.kind(), error.to_string())),
        }
    }
}

impl tokio::io::AsyncRead for Connection {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        std::pin::Pin::new(&mut self.stream).poll_read(cx, buf)
    }
}

impl tokio::io::AsyncWrite for Connection {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<io::Result<usize>> {
        std::pin::Pin::new(&mut self.stream).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        std::pin::Pin::new(&mut self.stream).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        std::pin::Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}
