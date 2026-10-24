#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use rustls::{
    RootCertStore,
    client::{WebPkiServerVerifier, danger::ServerCertVerifier},
    pki_types::{ServerName, UnixTime},
    sign::CertifiedKey,
};
use std::{
    net::IpAddr,
    time::{Duration, SystemTime},
};

mod advertised;

const DAY: Duration = Duration::from_secs(24 * 60 * 60);

fn ip(text: &str) -> IpAddr {
    text.parse().unwrap()
}

fn lan_names() -> Names {
    Names::new([ip("192.168.1.20"), ip("100.101.102.103")], Some("studio"))
}

/// Verifies `key`'s chain for `name` against `authority` alone, at `now`.
fn verify(
    authority: &Authority,
    key: &CertifiedKey,
    name: &str,
    now: SystemTime,
) -> std::result::Result<(), rustls::Error> {
    let mut roots = RootCertStore::empty();
    roots.add(authority.ca_der().to_vec().into()).unwrap();
    let verifier = WebPkiServerVerifier::builder_with_provider(
        Arc::new(roots),
        Arc::new(ring::default_provider()),
    )
    .build()
    .unwrap();
    let name = ServerName::try_from(name.to_owned()).unwrap();
    let now = UnixTime::since_unix_epoch(now.duration_since(SystemTime::UNIX_EPOCH).unwrap());
    verifier
        .verify_server_cert(&key.cert[0], &key.cert[1..], &name, &[], now)
        .map(|_| ())
}

#[test]
fn authority_is_created_once_and_reloaded() {
    let state = tempfile::tempdir().unwrap();
    let created = Authority::load_or_create(state.path()).unwrap();
    let reloaded = Authority::load_or_create(state.path()).unwrap();
    assert_eq!(created.fingerprint(), reloaded.fingerprint());
    assert_eq!(created.ca_der(), reloaded.ca_der());
    assert_eq!(created.ca_pem(), reloaded.ca_pem());
    assert!(created.ca_pem().starts_with("-----BEGIN CERTIFICATE-----"));
}

#[test]
fn fingerprint_is_uppercase_colon_separated_sha256() {
    let (authority, _) = Authority::create(Some("studio"), SystemTime::now()).unwrap();
    let parts: Vec<&str> = authority.fingerprint().split(':').collect();
    assert_eq!(parts.len(), 32);
    assert!(parts.iter().all(|part| {
        part.len() == 2
            && part
                .chars()
                .all(|c| c.is_ascii_digit() || c.is_ascii_uppercase())
    }));
}

#[cfg(unix)]
#[test]
fn authority_files_are_private() {
    use std::{os::unix::fs::PermissionsExt, path::Path};
    let state = tempfile::tempdir().unwrap();
    Authority::load_or_create(state.path()).unwrap();
    let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
    let dir = state.path().join("tls");
    assert_eq!(mode(&dir), 0o700);
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    files.sort();
    assert_eq!(files.len(), 2, "only the key and certificate, no leftovers");
    for file in files {
        assert_eq!(mode(&file), 0o600, "{}", file.display());
    }
}

#[test]
fn unreadable_authority_is_an_error_not_a_new_ca() {
    let state = tempfile::tempdir().unwrap();
    Authority::load_or_create(state.path()).unwrap();
    std::fs::write(state.path().join("tls/ca-key.pem"), "not a key").unwrap();
    let error = Authority::load_or_create(state.path()).unwrap_err();
    assert!(matches!(error, Error::InvalidAuthority { .. }), "{error:?}");
    assert!(std::error::Error::source(&error).is_some());
}

#[test]
fn leaf_verifies_for_its_addresses_and_local_name() {
    let state = tempfile::tempdir().unwrap();
    // A reloaded CA must issue the same chain a new one does.
    Authority::load_or_create(state.path()).unwrap();
    let authority = Authority::load_or_create(state.path()).unwrap();
    let now = SystemTime::now();
    let key = authority.issue(&lan_names(), now).unwrap();
    assert_eq!(key.cert.len(), 2);
    assert_eq!(key.cert[1].as_ref(), authority.ca_der());
    for name in [
        "192.168.1.20",
        "100.101.102.103",
        "127.0.0.1",
        "::1",
        "localhost",
        "studio.local",
    ] {
        verify(&authority, &key, name, now).unwrap_or_else(|error| panic!("{name}: {error}"));
    }
    assert!(verify(&authority, &key, "192.168.1.21", now).is_err());
    assert!(verify(&authority, &key, "studio.local", now + 91 * DAY).is_err());
}

#[test]
fn names_outside_the_private_ranges_are_never_listed() {
    let names = Names::new(
        [
            ip("8.8.8.8"),
            ip("2001:db8::1"),
            ip("fe80::1"),
            ip("10.0.0.7"),
        ],
        Some("studio.example.com"),
    );
    assert_eq!(
        names.ips,
        vec![ip("10.0.0.7"), ip("127.0.0.1"), ip("::1")],
        "public and link-local IPv6 addresses are dropped"
    );
    assert_eq!(names.dns, vec!["localhost", "studio.local"]);

    let (authority, _) = Authority::create(Some("studio"), SystemTime::now()).unwrap();
    let now = SystemTime::now();
    let key = authority.issue(&names, now).unwrap();
    assert!(verify(&authority, &key, "8.8.8.8", now).is_err());
}

#[test]
fn name_constraints_reject_a_public_name_even_in_a_leaf() {
    let (authority, _) = Authority::create(Some("studio"), SystemTime::now()).unwrap();
    let now = SystemTime::now();
    let names = Names {
        dns: vec!["example.com".to_owned()],
        ips: vec![ip("8.8.8.8")],
    };
    let key = authority.issue(&names, now).unwrap();
    for name in ["example.com", "8.8.8.8"] {
        let error = verify(&authority, &key, name, now).unwrap_err();
        // The name is in the leaf, so only the CA's constraints reject it;
        // webpki reports that violation as an opaque `Other`.
        assert!(
            matches!(
                error,
                rustls::Error::InvalidCertificate(rustls::CertificateError::Other(_))
            ),
            "{name}: {error:?}"
        );
    }
}

#[test]
fn added_dns_names_stay_inside_the_local_domains() {
    let mut names = lan_names();
    names.add_dns("Studio.tail1234.TS.NET.");
    names.add_dns("example.com");
    names.add_dns("notts.net");
    assert_eq!(names.dns, vec!["localhost", "studio.local"]);
}

#[test]
fn names_compare_equal_whatever_the_input_order() {
    let forward = Names::new([ip("192.168.1.20"), ip("10.0.0.7")], Some("Studio.local"));
    let backward = Names::new(
        [ip("10.0.0.7"), ip("192.168.1.20"), ip("10.0.0.7")],
        Some("studio"),
    );
    assert_eq!(forward, backward);
}

#[test]
fn host_names_that_are_not_dns_labels_get_no_local_name() {
    let names = Names::new([], Some("Fabien's MacBook"));
    assert_eq!(names.dns, vec!["localhost"]);
}

#[test]
fn refresh_reissues_only_when_names_change_or_expiry_nears() {
    let (authority, _) = Authority::create(Some("studio"), SystemTime::now()).unwrap();
    let start = SystemTime::now();
    let names = lan_names();
    let resolver = Resolver::issue(&authority, names.clone(), start).unwrap();
    let first = resolver.current();

    assert!(!resolver.refresh(&authority, &names, start + DAY).unwrap());
    assert!(Arc::ptr_eq(&first, &resolver.current()));

    let moved = Names::new([ip("192.168.7.9")], Some("studio"));
    assert!(resolver.refresh(&authority, &moved, start + DAY).unwrap());
    let second = resolver.current();
    assert!(!Arc::ptr_eq(&first, &second));
    verify(&authority, &second, "192.168.7.9", start + DAY).unwrap();
    assert!(verify(&authority, &second, "192.168.1.20", start + DAY).is_err());

    // Issued at start + 1 day for 90 days: renewed in its last week.
    assert!(
        !resolver
            .refresh(&authority, &moved, start + 83 * DAY)
            .unwrap()
    );
    assert!(
        resolver
            .refresh(&authority, &moved, start + 85 * DAY)
            .unwrap()
    );
    assert!(
        !resolver
            .refresh(&authority, &moved, start + 86 * DAY)
            .unwrap()
    );
    // Long past expiry, too.
    assert!(
        resolver
            .refresh(&authority, &moved, start + 900 * DAY)
            .unwrap()
    );
}

#[test]
fn mobileconfig_is_a_stable_plist_carrying_the_ca() {
    use base64::Engine;
    let (authority, _) = Authority::create(Some("studio"), SystemTime::now()).unwrap();
    let profile = String::from_utf8(authority.mobileconfig()).unwrap();
    assert_eq!(profile.as_bytes(), authority.mobileconfig().as_slice());

    let document = roxmltree::Document::parse_with_options(
        &profile,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        },
    )
    .unwrap();
    let text = |tag: &str| -> Vec<String> {
        document
            .descendants()
            .filter(|node| node.has_tag_name(tag))
            .map(|node| node.text().unwrap_or_default().to_owned())
            .collect()
    };
    let data: String = text("data")[0].split_whitespace().collect();
    let der = base64::engine::general_purpose::STANDARD
        .decode(data)
        .unwrap();
    assert_eq!(der, authority.ca_der());
    let strings = text("string");
    assert!(strings.iter().any(|s| s == "com.apple.security.root"));
    assert!(strings.iter().any(|s| s == "Configuration"));
    assert!(strings.iter().any(|s| s == "Herdr Remote"));

    let uuids: Vec<&String> = strings
        .iter()
        .filter(|s| uuid::Uuid::parse_str(s).is_ok())
        .collect();
    assert_eq!(uuids.len(), 2);
    assert_ne!(uuids[0], uuids[1]);
    assert!(uuids.iter().all(|u| u.to_ascii_uppercase() == **u));

    let (other, _) = Authority::create(Some("studio"), SystemTime::now()).unwrap();
    let other = String::from_utf8(other.mobileconfig()).unwrap();
    assert!(!other.contains(uuids[0].as_str()));
}

#[test]
fn origins_put_lan_first_then_tailscale_then_the_rest() {
    let mut names = Names::new(
        [
            ip("fd7a:115c:a1e0::1"),
            ip("100.101.102.103"),
            ip("169.254.3.4"),
            ip("fd12:3456::9"),
            ip("192.168.1.20"),
            ip("10.0.0.7"),
        ],
        Some("studio"),
    );
    names.add_dns("studio.tail1234.ts.net");
    assert_eq!(
        names.origins(7443),
        vec![
            "https://10.0.0.7:7443",
            "https://192.168.1.20:7443",
            "https://100.101.102.103:7443",
            "https://[fd7a:115c:a1e0::1]:7443",
            "https://169.254.3.4:7443",
            "https://[fd12:3456::9]:7443",
            "https://studio.local:7443",
            "https://127.0.0.1:7443",
            "https://[::1]:7443",
            "https://localhost:7443",
        ]
    );
}

#[test]
fn user_certificate_is_loaded_from_pem_and_never_reissued() {
    let dir = tempfile::tempdir().unwrap();
    let own = rcgen::generate_simple_self_signed(vec!["remote.example.com".to_owned()]).unwrap();
    let cert_path = dir.path().join("cert.pem");
    let key_path = dir.path().join("key.pem");
    std::fs::write(&cert_path, own.cert.pem()).unwrap();
    std::fs::write(&key_path, own.signing_key.serialize_pem()).unwrap();

    let resolver = Resolver::from_files(&cert_path, &key_path).unwrap();
    let current = resolver.current();
    assert_eq!(current.cert.len(), 1);
    assert_eq!(current.cert[0].as_ref(), own.cert.der().as_ref());

    let (authority, _) = Authority::create(Some("studio"), SystemTime::now()).unwrap();
    let now = SystemTime::now();
    assert!(!resolver.refresh(&authority, &lan_names(), now).unwrap());
    assert!(
        !resolver
            .refresh(&authority, &lan_names(), now + 900 * DAY)
            .unwrap()
    );
    assert!(Arc::ptr_eq(&current, &resolver.current()));
}

#[test]
fn user_certificate_errors_name_the_problem() {
    let dir = tempfile::tempdir().unwrap();
    let own = rcgen::generate_simple_self_signed(vec!["remote.example.com".to_owned()]).unwrap();
    let other = rcgen::KeyPair::generate().unwrap();
    let cert_path = dir.path().join("cert.pem");
    let key_path = dir.path().join("key.pem");
    let empty_path = dir.path().join("empty.pem");
    std::fs::write(&cert_path, own.cert.pem()).unwrap();
    std::fs::write(&key_path, other.serialize_pem()).unwrap();
    std::fs::write(&empty_path, "").unwrap();

    let error = Resolver::from_files(&cert_path, &key_path).unwrap_err();
    assert!(matches!(error, Error::InvalidIdentity(_)), "{error:?}");
    let error = Resolver::from_files(&empty_path, &key_path).unwrap_err();
    assert!(
        matches!(&error, Error::NoCertificate { path } if *path == empty_path),
        "{error:?}"
    );
    let error = Resolver::from_files(&cert_path, &empty_path).unwrap_err();
    assert!(matches!(error, Error::Pem { .. }), "{error:?}");
    let missing = dir.path().join("missing.pem");
    let error = Resolver::from_files(&missing, &key_path).unwrap_err();
    assert!(matches!(error, Error::Read { .. }), "{error:?}");
}

#[test]
fn server_config_offers_http_1_1_on_tls_1_2_and_1_3() {
    let (authority, _) = Authority::create(Some("studio"), SystemTime::now()).unwrap();
    let resolver = Resolver::issue(&authority, lan_names(), SystemTime::now()).unwrap();
    let config = server_config(Arc::new(resolver)).unwrap();
    assert_eq!(config.alpn_protocols, vec![b"http/1.1".to_vec()]);
    let versions: Vec<_> = config
        .crypto_provider()
        .cipher_suites
        .iter()
        .map(|suite| suite.version())
        .collect();
    assert!(versions.contains(&&version::TLS13));
    assert!(versions.contains(&&version::TLS12));
}

#[test]
fn errors_wrap_into_the_crate_error() {
    let error = crate::Error::from(Error::NoCertificate {
        path: "cert.pem".into(),
    });
    assert!(matches!(
        error,
        crate::Error::Tls(Error::NoCertificate { .. })
    ));
    assert_eq!(error.to_string(), "cert.pem holds no certificate");
}
