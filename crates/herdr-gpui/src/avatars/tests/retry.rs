use super::*;

const DEVBOX: Checkout<&str> = Checkout::Ssh {
    target: "devbox",
    git_dir: "/home/user/src/app/.git",
};

#[test]
fn an_unreachable_host_is_asked_again_only_after_the_retry_interval() {
    let (mut avatars, incoming, outgoing) = detached();
    let start = Instant::now();
    avatars.request(DEVBOX, start);
    assert_eq!(incoming.try_iter().count(), 1);
    assert!(
        outgoing
            .send((DEVBOX.to_owned(), Err(herdr_client::Error::SshTimeout)))
            .is_ok()
    );
    assert!(avatars.poll(start));
    assert!(avatars.image(DEVBOX).is_none());

    // A sidebar update before the interval ends must not read the host again.
    avatars.request(DEVBOX, start + RETRY_AFTER - Duration::from_millis(1));
    assert!(incoming.try_recv().is_err());

    avatars.request(DEVBOX, start + RETRY_AFTER);
    assert_eq!(incoming.try_iter().collect::<Vec<_>>(), [DEVBOX.to_owned()]);
    // The retry is pending again, so another update does not queue a third read.
    avatars.request(DEVBOX, start + RETRY_AFTER * 2);
    assert!(incoming.try_recv().is_err());

    let image = Arc::new(Image::empty());
    assert!(
        outgoing
            .send((DEVBOX.to_owned(), Ok(Some(image.clone()))))
            .is_ok()
    );
    assert!(avatars.poll(start + RETRY_AFTER));
    assert!(
        avatars
            .image(DEVBOX)
            .is_some_and(|cached| Arc::ptr_eq(&cached, &image))
    );
}

#[test]
fn a_host_read_without_a_github_owner_is_never_asked_again() {
    let (mut avatars, incoming, outgoing) = detached();
    let start = Instant::now();
    avatars.request(DEVBOX, start);
    assert!(outgoing.send((DEVBOX.to_owned(), Ok(None))).is_ok());
    assert!(avatars.poll(start));
    assert_eq!(incoming.try_iter().count(), 1);
    avatars.request(DEVBOX, start + RETRY_AFTER * 10);
    assert!(incoming.try_recv().is_err());
}

#[test]
fn an_unreachable_retry_is_not_refused_by_the_request_bound() {
    let (mut avatars, incoming, outgoing) = detached();
    let start = Instant::now();
    let dirs = (0..MAX_REQUESTS)
        .map(|index| format!("/repo/{index}/.git"))
        .collect::<Vec<_>>();
    for git_dir in &dirs {
        avatars.request(
            Checkout::Ssh {
                target: "devbox",
                git_dir,
            },
            start,
        );
    }
    assert_eq!(incoming.try_iter().count(), MAX_REQUESTS);
    let first = Checkout::Ssh {
        target: "devbox",
        git_dir: &dirs[0],
    };
    assert!(
        outgoing
            .send((first.to_owned(), Err(herdr_client::Error::SshClosed)))
            .is_ok()
    );
    assert!(avatars.poll(start));
    // The bound limits distinct checkouts; retrying one already counted is
    // not a new entry.
    avatars.request(first, start + RETRY_AFTER);
    assert_eq!(incoming.try_iter().collect::<Vec<_>>(), [first.to_owned()]);
}

#[test]
fn local_checkouts_never_fail_the_lookup() {
    let missing = std::env::temp_dir().join("herdr-avatar-no-such-checkout");
    let cwd = missing.to_str().expect("UTF-8 temporary directory");
    assert!(matches!(
        repo_owner(Checkout::Local(cwd), &Default::default()),
        Ok(None)
    ));
}
