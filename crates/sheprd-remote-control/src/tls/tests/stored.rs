use super::*;

fn presented(resolver: &Resolver) -> Vec<u8> {
    resolver.current_for(ip("192.168.1.20")).unwrap().cert[0].to_vec()
}

#[test]
fn a_restart_presents_the_same_certificate() {
    let state = tempfile::tempdir().unwrap();
    let now = SystemTime::now();
    let first = Resolver::issue(
        &Authority::load_or_create(state.path()).unwrap(),
        lan_names(),
        now,
    )
    .unwrap();
    let authority = Authority::load_or_create(state.path()).unwrap();
    let second = Resolver::issue(&authority, lan_names(), now + DAY).unwrap();
    assert_eq!(presented(&first), presented(&second));
    verify(
        &authority,
        &second.current_for(ip("192.168.1.20")).unwrap(),
        "192.168.1.20",
        now + DAY,
    )
    .unwrap();
}

#[test]
fn a_new_address_gets_its_own_saved_certificate() {
    let state = tempfile::tempdir().unwrap();
    let now = SystemTime::now();
    let authority = Authority::load_or_create(state.path()).unwrap();
    let resolver = Resolver::issue(&authority, lan_names(), now).unwrap();
    let before = presented(&resolver);
    let moved = Names::new([ip("192.168.1.20"), ip("10.0.0.7")], Some("studio"));
    assert!(resolver.refresh(&authority, &moved, now).unwrap());
    assert_eq!(presented(&resolver), before);
    verify(
        &authority,
        &resolver.current_for(ip("10.0.0.7")).unwrap(),
        "10.0.0.7",
        now,
    )
    .unwrap();
    let restarted = Resolver::issue(&authority, moved, now).unwrap();
    assert_eq!(presented(&restarted), presented(&resolver));
}

#[test]
fn an_address_going_away_keeps_the_certificate() {
    let state = tempfile::tempdir().unwrap();
    let now = SystemTime::now();
    let authority = Authority::load_or_create(state.path()).unwrap();
    let resolver = Resolver::issue(&authority, lan_names(), now).unwrap();
    let before = presented(&resolver);
    let fewer = Names::new([ip("192.168.1.20")], Some("studio"));
    assert!(!resolver.refresh(&authority, &fewer, now).unwrap());
    assert_eq!(presented(&resolver), before);
    assert_eq!(
        presented(&Resolver::issue(&authority, fewer, now).unwrap()),
        before
    );
}

#[test]
fn a_saved_certificate_near_expiry_is_replaced_at_start() {
    let state = tempfile::tempdir().unwrap();
    let now = SystemTime::now();
    let authority = Authority::load_or_create(state.path()).unwrap();
    let old = presented(&Resolver::issue(&authority, lan_names(), now).unwrap());
    let later = now + 85 * DAY;
    let renewed = Resolver::issue(&authority, lan_names(), later).unwrap();
    assert_ne!(presented(&renewed), old);
    verify(
        &authority,
        &renewed.current_for(ip("192.168.1.20")).unwrap(),
        "192.168.1.20",
        later,
    )
    .unwrap();
}

#[test]
fn a_certificate_from_another_authority_is_not_presented() {
    let (first, second) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let now = SystemTime::now();
    Resolver::issue(
        &Authority::load_or_create(first.path()).unwrap(),
        lan_names(),
        now,
    )
    .unwrap();
    let authority = Authority::load_or_create(second.path()).unwrap();
    std::fs::copy(
        first.path().join("tls/leaf.pem"),
        second.path().join("tls/leaf.pem"),
    )
    .unwrap();
    let resolver = Resolver::issue(&authority, lan_names(), now).unwrap();
    assert_eq!(resolver.current().cert[1].as_ref(), authority.ca_der());
    verify(
        &authority,
        &resolver.current_for(ip("192.168.1.20")).unwrap(),
        "192.168.1.20",
        now,
    )
    .unwrap();
}

#[cfg(unix)]
#[test]
fn the_saved_certificate_is_private() {
    use std::os::unix::fs::PermissionsExt;
    let state = tempfile::tempdir().unwrap();
    let authority = Authority::load_or_create(state.path()).unwrap();
    Resolver::issue(&authority, lan_names(), SystemTime::now()).unwrap();
    let mode = std::fs::metadata(state.path().join("tls/leaf.pem"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}
