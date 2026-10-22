use super::*;

const DEVBOX: Checkout<&str> = Checkout::Ssh {
    target: "devbox",
    git_dir: "/home/user/src/app/.git",
};

#[test]
fn a_reconnected_device_is_asked_again_at_once() {
    let (mut avatars, incoming, outgoing) = detached();
    let start = Instant::now();
    let online = start - Duration::from_secs(1);
    avatars.follow("dev", "devbox", Some(online));
    avatars.request(DEVBOX, start);
    assert_eq!(incoming.try_iter().count(), 1);
    assert!(
        outgoing
            .send((DEVBOX.to_owned(), Err(herdr_client::Error::SshTimeout)))
            .is_ok()
    );
    assert!(avatars.poll(start));

    // The same connection does not shorten the retry interval.
    avatars.follow("dev", "devbox", Some(online));
    avatars.request(DEVBOX, start + Duration::from_secs(1));
    assert!(incoming.try_recv().is_err());

    avatars.follow("dev", "devbox", None);
    let back = start + Duration::from_secs(2);
    avatars.follow("dev", "devbox", Some(back));
    avatars.request(DEVBOX, back);
    assert_eq!(incoming.try_iter().collect::<Vec<_>>(), [DEVBOX.to_owned()]);
}

#[test]
fn a_disconnected_host_stops_its_reads_until_one_device_is_back() {
    let (mut avatars, _incoming, _outgoing) = detached();
    let now = Instant::now();
    avatars.follow("a", "devbox", Some(now));
    avatars.follow("b", "devbox", Some(now));
    assert!(!links::is_offline(&avatars.offline, "devbox"));

    // Another session on the same host keeps it connected.
    avatars.follow("a", "devbox", None);
    assert!(!links::is_offline(&avatars.offline, "devbox"));
    avatars.follow("b", "devbox", None);
    assert!(links::is_offline(&avatars.offline, "devbox"));
    assert!(matches!(
        repo_owner(DEVBOX, &avatars.offline),
        Err(herdr_client::Error::SshCancelled)
    ));

    avatars.follow("a", "devbox", Some(now));
    assert!(!links::is_offline(&avatars.offline, "devbox"));
}

#[test]
fn an_endpoint_pointed_at_another_host_releases_the_old_one() {
    let (mut avatars, _incoming, _outgoing) = detached();
    let now = Instant::now();
    avatars.follow("dev", "old", Some(now));
    avatars.follow("dev", "new", Some(now));
    assert!(links::is_offline(&avatars.offline, "old"));
    assert!(!links::is_offline(&avatars.offline, "new"));
}

#[test]
fn a_read_stopped_by_a_disconnect_is_due_on_the_next_request() {
    let (mut avatars, incoming, outgoing) = detached();
    let now = Instant::now();
    avatars.request(DEVBOX, now);
    assert_eq!(incoming.try_iter().count(), 1);
    assert!(
        outgoing
            .send((DEVBOX.to_owned(), Err(herdr_client::Error::SshCancelled)))
            .is_ok()
    );
    assert!(avatars.poll(now));
    assert!(avatars.image(DEVBOX).is_none());
    avatars.request(DEVBOX, now);
    assert_eq!(incoming.try_iter().collect::<Vec<_>>(), [DEVBOX.to_owned()]);
}

#[test]
fn a_read_failing_after_its_device_reconnected_is_asked_again_at_once() {
    let (mut avatars, incoming, outgoing) = detached();
    let start = Instant::now();
    avatars.follow("dev", "devbox", Some(start));
    avatars.request(DEVBOX, start);
    assert_eq!(incoming.try_iter().count(), 1);

    // The device drops and comes back while the read is still pending, so
    // the new connection's snapshot cannot ask for it again.
    let back = start + Duration::from_secs(1);
    avatars.follow("dev", "devbox", None);
    avatars.follow("dev", "devbox", Some(back));
    avatars.request(DEVBOX, back);
    assert!(incoming.try_recv().is_err());

    for error in [
        herdr_client::Error::SshCancelled,
        herdr_client::Error::SshTimeout,
    ] {
        assert!(outgoing.send((DEVBOX.to_owned(), Err(error))).is_ok());
        assert!(avatars.poll(back + Duration::from_secs(1)));
        assert_eq!(incoming.try_iter().collect::<Vec<_>>(), [DEVBOX.to_owned()]);
        // Asked after the reconnection, so the retry is pending again with
        // the current connection.
        avatars.follow("dev", "devbox", None);
        avatars.follow("dev", "devbox", Some(back + Duration::from_secs(2)));
    }
}

#[test]
fn a_read_failing_on_the_same_connection_waits_for_its_retry() {
    let (mut avatars, incoming, outgoing) = detached();
    let start = Instant::now();
    avatars.follow("dev", "devbox", Some(start));
    avatars.request(DEVBOX, start);
    assert_eq!(incoming.try_iter().count(), 1);
    assert!(
        outgoing
            .send((DEVBOX.to_owned(), Err(herdr_client::Error::SshTimeout)))
            .is_ok()
    );
    assert!(avatars.poll(start));
    assert!(incoming.try_recv().is_err());
}

#[test]
fn a_device_removed_from_the_catalog_takes_its_host_offline() {
    let (mut avatars, _incoming, _outgoing) = detached();
    let now = Instant::now();
    avatars.follow("a", "devbox", Some(now));
    avatars.follow("b", "devbox", Some(now));
    avatars.keep_links(|id| id == "b");
    assert!(!links::is_offline(&avatars.offline, "devbox"));
    avatars.keep_links(|_| false);
    assert!(links::is_offline(&avatars.offline, "devbox"));
}
