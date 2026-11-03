//! Which certificate a handshake gets, by the local address it reached and
//! whether the client sent a name.

use super::*;

#[test]
fn a_nameless_handshake_on_an_unlisted_address_gets_the_dns_leaf() {
    let state = tempfile::tempdir().unwrap();
    let authority = Authority::load_or_create(state.path()).unwrap();
    let now = SystemTime::now();
    let names = Names::new([ip("192.168.1.20"), ip("fe80::1")], Some("studio"));
    let resolver = Resolver::issue(&authority, names, now).unwrap();
    let dns = resolver.current();

    // `studio.local` resolves to link-local IPv6 on the phone, which sends
    // no SNI for it; the DNS leaf is the one that names `studio.local`.
    let link_local = resolver.for_handshake(ip("fe80::1"), false);
    assert!(Arc::ptr_eq(&link_local, &dns));
    verify(&authority, &link_local, "studio.local", now).unwrap();

    let lan = resolver.for_handshake(ip("192.168.1.20"), false);
    assert!(!Arc::ptr_eq(&lan, &dns));
    verify(&authority, &lan, "192.168.1.20", now).unwrap();

    assert!(Arc::ptr_eq(
        &resolver.for_handshake(ip("192.168.1.20"), true),
        &dns
    ));
}

/// Runs a full in-memory handshake between `client` and a server that
/// resolves certificates as the listener does for local `address`, and
/// returns the leaf the client received.
fn leaf_presented_at(
    resolver: &Arc<Resolver>,
    address: IpAddr,
    client: rustls::ClientConfig,
    name: &str,
) -> std::result::Result<rustls::pki_types::CertificateDer<'static>, rustls::Error> {
    let mut server = server_config(Arc::clone(resolver)).unwrap();
    server.cert_resolver = resolver.at(address);
    let mut server = rustls::ServerConnection::new(Arc::new(server))?;
    let name = ServerName::try_from(name.to_owned()).unwrap();
    let mut client = rustls::ClientConnection::new(Arc::new(client), name)?;
    while client.is_handshaking() || server.is_handshaking() {
        let mut bytes = Vec::new();
        client.write_tls(&mut bytes).unwrap();
        server.read_tls(&mut bytes.as_slice()).unwrap();
        server.process_new_packets()?;
        bytes.clear();
        server.write_tls(&mut bytes).unwrap();
        client.read_tls(&mut bytes.as_slice()).unwrap();
        client.process_new_packets()?;
    }
    Ok(client.peer_certificates().unwrap()[0].clone().into_owned())
}

#[test]
fn a_phone_reaching_dot_local_at_link_local_completes_the_handshake() {
    let state = tempfile::tempdir().unwrap();
    let authority = Authority::load_or_create(state.path()).unwrap();
    let names = Names::new([ip("192.168.1.20"), ip("fe80::1")], Some("studio"));
    let resolver = Arc::new(Resolver::issue(&authority, names, SystemTime::now()).unwrap());
    let mut roots = RootCertStore::empty();
    roots.add(authority.ca_der().to_vec().into()).unwrap();
    let mut client =
        rustls::ClientConfig::builder_with_provider(Arc::new(ring::default_provider()))
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(roots)
            .with_no_client_auth();
    // iOS checks `studio.local` but sends no SNI for it.
    client.enable_sni = false;

    let leaf = leaf_presented_at(&resolver, ip("fe80::1"), client, "studio.local").unwrap();
    assert_eq!(leaf, resolver.current().cert[0]);
}

/// A phone that reaches `studio.local` at a listed LAN address sends no
/// SNI and gets that address's certificate, which must name both.
#[test]
fn a_listed_address_serves_its_ip_and_the_local_name_without_sni() {
    let state = tempfile::tempdir().unwrap();
    let authority = Authority::load_or_create(state.path()).unwrap();
    let names = Names::new([ip("192.168.1.20")], Some("studio"));
    let resolver = Arc::new(Resolver::issue(&authority, names, SystemTime::now()).unwrap());
    let client = |sni: bool| {
        let mut roots = RootCertStore::empty();
        roots.add(authority.ca_der().to_vec().into()).unwrap();
        let mut config =
            rustls::ClientConfig::builder_with_provider(Arc::new(ring::default_provider()))
                .with_safe_default_protocol_versions()
                .unwrap()
                .with_root_certificates(roots)
                .with_no_client_auth();
        config.enable_sni = sni;
        config
    };

    let by_name =
        leaf_presented_at(&resolver, ip("192.168.1.20"), client(false), "studio.local").unwrap();
    let by_ip =
        leaf_presented_at(&resolver, ip("192.168.1.20"), client(false), "192.168.1.20").unwrap();
    assert_eq!(by_name, by_ip);
    assert_eq!(
        by_ip,
        resolver.current_for(ip("192.168.1.20")).unwrap().cert[0]
    );
}
