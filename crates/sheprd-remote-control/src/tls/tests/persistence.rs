use super::*;

#[test]
fn transient_link_local_adapters_do_not_rotate_a_saved_certificate() {
    let state = tempfile::tempdir().unwrap();
    let authority = Authority::load_or_create(state.path()).unwrap();
    let now = SystemTime::now();
    let resolver = Resolver::issue(&authority, lan_names(), now).unwrap();
    let before = resolver.current().cert.clone();
    let names = Names::new(
        lan_names()
            .ips
            .into_iter()
            .chain([ip("169.254.244.63"), ip("fe80::1234")]),
        Some("studio"),
    );
    assert!(!resolver.refresh(&authority, &names, now).unwrap());
    assert_eq!(
        Resolver::issue(&authority, names, now)
            .unwrap()
            .current()
            .cert,
        before
    );
}

#[test]
fn an_unsaved_certificate_is_not_presented_at_startup() {
    let state = tempfile::tempdir().unwrap();
    let authority = Authority::load_or_create(state.path()).unwrap();
    let leaf = state.path().join("tls/leaf.pem");
    std::fs::create_dir(&leaf).unwrap();
    let error = Resolver::issue(&authority, lan_names(), SystemTime::now()).unwrap_err();
    assert!(matches!(&error, Error::Write { path, .. } if path == &leaf));
    assert!(std::error::Error::source(&error).is_some());
}

#[test]
fn a_failed_save_keeps_the_existing_certificate_and_authority() {
    let state = tempfile::tempdir().unwrap();
    let authority = Authority::load_or_create(state.path()).unwrap();
    let now = SystemTime::now();
    let resolver = Resolver::issue(&authority, lan_names(), now).unwrap();
    let before = resolver.current().cert.clone();
    let leaf = super::super::stored::path(&state.path().join("tls"), Some(ip("10.1.2.3")));
    std::fs::create_dir(&leaf).unwrap();
    let changed = Names::new([ip("10.1.2.3")], Some("studio"));
    assert!(matches!(
        resolver.refresh(&authority, &changed, now),
        Err(Error::Write { .. })
    ));
    assert_eq!(resolver.current().cert, before);
    let reloaded = Authority::load_or_create(state.path()).unwrap();
    assert_eq!(authority.ca_der(), reloaded.ca_der());
}
