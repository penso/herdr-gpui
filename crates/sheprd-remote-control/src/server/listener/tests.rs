#![allow(clippy::unwrap_used)]
use super::*;
use crate::tls::{Authority, Names, Resolver};
use rustls::pki_types::ServerName;
use std::{
    net::{Ipv4Addr, Ipv6Addr},
    time::SystemTime,
};

#[tokio::test]
async fn ip_connections_without_sni_select_the_destination_certificate() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let state = tempfile::tempdir().unwrap();
        let authority = Authority::load_or_create(state.path()).unwrap();
        let v4 = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let v6 = IpAddr::V6(Ipv6Addr::LOCALHOST);
        let resolver = Arc::new(
            Resolver::issue(
                &authority,
                Names::new([v4, v6], Some("studio")),
                SystemTime::now(),
            )
            .unwrap(),
        );
        let tcp = bind(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0).unwrap();
        let port = tcp.local_addr().unwrap().port();
        let config = Arc::new(crate::tls::server_config(resolver.clone()).unwrap());
        let mut listener = TlsListener::new(tcp, config, resolver.clone());
        let mut roots = rustls::RootCertStore::empty();
        roots.add(authority.ca_der().to_vec().into()).unwrap();
        let config = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
        let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
        for ip in [v4, v6] {
            let tcp = TcpStream::connect(SocketAddr::new(ip, port)).await.unwrap();
            // ServerName::IpAddress does not send SNI, including IPv4 accepted
            // on the dual-stack listener as an IPv4-mapped IPv6 local address.
            let stream = connector
                .connect(ServerName::IpAddress(ip.into()), tcp)
                .await
                .unwrap();
            assert_eq!(
                stream.get_ref().1.peer_certificates().unwrap()[0],
                resolver.current_for(ip).unwrap().cert[0]
            );
            drop(axum::serve::Listener::accept(&mut listener).await);
        }
        let tcp = TcpStream::connect(SocketAddr::new(v4, port)).await.unwrap();
        let stream = connector
            .connect(ServerName::try_from("studio.local").unwrap(), tcp)
            .await
            .unwrap();
        assert_eq!(
            stream.get_ref().1.peer_certificates().unwrap()[0],
            resolver.current().cert[0]
        );
        drop(axum::serve::Listener::accept(&mut listener).await);
    })
    .await
    .unwrap();
}
